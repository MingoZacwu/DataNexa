use std::collections::HashMap;
use std::str::FromStr;
use std::time::{Duration, Instant};

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sqlx::mysql::{MySqlConnectOptions, MySqlPoolOptions, MySqlRow, MySqlSslMode};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions, PgRow, PgSslMode};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions, SqliteRow};
use sqlx::{Column, Executor, MySqlPool, PgPool, Row, SqlitePool, ValueRef};
use tokio::sync::RwLock;
use tokio::time::timeout;

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

#[derive(Default)]
pub struct DatabaseManager {
    pools: RwLock<HashMap<String, ManagedPool>>,
}

impl DatabaseManager {
    pub async fn close(&self, connection_id: &str) {
        let pool = self.pools.write().await.remove(connection_id);
        if let Some(pool) = pool {
            close_pool(pool).await;
        }
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
                    sqlx::query("SELECT NULL AS schema_name, name, type FROM sqlite_master WHERE type IN ('table', 'view') AND name NOT LIKE 'sqlite_%' ORDER BY name")
                        .fetch_all(&pool),
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
                    sqlx::query("SELECT TABLE_SCHEMA, TABLE_NAME, TABLE_TYPE FROM information_schema.TABLES WHERE TABLE_SCHEMA = DATABASE() ORDER BY TABLE_NAME")
                        .fetch_all(&pool),
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
                    sqlx::query("SELECT table_schema, table_name, table_type FROM information_schema.tables WHERE table_schema NOT IN ('pg_catalog', 'information_schema') ORDER BY table_schema, table_name")
                        .fetch_all(&pool),
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
                let rows =
                    timeout(query_timeout(config), sqlx::query(&sql).fetch_all(&pool)).await??;
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
                    sqlx::query("SELECT COLUMN_NAME, DATA_TYPE, IS_NULLABLE, COLUMN_KEY FROM information_schema.COLUMNS WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = ? ORDER BY ORDINAL_POSITION")
                        .bind(table)
                        .fetch_all(&pool),
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
                    sqlx::query(
                        "SELECT c.column_name, c.data_type, c.is_nullable, COALESCE(tc.constraint_type = 'PRIMARY KEY', false) AS primary_key
                         FROM information_schema.columns c
                         LEFT JOIN information_schema.key_column_usage kcu
                           ON c.table_schema = kcu.table_schema AND c.table_name = kcu.table_name AND c.column_name = kcu.column_name
                         LEFT JOIN information_schema.table_constraints tc
                           ON kcu.constraint_schema = tc.constraint_schema AND kcu.constraint_name = tc.constraint_name
                         WHERE c.table_schema = $1 AND c.table_name = $2
                         ORDER BY c.ordinal_position",
                    )
                    .bind(schema)
                    .bind(table)
                    .fetch_all(&pool),
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
        let sql = format!("SELECT * FROM {table_sql} LIMIT {max_rows}");
        self.execute_internal(config, vault, &sql, sql.clone(), text)
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
            .execute_internal(config, vault, &rewritten, rewritten.clone(), text)
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
        let policy = PolicyEngine::check_with_text(&config.kind, sql, config.max_rows, text);
        if !policy.allowed {
            return Ok((policy, None));
        }

        let rewritten = policy
            .rewritten_sql
            .clone()
            .ok_or_else(|| anyhow::anyhow!("policy accepted SQL without a rewritten statement"))?;
        let explain_sql = format!("EXPLAIN {rewritten}");
        let result = self
            .execute_internal(config, vault, &explain_sql, explain_sql.clone(), text)
            .await?;
        Ok((policy, Some(result)))
    }

    async fn execute_internal(
        &self,
        config: &ConnectionConfig,
        vault: &CredentialVault,
        sql: &str,
        rewritten_sql: String,
        text: &BackendText,
    ) -> anyhow::Result<QueryResult> {
        let started = Instant::now();
        match self.pool(config, vault, text).await? {
            ManagedPool::Sqlite(pool) => {
                let rows =
                    timeout(query_timeout(config), sqlx::query(sql).fetch_all(&pool)).await??;
                Ok(sqlite_result(rows, started.elapsed(), rewritten_sql))
            }
            ManagedPool::Mysql(pool) => {
                let rows =
                    timeout(query_timeout(config), sqlx::query(sql).fetch_all(&pool)).await??;
                Ok(mysql_result(rows, started.elapsed(), rewritten_sql))
            }
            ManagedPool::Postgres(pool) => {
                let rows =
                    timeout(query_timeout(config), sqlx::query(sql).fetch_all(&pool)).await??;
                Ok(pg_result(rows, started.elapsed(), rewritten_sql))
            }
        }
    }

