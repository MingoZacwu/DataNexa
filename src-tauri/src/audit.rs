use std::collections::VecDeque;
use std::fs;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlparser::ast::{Value, VisitMut, VisitorMut};
use sqlparser::dialect::{Dialect, MySqlDialect, PostgreSqlDialect, SQLiteDialect};
use sqlparser::parser::Parser;
use tauri::{AppHandle, Manager};
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

use crate::config::DbKind;

pub const MAX_AUDIT_MAX_EVENTS: usize = 5000;
const MAX_SQL_CHARS: usize = 20_000;
const REDACTED_LITERAL: &str = "REDACTED";
const REDACTED_UNPARSEABLE_SQL: &str = "[SQL REDACTED: PARSE FAILED]";
const REDACTED_UNKNOWN_DIALECT_SQL: &str = "[SQL REDACTED: DIALECT UNKNOWN]";

pub struct AuditSql(String);

impl AuditSql {
    pub fn new(kind: &DbKind, sql: impl AsRef<str>) -> Self {
        Self(sanitize_sql(kind, sql.as_ref()))
    }

    pub fn raw(sql: impl Into<String>) -> Self {
        Self(sql.into())
    }

    pub fn redacted() -> Self {
        Self(REDACTED_UNKNOWN_DIALECT_SQL.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuditStatus {
    Allowed,
    Denied,
    Error,
    Timeout,
    Truncated,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub connection_id: Option<String>,
    #[serde(default)]
    pub connection_name: Option<String>,
    pub tool: String,
    pub status: AuditStatus,
    pub reason: Option<String>,
    pub elapsed_ms: Option<u64>,
    pub row_count: Option<usize>,
    pub sql: Option<String>,
}

pub struct AuditLogger {
    path: PathBuf,
    events: RwLock<VecDeque<AuditEvent>>,
    persist_lock: Mutex<()>,
}

#[derive(Debug, Serialize, Deserialize)]
struct AuditLogFile {
    #[serde(default)]
    events: Vec<AuditEvent>,
}

impl AuditLogger {
    pub fn new(app: &AppHandle, max_events: usize) -> anyhow::Result<Self> {
        let dir = app.path().app_config_dir()?;
        fs::create_dir_all(&dir)?;
        let path = dir.join("audit-log.json");
        let max_events = normalize_limit(max_events);
        let (events, trimmed) = load_events(&path, max_events)?;

        if trimmed {
            write_events(&path, &events)?;
        }

        Ok(Self {
            path,
            events: RwLock::new(events),
            persist_lock: Mutex::new(()),
        })
    }

    #[cfg(test)]
    pub(crate) fn for_test(path: PathBuf) -> Self {
        Self {
            path,
            events: RwLock::new(VecDeque::new()),
            persist_lock: Mutex::new(()),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn record_with_limit(
        &self,
        connection_id: Option<String>,
        connection_name: Option<String>,
        tool: impl Into<String>,
        status: AuditStatus,
        reason: Option<String>,
        elapsed_ms: Option<u64>,
        row_count: Option<usize>,
        sql: Option<AuditSql>,
        max_events: usize,
    ) -> anyhow::Result<()> {
        let _guard = self.persist_lock.lock().await;
        let tool = tool.into();
        let max_events = normalize_limit(max_events);
        let mut candidate = self.events.read().await.clone();
        candidate.push_front(AuditEvent {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            connection_id,
            connection_name,
            tool,
            status,
            reason,
            elapsed_ms,
            row_count,
            sql: sql.map(|sql| truncate_sql(sql.0)),
        });

        while candidate.len() > max_events {
            candidate.pop_back();
        }

        write_events(&self.path, &candidate)?;
        *self.events.write().await = candidate;
        Ok(())
    }

    pub async fn trim(&self, max_events: usize) -> anyhow::Result<()> {
        let _guard = self.persist_lock.lock().await;
        let max_events = normalize_limit(max_events);
        let mut candidate = self.events.read().await.clone();
        let original_len = candidate.len();
        while candidate.len() > max_events {
            candidate.pop_back();
        }

        if candidate.len() != original_len {
            write_events(&self.path, &candidate)?;
            *self.events.write().await = candidate;
        }
        Ok(())
    }

    pub async fn list(&self) -> Vec<AuditEvent> {
        self.events.read().await.iter().cloned().collect()
    }

    pub async fn clear(&self) -> anyhow::Result<()> {
        let _guard = self.persist_lock.lock().await;
        if self.events.read().await.is_empty() {
            return Ok(());
        }

        let candidate = VecDeque::new();
        write_events(&self.path, &candidate)?;
        *self.events.write().await = candidate;
        Ok(())
    }
}

fn normalize_limit(max_events: usize) -> usize {
    max_events.clamp(1, MAX_AUDIT_MAX_EVENTS)
}

fn load_events(path: &Path, max_events: usize) -> anyhow::Result<(VecDeque<AuditEvent>, bool)> {
    if !path.exists() {
        return Ok((VecDeque::new(), false));
    }

    let text = fs::read_to_string(path)?;
    if text.trim().is_empty() {
        return Ok((VecDeque::new(), false));
    }

    let mut file = serde_json::from_str::<AuditLogFile>(&text)?;
    let trimmed = file.events.len() > max_events;
    file.events.truncate(max_events);
    Ok((file.events.into_iter().collect(), trimmed))
}

fn write_events(path: &Path, events: &VecDeque<AuditEvent>) -> anyhow::Result<()> {
    let file = AuditLogFile {
        events: events.iter().cloned().collect(),
    };
    let text = serde_json::to_string_pretty(&file)?;
    crate::config::atomic_write(path, text.as_bytes())
}

fn truncate_sql(sql: String) -> String {
    let mut chars = sql.chars();
    let truncated = chars.by_ref().take(MAX_SQL_CHARS).collect::<String>();
    if chars.next().is_some() {
        format!("{truncated}\n/* truncated by DataNexa audit log */")
    } else {
        truncated
    }
}

fn sanitize_sql(kind: &DbKind, sql: &str) -> String {
    let mut statements = match Parser::parse_sql(dialect(kind), sql) {
        Ok(statements) if !statements.is_empty() => statements,
        Ok(_) | Err(_) => return REDACTED_UNPARSEABLE_SQL.to_string(),
    };
    let _ = statements.visit(&mut LiteralSanitizer);
    statements
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("; ")
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

struct LiteralSanitizer;

impl VisitorMut for LiteralSanitizer {
    type Break = ();

    fn pre_visit_value(&mut self, value: &mut Value) -> ControlFlow<Self::Break> {
        if !matches!(value, Value::Placeholder(_)) {
            *value = Value::SingleQuotedString(REDACTED_LITERAL.to_string());
        }
        ControlFlow::Continue(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    fn logger(path: PathBuf) -> AuditLogger {
        AuditLogger {
            path,
            events: RwLock::new(VecDeque::new()),
            persist_lock: Mutex::new(()),
        }
    }

    #[test]
    fn sanitizes_literals_for_each_database_dialect() {
        let cases = [
            (
                DbKind::Sqlite,
                "SELECT * FROM account WHERE email = 'fake.user@example.invalid' AND pin = 1234",
            ),
            (
                DbKind::Mysql,
                "SELECT * FROM account WHERE token = X'001122' LIMIT 25",
            ),
            (
                DbKind::Postgres,
                "SELECT * FROM account WHERE secret = $$fake-secret$$ AND enabled = true",
            ),
        ];

        for (kind, sql) in cases {
            let sanitized = AuditSql::new(&kind, sql).0;
            for sensitive in [
                "fake.user@example.invalid",
                "1234",
                "001122",
                "25",
                "fake-secret",
                "true",
            ] {
                assert!(!sanitized.contains(sensitive), "{kind:?}: {sensitive}");
            }
            assert!(sanitized.contains(REDACTED_LITERAL));
        }
    }

    #[test]
    fn keeps_placeholders_without_recording_bound_values() {
        let sanitized = AuditSql::new(
            &DbKind::Postgres,
            "SELECT * FROM account WHERE id = $1 AND state = 'test-state'",
        )
        .0;

        assert!(sanitized.contains("$1"));
        assert!(!sanitized.contains("test-state"));
    }

    #[test]
    fn parse_failure_and_unknown_dialect_never_keep_original_sql() {
        let invalid = "SELECT 'fake-secret' FROM";
        assert_eq!(
            AuditSql::new(&DbKind::Postgres, invalid).0,
            REDACTED_UNPARSEABLE_SQL
        );
        assert_eq!(AuditSql::redacted().0, REDACTED_UNKNOWN_DIALECT_SQL);
    }

    #[tokio::test]
    async fn persists_only_sanitized_sql_for_every_status() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("audit.json");
        let logger = logger(path.clone());

        for status in [
            AuditStatus::Allowed,
            AuditStatus::Denied,
            AuditStatus::Error,
            AuditStatus::Timeout,
            AuditStatus::Truncated,
        ] {
            logger
                .record_with_limit(
                    None,
                    None,
                    "test_sql",
                    status,
                    None,
                    None,
                    None,
                    Some(AuditSql::new(
                        &DbKind::Postgres,
                        "SELECT * FROM account WHERE secret = 'fake-secret'",
                    )),
                    10,
                )
                .await
                .expect("audit persists");
        }

        let persisted = fs::read_to_string(path).expect("audit file reads");
        assert!(!persisted.contains("fake-secret"));
        assert_eq!(logger.list().await.len(), 5);
    }

    #[tokio::test]
    async fn failed_persist_keeps_memory_unchanged() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let target = directory.path().join("audit-target");
        fs::create_dir(&target).expect("target directory");
        let logger = logger(target);

        let result = logger
            .record_with_limit(
                None,
                None,
                "test",
                AuditStatus::Allowed,
                None,
                None,
                None,
                None,
                10,
            )
            .await;

        assert!(result.is_err());
        assert!(logger.list().await.is_empty());
    }

    #[tokio::test]
    async fn concurrent_records_are_serialized_and_persisted() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("audit.json");
        let logger = Arc::new(logger(path.clone()));
        let mut tasks = Vec::new();
        for index in 0..20 {
            let logger = logger.clone();
            tasks.push(tokio::spawn(async move {
                logger
                    .record_with_limit(
                        None,
                        None,
                        format!("test_{index}"),
                        AuditStatus::Allowed,
                        None,
                        None,
                        None,
                        None,
                        50,
                    )
                    .await
            }));
        }
        for task in tasks {
            task.await
                .expect("audit task joins")
                .expect("audit persists");
        }

        assert_eq!(logger.list().await.len(), 20);
        let (persisted, _) = load_events(&path, 50).expect("audit file loads");
        assert_eq!(persisted.len(), 20);
    }

    #[tokio::test]
    async fn clear_and_trim_failures_keep_memory_and_existing_file() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let valid_path = directory.path().join("audit.json");
        let mut logger = logger(valid_path.clone());
        for index in 0..3 {
            logger
                .record_with_limit(
                    None,
                    None,
                    format!("event_{index}"),
                    AuditStatus::Allowed,
                    None,
                    None,
                    None,
                    None,
                    10,
                )
                .await
                .expect("audit persists");
        }
        let original_file = fs::read(&valid_path).expect("audit file reads");
        let invalid_target = directory.path().join("invalid-target");
        fs::create_dir(&invalid_target).expect("directory target");
        logger.path = invalid_target;

        assert!(logger.trim(1).await.is_err());
        assert_eq!(logger.list().await.len(), 3);
        assert_eq!(
            fs::read(&valid_path).expect("audit file remains"),
            original_file
        );

        assert!(logger.clear().await.is_err());
        assert_eq!(logger.list().await.len(), 3);
        assert_eq!(
            fs::read(&valid_path).expect("audit file remains"),
            original_file
        );
    }

    #[tokio::test]
    async fn sql_is_limited_to_twenty_thousand_characters() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let logger = logger(directory.path().join("audit.json"));
        logger
            .record_with_limit(
                None,
                None,
                "long_sql",
                AuditStatus::Allowed,
                None,
                None,
                None,
                Some(AuditSql("X".repeat(MAX_SQL_CHARS + 50))),
                10,
            )
            .await
            .expect("audit persists");

        let sql = logger.list().await[0].sql.clone().expect("SQL is recorded");
        assert_eq!(
            sql.lines().next().expect("first line").chars().count(),
            MAX_SQL_CHARS
        );
        assert!(sql.contains("truncated by DataNexa audit log"));
    }
}
