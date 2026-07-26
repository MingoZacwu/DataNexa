use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use futures_util::{Stream, StreamExt};
use regex::Regex;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sqlx::mysql::{MySqlConnectOptions, MySqlPoolOptions, MySqlRow, MySqlSslMode};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions, PgRow, PgSslMode};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions, SqliteRow};
use sqlx::{Column, Executor, MySqlPool, PgPool, Row, SqlitePool, TypeInfo, ValueRef};
use tokio::sync::{Mutex, RwLock};
use tokio::time::timeout;
use zeroize::Zeroizing;

use crate::config::{default_port, ConnectionConfig, DbKind};
use crate::i18n::BackendText;
use crate::policy::{PolicyCheckResult, PolicyEngine};
use crate::vault::CredentialVault;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableInfo {
    pub schema: Option<String>,
    pub name: String,
    pub table_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnInfo {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub primary_key: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Map<String, Value>>,
    pub row_count: usize,
    pub truncated: bool,
    pub truncation_reason: Option<String>,
    pub returned_bytes: usize,
    pub elapsed_ms: u64,
    pub rewritten_sql: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionDiagnostics {
    pub id: String,
    pub name: String,
    pub database_type: String,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub database: String,
    pub username: Option<String>,
    pub ssl_mode: Option<String>,
    pub credential_ref_present: bool,
    pub credential_state: String,
    pub query_timeout_ms: u64,
    pub max_connections: u32,
    pub hint: Option<String>,
}

#[derive(Clone)]
enum ManagedPool {
    Sqlite(SqlitePool),
    Mysql(MySqlPool),
    Postgres(PgPool),
}

#[derive(Clone)]
struct PoolEntry {
    generation: u64,
    signature: PoolSignature,
    pool: ManagedPool,
}

#[derive(Clone, PartialEq, Eq)]
struct PoolSignature {
    kind: u8,
    database: String,
    host: Option<String>,
    port: Option<u16>,
    username: Option<String>,
    credential_ref: Option<String>,
    ssl_mode: Option<String>,
    max_connections: u32,
}

enum PublishPool {
    Published,
    Existing(ManagedPool),
    Invalidated(ManagedPool),
}

#[derive(Default)]
struct PoolState {
    entries: HashMap<String, PoolEntry>,
    generations: HashMap<String, u64>,
}

#[derive(Default)]
pub struct DatabaseManager {
    pools: RwLock<PoolState>,
    creation_locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
}

impl DatabaseManager {
    pub async fn close(&self, connection_id: &str) {
        let pool = {
            let mut state = self.pools.write().await;
            let generation = state
                .generations
                .entry(connection_id.to_string())
                .or_default();
            *generation = generation.wrapping_add(1);
            state.entries.remove(connection_id).map(|entry| entry.pool)
        };
        if let Some(pool) = pool {
            close_pool(pool).await;
        }
    }

    #[cfg(test)]
    pub(crate) async fn generation_for_test(&self, connection_id: &str) -> u64 {
        self.pools
            .read()
            .await
            .generations
            .get(connection_id)
            .copied()
            .unwrap_or(0)
    }

    pub async fn test_connection(
        &self,
        config: &ConnectionConfig,
        vault: &CredentialVault,
        text: &BackendText,
    ) -> anyhow::Result<Duration> {
        let started = Instant::now();
        match self.pool(config, vault, text).await? {
            ManagedPool::Sqlite(pool) => {
                timeout(
                    query_timeout(config),
                    sqlx::query("SELECT 1").execute(&pool),
                )
                .await??;
            }
            ManagedPool::Mysql(pool) => {
                timeout(
                    query_timeout(config),
                    sqlx::query("SELECT 1").execute(&pool),
                )
                .await??;
            }
            ManagedPool::Postgres(pool) => {
                timeout(
                    query_timeout(config),
                    sqlx::query("SELECT 1").execute(&pool),
                )
                .await??;
            }
        }
        Ok(started.elapsed())
    }

    pub async fn test_connection_once(
        &self,
        config: &ConnectionConfig,
        vault: &CredentialVault,
        password_override: Option<&str>,
        text: &BackendText,
    ) -> anyhow::Result<Duration> {
        let started = Instant::now();
        let pool = connect_pool_with_password(config, vault, password_override, text).await?;
        let result = probe_pool(config, &pool).await;
        close_pool(pool).await;
        result?;
        Ok(started.elapsed())
    }

    pub fn diagnostics(
        &self,
        config: &ConnectionConfig,
        vault: &CredentialVault,
        text: &BackendText,
    ) -> ConnectionDiagnostics {
        connection_diagnostics(config, vault, text)
    }

    pub async fn list_schema(
        &self,
        config: &ConnectionConfig,
        vault: &CredentialVault,
        text: &BackendText,
    ) -> anyhow::Result<Vec<TableInfo>> {
        match self.pool(config, vault, text).await? {
            ManagedPool::Sqlite(pool) => {
                let rows = timeout(
                    query_timeout(config),
                    collect_metadata_rows(
                        sqlx::query("SELECT NULL AS schema_name, name, type FROM sqlite_master WHERE type IN ('table', 'view') AND name NOT LIKE 'sqlite_%' ORDER BY name")
                            .fetch(&pool),
                    ),
                )
                .await??;
                Ok(rows
                    .iter()
                    .map(|row| TableInfo {
                        schema: None,
                        name: row.try_get::<String, _>("name").unwrap_or_default(),
                        table_type: row
                            .try_get::<String, _>("type")
                            .unwrap_or_else(|_| "table".to_string()),
                    })
                    .collect())
            }
            ManagedPool::Mysql(pool) => {
                let rows = timeout(
                    query_timeout(config),
                    collect_metadata_rows(
                        sqlx::query("SELECT TABLE_SCHEMA, TABLE_NAME, TABLE_TYPE FROM information_schema.TABLES WHERE TABLE_SCHEMA = DATABASE() ORDER BY TABLE_NAME")
                            .fetch(&pool),
                    ),
                )
                .await??;
                Ok(rows
                    .iter()
                    .map(|row| TableInfo {
                        schema: row.try_get::<String, _>("TABLE_SCHEMA").ok(),
                        name: row.try_get::<String, _>("TABLE_NAME").unwrap_or_default(),
                        table_type: row
                            .try_get::<String, _>("TABLE_TYPE")
                            .unwrap_or_else(|_| "BASE TABLE".to_string()),
                    })
                    .collect())
            }
            ManagedPool::Postgres(pool) => {
                let rows = timeout(
                    query_timeout(config),
                    collect_metadata_rows(
                        sqlx::query("SELECT table_schema, table_name, table_type FROM information_schema.tables WHERE table_schema NOT IN ('pg_catalog', 'information_schema') ORDER BY table_schema, table_name")
                            .fetch(&pool),
                    ),
                )
                .await??;
                Ok(rows
                    .iter()
                    .map(|row| TableInfo {
                        schema: row.try_get::<String, _>("table_schema").ok(),
                        name: row.try_get::<String, _>("table_name").unwrap_or_default(),
                        table_type: row
                            .try_get::<String, _>("table_type")
                            .unwrap_or_else(|_| "BASE TABLE".to_string()),
                    })
                    .collect())
            }
        }
    }

    pub async fn describe_table(
        &self,
        config: &ConnectionConfig,
        vault: &CredentialVault,
        schema: Option<&str>,
        table: &str,
        text: &BackendText,
    ) -> anyhow::Result<Vec<ColumnInfo>> {
        validate_identifier(table)?;
        if let Some(schema) = schema {
            validate_identifier(schema)?;
        }

        match self.pool(config, vault, text).await? {
            ManagedPool::Sqlite(pool) => {
                let sql = format!(
                    "PRAGMA table_info({})",
                    quote_identifier(&config.kind, table)?
                );
                let rows = timeout(
                    query_timeout(config),
                    collect_metadata_rows(sqlx::query(&sql).fetch(&pool)),
                )
                .await??;
                Ok(rows
                    .iter()
                    .map(|row| ColumnInfo {
                        name: row.try_get::<String, _>("name").unwrap_or_default(),
                        data_type: row.try_get::<String, _>("type").unwrap_or_default(),
                        nullable: row.try_get::<i64, _>("notnull").unwrap_or(0) == 0,
                        primary_key: row.try_get::<i64, _>("pk").unwrap_or(0) > 0,
                    })
                    .collect())
            }
            ManagedPool::Mysql(pool) => {
                let rows = timeout(
                    query_timeout(config),
                    collect_metadata_rows(
                        sqlx::query("SELECT COLUMN_NAME, DATA_TYPE, IS_NULLABLE, COLUMN_KEY FROM information_schema.COLUMNS WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = ? ORDER BY ORDINAL_POSITION")
                            .bind(table)
                            .fetch(&pool),
                    ),
                )
                .await??;
                Ok(rows
                    .iter()
                    .map(|row| ColumnInfo {
                        name: row.try_get::<String, _>("COLUMN_NAME").unwrap_or_default(),
                        data_type: row.try_get::<String, _>("DATA_TYPE").unwrap_or_default(),
                        nullable: row.try_get::<String, _>("IS_NULLABLE").unwrap_or_default()
                            == "YES",
                        primary_key: row.try_get::<String, _>("COLUMN_KEY").unwrap_or_default()
                            == "PRI",
                    })
                    .collect())
            }
            ManagedPool::Postgres(pool) => {
                let schema = schema.unwrap_or("public");
                let rows = timeout(
                    query_timeout(config),
                    collect_metadata_rows(
                        sqlx::query(
                            "SELECT c.column_name, c.data_type, c.is_nullable,
                                EXISTS (
                                    SELECT 1
                                    FROM information_schema.table_constraints tc
                                    JOIN information_schema.key_column_usage kcu
                                      ON kcu.constraint_schema = tc.constraint_schema
                                     AND kcu.constraint_name = tc.constraint_name
                                    WHERE tc.constraint_type = 'PRIMARY KEY'
                                      AND kcu.table_schema = c.table_schema
                                      AND kcu.table_name = c.table_name
                                      AND kcu.column_name = c.column_name
                                ) AS primary_key
                         FROM information_schema.columns c
                         WHERE c.table_schema = $1 AND c.table_name = $2
                         ORDER BY c.ordinal_position",
                        )
                        .bind(schema)
                        .bind(table)
                        .fetch(&pool),
                    ),
                )
                .await??;
                Ok(rows
                    .iter()
                    .map(|row| ColumnInfo {
                        name: row.try_get::<String, _>("column_name").unwrap_or_default(),
                        data_type: row.try_get::<String, _>("data_type").unwrap_or_default(),
                        nullable: row.try_get::<String, _>("is_nullable").unwrap_or_default()
                            == "YES",
                        primary_key: row.try_get::<bool, _>("primary_key").unwrap_or(false),
                    })
                    .collect())
            }
        }
    }

    pub async fn sample_rows(
        &self,
        config: &ConnectionConfig,
        vault: &CredentialVault,
        schema: Option<&str>,
        table: &str,
        limit: Option<u32>,
        text: &BackendText,
    ) -> anyhow::Result<QueryResult> {
        validate_identifier(table)?;
        if let Some(schema) = schema {
            validate_identifier(schema)?;
        }

        let table_sql = match (&config.kind, schema) {
            (DbKind::Postgres, Some(schema)) => format!(
                "{}.{}",
                quote_identifier(&config.kind, schema)?,
                quote_identifier(&config.kind, table)?
            ),
            _ => quote_identifier(&config.kind, table)?,
        };
        let max_rows = limit.unwrap_or(config.max_rows).min(config.max_rows).max(1);
        let fetch_limit = max_rows
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("max_rows is too large"))?;
        let sql = format!("SELECT * FROM {table_sql} LIMIT {fetch_limit}");
        self.execute_internal(config, vault, &sql, sql.clone(), max_rows, text)
            .await
    }

    pub async fn execute_readonly(
        &self,
        config: &ConnectionConfig,
        vault: &CredentialVault,
        sql: &str,
        text: &BackendText,
    ) -> anyhow::Result<(PolicyCheckResult, Option<QueryResult>)> {
        let policy = PolicyEngine::check_with_text(&config.kind, sql, config.max_rows, text);
        if !policy.allowed {
            return Ok((policy, None));
        }

        let rewritten = policy
            .rewritten_sql
            .clone()
            .ok_or_else(|| anyhow::anyhow!("policy accepted SQL without a rewritten statement"))?;
        let result = self
            .execute_internal(
                config,
                vault,
                &rewritten,
                rewritten.clone(),
                config.max_rows.max(1),
                text,
            )
            .await?;
        Ok((policy, Some(result)))
    }

    pub async fn explain_sql(
        &self,
        config: &ConnectionConfig,
        vault: &CredentialVault,
        sql: &str,
        text: &BackendText,
    ) -> anyhow::Result<(PolicyCheckResult, Option<QueryResult>)> {
        let policy = PolicyEngine::check_explain_target_with_text(&config.kind, sql, text);
        if !policy.allowed {
            return Ok((policy, None));
        }

        let rewritten = policy
            .rewritten_sql
            .clone()
            .ok_or_else(|| anyhow::anyhow!("policy accepted SQL without a rewritten statement"))?;
        let explain_sql = if is_explain_statement(&rewritten) {
            rewritten
        } else {
            format!("EXPLAIN {rewritten}")
        };
        let result = self
            .execute_internal(
                config,
                vault,
                &explain_sql,
                explain_sql.clone(),
                config.max_rows.max(1),
                text,
            )
            .await?;
        Ok((policy, Some(result)))
    }

    async fn execute_internal(
        &self,
        config: &ConnectionConfig,
        vault: &CredentialVault,
        sql: &str,
        rewritten_sql: String,
        max_rows: u32,
        text: &BackendText,
    ) -> anyhow::Result<QueryResult> {
        let started = Instant::now();
        match self.pool(config, vault, text).await? {
            ManagedPool::Sqlite(pool) => {
                let rows = timeout(
                    query_timeout(config),
                    collect_query_rows(
                        sqlx::query(sql).fetch(&pool),
                        max_rows,
                        config.max_result_bytes,
                        |row| unique_column_names(row.columns().iter().map(|column| column.name())),
                        sqlite_raw_row_bytes,
                        |row, columns| {
                            columns
                                .iter()
                                .enumerate()
                                .map(|(index, column)| (column.clone(), sqlite_cell(row, index)))
                                .collect()
                        },
                    ),
                )
                .await??;
                Ok(result(rows, started.elapsed(), rewritten_sql))
            }
            ManagedPool::Mysql(pool) => {
                let rows = timeout(
                    query_timeout(config),
                    collect_query_rows(
                        sqlx::query(sql).fetch(&pool),
                        max_rows,
                        config.max_result_bytes,
                        |row| unique_column_names(row.columns().iter().map(|column| column.name())),
                        mysql_raw_row_bytes,
                        |row, columns| {
                            columns
                                .iter()
                                .enumerate()
                                .map(|(index, column)| (column.clone(), mysql_cell(row, index)))
                                .collect()
                        },
                    ),
                )
                .await??;
                Ok(result(rows, started.elapsed(), rewritten_sql))
            }
            ManagedPool::Postgres(pool) => {
                let rows = timeout(
                    query_timeout(config),
                    collect_query_rows(
                        sqlx::query(sql).fetch(&pool),
                        max_rows,
                        config.max_result_bytes,
                        |row| unique_column_names(row.columns().iter().map(|column| column.name())),
                        pg_raw_row_bytes,
                        |row, columns| {
                            columns
                                .iter()
                                .enumerate()
                                .map(|(index, column)| (column.clone(), pg_cell(row, index)))
                                .collect()
                        },
                    ),
                )
                .await??;
                Ok(result(rows, started.elapsed(), rewritten_sql))
            }
        }
    }

    async fn pool(
        &self,
        config: &ConnectionConfig,
        vault: &CredentialVault,
        text: &BackendText,
    ) -> anyhow::Result<ManagedPool> {
        let signature = pool_signature(config);
        let creation_lock = {
            let mut locks = self.creation_locks.lock().await;
            locks
                .entry(config.id.clone())
                .or_insert_with(|| Arc::new(Mutex::new(())))
                .clone()
        };
        let _creation_guard = creation_lock.lock().await;
        let (generation, stale_pool) = {
            let mut state = self.pools.write().await;
            let generation = state.generations.get(&config.id).copied().unwrap_or(0);
            if let Some(entry) = state.entries.get(&config.id) {
                if entry.generation == generation && entry.signature == signature {
                    return Ok(entry.pool.clone());
                }
            }
            let stale_pool = state.entries.remove(&config.id).map(|entry| entry.pool);
            (generation, stale_pool)
        };
        if let Some(pool) = stale_pool {
            close_pool(pool).await;
        }

        let pool = connect_pool_with_password(config, vault, None, text).await?;
        let mut state = self.pools.write().await;
        match publish_pool(
            &mut state,
            config.id.clone(),
            generation,
            signature,
            pool.clone(),
        ) {
            PublishPool::Published => Ok(pool),
            PublishPool::Existing(existing) => {
                drop(state);
                close_pool(pool).await;
                Ok(existing)
            }
            PublishPool::Invalidated(invalidated) => {
                drop(state);
                close_pool(invalidated).await;
                Err(anyhow::anyhow!(
                    "connection was invalidated while its pool was being created; retry the request"
                ))
            }
        }
    }
}

fn pool_signature(config: &ConnectionConfig) -> PoolSignature {
    PoolSignature {
        kind: match config.kind {
            DbKind::Sqlite => 0,
            DbKind::Mysql => 1,
            DbKind::Postgres => 2,
        },
        database: config.database.clone(),
        host: config.host.clone(),
        port: config.port,
        username: config.username.clone(),
        credential_ref: config.credential_ref.clone(),
        ssl_mode: config.ssl_mode.clone(),
        max_connections: config.max_connections,
    }
}

fn publish_pool(
    state: &mut PoolState,
    connection_id: String,
    generation: u64,
    signature: PoolSignature,
    pool: ManagedPool,
) -> PublishPool {
    let current_generation = state.generations.get(&connection_id).copied().unwrap_or(0);
    if current_generation != generation {
        return PublishPool::Invalidated(pool);
    }
    if let Some(entry) = state.entries.get(&connection_id) {
        if entry.signature == signature && entry.generation == generation {
            return PublishPool::Existing(entry.pool.clone());
        }
        return PublishPool::Invalidated(pool);
    }
    state.entries.insert(
        connection_id,
        PoolEntry {
            generation,
            signature,
            pool,
        },
    );
    PublishPool::Published
}

async fn connect_pool_with_password(
    config: &ConnectionConfig,
    vault: &CredentialVault,
    password_override: Option<&str>,
    text: &BackendText,
) -> anyhow::Result<ManagedPool> {
    let max_connections = config.max_connections.clamp(1, 3);
    match config.kind {
        DbKind::Sqlite => {
            let options = SqliteConnectOptions::from_str(&config.database)?
                .read_only(true)
                .create_if_missing(false)
                .busy_timeout(Duration::from_secs(5));
            let pool = SqlitePoolOptions::new()
                .max_connections(max_connections)
                .acquire_timeout(Duration::from_millis(config.query_timeout_ms))
                .connect_with(options)
                .await?;
            Ok(ManagedPool::Sqlite(pool))
        }
        DbKind::Mysql => {
            let password = resolve_password(config, vault, password_override)?;
            let host = required(config.host.as_deref(), "MySQL host")?;
            let options = MySqlConnectOptions::new()
                .host(host)
                .port(
                    config
                        .port
                        .or_else(|| default_port(&config.kind))
                        .unwrap_or(3306),
                )
                .database(&config.database)
                .username(config.username.as_deref().unwrap_or(""))
                .password(&password)
                .ssl_mode(mysql_ssl_mode(config)?);
            let pool = MySqlPoolOptions::new()
                .max_connections(max_connections)
                .acquire_timeout(Duration::from_millis(config.query_timeout_ms))
                .after_connect(|connection, _meta| {
                    Box::pin(async move {
                        connection
                            .execute("SET SESSION TRANSACTION READ ONLY")
                            .await?;
                        Ok(())
                    })
                })
                .connect_with(options)
                .await
                .map_err(|error| annotate_connect_error(&config.kind, error, text))?;
            Ok(ManagedPool::Mysql(pool))
        }
        DbKind::Postgres => {
            let password = resolve_password(config, vault, password_override)?;
            let host = required(config.host.as_deref(), "PostgreSQL host")?;
            let ssl_mode = pg_ssl_mode(config)?;
            let options = PgConnectOptions::new()
                .host(host)
                .port(
                    config
                        .port
                        .or_else(|| default_port(&config.kind))
                        .unwrap_or(5432),
                )
                .database(&config.database)
                .username(config.username.as_deref().unwrap_or(""))
                .password(&password)
                .ssl_mode(ssl_mode)
                .application_name("DataNexa")
                .options([("default_transaction_read_only", "on")]);
            let pool = PgPoolOptions::new()
                .max_connections(max_connections)
                .acquire_timeout(Duration::from_millis(config.query_timeout_ms))
                .connect_with(options)
                .await
                .map_err(|error| annotate_connect_error(&config.kind, error, text))?;
            Ok(ManagedPool::Postgres(pool))
        }
    }
}

async fn close_pool(pool: ManagedPool) {
    match pool {
        ManagedPool::Sqlite(pool) => pool.close().await,
        ManagedPool::Mysql(pool) => pool.close().await,
        ManagedPool::Postgres(pool) => pool.close().await,
    }
}

async fn probe_pool(config: &ConnectionConfig, pool: &ManagedPool) -> anyhow::Result<()> {
    match pool {
        ManagedPool::Sqlite(pool) => {
            timeout(query_timeout(config), sqlx::query("SELECT 1").execute(pool)).await??;
        }
        ManagedPool::Mysql(pool) => {
            timeout(query_timeout(config), sqlx::query("SELECT 1").execute(pool)).await??;
        }
        ManagedPool::Postgres(pool) => {
            timeout(query_timeout(config), sqlx::query("SELECT 1").execute(pool)).await??;
        }
    }
    Ok(())
}

fn annotate_connect_error(kind: &DbKind, error: sqlx::Error, text: &BackendText) -> anyhow::Error {
    if let sqlx::Error::Database(db_error) = &error {
        let code = db_error
            .code()
            .map(|code| code.to_string())
            .unwrap_or_default();
        if matches!(kind, DbKind::Mysql) && code == "1045" {
            return anyhow::anyhow!(text.mysql_auth_failed(db_error.message()));
        }
        if matches!(kind, DbKind::Mysql) && code == "1049" {
            return anyhow::anyhow!(text.mysql_database_missing(db_error.message()));
        }
        if matches!(kind, DbKind::Postgres) && matches!(code.as_str(), "28P01" | "28000") {
            return anyhow::anyhow!(text.postgres_auth_failed(db_error.message()));
        }
        if matches!(kind, DbKind::Postgres) && code == "3D000" {
            return anyhow::anyhow!(text.postgres_database_missing(db_error.message()));
        }
        if matches!(kind, DbKind::Postgres) && code == "42501" {
            return anyhow::anyhow!(text.postgres_permission_denied(db_error.message()));
        }
    }

    match &error {
        sqlx::Error::Io(io_error) => {
            return anyhow::anyhow!(text.network_failed(db_label(kind), &io_error.to_string()));
        }
        sqlx::Error::Tls(tls_error) => {
            return anyhow::anyhow!(text.tls_failed(db_label(kind), &tls_error.to_string()));
        }
        sqlx::Error::PoolTimedOut => {
            return anyhow::anyhow!(text.connection_timeout(db_label(kind)));
        }
        _ => {}
    }

    error.into()
}

fn mysql_ssl_mode(config: &ConnectionConfig) -> anyhow::Result<MySqlSslMode> {
    Ok(match config.ssl_mode.as_deref().unwrap_or("preferred") {
        "disable" | "disabled" => MySqlSslMode::Disabled,
        "prefer" | "preferred" => MySqlSslMode::Preferred,
        "require" | "required" => MySqlSslMode::Required,
        "verify_ca" | "verify-ca" => MySqlSslMode::VerifyCa,
        "verify_identity" | "verify-identity" => MySqlSslMode::VerifyIdentity,
        other => return Err(anyhow::anyhow!("unsupported MySQL ssl_mode: {other}")),
    })
}

fn pg_ssl_mode(config: &ConnectionConfig) -> anyhow::Result<PgSslMode> {
    Ok(match config.ssl_mode.as_deref().unwrap_or("prefer") {
        "disable" | "disabled" => PgSslMode::Disable,
        "allow" => PgSslMode::Allow,
        "prefer" | "preferred" => PgSslMode::Prefer,
        "require" | "required" => PgSslMode::Require,
        "verify_ca" | "verify-ca" => PgSslMode::VerifyCa,
        "verify_full" | "verify-full" => PgSslMode::VerifyFull,
        other => return Err(anyhow::anyhow!("unsupported PostgreSQL ssl_mode: {other}")),
    })
}

fn resolve_password(
    config: &ConnectionConfig,
    vault: &CredentialVault,
    password_override: Option<&str>,
) -> anyhow::Result<Zeroizing<String>> {
    if let Some(password) = password_override {
        return Ok(Zeroizing::new(password.to_string()));
    }

    match config.credential_ref.as_deref() {
        Some(credential_ref) => Ok(vault
            .get(credential_ref)?
            .unwrap_or_else(|| Zeroizing::new(String::new()))),
        None => Ok(Zeroizing::new(String::new())),
    }
}

fn connection_diagnostics(
    config: &ConnectionConfig,
    vault: &CredentialVault,
    text: &BackendText,
) -> ConnectionDiagnostics {
    let credential_ref_present = config.credential_ref.is_some();
    let credential_state = match (&config.kind, config.credential_ref.as_deref()) {
        (DbKind::Sqlite, _) => "not_required".to_string(),
        (_, None) => "not_saved".to_string(),
        (_, Some(credential_ref)) => match vault.get(credential_ref) {
            Ok(Some(password)) if password.is_empty() => "saved_empty".to_string(),
            Ok(Some(_)) => "saved".to_string(),
            Ok(None) => "missing_in_vault".to_string(),
            Err(_) => "vault_error".to_string(),
        },
    };

    let hint = match (
        &config.kind,
        credential_state.as_str(),
        config.host.as_deref(),
        config.ssl_mode.as_deref(),
    ) {
        (DbKind::Mysql | DbKind::Postgres, "missing_in_vault" | "not_saved", _, _) => {
            Some(text.missing_password_hint().to_string())
        }
        (DbKind::Mysql, _, Some("127.0.0.1"), _) => Some(text.mysql_127_hint().to_string()),
        (DbKind::Mysql, _, Some("localhost"), _) => Some(text.mysql_localhost_hint().to_string()),
        (
            DbKind::Mysql | DbKind::Postgres,
            _,
            _,
            Some("require" | "verify_ca" | "verify_full" | "verify_identity"),
        ) => Some(text.ssl_required_hint().to_string()),
        _ => None,
    };

    ConnectionDiagnostics {
        id: config.id.clone(),
        name: config.name.clone(),
        database_type: db_kind(config).to_string(),
        host: config.host.clone(),
        port: config.port.or_else(|| default_port(&config.kind)),
        database: config.database.clone(),
        username: config.username.clone(),
        ssl_mode: config.ssl_mode.clone(),
        credential_ref_present,
        credential_state,
        query_timeout_ms: config.query_timeout_ms,
        max_connections: config.max_connections,
        hint,
    }
}

fn db_label(kind: &DbKind) -> &'static str {
    match kind {
        DbKind::Sqlite => "SQLite",
        DbKind::Mysql => "MySQL",
        DbKind::Postgres => "PostgreSQL",
    }
}

