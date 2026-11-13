use std::ops::ControlFlow;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlparser::ast::{Value, VisitMut, VisitorMut};
use sqlparser::dialect::{Dialect, GenericDialect, MySqlDialect, PostgreSqlDialect, SQLiteDialect};
use sqlparser::parser::Parser;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use sqlx::{Sqlite, Transaction};
use tauri::{AppHandle, Manager};
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

use crate::config::DbKind;

/// Maximum number of events returned to the frontend in a single snapshot.
pub const MAX_AUDIT_LIST_EVENTS: usize = 5000;
/// User-selectable audit log retention windows, in days.
pub const AUDIT_RETENTION_DAY_OPTIONS: [u32; 4] = [3, 7, 15, 30];
pub const DEFAULT_AUDIT_RETENTION_DAYS: u32 = 7;
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
    #[serde(default)]
    pub token_id: Option<String>,
    #[serde(default = "legacy_access_source")]
    pub access_source: String,
    #[serde(default)]
    pub token_name: Option<String>,
    #[serde(default)]
    pub token_deleted: bool,
    #[serde(default)]
    pub token_enabled: bool,
    #[serde(default)]
    pub request_id: Option<String>,
    #[serde(default)]
    pub operation: Option<String>,
    #[serde(default)]
    pub schema: Option<String>,
    #[serde(default)]
    pub table: Option<String>,
    #[serde(default)]
    pub reason_code: Option<String>,
    #[serde(default)]
    pub write_status: Option<String>,
    #[serde(default)]
    pub revision: Option<u64>,
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub stage: Option<String>,
    #[serde(default)]
    pub outcome_unknown: Option<bool>,
}

/// Audit projection only: never persist SQL, request values, or idempotency keys.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct WriteAuditMetadata {
    request_id: Option<String>,
    operation: Option<String>,
    schema: Option<String>,
    table: Option<String>,
    reason_code: Option<String>,
    write_status: Option<String>,
    revision: Option<u64>,
    updated_at: Option<DateTime<Utc>>,
    stage: Option<String>,
    outcome_unknown: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct AuditActor {
    pub token_id: Option<String>,
    pub authentication_fingerprint: Option<String>,
    pub source: &'static str,
    pub denied_connections: Vec<String>,
    pub denied_tools: Vec<String>,
}

impl AuditActor {
    pub fn token_with_permissions(
        token_id: String,
        denied_connections: Vec<String>,
        denied_tools: Vec<String>,
    ) -> Self {
        Self {
            token_id: Some(token_id),
            authentication_fingerprint: None,
            source: "token",
            denied_connections,
            denied_tools,
        }
    }
    pub fn unauthenticated() -> Self {
        Self {
            token_id: None,
            authentication_fingerprint: None,
            source: "unauthenticated",
            denied_connections: Vec::new(),
            denied_tools: Vec::new(),
        }
    }
    pub fn system() -> Self {
        Self {
            token_id: None,
            authentication_fingerprint: None,
            source: "system",
            denied_connections: Vec::new(),
            denied_tools: Vec::new(),
        }
    }

    /// System actor attributed to a specific token, used for events the backend
    /// generates about that token (e.g. the auto circuit breaker).
    pub fn system_for_token(token_id: String) -> Self {
        Self {
            token_id: Some(token_id),
            authentication_fingerprint: None,
            source: "system",
            denied_connections: Vec::new(),
            denied_tools: Vec::new(),
        }
    }

    pub fn allows_tool(&self, tool: &str) -> bool {
        !self.denied_tools.iter().any(|denied| denied == tool)
    }

