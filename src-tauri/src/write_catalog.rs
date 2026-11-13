use std::time::Duration;

use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::mysql::MySqlRow;
use tokio::time::timeout;

use crate::config::{ConnectionConfig, DbKind, WriteTable};
use crate::db::{DatabaseManager, ManagedPool};
use crate::i18n::backend_text;
use crate::vault::CredentialVault;
use crate::write::{TableCapability, WriteError, WriteOperation};

/// Most tables listed in one schema read. A larger catalogue is narrowed with
/// the search term instead of being paged through.
const MAX_LISTED_TABLES: usize = 500;

#[derive(Debug, Clone, Serialize)]
pub struct WriteSchema {
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WriteTarget {
    pub target_id: String,
    pub schema: String,
    pub table: String,
    /// Operations the table supports today: any of insert/update/delete.
    pub operations: Vec<String>,
    pub reason_code: Option<String>,
    pub reason: Option<String>,
    /// Concrete metadata behind a rejection (a column and its declared type,
    /// for example), already free of values and database prose.
    pub reason_detail: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WriteTargetDetail {
    pub target: WriteTarget,
    pub columns: Vec<crate::write::ColumnCapability>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UnresolvedWriteTarget {
    pub schema: String,
    pub table: String,
    pub reason_code: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResolvedWriteTargets {
    pub targets: Vec<WriteTable>,
    pub unresolved: Vec<UnresolvedWriteTarget>,
}

pub fn failure(code: &str, message: &str) -> WriteError {
    WriteError {
        code: code.to_string(),
        message: message.to_string(),
        outcome_unknown: false,
    }
}

fn metadata_error(error: sqlx::Error) -> WriteError {
    // Never include database messages or decoded values in diagnostics.
    let (category, code, message) = match &error {
        sqlx::Error::ColumnDecode { .. } | sqlx::Error::Decode(_) => (
            "decode",
            "metadata_decode_error",
            "Database metadata could not be decoded. This is a metadata format compatibility error, not evidence of missing permissions.",
        ),
        sqlx::Error::ColumnNotFound(_) | sqlx::Error::ColumnIndexOutOfBounds { .. } => (
            "column",
            "metadata_format_error",
            "The metadata result is missing an expected column.",
        ),
        sqlx::Error::PoolTimedOut => (
            "pool_timeout",
            "metadata_timeout",
            "Timed out waiting for the metadata connection.",
        ),
        sqlx::Error::Io(_) | sqlx::Error::Tls(_) | sqlx::Error::Protocol(_) => (
            "connection",
            "metadata_connection_error",
            "The metadata connection was interrupted. Check connectivity and the database service.",
        ),
        sqlx::Error::Database(_) => (
            "database",
            "metadata_query_error",
            "The database rejected a metadata query. Check the database error code and metadata access.",
        ),
        _ => (
            "other",
            "metadata_unavailable",
            "Unable to read database metadata. Check the connection and metadata permissions.",
        ),
    };
    let database_code = error
        .as_database_error()
        .and_then(|error| error.code())
        .filter(|value| value.len() <= 16 && value.bytes().all(|byte| byte.is_ascii_alphanumeric()))
        .map(|value| value.into_owned());
    crate::debug_log::warn(
        "write_catalog",
        format_args!(
            "metadata query failed (category={category}, code={})",
            database_code.as_deref().unwrap_or("none"),
        ),
    );
    let suffix = database_code
        .map(|code| format!(" (database code {code})"))
        .unwrap_or_default();
    failure(code, &format!("{message}{suffix}"))
}

fn mysql_metadata_text(row: &MySqlRow, index: usize) -> Result<String, WriteError> {
    crate::db::mysql_metadata_text(row, index).map_err(metadata_error)
}

fn validate_lookup(value: &str) -> Result<(), WriteError> {
    if value.is_empty() || value.chars().count() > 128 || value.contains('\0') {
        return Err(failure(
            "invalid_target",
            "A database-provided schema and table name are required.",
        ));
    }
    Ok(())
}

fn limit(config: &ConnectionConfig) -> Duration {
    Duration::from_millis(config.query_timeout_ms.clamp(500, 60_000))
}

async fn pool(
    db: &DatabaseManager,
    config: &ConnectionConfig,
    vault: &CredentialVault,
) -> Result<ManagedPool, WriteError> {
    if !config.enabled || config.kind == DbKind::Jdbc {
        return Err(failure(
            "connection_unavailable",
            "Choose an enabled native database connection.",
        ));
    }
    timeout(limit(config), db.pool(config, vault, &backend_text("en")))
        .await
        .map_err(|_| {
            failure(
                "metadata_timeout",
                "Opening the metadata connection timed out.",
            )
        })?
        .map_err(|_| {
            failure(
                "metadata_unavailable",
                "Unable to open the read-only metadata connection.",
            )
        })
}

pub fn target_id(config: &ConnectionConfig, schema: &str, table: &str) -> String {
    let mut digest = Sha256::new();
    for part in [config.id.as_str(), schema, table] {
        digest.update((part.len() as u64).to_be_bytes());
        digest.update(part.as_bytes());
    }
    format!("wt_{:x}", digest.finalize())
}

pub fn valid_target_id(id: &str) -> bool {
    id.len() == 67 && id.starts_with("wt_") && id[3..].bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn operations_of(capability: &TableCapability) -> Vec<String> {
    let mut operations = Vec::new();
    if capability.insert {
        operations.push(WriteOperation::Insert.as_str().to_string());
    }
    if capability.update {
        operations.push(WriteOperation::Update.as_str().to_string());
    }
    if capability.delete {
        operations.push(WriteOperation::Delete.as_str().to_string());
    }
    operations
}

fn target_of(
    config: &ConnectionConfig,
    schema: &str,
    table: &str,
    capability: &TableCapability,
) -> WriteTarget {
    WriteTarget {
        target_id: target_id(config, schema, table),
        schema: schema.to_string(),
        table: table.to_string(),
        operations: operations_of(capability),
        reason_code: capability.reason_code.clone(),
        reason: capability.reason.clone(),
        reason_detail: capability.reason_detail.clone(),
    }
}

pub async fn list_schemas(
    db: &DatabaseManager,
    config: &ConnectionConfig,
    vault: &CredentialVault,
) -> Result<Vec<WriteSchema>, WriteError> {
    let managed = pool(db, config, vault).await?;
    let names: Vec<String> = timeout(limit(config), async {
        match managed {
            ManagedPool::Sqlite(_) => Ok(vec!["main".to_string()]),
            ManagedPool::Mysql(pool) => {
                let rows = sqlx::query("SELECT SCHEMA_NAME FROM information_schema.SCHEMATA WHERE SCHEMA_NAME NOT IN ('mysql','sys','information_schema','performance_schema') ORDER BY CAST(SCHEMA_NAME AS BINARY) LIMIT 5001")
                    .fetch_all(&pool).await.map_err(metadata_error)?;
                rows.iter().map(|row| mysql_metadata_text(row, 0)).collect()
            }
            ManagedPool::Postgres(pool) => sqlx::query_scalar("SELECT schema_name FROM information_schema.schemata WHERE schema_name <> 'information_schema' AND schema_name NOT LIKE 'pg\\_%' ESCAPE '\\' ORDER BY schema_name COLLATE \"C\" LIMIT 5001").fetch_all(&pool).await.map_err(metadata_error),
        }
    }).await.map_err(|_| failure("metadata_timeout", "Listing database schemas timed out."))??;
    if names.len() > 5000 {
        return Err(failure(
            "metadata_limit",
            "Too many visible schemas; use a more narrowly scoped database account.",
        ));
    }
    Ok(names.into_iter().map(|name| WriteSchema { name }).collect())
}

async fn mysql_case_mode(pool: &sqlx::MySqlPool) -> Result<i64, WriteError> {
    let mode: i64 = sqlx::query_scalar("SELECT CAST(@@lower_case_table_names AS SIGNED)")
        .fetch_one(pool)
        .await
        .map_err(metadata_error)?;
    if !(0..=2).contains(&mode) {
        return Err(failure(
            "identifier_rules_unknown",
            "The database identifier case rules could not be verified.",
        ));
    }
    Ok(mode)
}

/// Canonical name of one table as the server reports it, or `target_not_found`
/// when it is not visible.
pub async fn resolve_target(
    db: &DatabaseManager,
    config: &ConnectionConfig,
    vault: &CredentialVault,
    schema: &str,
    table: &str,
) -> Result<(String, String), WriteError> {
    validate_lookup(schema)?;
    validate_lookup(table)?;
    let managed = pool(db, config, vault).await?;
    let rows: Vec<(String, String, String)> = timeout(limit(config), async {
        match managed {
            ManagedPool::Mysql(pool) => {
                let mode = mysql_case_mode(&pool).await?;
                let rows = sqlx::query("SELECT TABLE_SCHEMA,TABLE_NAME,TABLE_TYPE FROM information_schema.TABLES WHERE TABLE_SCHEMA NOT IN ('mysql','sys','information_schema','performance_schema') AND ((? = 0 AND CAST(TABLE_SCHEMA AS BINARY)=CAST(? AS BINARY) AND CAST(TABLE_NAME AS BINARY)=CAST(? AS BINARY)) OR (? <> 0 AND LOWER(TABLE_SCHEMA)=LOWER(?) AND LOWER(TABLE_NAME)=LOWER(?))) LIMIT 2")
                    .bind(mode).bind(schema).bind(table).bind(mode).bind(schema).bind(table).fetch_all(&pool).await.map_err(metadata_error)?;
                rows.iter().map(|row| Ok((mysql_metadata_text(row, 0)?, mysql_metadata_text(row, 1)?, mysql_metadata_text(row, 2)?))).collect()
            }
            ManagedPool::Postgres(pool) => sqlx::query_as("SELECT table_schema,table_name,table_type FROM information_schema.tables WHERE table_schema=$1 AND table_name=$2 AND table_schema <> 'information_schema' AND table_schema NOT LIKE 'pg\\_%' ESCAPE '\\' LIMIT 2")
                .bind(schema).bind(table).fetch_all(&pool).await.map_err(metadata_error),
            ManagedPool::Sqlite(pool) => {
                if !schema.eq_ignore_ascii_case("main") { return Err(failure("target_not_found", "SQLite writes are limited to the current file's main schema.")); }
                sqlx::query_as("SELECT 'main',name,type FROM main.sqlite_schema WHERE name=? COLLATE NOCASE AND type IN ('table','view') AND name NOT LIKE 'sqlite_%' LIMIT 2")
                    .bind(table).fetch_all(&pool).await.map_err(metadata_error)
            }
        }
    }).await.map_err(|_| failure("metadata_timeout", "Resolving the database target timed out."))??;
    if rows.len() != 1 {
        let code = if rows.is_empty() {
            "target_not_found"
        } else {
            "target_ambiguous"
        };
        return Err(failure(
            code,
            "The target could not be resolved uniquely from visible database metadata. Refresh the table list.",
        ));
    }
    let (schema, table, _) = rows
        .into_iter()
        .next()
        .ok_or_else(|| failure("target_not_found", "The target was not found."))?;
    Ok((schema, table))
}

async fn table_names(
    db: &DatabaseManager,
    config: &ConnectionConfig,
    vault: &CredentialVault,
    schema: &str,
    search: &str,
) -> Result<Vec<String>, WriteError> {
    validate_lookup(schema)?;
    if search.chars().count() > 128 || search.contains('\0') {
        return Err(failure(
            "invalid_request",
            "The search term is invalid.",
        ));
    }
    let managed = pool(db, config, vault).await?;
    let limit_items = (MAX_LISTED_TABLES + 1) as i64;
    timeout(limit(config), async {
        match managed {
            ManagedPool::Mysql(pool) => {
                let mode = mysql_case_mode(&pool).await?;
                let rows = sqlx::query("SELECT TABLE_NAME FROM information_schema.TABLES WHERE TABLE_SCHEMA NOT IN ('mysql','sys','information_schema','performance_schema') AND ((?=0 AND CAST(TABLE_SCHEMA AS BINARY)=CAST(? AS BINARY)) OR (?<>0 AND LOWER(TABLE_SCHEMA)=LOWER(?))) AND LOCATE(LOWER(?),LOWER(TABLE_NAME))>0 ORDER BY CAST(TABLE_NAME AS BINARY) LIMIT ?")
                    .bind(mode).bind(schema).bind(mode).bind(schema).bind(search).bind(limit_items).fetch_all(&pool).await.map_err(metadata_error)?;
                rows.iter()
                    .map(|row| mysql_metadata_text(row, 0))
                    .collect::<Result<Vec<_>, WriteError>>()
            }
            ManagedPool::Postgres(pool) => Ok(sqlx::query_scalar("SELECT table_name FROM information_schema.tables WHERE table_schema=$1 AND table_schema <> 'information_schema' AND table_schema NOT LIKE 'pg\\_%' ESCAPE '\\' AND strpos(lower(table_name),lower($2))>0 ORDER BY table_name COLLATE \"C\" LIMIT $3")
                .bind(schema).bind(search).bind(limit_items).fetch_all(&pool).await.map_err(metadata_error)?),
            ManagedPool::Sqlite(pool) => {
                if !schema.eq_ignore_ascii_case("main") { return Err(failure("target_not_found", "SQLite only exposes the main schema for writes.")); }
                Ok(sqlx::query_scalar("SELECT name FROM main.sqlite_schema WHERE type IN ('table','view') AND name NOT LIKE 'sqlite_%' AND instr(lower(name),lower(?))>0 ORDER BY name COLLATE BINARY LIMIT ?")
                    .bind(search).bind(limit_items).fetch_all(&pool).await.map_err(metadata_error)?)
            }
        }
    }).await.map_err(|_| failure("metadata_timeout", "Listing database tables timed out."))?
}

/// Every visible table of one schema together with what it supports right now.
/// Capability comes from one set-based catalogue read per schema, so opening a
/// schema with hundreds of tables costs a bounded number of round trips instead
/// of a per-table inspection.
pub async fn list_tables(
    db: &DatabaseManager,
    config: &ConnectionConfig,
    vault: &CredentialVault,
    schema: &str,
    search: Option<&str>,
) -> Result<Vec<WriteTarget>, WriteError> {
    let search = search.unwrap_or("");
    let mut names = table_names(db, config, vault, schema, search).await?;
    // A larger catalogue is narrowed with the search term rather than paged
    // through; the cap keeps one metadata refresh bounded.
    names.truncate(MAX_LISTED_TABLES);
    let mut session = crate::write::MetadataSession::open(config, vault).await?;
    let capabilities = session.capabilities(schema, &names).await?;
    let targets = names
        .into_iter()
        .zip(capabilities)
        .map(|(table, capability)| target_of(config, schema, &table, &capability))
        .collect();
    Ok(targets)
}

/// Columns, warnings, and operations of one table, as the approval path sees
/// them.
pub async fn inspect_target(
    config: &ConnectionConfig,
    vault: &CredentialVault,
    schema: &str,
    table: &str,
) -> Result<WriteTargetDetail, WriteError> {
    let capability = crate::write::inspect_table(config, vault, schema, table).await?;
    Ok(WriteTargetDetail {
        target: target_of(config, schema, table, &capability),
        columns: capability.columns,
        warnings: capability.warnings,
    })
}

/// Re-resolve the tables already saved for a connection. Entries the server no
/// longer reports are returned separately instead of being dropped silently.
pub async fn resolve_saved(
    db: &DatabaseManager,
    config: &ConnectionConfig,
    vault: &CredentialVault,
) -> ResolvedWriteTargets {
    let mut result = ResolvedWriteTargets {
        targets: Vec::new(),
        unresolved: Vec::new(),
    };
    for entry in &config.write_tables {
        match resolve_target(db, config, vault, &entry.schema, &entry.table).await {
            Ok((schema, table)) => {
                let identity = target_id(config, &schema, &table);
                if entry
                    .target_id
                    .as_ref()
                    .is_some_and(|saved| saved != &identity)
                {
                    result.unresolved.push(UnresolvedWriteTarget {
                        schema: entry.schema.clone(),
                        table: entry.table.clone(),
                        reason_code: "target_changed".to_string(),
                        reason:
                            "The stored target identity no longer matches; select the table again."
                                .to_string(),
                    });
                    continue;
                }
                if result
                    .targets
                    .iter()
                    .all(|saved| saved.target_id.as_ref() != Some(&identity))
                {
                    result.targets.push(WriteTable {
                        schema,
                        table,
                        target_id: Some(identity),
                    });
                }
            }
            Err(error) => result.unresolved.push(UnresolvedWriteTarget {
                schema: entry.schema.clone(),
                table: entry.table.clone(),
                reason_code: error.code,
                reason: error.message,
            }),
        }
    }
    result
}

/// Confirm that the requested table is one the operator selected for this
/// connection. `target_id` is the canonical form; explicit names stay
/// supported for compatibility with older clients.
pub async fn resolve_allowed(
    db: &DatabaseManager,
    config: &ConnectionConfig,
    vault: &CredentialVault,
    requested_id: Option<&str>,
    schema: &str,
    table: &str,
) -> Result<(String, String), WriteError> {
    if let Some(id) = requested_id {
        if !valid_target_id(id) {
            return Err(failure(
                "invalid_target",
                "The target identifier is invalid. Discover allowed write targets first.",
            ));
        }
        for entry in &config.write_tables {
            if entry.target_id.as_ref().is_some_and(|saved| saved != id) {
                continue;
            }
            let resolved = resolve_target(db, config, vault, &entry.schema, &entry.table).await;
            let (resolved_schema, resolved_table) = match resolved {
                Ok(resolved) => resolved,
                Err(_) if entry.target_id.as_deref() == Some(id) => continue,
                Err(error) => return Err(error),
            };
            if target_id(config, &resolved_schema, &resolved_table) != id {
                continue;
            }
            if (!schema.is_empty() && schema != resolved_schema)
                || (!table.is_empty() && table != resolved_table)
            {
                return Err(failure(
                    "target_mismatch",
                    "The supplied names do not match the selected target identifier.",
                ));
            }
            return Ok((resolved_schema, resolved_table));
        }
        return Err(failure(
            "table_not_allowed",
            "The target identifier is not selected for this connection. Refresh allowed write targets.",
        ));
    }
    let (requested_schema, requested_table) = resolve_target(db, config, vault, schema, table).await?;
    let identity = target_id(config, &requested_schema, &requested_table);
    let folded = matches!(config.kind, DbKind::Mysql | DbKind::Sqlite);
    for entry in &config.write_tables {
        if entry.target_id.as_ref() == Some(&identity) {
            return Ok((requested_schema, requested_table));
        }
        let names_match = if folded {
            entry.schema.eq_ignore_ascii_case(&requested_schema)
                && entry.table.eq_ignore_ascii_case(&requested_table)
        } else {
            entry.schema == requested_schema && entry.table == requested_table
        };
        if entry.target_id.is_none() && names_match {
            return Ok((requested_schema, requested_table));
        }
    }
    Err(failure(
        "table_not_allowed",
        "The resolved table is not selected in Write Center for this connection. Refresh and save the table selection.",
    ))
}