fn db_kind(config: &ConnectionConfig) -> &'static str {
    match config.kind {
        DbKind::Sqlite => "sqlite",
        DbKind::Mysql => "mysql",
        DbKind::Postgres => "postgres",
    }
}

fn required<'a>(value: Option<&'a str>, field: &str) -> anyhow::Result<&'a str> {
    value
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("{field} is required"))
}

fn query_timeout(config: &ConnectionConfig) -> Duration {
    Duration::from_millis(config.query_timeout_ms.max(500))
}

fn validate_identifier(identifier: &str) -> anyhow::Result<()> {
    let re = Regex::new(r"^[A-Za-z_][A-Za-z0-9_$]*$").expect("valid identifier regex");
    if re.is_match(identifier) {
        Ok(())
    } else {
        Err(anyhow::anyhow!("unsafe identifier rejected"))
    }
}

fn quote_identifier(kind: &DbKind, identifier: &str) -> anyhow::Result<String> {
    validate_identifier(identifier)?;
    let quote = match kind {
        DbKind::Mysql => "`",
        DbKind::Sqlite | DbKind::Postgres => "\"",
    };
    Ok(format!("{quote}{identifier}{quote}"))
}

fn is_explain_statement(sql: &str) -> bool {
    sql.trim_start()
        .get(..7)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("EXPLAIN"))
        && sql
            .trim_start()
            .get(7..8)
            .is_some_and(|separator| separator.chars().all(char::is_whitespace))
}