    pub fn allows_connection(&self, connection_id: &str) -> bool {
        !self
            .denied_connections
            .iter()
            .any(|denied| denied == connection_id)
    }
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
    pub fn new(app: &AppHandle) -> anyhow::Result<Self> {
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

    pub async fn initialize(&self, retention_days: u32) -> anyhow::Result<()> {
        if let Err(error) = self.prepare_schema().await {
            return self.fail(error).await;
        }
        self.initialized.store(true, Ordering::Release);
        if self.legacy_path.exists() {
            self.migrate(retention_days).await
        } else {
            *self.migration.write().await = AuditMigrationState::Ready;
            Ok(())
        }
    }

    async fn prepare_schema(&self) -> anyhow::Result<()> {
        sqlx::query("PRAGMA journal_mode=WAL")
            .execute(&self.pool)
            .await?;
        sqlx::query("CREATE TABLE IF NOT EXISTS audit_events (seq INTEGER PRIMARY KEY AUTOINCREMENT, event_id TEXT NOT NULL UNIQUE, timestamp_ms INTEGER NOT NULL, connection_id TEXT, connection_name TEXT, tool TEXT NOT NULL, status TEXT NOT NULL, reason TEXT, elapsed_ms INTEGER, row_count INTEGER, sql TEXT, token_id TEXT, access_source TEXT NOT NULL DEFAULT 'legacy', write_metadata TEXT)").execute(&self.pool).await?;
        let columns =
            sqlx::query_scalar::<_, String>("SELECT name FROM pragma_table_info('audit_events')")
                .fetch_all(&self.pool)
                .await?;
        if !columns.iter().any(|column| column == "token_id") {
            sqlx::query("ALTER TABLE audit_events ADD COLUMN token_id TEXT")
                .execute(&self.pool)
                .await?;
        }
        if !columns.iter().any(|column| column == "access_source") {
            sqlx::query(
                "ALTER TABLE audit_events ADD COLUMN access_source TEXT NOT NULL DEFAULT 'legacy'",
            )
            .execute(&self.pool)
            .await?;
        }
        if !columns.iter().any(|column| column == "write_metadata") {
            sqlx::query("ALTER TABLE audit_events ADD COLUMN write_metadata TEXT")
                .execute(&self.pool)
                .await?;
        }
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS audit_meta (key TEXT PRIMARY KEY, value INTEGER NOT NULL)",
        )
        .execute(&self.pool)
        .await?;
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
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_audit_events_token ON audit_events(token_id, seq DESC)",
        )
        .execute(&self.pool)
        .await?;
        sqlx::query("PRAGMA user_version = 2")
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

    pub async fn migrate(&self, retention_days: u32) -> anyhow::Result<()> {
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
        if let Err(error) = trim_transaction(&mut tx, retention_days).await {
            return self.fail(error).await;
        }
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
        crate::debug_log::info(
            "audit",
            format_args!("audit migration completed (events={total})"),
        );
        *self.migration.write().await = AuditMigrationState::Ready;
        Ok(())
    }

    async fn fail<T: std::fmt::Display>(&self, error: T) -> anyhow::Result<()> {
        let reason = sanitize_reason(&error.to_string());
        // The audit store being unusable is exactly the kind of failure that
        // has no other witness, so it is recorded here rather than only
        // surfacing through the migration state.
        crate::debug_log::error("audit", format_args!("audit migration failed: {reason}"));
        *self.migration.write().await = AuditMigrationState::Failed {
            reason: reason.clone(),
        };
        Err(anyhow::anyhow!(reason))
    }