    async fn pool(
        &self,
        config: &ConnectionConfig,
        vault: &CredentialVault,
        text: &BackendText,
    ) -> anyhow::Result<ManagedPool> {
        {
            let pools = self.pools.read().await;
            if let Some(pool) = pools.get(&config.id) {
                return Ok(pool.clone());
            }
        }

        let pool = connect_pool(config, vault, text).await?;
        let mut pools = self.pools.write().await;
        pools.insert(config.id.clone(), pool.clone());
        Ok(pool)
    }
}

async fn connect_pool(
    config: &ConnectionConfig,
    vault: &CredentialVault,
    text: &BackendText,
) -> anyhow::Result<ManagedPool> {
    connect_pool_with_password(config, vault, None, text).await
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
) -> anyhow::Result<String> {
    if let Some(password) = password_override {
        return Ok(password.to_string());
    }

    match config.credential_ref.as_deref() {
        Some(credential_ref) => Ok(vault.get(credential_ref)?.unwrap_or_default()),
        None => Ok(String::new()),
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

fn sqlite_result(rows: Vec<SqliteRow>, elapsed: Duration, rewritten_sql: String) -> QueryResult {
    let columns: Vec<String> = rows
        .first()
        .map(|row| {
            row.columns()
                .iter()
                .map(|column| column.name().to_string())
                .collect()
        })
        .unwrap_or_default();
    let row_maps = rows
        .iter()
        .map(|row| {
            columns
                .iter()
                .enumerate()
                .map(|(index, column)| (column.clone(), sqlite_cell(row, index)))
                .collect()
        })
        .collect::<Vec<Map<String, Value>>>();
    result(columns, row_maps, elapsed, rewritten_sql)
}

fn mysql_result(rows: Vec<MySqlRow>, elapsed: Duration, rewritten_sql: String) -> QueryResult {
    let columns: Vec<String> = rows
        .first()
        .map(|row| {
            row.columns()
                .iter()
                .map(|column| column.name().to_string())
                .collect()
        })
        .unwrap_or_default();
    let row_maps = rows
        .iter()
        .map(|row| {
            columns
                .iter()
                .enumerate()
                .map(|(index, column)| (column.clone(), mysql_cell(row, index)))
                .collect()
        })
        .collect::<Vec<Map<String, Value>>>();
    result(columns, row_maps, elapsed, rewritten_sql)
}

fn pg_result(rows: Vec<PgRow>, elapsed: Duration, rewritten_sql: String) -> QueryResult {
    let columns: Vec<String> = rows
        .first()
        .map(|row| {
            row.columns()
                .iter()
                .map(|column| column.name().to_string())
                .collect()
        })
        .unwrap_or_default();
    let row_maps = rows
        .iter()
        .map(|row| {
            columns
                .iter()
                .enumerate()
                .map(|(index, column)| (column.clone(), pg_cell(row, index)))
                .collect()
        })
        .collect::<Vec<Map<String, Value>>>();
    result(columns, row_maps, elapsed, rewritten_sql)
}

fn result(
    columns: Vec<String>,
    rows: Vec<Map<String, Value>>,
    elapsed: Duration,
    rewritten_sql: String,
) -> QueryResult {
    let row_count = rows.len();
    QueryResult {
        columns,
        rows,
        row_count,
        truncated: false,
        elapsed_ms: elapsed.as_millis().try_into().unwrap_or(u64::MAX),
        rewritten_sql,
    }
}

fn sqlite_cell(row: &SqliteRow, index: usize) -> Value {
    if sqlite_is_null(row, index) {
        return Value::Null;
    }
    if let Ok(value) = row.try_get::<String, _>(index) {
        return Value::String(value);
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
    Value::String("<unrenderable>".to_string())
}

fn mysql_cell(row: &MySqlRow, index: usize) -> Value {
    if mysql_is_null(row, index) {
        return Value::Null;
    }
    if let Ok(value) = row.try_get::<String, _>(index) {
        return Value::String(value);
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
    if let Ok(value) = row.try_get::<bool, _>(index) {
        return Value::Bool(value);
    }
    Value::String("<unrenderable>".to_string())
}

fn pg_cell(row: &PgRow, index: usize) -> Value {
    if pg_is_null(row, index) {
        return Value::Null;
    }
    if let Ok(value) = row.try_get::<String, _>(index) {
        return Value::String(value);
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
    if let Ok(value) = row.try_get::<bool, _>(index) {
        return Value::Bool(value);
    }
    Value::String("<unrenderable>".to_string())
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
