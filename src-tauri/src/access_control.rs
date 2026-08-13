use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use tauri::{AppHandle, Manager};
use uuid::Uuid;

use crate::config::{AppConfig, ConfigStore, MCP_TOOL_NAMES};

const LEGACY_TOKEN_ID: &str = "legacy-default-token";
const TOKEN_NAME_MAX_CHARS: usize = 80;

#[derive(Debug, Clone)]
pub struct AccessIdentity {
    pub token_id: String,
    pub denied_connections: Vec<String>,
    pub denied_tools: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AccessTokenInfo {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub denied_connections: Vec<String>,
    pub denied_tools: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct TokenAuditInfo {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub deleted: bool,
}

pub struct AccessControlStore {
    pool: SqlitePool,
    path: PathBuf,
}

impl AccessControlStore {
    pub fn new(app: &AppHandle) -> anyhow::Result<Self> {
        let dir = app.path().app_config_dir()?;
        std::fs::create_dir_all(&dir)?;
        Self::from_path(dir.join("access-control.db"))
    }

    fn from_path(path: PathBuf) -> anyhow::Result<Self> {
        let options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true);
        Ok(Self {
            pool: SqlitePoolOptions::new()
                .max_connections(1)
                .connect_lazy_with(options),
            path,
        })
    }

    #[cfg(test)]
    pub(crate) fn for_test(path: PathBuf) -> Self {
        Self::from_path(path).expect("test access-control store")
    }

    pub async fn initialize(
        &self,
        store: &ConfigStore,
        config: &mut AppConfig,
    ) -> anyhow::Result<()> {
        sqlx::query("PRAGMA journal_mode=WAL")
            .execute(&self.pool)
            .await?;
        sqlx::query("PRAGMA foreign_keys=ON")
            .execute(&self.pool)
            .await?;
        sqlx::query("CREATE TABLE IF NOT EXISTS access_tokens (id TEXT PRIMARY KEY, name TEXT NOT NULL, secret TEXT UNIQUE, enabled INTEGER NOT NULL, created_at_ms INTEGER NOT NULL, updated_at_ms INTEGER NOT NULL, last_used_at_ms INTEGER, deleted_at_ms INTEGER)").execute(&self.pool).await?;
        sqlx::query("CREATE TABLE IF NOT EXISTS token_denied_connections (token_id TEXT NOT NULL, connection_id TEXT NOT NULL, PRIMARY KEY(token_id, connection_id), FOREIGN KEY(token_id) REFERENCES access_tokens(id) ON DELETE CASCADE)").execute(&self.pool).await?;
        sqlx::query("CREATE TABLE IF NOT EXISTS token_denied_tools (token_id TEXT NOT NULL, tool_name TEXT NOT NULL, PRIMARY KEY(token_id, tool_name), FOREIGN KEY(token_id) REFERENCES access_tokens(id) ON DELETE CASCADE)").execute(&self.pool).await?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS access_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
        )
        .execute(&self.pool)
        .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_access_tokens_secret ON access_tokens(secret) WHERE secret IS NOT NULL").execute(&self.pool).await?;
        sqlx::query("PRAGMA user_version=1")
            .execute(&self.pool)
            .await?;
        restrict_file_permissions(&self.path)?;

        if let Some(secret) = config
            .server
            .token
            .clone()
            .filter(|value| !value.is_empty())
        {
            let migrated =
                sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM access_tokens WHERE id = ?")
                    .bind(LEGACY_TOKEN_ID)
                    .fetch_one(&self.pool)
                    .await?
                    > 0;
            if !migrated {
                let now = Utc::now().timestamp_millis();
                let name = if config.settings.language == "zh-CN" {
                    "默认令牌"
                } else {
                    "Default Token"
                };
                sqlx::query("INSERT INTO access_tokens (id,name,secret,enabled,created_at_ms,updated_at_ms) VALUES (?,?,?,?,?,?)")
                    .bind(LEGACY_TOKEN_ID).bind(name).bind(secret).bind(true).bind(now).bind(now)
                    .execute(&self.pool).await?;
            }
            let mut migrated_config = config.clone();
            migrated_config.server.token = None;
            if store.save(&migrated_config).is_ok() {
                *config = migrated_config;
            }
        }
        Ok(())
    }

    pub async fn ensure_available(&self) -> anyhow::Result<()> {
        sqlx::query("SELECT 1").execute(&self.pool).await?;
        Ok(())
    }