    pub async fn retry(&self, retention_days: u32) -> anyhow::Result<()> {
        *self.migration.write().await = AuditMigrationState::Migrating {
            phase: AuditMigrationPhase::ReadingLegacyFile,
            processed: 0,
            total: 0,
        };
        self.migrate(retention_days).await
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
            clear_transaction(&mut transaction).await?;
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
        retention_days: u32,
    ) -> anyhow::Result<()> {
        self.record_with_actor(
            AuditActor::system(),
            connection_id,
            connection_name,
            tool,
            status,
            reason,
            elapsed_ms,
            row_count,
            sql,
            retention_days,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn record_with_actor(
        &self,
        actor: AuditActor,
        connection_id: Option<String>,
        connection_name: Option<String>,
        tool: impl Into<String>,
        status: AuditStatus,
        reason: Option<String>,
        elapsed_ms: Option<u64>,
        row_count: Option<usize>,
        sql: Option<AuditSql>,
        retention_days: u32,
    ) -> anyhow::Result<()> {
        let result = async {
            if !self.is_ready().await {
                return Err(anyhow::anyhow!("audit migration is not ready"));
            }
            let _guard = self.persist_lock.lock().await;
            self.ensure_initialized().await?;
            let mut tx = self.pool.begin().await?;
            sqlx::query("INSERT INTO audit_events (event_id,timestamp_ms,connection_id,connection_name,tool,status,reason,elapsed_ms,row_count,sql,token_id,access_source) VALUES (?,?,?,?,?,?,?,?,?,?,?,?)")
                .bind(Uuid::new_v4().to_string()).bind(Utc::now().timestamp_millis()).bind(connection_id).bind(connection_name).bind(tool.into())
                .bind(status_text(&status)).bind(reason).bind(elapsed_ms.map(|v| v as i64)).bind(row_count.map(|v| v as i64)).bind(sql.map(|v| truncate_sql(v.0))).bind(actor.token_id).bind(actor.source).execute(&mut *tx).await?;
            trim_transaction(&mut tx, retention_days).await?;
            tx.commit().await?;
            Ok::<(), anyhow::Error>(())
        }.await;
        result.map_err(|error| {
            // Callers frequently discard this result, so a dropped audit event
            // would otherwise leave no trace at all.
            crate::debug_log::error(
                "audit",
                format_args!("audit event could not be recorded: {error}"),
            );
            anyhow::anyhow!("audit storage unavailable: {error}")
        })
    }

    /// Projects one durable request into the ordinary audit list. Replaying a
    /// snapshot never advances an older revision or recreates cleared history.
    pub async fn sync_write_status(
        &self,
        status: &crate::write_operations::WriteStatus,
        connection_name: Option<String>,
        retention_days: u32,
    ) -> anyhow::Result<()> {
        let result = async {
            if !self.is_ready().await {
                return Err(anyhow::anyhow!("audit migration is not ready"));
            }
            let _guard = self.persist_lock.lock().await;
            self.ensure_initialized().await?;
            let mut tx = self.pool.begin().await?;
            let created_ms = status.created_at.timestamp_millis();
            let excluded = sqlx::query_scalar::<_, i64>(
                "SELECT EXISTS(SELECT 1 FROM audit_meta WHERE (key='clear_watermark_ms' AND value>=?) OR (key='retention_cutoff_ms' AND value>?))",
            )
            .bind(created_ms)
            .bind(created_ms)
            .fetch_one(&mut *tx)
            .await?;
            if excluded != 0 {
                tx.commit().await?;
                return Ok(());
            }
            if created_ms < retention_cutoff_ms(retention_days) {
                trim_transaction(&mut tx, retention_days).await?;
                tx.commit().await?;
                return Ok(());
            }
            let event_id = format!("write:{}", status.request_id);
            let existing = sqlx::query_scalar::<_, Option<String>>(
                "SELECT write_metadata FROM audit_events WHERE event_id=?",
            )
            .bind(&event_id)
            .fetch_optional(&mut *tx)
            .await?;
            if let Some(Some(metadata)) = existing {
                let stored: WriteAuditMetadata = serde_json::from_str(&metadata)?;
                if stored.revision.is_some_and(|revision| revision >= status.revision) {
                    tx.commit().await?;
                    return Ok(());
                }
            }
            let metadata = WriteAuditMetadata {
                request_id: Some(status.request_id.clone()),
                operation: Some(status.operation.as_str().to_string()),
                schema: Some(status.schema.clone()),
                table: Some(status.table.clone()),
                reason_code: status.error_code.as_deref().map(safe_reason_code),
                write_status: Some(write_status_text(status.status).to_string()),
                revision: Some(status.revision),
                updated_at: Some(status.updated_at),
                stage: write_stage(status.status).map(str::to_string),
                outcome_unknown: Some(status.outcome_unknown),
            };
            // WriteStatus carries only allowlisted error messages; user request
            // reasons, parameter values and SQL must never enter this projection.
            sqlx::query("INSERT INTO audit_events (event_id,timestamp_ms,connection_id,connection_name,tool,status,reason,elapsed_ms,row_count,sql,token_id,access_source,write_metadata) VALUES (?,?,?,?,?,?,?,NULL,?,NULL,?,'token',?) ON CONFLICT(event_id) DO UPDATE SET connection_name=COALESCE(excluded.connection_name,audit_events.connection_name),status=excluded.status,reason=excluded.reason,row_count=excluded.row_count,sql=NULL,write_metadata=excluded.write_metadata")
                .bind(event_id)
                .bind(created_ms)
                .bind(&status.connection_id)
                .bind(connection_name)
                .bind("datanexa_submit_write")
                .bind(status_text(&write_audit_status(status)))
                .bind(status.error_message.as_deref().map(sanitize_reason))
                .bind(status.affected_rows.and_then(|rows| i64::try_from(rows).ok()))
                .bind(&status.token_id)
                .bind(serde_json::to_string(&metadata)?)
                .execute(&mut *tx)
                .await?;
            trim_transaction(&mut tx, retention_days).await?;
            tx.commit().await?;
            Ok::<(), anyhow::Error>(())
        }
        .await;
        result.map_err(|_| write_audit_storage_error())
    }

    /// Records a rejected submission that has no durable request ID. Callers
    /// must supply a static, safe error message, never parser or database output.
    #[allow(clippy::too_many_arguments)]
    pub async fn record_write_rejection(
        &self,
        actor: AuditActor,
        connection_id: Option<String>,
        operation: Option<String>,
        schema: Option<String>,
        table: Option<String>,
        code: &str,
        message: &str,
        retention_days: u32,
    ) -> anyhow::Result<()> {
        let result = async {
            if !self.is_ready().await {
                return Err(anyhow::anyhow!("audit migration is not ready"));
            }
            let _guard = self.persist_lock.lock().await;
            self.ensure_initialized().await?;
            let now = Utc::now();
            let metadata = WriteAuditMetadata {
                operation: operation.filter(|value| matches!(value.as_str(), "insert" | "update" | "delete")),
                schema: schema.filter(|value| safe_write_identifier(value)),
                table: table.filter(|value| safe_write_identifier(value)),
                reason_code: Some(safe_reason_code(code)),
                write_status: Some("rejected".to_string()),
                updated_at: Some(now),
                stage: Some("submission".to_string()),
                outcome_unknown: Some(false),
                ..WriteAuditMetadata::default()
            };
            let mut tx = self.pool.begin().await?;
            sqlx::query("INSERT INTO audit_events (event_id,timestamp_ms,connection_id,tool,status,reason,sql,token_id,access_source,write_metadata) VALUES (?,?,?,'datanexa_submit_write','denied',?,NULL,?,?,?)")
                .bind(format!("write-attempt:{}", Uuid::new_v4()))
                .bind(now.timestamp_millis())
                .bind(connection_id)
                .bind(sanitize_reason(message))
                .bind(actor.token_id)
                .bind(actor.source)
                .bind(serde_json::to_string(&metadata)?)
                .execute(&mut *tx)
                .await?;
            trim_transaction(&mut tx, retention_days).await?;
            tx.commit().await?;
            Ok::<(), anyhow::Error>(())
        }
        .await;
        result.map_err(|_| write_audit_storage_error())
    }

    pub async fn trim(&self, retention_days: u32) -> anyhow::Result<()> {
        if !self.is_ready().await {
            return Ok(());
        }
        let _guard = self.persist_lock.lock().await;
        self.ensure_initialized().await?;
        let mut tx = self.pool.begin().await?;
        trim_transaction(&mut tx, retention_days).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Counts denied events recorded for a token since the cutoff. Events at or
    /// before the latest breaker marker for the token are excluded so a manually
    /// re-enabled token starts from a fresh count, and the breaker's own marker
    /// events never count toward the threshold.
    pub async fn count_token_denials(
        &self,
        token_id: &str,
        breaker_tool: &str,
        cutoff_ms: i64,
    ) -> anyhow::Result<u64> {
        if !self.is_ready().await {
            return Ok(0);
        }
        self.ensure_initialized().await?;
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM audit_events WHERE token_id=? AND status='denied' AND tool!=? AND timestamp_ms>=? AND timestamp_ms > COALESCE((SELECT MAX(timestamp_ms) FROM audit_events WHERE token_id=? AND tool=?), 0)",
        )
        .bind(token_id)
        .bind(breaker_tool)
        .bind(cutoff_ms)
        .bind(token_id)
        .bind(breaker_tool)
        .fetch_one(&self.pool)
        .await?;
        Ok(count.max(0) as u64)
    }
    pub async fn list(&self) -> anyhow::Result<Vec<AuditEvent>> {
        self.ensure_initialized().await?;
        let rows = sqlx::query_as::<_, (String, i64, Option<String>, Option<String>, String, String, Option<String>, Option<i64>, Option<i64>, Option<String>, Option<String>, String, Option<String>)>("SELECT event_id,timestamp_ms,connection_id,connection_name,tool,status,reason,elapsed_ms,row_count,sql,token_id,access_source,write_metadata FROM audit_events ORDER BY timestamp_ms DESC,seq DESC LIMIT ?").bind(MAX_AUDIT_LIST_EVENTS as i64).fetch_all(&self.pool).await?;
        let mut events = Vec::with_capacity(rows.len());
        for row in rows {
            let Some(timestamp) = DateTime::<Utc>::from_timestamp_millis(row.1) else {
                continue;
            };
            let is_write = row.12.is_some();
            let metadata = row
                .12
                .as_deref()
                .map(serde_json::from_str::<WriteAuditMetadata>)
                .transpose()
                .map_err(|_| write_audit_storage_error())?
                .unwrap_or_default();
            events.push(AuditEvent {
                id: row.0,
                timestamp,
                connection_id: row.2,
                connection_name: row.3,
                tool: row.4,
                status: parse_status(&row.5),
                reason: row.6,
                elapsed_ms: row.7.map(|value| value as u64),
                row_count: row.8.map(|value| value as usize),
                sql: if is_write { None } else { row.9 },
                token_id: row.10,
                access_source: row.11,
                token_name: None,
                token_deleted: false,
                token_enabled: false,
                request_id: metadata.request_id,
                operation: metadata.operation,
                schema: metadata.schema,
                table: metadata.table,
                reason_code: metadata.reason_code,
                write_status: metadata.write_status,
                revision: metadata.revision,
                updated_at: metadata.updated_at,
                stage: metadata.stage,
                outcome_unknown: metadata.outcome_unknown,
            });
        }
        Ok(events)
    }
    pub async fn clear(&self) -> anyhow::Result<()> {
        let _guard = self.persist_lock.lock().await;
        self.ensure_initialized().await?;
        let mut tx = self.pool.begin().await?;
        clear_transaction(&mut tx).await?;
        tx.commit().await?;
        Ok(())
    }
}

async fn trim_transaction(
    tx: &mut Transaction<'_, Sqlite>,
    retention_days: u32,
) -> Result<(), sqlx::Error> {
    // The cutoff only advances: expanding retention must not restore history
    // already removed. Write timestamps remain their original creation time.
    sqlx::query("INSERT INTO audit_meta (key,value) VALUES ('retention_cutoff_ms',?) ON CONFLICT(key) DO UPDATE SET value=MAX(audit_meta.value,excluded.value)")
        .bind(retention_cutoff_ms(retention_days))
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM audit_events WHERE timestamp_ms < (SELECT value FROM audit_meta WHERE key='retention_cutoff_ms')")
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn clear_transaction(tx: &mut Transaction<'_, Sqlite>) -> Result<(), sqlx::Error> {
    // Include future-dated events if the system clock moved backwards. Both
    // the watermark and deletion commit together; the ledger is never touched.
    sqlx::query("INSERT INTO audit_meta (key,value) VALUES ('clear_watermark_ms',MAX(?,COALESCE((SELECT MAX(timestamp_ms) FROM audit_events),?))) ON CONFLICT(key) DO UPDATE SET value=MAX(audit_meta.value,excluded.value)")
        .bind(Utc::now().timestamp_millis())
        .bind(Utc::now().timestamp_millis())
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM audit_events")
        .execute(&mut **tx)
        .await?;
    Ok(())
}

fn write_audit_storage_error() -> anyhow::Error {
    // Never log SQL errors or serialized records from this sensitive path.
    crate::debug_log::error("audit", format_args!("write audit storage unavailable"));
    anyhow::anyhow!("write audit storage unavailable")
}

/// Audit metadata only stores identifiers that are printable and bounded, so a
/// hostile or garbled name cannot be used for log injection. Non-ASCII names are
/// legitimate (server metadata may contain them), so they are not rejected.
fn safe_write_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.chars().count() <= 128
        && !value.chars().any(|character| character.is_control())
}

fn safe_reason_code(code: &str) -> String {
    if !code.is_empty()
        && code.len() <= 64
        && code
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        code.to_string()
    } else {
        "write_rejected".to_string()
    }
}

fn write_status_text(status: crate::write_operations::WriteOperationStatus) -> &'static str {
    use crate::write_operations::WriteOperationStatus;
    match status {
        WriteOperationStatus::Pending => "pending",
        WriteOperationStatus::Rejected => "rejected",
        WriteOperationStatus::Expired => "expired",
        WriteOperationStatus::Cancelled => "cancelled",
        WriteOperationStatus::Executing => "executing",
        WriteOperationStatus::Committing => "committing",
        WriteOperationStatus::Committed => "committed",
        WriteOperationStatus::RolledBack => "rolled_back",
        WriteOperationStatus::Failed => "failed",
        WriteOperationStatus::Unknown => "unknown",
    }
}