const METADATA_MAX_ROWS: usize = 10_000;

async fn collect_metadata_rows<S, T>(stream: S) -> anyhow::Result<Vec<T>>
where
    S: Stream<Item = Result<T, sqlx::Error>>,
{
    let mut stream = Box::pin(stream);
    let mut rows = Vec::new();
    while let Some(row) = stream.next().await.transpose()? {
        if rows.len() >= METADATA_MAX_ROWS {
            return Err(anyhow::anyhow!(
                "metadata result exceeds the safety limit of {METADATA_MAX_ROWS} rows"
            ));
        }
        rows.push(row);
    }
    Ok(rows)
}

struct CollectedRows {
    columns: Vec<String>,
    rows: Vec<Map<String, Value>>,
    truncated: bool,
    truncation_reason: Option<String>,
    returned_bytes: usize,
}

async fn collect_query_rows<S, T, C, R, F>(
    stream: S,
    max_rows: u32,
    max_bytes: usize,
    columns_for: C,
    raw_bytes_for: R,
    map_row: F,
) -> anyhow::Result<CollectedRows>
where
    S: Stream<Item = Result<T, sqlx::Error>>,
    C: Fn(&T) -> Vec<String>,
    R: Fn(&T) -> usize,
    F: Fn(&T, &[String]) -> Map<String, Value>,
{
    let mut stream = Box::pin(stream);
    let max_rows = usize::try_from(max_rows.max(1))?;
    let mut columns = Vec::new();
    let mut rows = Vec::with_capacity(max_rows.min(1_024));
    let mut returned_bytes = 2usize;
    let mut truncation_reason = None;

    while let Some(row) = stream.next().await.transpose()? {
        if rows.len() >= max_rows {
            truncation_reason = Some("rows".to_string());
            break;
        }
        if columns.is_empty() {
            columns = columns_for(&row);
        }
        if returned_bytes.saturating_add(raw_bytes_for(&row)) > max_bytes.max(1) {
            truncation_reason = Some("bytes".to_string());
            break;
        }
        let candidate = map_row(&row, &columns);
        let candidate_bytes = serde_json::to_vec(&candidate)?.len();
        let separator_bytes = usize::from(!rows.is_empty());
        if returned_bytes
            .saturating_add(separator_bytes)
            .saturating_add(candidate_bytes)
            > max_bytes.max(1)
        {
            truncation_reason = Some("bytes".to_string());
            break;
        }
        returned_bytes = returned_bytes
            .saturating_add(separator_bytes)
            .saturating_add(candidate_bytes);
        rows.push(candidate);
    }

    Ok(CollectedRows {
        columns,
        rows,
        truncated: truncation_reason.is_some(),
        truncation_reason,
        returned_bytes,
    })
}

