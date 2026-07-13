use serde::{Deserialize, Serialize};
use sqlparser::ast::{
    Expr, LimitClause, ObjectName, Query, SetExpr, Statement, TableFactor, Value, Visit, Visitor,
};
use sqlparser::dialect::{Dialect, MySqlDialect, PostgreSqlDialect, SQLiteDialect};
use sqlparser::parser::Parser;
use std::ops::ControlFlow;

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
        Self::check_internal(kind, sql, max_rows, text, true)
    }

    pub fn check_explain_target_with_text(
        kind: &DbKind,
        sql: &str,
        text: &BackendText,
    ) -> PolicyCheckResult {
        Self::check_internal(kind, sql, 1, text, false)
    }

    fn check_internal(
        kind: &DbKind,
        sql: &str,
        max_rows: u32,
        text: &BackendText,
        enforce_row_limit: bool,
    ) -> PolicyCheckResult {
        let trimmed = sql.trim();
        if trimmed.is_empty() {
            return deny(text.policy_sql_empty());
        }

        let sql_dialect = dialect(kind);
        let mut statements = match Parser::parse_sql(sql_dialect, trimmed) {
            Ok(statements) => statements,
            Err(error) => return deny(text.policy_parser_rejected(&error.to_string())),
        };

        if statements.len() != 1 {
            return deny(text.policy_single_statement_only());
        }

        if let Err(violation) = validate_statement(&statements[0], kind) {
            return deny(violation.reason(text));
        }

        let max_rows = max_rows.max(1);
        let probe_rows = u64::from(max_rows) + 1;
        let requires_bounded_result =
            enforce_row_limit && matches!(statements[0], Statement::Query(_));
        if requires_bounded_result {
            rewrite_outer_limit(&mut statements[0], max_rows, probe_rows);
        }

        let rewritten_sql = statements[0].to_string();
        let reparsed = match Parser::parse_sql(sql_dialect, &rewritten_sql) {
            Ok(statements) => statements,
            Err(error) => return deny(text.policy_parser_rejected(&error.to_string())),
        };
        if reparsed.len() != 1 {
            return deny(text.policy_single_statement_only());
        }
        if let Err(violation) = validate_statement(&reparsed[0], kind) {
            return deny(violation.reason(text));
        }
        if requires_bounded_result && !has_bounded_outer_limit(&reparsed[0], probe_rows) {
            return deny(text.policy_parser_rejected(
                "rewritten query does not have a statically bounded outer row limit",
            ));
        }

        PolicyCheckResult {
            allowed: true,
            reason: text.policy_allowed().to_string(),
            rewritten_sql: Some(rewritten_sql),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum PolicyViolation {
    Destructive,
    SideEffect,
    SelectOnly,
}

impl PolicyViolation {
    fn reason(self, text: &BackendText) -> String {
        match self {
            Self::Destructive => text.policy_destructive_blocked().to_string(),
            Self::SideEffect => text.policy_side_effect_blocked().to_string(),
            Self::SelectOnly => text.policy_select_only().to_string(),
        }
    }
}

fn dialect(kind: &DbKind) -> &'static dyn Dialect {
    static SQLITE: SQLiteDialect = SQLiteDialect {};
    static MYSQL: MySqlDialect = MySqlDialect {};
    static POSTGRES: PostgreSqlDialect = PostgreSqlDialect {};

    match kind {
        DbKind::Sqlite => &SQLITE,
        DbKind::Mysql => &MYSQL,
        DbKind::Postgres => &POSTGRES,
    }
}

fn validate_statement(statement: &Statement, kind: &DbKind) -> Result<(), PolicyViolation> {
    let query = match statement {
        Statement::Query(query) => query.as_ref(),
        Statement::Explain {
            analyze,
            options,
            statement,
            ..
        } => {
            if *analyze || explain_options_enable_analyze(options.as_deref()) {
                return Err(PolicyViolation::Destructive);
            }
            match statement.as_ref() {
                Statement::Query(query) => query.as_ref(),
                _ => return Err(PolicyViolation::SelectOnly),
            }
        }
        _ => return Err(PolicyViolation::SelectOnly),
    };

    match query.visit(&mut SafetyVisitor { kind }) {
        ControlFlow::Continue(()) => Ok(()),
        ControlFlow::Break(violation) => Err(violation),
    }
}

fn explain_options_enable_analyze(options: Option<&[sqlparser::ast::UtilityOption]>) -> bool {
    options.is_some_and(|options| {
        options.iter().any(|option| {
            option.name.value.eq_ignore_ascii_case("analyze")
                && !option.arg.as_ref().is_some_and(is_explicit_false)
        })
    })
}

fn is_explicit_false(expr: &Expr) -> bool {
    matches!(expr, Expr::Value(value) if matches!(value.value, Value::Boolean(false)))
}

struct SafetyVisitor<'a> {
    kind: &'a DbKind,
}

