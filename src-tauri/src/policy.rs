use regex::Regex;
use serde::{Deserialize, Serialize};
use sqlparser::dialect::{MySqlDialect, PostgreSqlDialect, SQLiteDialect};
use sqlparser::parser::Parser;

use crate::config::DbKind;
use crate::i18n::BackendText;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyCheckResult {
    pub allowed: bool,
    pub reason: String,
    pub rewritten_sql: Option<String>,
}

pub struct PolicyEngine;

impl PolicyEngine {
    pub fn check_with_text(
        kind: &DbKind,
        sql: &str,
        max_rows: u32,
        text: &BackendText,
    ) -> PolicyCheckResult {
        let trimmed = sql.trim();
        if trimmed.is_empty() {
            return deny(text.policy_sql_empty());
        }

        let destructive = Regex::new(
            r"(?i)\b(DROP|TRUNCATE|DELETE|UPDATE|INSERT|ALTER|CREATE|GRANT|REVOKE|MERGE|REPLACE|VACUUM|ANALYZE|ATTACH|DETACH|COPY|CALL|EXEC|EXECUTE)\b",
        )
        .expect("valid destructive SQL regex");
        if destructive.is_match(trimmed) {
            return deny(text.policy_destructive_blocked());
        }

        let side_effect = Regex::new(
            r"(?i)\b(pg_sleep|lo_import|lo_export|load_file|outfile|infile|dblink|postgres_fdw|xp_cmdshell)\b",
        )
        .expect("valid side-effect function regex");
        if side_effect.is_match(trimmed) {
            return deny(text.policy_side_effect_blocked());
        }

        let parse_result = match kind {
            DbKind::Sqlite => Parser::parse_sql(&SQLiteDialect {}, trimmed),
            DbKind::Mysql => Parser::parse_sql(&MySqlDialect {}, trimmed),
            DbKind::Postgres => Parser::parse_sql(&PostgreSqlDialect {}, trimmed),
        };

        let statements = match parse_result {
            Ok(statements) => statements,
            Err(error) => return deny(text.policy_parser_rejected(&error.to_string())),
        };

        if statements.len() != 1 {
            return deny(text.policy_single_statement_only());
        }

        let statement_debug = format!("{:?}", statements[0]);
        if !(statement_debug.starts_with("Query") || statement_debug.starts_with("Explain")) {
            return deny(text.policy_select_only());
        }

        let normalized = trim_sql_terminator(trimmed);
        let rewritten_sql = if should_apply_limit(&statement_debug, normalized) {
            Some(format!("{normalized} LIMIT {}", max_rows.max(1)))
        } else {
            Some(normalized.to_string())
        };

        PolicyCheckResult {
            allowed: true,
            reason: text.policy_allowed().to_string(),
            rewritten_sql,
        }
    }
}

fn should_apply_limit(statement_debug: &str, sql: &str) -> bool {
    if !statement_debug.starts_with("Query") {
        return false;
    }

    let has_limit = Regex::new(r"(?i)\blimit\b")
        .expect("valid limit regex")
        .is_match(sql);
    !has_limit
}

fn trim_sql_terminator(sql: &str) -> &str {
    sql.trim().trim_end_matches(';').trim()
}

fn deny(reason: impl Into<String>) -> PolicyCheckResult {
    PolicyCheckResult {
        allowed: false,
        reason: reason.into(),
        rewritten_sql: None,
    }
}