fn sqlite_raw_row_bytes(row: &SqliteRow) -> usize {
    borrowed_row_bytes(row)
}

fn mysql_raw_row_bytes(row: &MySqlRow) -> usize {
    borrowed_row_bytes(row)
}

fn pg_raw_row_bytes(row: &PgRow) -> usize {
    borrowed_row_bytes(row)
}

fn borrowed_row_bytes<R>(row: &R) -> usize
where
    R: Row,
    usize: sqlx::ColumnIndex<R>,
    for<'value> &'value [u8]: sqlx::Decode<'value, R::Database> + sqlx::Type<R::Database>,
{
    (0..row.len()).fold(0usize, |total, index| {
        total.saturating_add(row.try_get::<&[u8], _>(index).map_or(0, <[u8]>::len))
    })
}

fn unique_column_names<'a>(names: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut used = HashSet::<String>::new();
    let mut next_suffix = HashMap::<String, usize>::new();
    names
        .into_iter()
        .map(|name| {
            if used.insert(name.to_string()) {
                next_suffix.entry(name.to_string()).or_insert(2);
                return name.to_string();
            }
            let suffix = next_suffix.entry(name.to_string()).or_insert(2);
            loop {
                let candidate = format!("{name}__{suffix}");
                *suffix += 1;
                if used.insert(candidate.clone()) {
                    break candidate;
                }
            }
        })
        .collect()
}