impl Visitor for SafetyVisitor<'_> {
    type Break = PolicyViolation;

    fn pre_visit_query(&mut self, query: &Query) -> ControlFlow<Self::Break> {
        if !query.locks.is_empty()
            || set_expr_writes(&query.body)
            || set_expr_selects_into(&query.body)
        {
            return ControlFlow::Break(PolicyViolation::Destructive);
        }
        ControlFlow::Continue(())
    }

    fn pre_visit_statement(&mut self, _statement: &Statement) -> ControlFlow<Self::Break> {
        ControlFlow::Break(PolicyViolation::Destructive)
    }

    fn pre_visit_table_factor(&mut self, table_factor: &TableFactor) -> ControlFlow<Self::Break> {
        let function = match table_factor {
            TableFactor::Table {
                name,
                args: Some(_),
                ..
            }
            | TableFactor::Function { name, .. } => Some(name),
            _ => None,
        };
        if function
            .and_then(function_name)
            .is_some_and(|name| is_side_effect_function(self.kind, name))
        {
            return ControlFlow::Break(PolicyViolation::SideEffect);
        }
        ControlFlow::Continue(())
    }

    fn pre_visit_expr(&mut self, expr: &Expr) -> ControlFlow<Self::Break> {
        if let Expr::Function(function) = expr {
            if function_name(&function.name)
                .is_some_and(|name| is_side_effect_function(self.kind, name))
            {
                return ControlFlow::Break(PolicyViolation::SideEffect);
            }
        }
        ControlFlow::Continue(())
    }
}

fn set_expr_writes(expr: &SetExpr) -> bool {
    match expr {
        SetExpr::Insert(_) | SetExpr::Update(_) | SetExpr::Delete(_) | SetExpr::Merge(_) => true,
        SetExpr::Query(query) => set_expr_writes(&query.body),
        SetExpr::SetOperation { left, right, .. } => {
            set_expr_writes(left) || set_expr_writes(right)
        }
        SetExpr::Select(_) | SetExpr::Values(_) | SetExpr::Table(_) => false,
    }
}

fn set_expr_selects_into(expr: &SetExpr) -> bool {
    match expr {
        SetExpr::Select(select) => select.into.is_some(),
        SetExpr::Query(query) => set_expr_selects_into(&query.body),
        SetExpr::SetOperation { left, right, .. } => {
            set_expr_selects_into(left) || set_expr_selects_into(right)
        }
        SetExpr::Insert(_)
        | SetExpr::Update(_)
        | SetExpr::Delete(_)
        | SetExpr::Merge(_)
        | SetExpr::Values(_)
        | SetExpr::Table(_) => false,
    }
}

fn function_name(name: &ObjectName) -> Option<&str> {
    name.0.last()?.as_ident().map(|ident| ident.value.as_str())
}