fn write_stage(status: crate::write_operations::WriteOperationStatus) -> Option<&'static str> {
    use crate::write_operations::WriteOperationStatus;
    match status {
        WriteOperationStatus::Pending => Some("preparation_or_approval"),
        WriteOperationStatus::Rejected | WriteOperationStatus::Expired => Some("approval"),
        WriteOperationStatus::Executing => Some("execution"),
        WriteOperationStatus::Committing | WriteOperationStatus::Committed => Some("commit"),
        WriteOperationStatus::RolledBack => Some("rollback"),
        WriteOperationStatus::Cancelled
        | WriteOperationStatus::Failed
        | WriteOperationStatus::Unknown => None,
    }
}

fn write_audit_status(status: &crate::write_operations::WriteStatus) -> AuditStatus {
    use crate::write_operations::WriteOperationStatus;
    if status.outcome_unknown {
        return AuditStatus::Error;
    }
    match status.status {
        WriteOperationStatus::Committed => AuditStatus::Allowed,
        WriteOperationStatus::Rejected | WriteOperationStatus::Cancelled => AuditStatus::Denied,
        WriteOperationStatus::Expired => AuditStatus::Timeout,
        WriteOperationStatus::Failed
        | WriteOperationStatus::RolledBack
        | WriteOperationStatus::Unknown => AuditStatus::Error,
        // "Allowed" remains the authorization classification, not a success
        // claim. Consumers must prefer write_status for in-flight operations.
        WriteOperationStatus::Pending
        | WriteOperationStatus::Executing
        | WriteOperationStatus::Committing => AuditStatus::Allowed,
    }
}

fn legacy_access_source() -> String {
    "legacy".to_string()
}

/// Maps a configured retention window onto the supported day options,
/// falling back to the default when the value is not user-selectable.
pub fn normalize_retention_days(days: u32) -> u32 {
    if AUDIT_RETENTION_DAY_OPTIONS.contains(&days) {
        days
    } else {
        DEFAULT_AUDIT_RETENTION_DAYS
    }
}

fn retention_cutoff_ms(retention_days: u32) -> i64 {
    Utc::now().timestamp_millis() - i64::from(normalize_retention_days(retention_days)) * 86_400_000
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
    static GENERIC: GenericDialect = GenericDialect {};
    match kind {
        DbKind::Sqlite => &SQLITE,
        DbKind::Mysql => &MYSQL,
        DbKind::Postgres => &POSTGRES,
        DbKind::Jdbc => &GENERIC,
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
                    DEFAULT_AUDIT_RETENTION_DAYS,
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