fn result(collected: CollectedRows, elapsed: Duration, rewritten_sql: String) -> QueryResult {
    let row_count = collected.rows.len();
    QueryResult {
        columns: collected.columns,
        rows: collected.rows,
        row_count,
        truncated: collected.truncated,
        truncation_reason: collected.truncation_reason,
        returned_bytes: collected.returned_bytes,
        elapsed_ms: elapsed.as_millis().try_into().unwrap_or(u64::MAX),
        rewritten_sql,
    }
}

fn sqlite_cell(row: &SqliteRow, index: usize) -> Value {
    if sqlite_is_null(row, index) {
        return Value::Null;
    }
    let database_type = row.columns()[index].type_info().name();
    if database_type.eq_ignore_ascii_case("JSON") {
        if let Ok(value) = row.try_get::<sqlx::types::Json<Value>, _>(index) {
            return value.0;
        }
    }
    if let Ok(value) = row.try_get::<String, _>(index) {
        return Value::String(value);
    }
    if matches!(database_type, "DATETIME" | "TIMESTAMP") {
        if let Ok(value) = row.try_get::<NaiveDateTime, _>(index) {
            return Value::String(value.to_string());
        }
    }
    if database_type == "DATE" {
        if let Ok(value) = row.try_get::<NaiveDate, _>(index) {
            return Value::String(value.to_string());
        }
    }
    if database_type == "TIME" {
        if let Ok(value) = row.try_get::<NaiveTime, _>(index) {
            return Value::String(value.to_string());
        }
    }
    if let Ok(value) = row.try_get::<i64, _>(index) {
        return Value::Number(value.into());
    }
    if let Ok(value) = row.try_get::<f64, _>(index) {
        return serde_json::Number::from_f64(value)
            .map(Value::Number)
            .unwrap_or(Value::Null);
    }
    if let Ok(value) = row.try_get::<bool, _>(index) {
        return Value::Bool(value);
    }
    if let Ok(value) = row.try_get::<Vec<u8>, _>(index) {
        return binary_value(value);
    }
    unsupported_cell(database_type)
}