fn is_side_effect_function(kind: &DbKind, name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    match kind {
        DbKind::Sqlite => matches!(
            name.as_str(),
            "edit" | "load_extension" | "readfile" | "shell" | "writefile"
        ),
        DbKind::Mysql => matches!(
            name.as_str(),
            "benchmark"
                | "get_lock"
                | "load_file"
                | "master_pos_wait"
                | "release_all_locks"
                | "release_lock"
                | "service_get_write_locks"
                | "sleep"
                | "source_pos_wait"
                | "sys_exec"
                | "sys_eval"
        ),
        DbKind::Postgres => {
            name.starts_with("dblink")
                || matches!(
                    name.as_str(),
                    "lo_export"
                        | "lo_import"
                        | "lo_create"
                        | "lo_unlink"
                        | "lo_from_bytea"
                        | "lo_put"
                        | "lo_truncate"
                        | "lo_truncate64"
                        | "lowrite"
                        | "nextval"
                        | "pg_advisory_lock"
                        | "pg_advisory_lock_shared"
                        | "pg_advisory_unlock"
                        | "pg_advisory_unlock_all"
                        | "pg_advisory_unlock_shared"
                        | "pg_advisory_xact_lock"
                        | "pg_advisory_xact_lock_shared"
                        | "pg_cancel_backend"
                        | "pg_create_restore_point"
                        | "pg_backup_start"
                        | "pg_backup_stop"
                        | "pg_start_backup"
                        | "pg_stop_backup"
                        | "pg_create_physical_replication_slot"
                        | "pg_create_logical_replication_slot"
                        | "pg_copy_physical_replication_slot"
                        | "pg_copy_logical_replication_slot"
                        | "pg_drop_replication_slot"
                        | "pg_replication_slot_advance"
                        | "pg_export_snapshot"
                        | "pg_log_backend_memory_context"
                        | "pg_logical_emit_message"
                        | "pg_logical_slot_get_binary_changes"
                        | "pg_logical_slot_get_changes"
                        | "pg_notify"
                        | "pg_promote"
                        | "pg_replication_origin_advance"
                        | "pg_replication_origin_create"
                        | "pg_replication_origin_drop"
                        | "pg_replication_origin_session_reset"
                        | "pg_replication_origin_session_setup"
                        | "pg_replication_origin_xact_reset"
                        | "pg_replication_origin_xact_setup"
                        | "pg_reload_conf"
                        | "pg_rotate_logfile"
                        | "pg_sleep"
                        | "pg_sleep_for"
                        | "pg_sleep_until"
                        | "pg_stat_reset"
                        | "pg_stat_reset_shared"
                        | "pg_stat_reset_single_function_counters"
                        | "pg_stat_reset_single_table_counters"
                        | "pg_stat_reset_slru"
                        | "pg_switch_wal"
                        | "pg_terminate_backend"
                        | "pg_try_advisory_lock"
                        | "pg_try_advisory_lock_shared"
                        | "pg_try_advisory_xact_lock"
                        | "pg_try_advisory_xact_lock_shared"
                        | "pg_wal_replay_pause"
                        | "pg_wal_replay_resume"
                        | "set_config"
                        | "setseed"
                        | "setval"
                )
        }
    }
}

fn rewrite_outer_limit(statement: &mut Statement, max_rows: u32, probe_rows: u64) {
    let query = match statement {
        Statement::Query(query) => query.as_mut(),
        _ => return,
    };

    if let Some(limit_clause) = &mut query.limit_clause {
        tighten_limit_clause(limit_clause, max_rows, probe_rows);
    }
    if let Some(fetch) = &mut query.fetch {
        let safe_quantity = fetch
            .quantity
            .as_ref()
            .and_then(constant_row_count)
            .is_some_and(|quantity| quantity <= u64::from(max_rows));
        if fetch.percent || fetch.with_ties || (fetch.quantity.is_some() && !safe_quantity) {
            fetch.quantity = Some(row_count_expr(probe_rows));
            fetch.percent = false;
            fetch.with_ties = false;
        }
    }
    if query.limit_clause.is_none() && query.fetch.is_none() {
        query.limit_clause = Some(LimitClause::LimitOffset {
            limit: Some(row_count_expr(probe_rows)),
            offset: None,
            limit_by: Vec::new(),
        });
    }
}

fn tighten_limit_clause(limit_clause: &mut LimitClause, max_rows: u32, probe_rows: u64) {
    let max_rows = u64::from(max_rows);
    match limit_clause {
        LimitClause::LimitOffset { limit, .. } => {
            let safe = limit
                .as_ref()
                .and_then(constant_row_count)
                .is_some_and(|quantity| quantity <= max_rows);
            if !safe {
                *limit = Some(row_count_expr(probe_rows));
            }
        }
        LimitClause::OffsetCommaLimit { limit, .. } => {
            if constant_row_count(limit).is_none_or(|quantity| quantity > max_rows) {
                *limit = row_count_expr(probe_rows);
            }
        }
    }
}

fn constant_row_count(expr: &Expr) -> Option<u64> {
    match expr {
        Expr::Value(value) => match &value.value {
            Value::Number(number, _) => number.parse().ok(),
            _ => None,
        },
        _ => None,
    }
}

fn row_count_expr(row_count: u64) -> Expr {
    Expr::Value(Value::Number(row_count.to_string(), false).into())
}

