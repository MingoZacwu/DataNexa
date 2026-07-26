use std::ops::ControlFlow;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlparser::ast::{Value, VisitMut, VisitorMut};
use sqlparser::dialect::{Dialect, MySqlDialect, PostgreSqlDialect, SQLiteDialect};
use sqlparser::parser::Parser;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum AuditMigrationState {
    Ready,
    Migrating {
        phase: AuditMigrationPhase,
        processed: u64,
        total: u64,
    },
    Failed {
        reason: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditMigrationPhase {
    ReadingLegacyFile,
    PreparingDatabase,
    ImportingEvents,
    Committing,
    Finalizing,
}

pub struct AuditLogger {
    legacy_path: PathBuf,
    pool: SqlitePool,
    persist_lock: Mutex<()>,
    migration: RwLock<AuditMigrationState>,
    initialized: AtomicBool,
}

#[derive(Debug, Deserialize)]
struct AuditLogFile {
    #[serde(default)]
    events: Vec<AuditEvent>,
}

impl AuditLogger {
    pub fn new(app: &AppHandle, _max_events: usize) -> anyhow::Result<Self> {
        let dir = app.path().app_config_dir()?;
        std::fs::create_dir_all(&dir)?;
        let db_path = dir.join("audit.db");
        let legacy_path = dir.join("audit-log.json");
        let options = SqliteConnectOptions::new()
            .filename(&db_path)
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_lazy_with(options);
        let phase = if legacy_path.exists() {
            AuditMigrationPhase::ReadingLegacyFile
        } else {
            AuditMigrationPhase::PreparingDatabase
        };
        let migration = AuditMigrationState::Migrating {
            phase,
            processed: 0,
            total: 0,
        };
        Ok(Self {
            legacy_path,
            pool,
            persist_lock: Mutex::new(()),
            migration: RwLock::new(migration),
            initialized: AtomicBool::new(false),
        })
    }

    #[cfg(test)]
    pub(crate) fn for_test(path: PathBuf) -> Self {
        let options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true);
        Self {
            legacy_path: path.with_extension("legacy.json"),
            pool: SqlitePoolOptions::new()
                .max_connections(1)
                .connect_lazy_with(options),
            persist_lock: Mutex::new(()),
            migration: RwLock::new(AuditMigrationState::Ready),
            initialized: AtomicBool::new(false),
        }
    }

    pub async fn initialize(&self, max_events: usize) -> anyhow::Result<()> {
        if let Err(error) = self.prepare_schema().await {
            return self.fail(error).await;
        }
        self.initialized.store(true, Ordering::Release);
        if self.legacy_path.exists() {
            self.migrate(max_events).await
        } else {
            *self.migration.write().await = AuditMigrationState::Ready;
            Ok(())
        }
    }

    async fn prepare_schema(&self) -> anyhow::Result<()> {
        sqlx::query("PRAGMA journal_mode=WAL")
            .execute(&self.pool)
            .await?;
        sqlx::query("CREATE TABLE IF NOT EXISTS audit_events (seq INTEGER PRIMARY KEY AUTOINCREMENT, event_id TEXT NOT NULL UNIQUE, timestamp_ms INTEGER NOT NULL, connection_id TEXT, connection_name TEXT, tool TEXT NOT NULL, status TEXT NOT NULL, reason TEXT, elapsed_ms INTEGER, row_count INTEGER, sql TEXT)").execute(&self.pool).await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_audit_events_timestamp ON audit_events(timestamp_ms DESC)").execute(&self.pool).await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_audit_events_connection ON audit_events(connection_id, seq DESC)").execute(&self.pool).await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_audit_events_tool ON audit_events(tool, seq DESC)",
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_audit_events_status ON audit_events(status, seq DESC)",
        )
        .execute(&self.pool)
        .await?;
        sqlx::query("PRAGMA user_version = 1")
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn ensure_initialized(&self) -> anyhow::Result<()> {
        if !self.initialized.load(Ordering::Acquire) {
            self.prepare_schema().await?;
            self.initialized.store(true, Ordering::Release);
        }
        Ok(())
    }

    pub async fn migration_state(&self) -> AuditMigrationState {
        self.migration.read().await.clone()
    }
    pub async fn is_ready(&self) -> bool {
        matches!(*self.migration.read().await, AuditMigrationState::Ready)
    }

    pub async fn migrate(&self, max_events: usize) -> anyhow::Result<()> {
        let text = match tokio::fs::read_to_string(&self.legacy_path).await {
            Ok(value) => value,
            Err(error) => return self.fail(error).await,
        };
        let events = match serde_json::from_str::<AuditLogFile>(&text) {
            Ok(file) => file.events,
            Err(error) => return self.fail(error).await,
        };
        let total = events.len() as u64;
        *self.migration.write().await = AuditMigrationState::Migrating {
            phase: AuditMigrationPhase::PreparingDatabase,
            processed: 0,
            total,
        };
        if let Err(error) = self.prepare_schema().await {
            return self.fail(error).await;
        }
        let mut tx = match self.pool.begin().await {
            Ok(tx) => tx,
            Err(error) => return self.fail(error).await,
        };
        *self.migration.write().await = AuditMigrationState::Migrating {
            phase: AuditMigrationPhase::ImportingEvents,
            processed: 0,
            total,
        };
        for (index, event) in events.iter().enumerate() {
            if let Err(error) = sqlx::query("INSERT OR IGNORE INTO audit_events (event_id,timestamp_ms,connection_id,connection_name,tool,status,reason,elapsed_ms,row_count,sql) VALUES (?,?,?,?,?,?,?,?,?,?)")
                .bind(&event.id).bind(event.timestamp.timestamp_millis()).bind(&event.connection_id).bind(&event.connection_name).bind(&event.tool)
                .bind(status_text(&event.status)).bind(&event.reason).bind(event.elapsed_ms.map(|v| v as i64)).bind(event.row_count.map(|v| v as i64)).bind(&event.sql).execute(&mut *tx).await { return self.fail(error).await; }
            if index % 100 == 0 || index + 1 == events.len() {
                *self.migration.write().await = AuditMigrationState::Migrating {
                    phase: AuditMigrationPhase::ImportingEvents,
                    processed: (index + 1) as u64,
                    total,
                };
            }
        }
        *self.migration.write().await = AuditMigrationState::Migrating {
            phase: AuditMigrationPhase::Committing,
            processed: total,
            total,
        };
        let limit = normalize_limit(max_events) as i64;
        if let Err(error) = sqlx::query("DELETE FROM audit_events WHERE seq NOT IN (SELECT seq FROM audit_events ORDER BY seq DESC LIMIT ?)").bind(limit).execute(&mut *tx).await { return self.fail(error).await; }
        if let Err(error) = tx.commit().await {
            return self.fail(error).await;
        }
        *self.migration.write().await = AuditMigrationState::Migrating {
            phase: AuditMigrationPhase::Finalizing,
            processed: total,
            total,
        };
        let mut migrated = self.legacy_path.with_extension("json.migrated");
        if migrated.exists() {
            migrated = self
                .legacy_path
                .with_extension(format!("json.migrated.{}", Uuid::new_v4()));
        }
        if self.legacy_path.exists() {
            if let Err(error) = tokio::fs::rename(&self.legacy_path, migrated).await {
                return self.fail(error).await;
            }
        }
        *self.migration.write().await = AuditMigrationState::Ready;
        Ok(())
    }

    async fn fail<T: std::fmt::Display>(&self, error: T) -> anyhow::Result<()> {
        let reason = sanitize_reason(&error.to_string());
        *self.migration.write().await = AuditMigrationState::Failed {
            reason: reason.clone(),
        };
        Err(anyhow::anyhow!(reason))
    }

    pub async fn retry(&self, max_events: usize) -> anyhow::Result<()> {
        *self.migration.write().await = AuditMigrationState::Migrating {
            phase: AuditMigrationPhase::ReadingLegacyFile,
            processed: 0,
            total: 0,
        };
        self.migrate(max_events).await
    }

    pub async fn clear_legacy(&self) -> anyhow::Result<()> {
        let _guard = self.persist_lock.lock().await;
        self.ensure_initialized().await?;
        if self.legacy_path.exists() {
            let discarded = self
                .legacy_path
                .with_extension(format!("json.discarded.{}", Uuid::new_v4()));
            tokio::fs::rename(&self.legacy_path, &discarded).await?;
        }
        let clear_result = async {
            let mut transaction = self.pool.begin().await?;
            sqlx::query("DELETE FROM audit_events")
                .execute(&mut *transaction)
                .await?;
            sqlx::query("DELETE FROM sqlite_sequence WHERE name = 'audit_events'")
                .execute(&mut *transaction)
                .await?;
            transaction.commit().await
        }
        .await;
        if let Err(error) = clear_result {
            *self.migration.write().await = AuditMigrationState::Failed {
                reason: sanitize_reason(&error.to_string()),
            };
            return Err(error.into());
        }
        self.prepare_schema().await?;
        *self.migration.write().await = AuditMigrationState::Ready;
        Ok(())
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
        let result = async {
            if !self.is_ready().await {
                return Err(anyhow::anyhow!("audit migration is not ready"));
            }
            let _guard = self.persist_lock.lock().await;
            self.ensure_initialized().await?;
            let mut tx = self.pool.begin().await?;
            sqlx::query("INSERT INTO audit_events (event_id,timestamp_ms,connection_id,connection_name,tool,status,reason,elapsed_ms,row_count,sql) VALUES (?,?,?,?,?,?,?,?,?,?)")
                .bind(Uuid::new_v4().to_string()).bind(Utc::now().timestamp_millis()).bind(connection_id).bind(connection_name).bind(tool.into())
                .bind(status_text(&status)).bind(reason).bind(elapsed_ms.map(|v| v as i64)).bind(row_count.map(|v| v as i64)).bind(sql.map(|v| truncate_sql(v.0))).execute(&mut *tx).await?;
            let limit = normalize_limit(max_events) as i64;
            sqlx::query("DELETE FROM audit_events WHERE seq NOT IN (SELECT seq FROM audit_events ORDER BY seq DESC LIMIT ?)").bind(limit).execute(&mut *tx).await?;
            tx.commit().await?;
            Ok::<(), anyhow::Error>(())
        }.await;
        result.map_err(|error| anyhow::anyhow!("audit storage unavailable: {error}"))
    }

    pub async fn trim(&self, max_events: usize) -> anyhow::Result<()> {
        if !self.is_ready().await {
            return Ok(());
        }
        self.ensure_initialized().await?;
        sqlx::query("DELETE FROM audit_events WHERE seq NOT IN (SELECT seq FROM audit_events ORDER BY seq DESC LIMIT ?)").bind(normalize_limit(max_events) as i64).execute(&self.pool).await?;
        Ok(())
    }
    pub async fn list(&self) -> anyhow::Result<Vec<AuditEvent>> {
        self.ensure_initialized().await?;
        let rows = sqlx::query_as::<_, (String, i64, Option<String>, Option<String>, String, String, Option<String>, Option<i64>, Option<i64>, Option<String>)>("SELECT event_id,timestamp_ms,connection_id,connection_name,tool,status,reason,elapsed_ms,row_count,sql FROM audit_events ORDER BY seq DESC LIMIT 5000").fetch_all(&self.pool).await?;
        Ok(rows
            .into_iter()
            .filter_map(|r| {
                DateTime::<Utc>::from_timestamp_millis(r.1).map(|timestamp| AuditEvent {
                    id: r.0,
                    timestamp,
                    connection_id: r.2,
                    connection_name: r.3,
                    tool: r.4,
                    status: parse_status(&r.5),
                    reason: r.6,
                    elapsed_ms: r.7.map(|v| v as u64),
                    row_count: r.8.map(|v| v as usize),
                    sql: r.9,
                })
            })
            .collect())
    }
    pub async fn clear(&self) -> anyhow::Result<()> {
        self.ensure_initialized().await?;
        sqlx::query("DELETE FROM audit_events")
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

fn normalize_limit(max_events: usize) -> usize {
    max_events.clamp(1, MAX_AUDIT_MAX_EVENTS)
}
fn status_text(status: &AuditStatus) -> &'static str {
    match status {
        AuditStatus::Allowed => "allowed",
        AuditStatus::Denied => "denied",
        AuditStatus::Error => "error",
        AuditStatus::Timeout => "timeout",
        AuditStatus::Truncated => "truncated",
    }
}
fn parse_status(status: &str) -> AuditStatus {
    match status {
        "denied" => AuditStatus::Denied,
        "error" => AuditStatus::Error,
        "timeout" => AuditStatus::Timeout,
        "truncated" => AuditStatus::Truncated,
        _ => AuditStatus::Allowed,
    }
}
fn sanitize_reason(reason: &str) -> String {
    reason.chars().take(500).collect()
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
    use super::*;

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
        assert_eq!(
            AuditSql::new(&DbKind::Postgres, "SELECT 'fake-secret' FROM").0,
            REDACTED_UNPARSEABLE_SQL
        );
        assert_eq!(AuditSql::redacted().0, REDACTED_UNKNOWN_DIALECT_SQL);
    }

    #[tokio::test]
    async fn persists_only_sanitized_sql_in_sqlite() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let logger = AuditLogger::for_test(directory.path().join("audit.db"));
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
        let events = logger.list().await.expect("audit events load");
        assert_eq!(events.len(), 5);
        assert!(events.iter().all(|event| !event
            .sql
            .as_deref()
            .unwrap_or_default()
            .contains("fake-secret")));
    }
}