fn mysql_cell(row: &MySqlRow, index: usize) -> Value {
    if mysql_is_null(row, index) {
        return Value::Null;
    }
    if let Ok(value) = row.try_get::<sqlx::types::Json<Value>, _>(index) {
        return value.0;
    }
    if let Ok(value) = row.try_get::<Decimal, _>(index) {
        return Value::String(value.to_string());
    }
    if let Ok(value) = row.try_get::<NaiveDateTime, _>(index) {
        return Value::String(value.to_string());
    }
    if let Ok(value) = row.try_get::<NaiveDate, _>(index) {
        return Value::String(value.to_string());
    }
    if let Ok(value) = row.try_get::<NaiveTime, _>(index) {
        return Value::String(value.to_string());
    }
    if let Ok(value) = row.try_get::<bool, _>(index) {
        return Value::Bool(value);
    }
    if let Ok(value) = row.try_get::<i64, _>(index) {
        return Value::Number(value.into());
    }
    if let Ok(value) = row.try_get::<u64, _>(index) {
        return Value::Number(value.into());
    }
    if let Ok(value) = row.try_get::<f64, _>(index) {
        return serde_json::Number::from_f64(value)
            .map(Value::Number)
            .unwrap_or(Value::Null);
    }
    if let Ok(value) = row.try_get::<Vec<u8>, _>(index) {
        return binary_value(value);
    }
    if let Ok(value) = row.try_get::<String, _>(index) {
        return Value::String(value);
    }
    unsupported_cell(row.columns()[index].type_info().name())
}