fn has_bounded_outer_limit(statement: &Statement, probe_rows: u64) -> bool {
    let query = match statement {
        Statement::Query(query) => query.as_ref(),
        _ => return false,
    };

    let limit_is_bounded = query
        .limit_clause
        .as_ref()
        .is_some_and(|limit| match limit {
            LimitClause::LimitOffset {
                limit: Some(limit), ..
            }
            | LimitClause::OffsetCommaLimit { limit, .. } => {
                constant_row_count(limit).is_some_and(|quantity| quantity <= probe_rows)
            }
            LimitClause::LimitOffset { limit: None, .. } => false,
        });
    let fetch_is_bounded = query.fetch.as_ref().is_some_and(|fetch| {
        !fetch.percent
            && !fetch.with_ties
            && fetch
                .quantity
                .as_ref()
                .map(constant_row_count)
                .unwrap_or(Some(1))
                .is_some_and(|quantity| quantity <= probe_rows)
    });

    limit_is_bounded || fetch_is_bounded
}

fn deny(reason: impl Into<String>) -> PolicyCheckResult {
    PolicyCheckResult {
        allowed: false,
        reason: reason.into(),
        rewritten_sql: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::backend_text;

    const MAX_ROWS: u32 = 10;

    fn check(kind: &DbKind, sql: &str) -> PolicyCheckResult {
        PolicyEngine::check_with_text(kind, sql, MAX_ROWS, &backend_text("en"))
    }

    fn rewritten(kind: &DbKind, sql: &str) -> String {
        let result = check(kind, sql);
        assert!(result.allowed, "{}", result.reason);
        result.rewritten_sql.expect("allowed SQL is rewritten")
    }

    #[test]
    fn supports_common_query_forms_in_all_dialects() {
        for kind in [&DbKind::Sqlite, &DbKind::Mysql, &DbKind::Postgres] {
            assert!(rewritten(kind, "SELECT * FROM users").ends_with("LIMIT 11"));
            assert!(rewritten(
                kind,
                "WITH active AS (SELECT * FROM users) SELECT * FROM active"
            )
            .ends_with("LIMIT 11"));
            assert!(rewritten(kind, "SELECT 1 UNION SELECT 2").ends_with("LIMIT 11"));
        }
    }

    #[test]
    fn preserves_small_limit_and_tightens_unbounded_limits() {
        for kind in [&DbKind::Sqlite, &DbKind::Mysql, &DbKind::Postgres] {
            assert!(rewritten(kind, "SELECT * FROM users LIMIT 5").ends_with("LIMIT 5"));
            assert!(rewritten(kind, "SELECT * FROM users LIMIT 5000000").ends_with("LIMIT 11"));
            assert!(rewritten(kind, "SELECT * FROM users LIMIT 4 OFFSET 20")
                .ends_with("LIMIT 4 OFFSET 20"));
            assert!(rewritten(kind, "SELECT * FROM users LIMIT NULL").ends_with("LIMIT 11"));
        }
    }

    #[test]
    fn handles_mysql_offset_comma_limit() {
        assert!(
            rewritten(&DbKind::Mysql, "SELECT * FROM users LIMIT 20, 500")
                .ends_with("LIMIT 20, 11")
        );
        assert!(
            rewritten(&DbKind::Mysql, "SELECT * FROM users LIMIT 20, 5").ends_with("LIMIT 20, 5")
        );
    }

    #[test]
    fn comments_and_strings_do_not_affect_limit_or_function_checks() {
        for kind in [&DbKind::Sqlite, &DbKind::Mysql, &DbKind::Postgres] {
            assert!(
                rewritten(kind, "SELECT 'limit 999; pg_sleep(1)' AS note").ends_with("LIMIT 11")
            );
            assert!(
                rewritten(kind, "SELECT 1 /* LIMIT 999; DELETE FROM users */")
                    .ends_with("LIMIT 11")
            );
        }
    }

    #[test]
    fn tightens_fetch_variants_without_changing_safe_fetch() {
        assert!(rewritten(
            &DbKind::Postgres,
            "SELECT * FROM users FETCH FIRST 5 ROWS ONLY"
        )
        .ends_with("FETCH FIRST 5 ROWS ONLY"));
        assert!(rewritten(
            &DbKind::Postgres,
            "SELECT * FROM users FETCH FIRST 500 ROWS ONLY"
        )
        .ends_with("FETCH FIRST 11 ROWS ONLY"));
        assert!(rewritten(
            &DbKind::Postgres,
            "SELECT * FROM users ORDER BY id FETCH FIRST 5 ROWS WITH TIES"
        )
        .ends_with("FETCH FIRST 11 ROWS ONLY"));
        assert!(rewritten(
            &DbKind::Postgres,
            "SELECT * FROM users FETCH FIRST 5 PERCENT ROWS ONLY"
        )
        .ends_with("FETCH FIRST 11 ROWS ONLY"));
    }

    #[test]
    fn rejects_select_into_locks_multiple_statements_and_nested_dml() {
        assert!(!check(&DbKind::Postgres, "SELECT * INTO backup FROM users").allowed);
        assert!(!check(&DbKind::Postgres, "SELECT * FROM users FOR UPDATE").allowed);
        assert!(!check(&DbKind::Postgres, "SELECT * FROM users FOR SHARE").allowed);
        assert!(!check(&DbKind::Sqlite, "SELECT 1; SELECT 2").allowed);
        assert!(
            !check(
                &DbKind::Postgres,
                "WITH removed AS (DELETE FROM users RETURNING id) SELECT id FROM removed"
            )
            .allowed
        );
    }

    #[test]
    fn permits_readonly_explain_and_rejects_executing_explain() {
        assert_eq!(
            rewritten(&DbKind::Postgres, "EXPLAIN SELECT * FROM users"),
            "EXPLAIN SELECT * FROM users"
        );
        assert_eq!(
            rewritten(
                &DbKind::Postgres,
                "EXPLAIN (ANALYZE FALSE) SELECT * FROM users"
            ),
            "EXPLAIN (ANALYZE false) SELECT * FROM users"
        );
        assert_eq!(
            rewritten(
                &DbKind::Postgres,
                "EXPLAIN SELECT * FROM users LIMIT 5000000"
            ),
            "EXPLAIN SELECT * FROM users LIMIT 5000000"
        );
        assert!(!check(&DbKind::Postgres, "EXPLAIN ANALYZE SELECT * FROM users").allowed);
        assert!(
            !check(
                &DbKind::Postgres,
                "EXPLAIN (ANALYZE TRUE) SELECT * FROM users"
            )
            .allowed
        );
        assert!(
            check(
                &DbKind::Postgres,
                "EXPLAIN (ANALYZE FALSE) SELECT * FROM users"
            )
            .allowed
        );
        assert!(
            !check(
                &DbKind::Postgres,
                "EXPLAIN SELECT pg_notify('test_channel', 'test_payload')"
            )
            .allowed
        );
    }

    #[test]
    fn blocks_dialect_specific_side_effect_functions() {
        for sql in [
            "SELECT pg_sleep(1)",
            "SELECT nextval('sequence_name')",
            "SELECT pg_catalog.set_config('work_mem', '1MB', false)",
            "SELECT dblink_connect('connection_name')",
            "SELECT * FROM dblink('connection_name', 'SELECT 1') AS t(value integer)",
            "SELECT pg_notify('test_channel', 'test_payload')",
            "SELECT lo_create(1000)",
            "SELECT lo_unlink(1000)",
            "SELECT pg_logical_emit_message(true, 'test_prefix', 'test_payload')",
            "SELECT lo_put(1000, 0, decode('00', 'hex'))",
            "SELECT pg_logical_slot_get_changes('test_slot', NULL, NULL)",
            "SELECT pg_replication_origin_advance('test_origin', '0/1')",
            "SELECT pg_stat_reset()",
            "SELECT setseed(0.5)",
            "SELECT pg_drop_replication_slot('test_slot')",
            "SELECT pg_catalog.pg_replication_slot_advance('test_slot', '0/1')",
            "SELECT pg_backup_start('test_label')",
            "SELECT * FROM pg_catalog.pg_backup_stop()",
            "SELECT * FROM pg_catalog.pg_create_logical_replication_slot('test_slot', 'test_plugin')",
        ] {
            let result = check(&DbKind::Postgres, sql);
            assert!(!result.allowed, "{sql}");
            assert_eq!(
                result.reason,
                backend_text("en").policy_side_effect_blocked()
            );
        }
        for sql in [
            "SELECT SLEEP(1)",
            "SELECT BENCHMARK(10, 1)",
            "SELECT GET_LOCK('lock_name', 1)",
        ] {
            assert!(!check(&DbKind::Mysql, sql).allowed, "{sql}");
        }
        assert!(!check(&DbKind::Sqlite, "SELECT load_extension('extension_name')").allowed);
    }

    #[test]
    fn uses_wider_integer_for_probe_row_count() {
        let result = PolicyEngine::check_with_text(
            &DbKind::Postgres,
            "SELECT 1",
            u32::MAX,
            &backend_text("en"),
        );
        assert!(result.allowed);
        assert!(result
            .rewritten_sql
            .expect("allowed SQL is rewritten")
            .ends_with("LIMIT 4294967296"));
    }
}