    pub async fn list(&self) -> anyhow::Result<Vec<AccessTokenInfo>> {
        let rows = sqlx::query_as::<_, (String, String, bool, i64, i64, Option<i64>)>("SELECT id,name,enabled,created_at_ms,updated_at_ms,last_used_at_ms FROM access_tokens WHERE deleted_at_ms IS NULL ORDER BY created_at_ms,id")
            .fetch_all(&self.pool).await?;
        let mut result = Vec::with_capacity(rows.len());
        for row in rows {
            result.push(AccessTokenInfo {
                denied_connections: self.denied_connections(&row.0).await?,
                denied_tools: self.denied_tools(&row.0).await?,
                id: row.0,
                name: row.1,
                enabled: row.2,
                created_at: timestamp(row.3)?,
                updated_at: timestamp(row.4)?,
                last_used_at: row.5.map(timestamp).transpose()?,
            });
        }
        Ok(result)
    }

    pub async fn list_audit_info(&self) -> anyhow::Result<Vec<TokenAuditInfo>> {
        let rows = sqlx::query_as::<_, (String, String, bool, Option<i64>)>(
            "SELECT id,name,enabled,deleted_at_ms FROM access_tokens ORDER BY created_at_ms,id",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| TokenAuditInfo {
                id: row.0,
                name: row.1,
                enabled: row.2,
                deleted: row.3.is_some(),
            })
            .collect())
    }

    pub async fn create(&self, name: &str) -> anyhow::Result<(AccessTokenInfo, String)> {
        let name = validate_name(name)?;
        let id = Uuid::new_v4().to_string();
        let secret = Uuid::new_v4().to_string();
        let now = Utc::now().timestamp_millis();
        sqlx::query("INSERT INTO access_tokens (id,name,secret,enabled,created_at_ms,updated_at_ms) VALUES (?,?,?,?,?,?)")
            .bind(&id).bind(name).bind(&secret).bind(true).bind(now).bind(now)
            .execute(&self.pool).await?;
        let token = self.get_active_info(&id).await?;
        Ok((token, secret))
    }

    pub async fn rename(&self, id: &str, name: &str) -> anyhow::Result<()> {
        let name = validate_name(name)?;
        let changed = sqlx::query(
            "UPDATE access_tokens SET name=?,updated_at_ms=? WHERE id=? AND deleted_at_ms IS NULL",
        )
        .bind(name)
        .bind(Utc::now().timestamp_millis())
        .bind(id)
        .execute(&self.pool)
        .await?
        .rows_affected();
        ensure_changed(changed)
    }

    pub async fn set_enabled(&self, id: &str, enabled: bool) -> anyhow::Result<()> {
        let changed = sqlx::query("UPDATE access_tokens SET enabled=?,updated_at_ms=? WHERE id=? AND deleted_at_ms IS NULL")
            .bind(enabled).bind(Utc::now().timestamp_millis()).bind(id).execute(&self.pool).await?.rows_affected();
        ensure_changed(changed)
    }

    pub async fn rotate(&self, id: &str) -> anyhow::Result<String> {
        let secret = Uuid::new_v4().to_string();
        let changed = sqlx::query("UPDATE access_tokens SET secret=?,updated_at_ms=? WHERE id=? AND deleted_at_ms IS NULL")
            .bind(&secret).bind(Utc::now().timestamp_millis()).bind(id).execute(&self.pool).await?.rows_affected();
        ensure_changed(changed)?;
        Ok(secret)
    }

    pub async fn delete(&self, id: &str) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;
        let changed = sqlx::query("UPDATE access_tokens SET secret=NULL,enabled=0,updated_at_ms=?,deleted_at_ms=? WHERE id=? AND deleted_at_ms IS NULL")
            .bind(Utc::now().timestamp_millis()).bind(Utc::now().timestamp_millis()).bind(id).execute(&mut *tx).await?.rows_affected();
        ensure_changed(changed)?;
        sqlx::query("DELETE FROM token_denied_connections WHERE token_id=?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM token_denied_tools WHERE token_id=?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn secret(&self, id: &str) -> anyhow::Result<String> {
        sqlx::query_scalar(
            "SELECT secret FROM access_tokens WHERE id=? AND enabled=1 AND deleted_at_ms IS NULL",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .flatten()
        .ok_or_else(|| anyhow::anyhow!("access token not found"))
    }

    pub async fn set_connection_allowed(
        &self,
        id: &str,
        connection_id: &str,
        allowed: bool,
    ) -> anyhow::Result<()> {
        self.ensure_active(id).await?;
        if allowed {
            sqlx::query(
                "DELETE FROM token_denied_connections WHERE token_id=? AND connection_id=?",
            )
            .bind(id)
            .bind(connection_id)
            .execute(&self.pool)
            .await?;
        } else {
            sqlx::query("INSERT OR IGNORE INTO token_denied_connections (token_id,connection_id) VALUES (?,?)").bind(id).bind(connection_id).execute(&self.pool).await?;
        }
        self.touch(id).await
    }

    pub async fn set_tool_allowed(
        &self,
        id: &str,
        tool: &str,
        allowed: bool,
    ) -> anyhow::Result<()> {
        self.ensure_active(id).await?;
        if !MCP_TOOL_NAMES.contains(&tool) {
            return Err(anyhow::anyhow!("unknown MCP tool"));
        }
        if allowed {
            sqlx::query("DELETE FROM token_denied_tools WHERE token_id=? AND tool_name=?")
                .bind(id)
                .bind(tool)
                .execute(&self.pool)
                .await?;
        } else {
            sqlx::query(
                "INSERT OR IGNORE INTO token_denied_tools (token_id,tool_name) VALUES (?,?)",
            )
            .bind(id)
            .bind(tool)
            .execute(&self.pool)
            .await?;
        }
        self.touch(id).await
    }

    pub async fn authenticate(&self, secret: &str) -> anyhow::Result<Option<AccessIdentity>> {
        if secret.is_empty() {
            return Ok(None);
        }
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query_as::<_, (String, Option<i64>)>("SELECT id,last_used_at_ms FROM access_tokens WHERE secret=? AND enabled=1 AND deleted_at_ms IS NULL")
            .bind(secret).fetch_optional(&mut *tx).await?;
        let Some((token_id, last_used)) = row else {
            tx.commit().await?;
            return Ok(None);
        };
        let denied_connections = sqlx::query_scalar("SELECT connection_id FROM token_denied_connections WHERE token_id=? ORDER BY connection_id")
            .bind(&token_id).fetch_all(&mut *tx).await?;
        let denied_tools = sqlx::query_scalar(
            "SELECT tool_name FROM token_denied_tools WHERE token_id=? ORDER BY tool_name",
        )
        .bind(&token_id)
        .fetch_all(&mut *tx)
        .await?;
        {
            let cutoff = (Utc::now() - chrono::Duration::minutes(1)).timestamp_millis();
            if last_used.is_none_or(|value| value < cutoff) {
                sqlx::query("UPDATE access_tokens SET last_used_at_ms=? WHERE id=?")
                    .bind(Utc::now().timestamp_millis())
                    .bind(&token_id)
                    .execute(&mut *tx)
                    .await?;
            }
        }
        tx.commit().await?;
        Ok(Some(AccessIdentity {
            token_id,
            denied_connections,
            denied_tools,
        }))
    }

    async fn denied_connections(&self, id: &str) -> anyhow::Result<Vec<String>> {
        Ok(sqlx::query_scalar("SELECT connection_id FROM token_denied_connections WHERE token_id=? ORDER BY connection_id").bind(id).fetch_all(&self.pool).await?)
    }
    async fn denied_tools(&self, id: &str) -> anyhow::Result<Vec<String>> {
        Ok(sqlx::query_scalar(
            "SELECT tool_name FROM token_denied_tools WHERE token_id=? ORDER BY tool_name",
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await?)
    }
    async fn get_active_info(&self, id: &str) -> anyhow::Result<AccessTokenInfo> {
        self.list()
            .await?
            .into_iter()
            .find(|token| token.id == id)
            .ok_or_else(|| anyhow::anyhow!("access token not found"))
    }
    async fn ensure_active(&self, id: &str) -> anyhow::Result<()> {
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM access_tokens WHERE id=? AND deleted_at_ms IS NULL",
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await?;
        ensure_changed(count as u64)
    }
    async fn touch(&self, id: &str) -> anyhow::Result<()> {
        sqlx::query("UPDATE access_tokens SET updated_at_ms=? WHERE id=?")
            .bind(Utc::now().timestamp_millis())
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

fn validate_name(name: &str) -> anyhow::Result<String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > TOKEN_NAME_MAX_CHARS {
        return Err(anyhow::anyhow!(
            "token name must contain 1 to 80 characters"
        ));
    }
    Ok(name.to_string())
}

fn ensure_changed(changed: u64) -> anyhow::Result<()> {
    if changed == 0 {
        Err(anyhow::anyhow!("access token not found"))
    } else {
        Ok(())
    }
}

fn timestamp(value: i64) -> anyhow::Result<DateTime<Utc>> {
    DateTime::<Utc>::from_timestamp_millis(value)
        .ok_or_else(|| anyhow::anyhow!("invalid access token timestamp"))
}

fn restrict_file_permissions(path: &std::path::Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}