fn pg_cell(row: &PgRow, index: usize) -> Value {
    if pg_is_null(row, index) {
        return Value::Null;
    }
    if let Ok(value) = row.try_get::<sqlx::types::Json<Value>, _>(index) {
        return value.0;
    }
    if let Ok(value) = row.try_get::<uuid::Uuid, _>(index) {
        return Value::String(value.to_string());
    }
    if let Ok(value) = row.try_get::<Decimal, _>(index) {
        return Value::String(value.to_string());
    }
    if let Ok(value) = row.try_get::<DateTime<Utc>, _>(index) {
        return Value::String(value.to_rfc3339());
    }
    if let Ok(value) = row.try_get::<DateTime<FixedOffset>, _>(index) {
        return Value::String(value.to_rfc3339());
    }
    if let Ok(value) = row.try_get::<NaiveDateTime, _>(index) {
        return Value::String(value.to_string());
    }
    if let Ok(value) = row.try_get::<NaiveDate, _>(index) {
        return Value::String(value.to_string());
    }
    if let Ok(value) = row.try_get::<NaiveTime, _>(index) {
        return Value::String(value.to_string());
    }
    if let Ok(value) = row.try_get::<bool, _>(index) {
        return Value::Bool(value);
    }
    if let Ok(value) = row.try_get::<i64, _>(index) {
        return Value::Number(value.into());
    }
    if let Ok(value) = row.try_get::<i32, _>(index) {
        return Value::Number(value.into());
    }
    if let Ok(value) = row.try_get::<f64, _>(index) {
        return serde_json::Number::from_f64(value)
            .map(Value::Number)
            .unwrap_or(Value::Null);
    }
    if let Ok(value) = row.try_get::<Vec<u8>, _>(index) {
        return binary_value(value);
    }
    if let Ok(value) = row.try_get::<String, _>(index) {
        return Value::String(value);
    }
    unsupported_cell(row.columns()[index].type_info().name())
}

fn binary_value(value: Vec<u8>) -> Value {
    Value::String(format!("base64:{}", BASE64_STANDARD.encode(value)))
}

fn unsupported_cell(database_type: &str) -> Value {
    serde_json::json!({
        "unsupported": {
            "database_type": database_type,
        }
    })
}

fn sqlite_is_null(row: &SqliteRow, index: usize) -> bool {
    row.try_get_raw(index)
        .map(|value| value.is_null())
        .unwrap_or(true)
}

fn mysql_is_null(row: &MySqlRow, index: usize) -> bool {
    row.try_get_raw(index)
        .map(|value| value.is_null())
        .unwrap_or(true)
}

fn pg_is_null(row: &PgRow, index: usize) -> bool {
    row.try_get_raw(index)
        .map(|value| value.is_null())
        .unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::sync::Barrier;

    use super::*;

    fn test_signature(database: &str) -> PoolSignature {
        PoolSignature {
            kind: 0,
            database: database.to_string(),
            host: None,
            port: None,
            username: None,
            credential_ref: None,
            ssl_mode: None,
            max_connections: 1,
        }
    }

    async fn test_sqlite_pool() -> ManagedPool {
        ManagedPool::Sqlite(
            SqlitePoolOptions::new()
                .max_connections(1)
                .connect("sqlite::memory:")
                .await
                .expect("in-memory SQLite pool should open"),
        )
    }

    async fn sqlite_connection(directory: &std::path::Path, name: &str) -> ConnectionConfig {
        let path = directory.join(name);
        SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(&path)
                    .create_if_missing(true),
            )
            .await
            .expect("SQLite fixture opens")
            .close()
            .await;
        ConnectionConfig {
            id: "test_connection".to_string(),
            name: "Test Connection".to_string(),
            kind: DbKind::Sqlite,
            enabled: true,
            database: format!("sqlite://{}", path.to_string_lossy().replace('\\', "/")),
            host: None,
            port: None,
            username: None,
            credential_ref: None,
            ssl_mode: None,
            max_rows: 10,
            query_timeout_ms: 2_000,
            max_connections: 1,
            max_result_bytes: crate::config::default_max_result_bytes(),
        }
    }

    #[test]
    fn duplicate_column_names_are_stable_and_unique() {
        assert_eq!(
            unique_column_names(["id", "name", "id__2", "id", "id", "name"]),
            ["id", "name", "id__2", "id__3", "id__4", "name__2"]
        );
    }

    #[test]
    fn unsupported_values_include_the_database_type() {
        assert_eq!(
            unsupported_cell("GEOGRAPHY"),
            serde_json::json!({
                "unsupported": {
                    "database_type": "GEOGRAPHY",
                }
            })
        );
    }

    #[test]
    fn detects_only_complete_explain_keyword() {
        assert!(is_explain_statement("EXPLAIN SELECT 1"));
        assert!(is_explain_statement("  explain\nSELECT 1"));
        assert!(!is_explain_statement("EXPLAINABLE SELECT 1"));
        assert!(!is_explain_statement("SELECT 1"));
    }

    #[test]
    fn explain_target_policy_does_not_change_query_semantics() {
        let policy = PolicyEngine::check_explain_target_with_text(
            &DbKind::Postgres,
            "SELECT * FROM records ORDER BY id",
            &crate::i18n::backend_text("en"),
        );
        assert!(policy.allowed);
        assert_eq!(
            policy.rewritten_sql.as_deref(),
            Some("SELECT * FROM records ORDER BY id")
        );
    }

    #[tokio::test]
    async fn sqlite_rows_are_bounded_and_encoded_without_column_loss() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory SQLite pool should open");
        let rows = collect_query_rows(
            sqlx::query(
                "SELECT 1 AS id, 2 AS id, X'0001FF' AS payload, NULL AS missing
                 UNION ALL
                 SELECT 3, 4, X'02', 'present'",
            )
            .fetch(&pool),
            1,
            crate::config::default_max_result_bytes(),
            |row| unique_column_names(row.columns().iter().map(|column| column.name())),
            sqlite_raw_row_bytes,
            |row, columns| {
                columns
                    .iter()
                    .enumerate()
                    .map(|(index, column)| (column.clone(), sqlite_cell(row, index)))
                    .collect()
            },
        )
        .await
        .expect("SQLite rows should stream");

        let result = result(rows, Duration::ZERO, "SELECT".to_string());

        assert_eq!(result.columns, ["id", "id__2", "payload", "missing"]);
        assert_eq!(result.row_count, 1);
        assert!(result.truncated);
        assert_eq!(result.rows[0].get("id"), Some(&Value::Number(1.into())));
        assert_eq!(result.rows[0].get("id__2"), Some(&Value::Number(2.into())));
        assert_eq!(
            result.rows[0].get("payload"),
            Some(&Value::String("base64:AAH/".to_string()))
        );
        assert_eq!(result.rows[0].get("missing"), Some(&Value::Null));
    }

    #[tokio::test]
    async fn invalidated_pool_build_cannot_publish_after_barrier() {
        let manager = Arc::new(DatabaseManager::default());
        let ready = Arc::new(Barrier::new(2));
        let invalidated = Arc::new(Barrier::new(2));
        let task_manager = manager.clone();
        let task_ready = ready.clone();
        let task_invalidated = invalidated.clone();

        let builder = tokio::spawn(async move {
            let generation = task_manager
                .pools
                .read()
                .await
                .generations
                .get("race")
                .copied()
                .unwrap_or(0);
            let pool = test_sqlite_pool().await;
            task_ready.wait().await;
            task_invalidated.wait().await;
            let mut state = task_manager.pools.write().await;
            let outcome = publish_pool(
                &mut state,
                "race".to_string(),
                generation,
                test_signature("old.db"),
                pool,
            );
            match outcome {
                PublishPool::Invalidated(pool) => {
                    close_pool(pool).await;
                    true
                }
                PublishPool::Published | PublishPool::Existing(_) => false,
            }
        });

        ready.wait().await;
        manager.close("race").await;
        invalidated.wait().await;

        assert!(builder.await.expect("builder task joins"));
        assert!(!manager.pools.read().await.entries.contains_key("race"));
    }

    #[tokio::test]
    async fn duplicate_signature_reuses_only_registered_pool() {
        let first = test_sqlite_pool().await;
        let second = test_sqlite_pool().await;
        let mut state = PoolState::default();
        assert!(matches!(
            publish_pool(
                &mut state,
                "same".to_string(),
                0,
                test_signature("same.db"),
                first,
            ),
            PublishPool::Published
        ));
        let outcome = publish_pool(
            &mut state,
            "same".to_string(),
            0,
            test_signature("same.db"),
            second.clone(),
        );
        assert!(matches!(outcome, PublishPool::Existing(_)));
        assert_eq!(state.entries.len(), 1);

        close_pool(second).await;
        if let Some(entry) = state.entries.remove("same") {
            close_pool(entry.pool).await;
        }
    }

    #[tokio::test]
    async fn same_generation_concurrent_misses_register_one_pool() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let config = Arc::new(sqlite_connection(directory.path(), "shared.db").await);
        let manager = Arc::new(DatabaseManager::default());
        let vault = Arc::new(CredentialVault::new());
        let barrier = Arc::new(Barrier::new(3));
        let mut tasks = Vec::new();
        for _ in 0..2 {
            let config = config.clone();
            let manager = manager.clone();
            let vault = vault.clone();
            let barrier = barrier.clone();
            tasks.push(tokio::spawn(async move {
                barrier.wait().await;
                manager
                    .pool(&config, &vault, &crate::i18n::backend_text("en"))
                    .await
            }));
        }
        barrier.wait().await;
        for task in tasks {
            task.await
                .expect("pool task joins")
                .expect("pool is available");
        }
        assert_eq!(manager.pools.read().await.entries.len(), 1);
        manager.close(&config.id).await;
    }

    #[tokio::test]
    async fn invalidation_forces_same_signature_and_new_config_to_build_again() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let first = sqlite_connection(directory.path(), "first.db").await;
        let second = sqlite_connection(directory.path(), "second.db").await;
        let manager = DatabaseManager::default();
        let vault = CredentialVault::new();
        let text = crate::i18n::backend_text("en");

        manager
            .pool(&first, &vault, &text)
            .await
            .expect("first pool");
        manager.close(&first.id).await;
        manager
            .pool(&first, &vault, &text)
            .await
            .expect("same signature rebuilds after credential-style invalidation");
        assert_eq!(manager.pools.read().await.entries[&first.id].generation, 1);

        manager.close(&first.id).await;
        manager
            .pool(&second, &vault, &text)
            .await
            .expect("new config builds after publication");
        assert_eq!(
            manager.pools.read().await.entries[&second.id]
                .signature
                .database,
            second.database
        );
        manager.close(&second.id).await;
    }
}
