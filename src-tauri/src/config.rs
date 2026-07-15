use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub version: u16,
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub settings: SettingsConfig,
    #[serde(default = "default_tool_configs")]
    pub tools: Vec<ToolConfig>,
    #[serde(default)]
    pub connections: Vec<ConnectionConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub require_token: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingsConfig {
    #[serde(default = "default_audit_max_events")]
    pub audit_max_events: usize,
    // Enable this explicitly when audit logs must not retain SQL literal values.
    #[serde(default)]
    pub audit_redact_sql_literals: bool,
    #[serde(default = "default_true")]
    pub auto_check_updates: bool,
    #[serde(default = "default_language")]
    pub language: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolConfig {
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DbKind {
    Sqlite,
    Mysql,
    Postgres,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionConfig {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: DbKind,
    pub enabled: bool,
    pub database: String,
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub credential_ref: Option<String>,
    #[serde(default)]
    pub ssl_mode: Option<String>,
    #[serde(default = "default_max_rows")]
    pub max_rows: u32,
    #[serde(default = "default_query_timeout_ms")]
    pub query_timeout_ms: u64,
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
}

pub struct ConfigStore {
    path: PathBuf,
}

impl ConfigStore {
    pub fn new(app: &AppHandle) -> anyhow::Result<Self> {
        let dir = app.path().app_config_dir()?;
        fs::create_dir_all(&dir)?;
        Ok(Self {
            path: dir.join("config.toml"),
        })
    }

    pub fn load(&self) -> anyhow::Result<AppConfig> {
        if !self.path.exists() {
            let config = AppConfig::default();
            self.save(&config)?;
            return Ok(config);
        }

        let text = fs::read_to_string(&self.path)?;
        let mut config = toml::from_str::<AppConfig>(&text)?;
        config.normalize_and_validate()?;
        Ok(config)
    }

    pub fn save(&self, config: &AppConfig) -> anyhow::Result<()> {
        let mut candidate = config.clone();
        candidate.normalize_and_validate()?;
        atomic_write(&self.path, toml::to_string_pretty(&candidate)?.as_bytes())
    }

    #[cfg(test)]
    pub(crate) fn for_test(path: PathBuf) -> Self {
        Self { path }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: 1,
            server: ServerConfig::default(),
            settings: SettingsConfig::default(),
            tools: default_tool_configs(),
            connections: Vec::new(),
        }
    }
}

impl AppConfig {
    pub fn normalize(&mut self) {
        normalize_tool_configs(&mut self.tools);
        normalize_settings(&mut self.settings);
    }

    pub fn normalize_and_validate(&mut self) -> anyhow::Result<()> {
        if self.version != 1 {
            return Err(anyhow::anyhow!(
                "unsupported config version: {}",
                self.version
            ));
        }

        self.server.host = self.server.host.trim().to_ascii_lowercase();
        if !matches!(self.server.host.as_str(), "127.0.0.1" | "localhost") {
            return Err(anyhow::anyhow!("MCP host must be 127.0.0.1 or localhost"));
        }
        if self.server.port == 0 {
            return Err(anyhow::anyhow!("MCP port must be between 1 and 65535"));
        }

        self.normalize();
        self.settings.audit_max_events = self
            .settings
            .audit_max_events
            .clamp(1, crate::audit::MAX_AUDIT_MAX_EVENTS);

        let mut connection_ids = HashSet::new();
        for connection in &mut self.connections {
            normalize_connection(connection)?;
            if !connection_ids.insert(connection.id.clone()) {
                return Err(anyhow::anyhow!(
                    "duplicate connection id: {}",
                    connection.id
                ));
            }
        }
        Ok(())
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 17321,
            require_token: true,
            token: None,
        }
    }
}

impl Default for SettingsConfig {
    fn default() -> Self {
        Self {
            audit_max_events: default_audit_max_events(),
            audit_redact_sql_literals: false,
            auto_check_updates: true,
            language: default_language(),
        }
    }
}

pub fn default_port(kind: &DbKind) -> Option<u16> {
    match kind {
        DbKind::Sqlite => None,
        DbKind::Mysql => Some(3306),
        DbKind::Postgres => Some(5432),
    }
}

fn default_max_rows() -> u32 {
    500
}

fn default_query_timeout_ms() -> u64 {
    8000
}

fn default_max_connections() -> u32 {
    1
}

fn default_audit_max_events() -> usize {
    300
}

fn default_language() -> String {
    "zh-CN".to_string()
}

fn normalize_settings(settings: &mut SettingsConfig) {
    settings.language = settings.language.trim().to_string();
    if settings.language.is_empty() {
        settings.language = default_language();
    }
}

pub const MCP_TOOL_NAMES: [&str; 7] = [
    "datanexa_list_connections",
    "datanexa_get_schema",
    "datanexa_describe_table",
    "datanexa_sample_rows",
    "datanexa_execute_readonly_sql",
    "datanexa_explain_sql",
    "datanexa_policy_check",
];

pub fn is_known_tool(name: &str) -> bool {
    MCP_TOOL_NAMES.contains(&name)
}

pub fn is_tool_enabled(tools: &[ToolConfig], name: &str) -> bool {
    tools
        .iter()
        .find(|tool| tool.name == name)
        .map(|tool| tool.enabled)
        .unwrap_or_else(|| is_known_tool(name))
}

pub fn normalize_tool_configs(tools: &mut Vec<ToolConfig>) {
    let known = MCP_TOOL_NAMES.iter().copied().collect::<HashSet<&str>>();
    let mut seen = HashSet::new();
    tools.retain(|tool| known.contains(tool.name.as_str()) && seen.insert(tool.name.clone()));

    for name in MCP_TOOL_NAMES {
        if !tools.iter().any(|tool| tool.name == name) {
            tools.push(ToolConfig {
                name: name.to_string(),
                enabled: true,
            });
        }
    }
}

fn default_tool_configs() -> Vec<ToolConfig> {
    MCP_TOOL_NAMES
        .iter()
        .map(|name| ToolConfig {
            name: (*name).to_string(),
            enabled: true,
        })
        .collect()
}

fn default_true() -> bool {
    true
}

fn normalize_connection(connection: &mut ConnectionConfig) -> anyhow::Result<()> {
    connection.id = connection.id.trim().to_string();
    connection.name = connection.name.trim().to_string();
    connection.database = connection.database.trim().to_string();
    connection.host = connection
        .host
        .take()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    connection.username = connection
        .username
        .take()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    connection.ssl_mode = connection
        .ssl_mode
        .take()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty());
    connection.max_rows = connection.max_rows.clamp(1, 5000);
    connection.query_timeout_ms = connection.query_timeout_ms.clamp(500, 60_000);
    connection.max_connections = connection.max_connections.clamp(1, 3);

    if connection.id.is_empty()
        || !connection.id.chars().enumerate().all(|(index, value)| {
            (index > 0 && value.is_ascii_digit())
                || value.is_ascii_alphabetic()
                || value == '_'
                || (index > 0 && value == '-')
        })
        || connection.id.len() > 64
    {
        return Err(anyhow::anyhow!("invalid connection id"));
    }
    if connection.name.is_empty() || connection.database.is_empty() {
        return Err(anyhow::anyhow!("connection name and database are required"));
    }

    match connection.kind {
        DbKind::Sqlite => {
            connection.credential_ref = None;
            connection.host = None;
            connection.port = None;
            connection.username = None;
            connection.ssl_mode = None;
        }
        DbKind::Mysql | DbKind::Postgres => {
            if let Some(credential_ref) = connection.credential_ref.as_deref() {
                let expected = crate::vault::CredentialVault::credential_ref(&connection.id);
                if credential_ref != expected {
                    return Err(anyhow::anyhow!(
                        "credential_ref must match the connection id"
                    ));
                }
            }
            if connection.host.is_none() {
                return Err(anyhow::anyhow!("database host is required"));
            }
            if connection.port.is_none() {
                connection.port = default_port(&connection.kind);
            }
            let valid_ssl = match connection.kind {
                DbKind::Mysql => matches!(
                    connection.ssl_mode.as_deref().unwrap_or("preferred"),
                    "disable"
                        | "disabled"
                        | "prefer"
                        | "preferred"
                        | "require"
                        | "required"
                        | "verify_ca"
                        | "verify-ca"
                        | "verify_identity"
                        | "verify-identity"
                ),
                DbKind::Postgres => matches!(
                    connection.ssl_mode.as_deref().unwrap_or("prefer"),
                    "disable"
                        | "disabled"
                        | "allow"
                        | "prefer"
                        | "preferred"
                        | "require"
                        | "required"
                        | "verify_ca"
                        | "verify-ca"
                        | "verify_full"
                        | "verify-full"
                ),
                DbKind::Sqlite => true,
            };
            if !valid_ssl {
                return Err(anyhow::anyhow!("unsupported database ssl_mode"));
            }
        }
    }
    Ok(())
}

pub(crate) fn atomic_write(path: &std::path::Path, bytes: &[u8]) -> anyhow::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("configuration path has no parent directory"))?;
    fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        temporary
            .as_file()
            .set_permissions(fs::Permissions::from_mode(0o600))?;
    }

    temporary.write_all(bytes)?;
    temporary.as_file_mut().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;

    #[cfg(unix)]
    fs::set_permissions(path, {
        use std::os::unix::fs::PermissionsExt;
        fs::Permissions::from_mode(0o600)
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_server_fields_are_ignored() {
        let input = r#"
version = 1
[server]
host = "127.0.0.1"
port = 17321
streamable_http = true
legacy_sse_compat = true
require_token = true
"#;
        let mut config: AppConfig = toml::from_str(input).expect("legacy config parses");
        config
            .normalize_and_validate()
            .expect("legacy fields are ignored");
        let serialized = toml::to_string(&config).expect("config serializes");
        assert!(!serialized.contains("legacy_sse"));
        assert!(!serialized.contains("streamable_http"));
    }

    #[test]
    fn invalid_runtime_values_are_normalized_or_rejected() {
        let mut config = AppConfig::default();
        config.server.port = 0;
        assert!(config.normalize_and_validate().is_err());

        let mut config = AppConfig::default();
        config.settings.audit_max_events = usize::MAX;
        config
            .normalize_and_validate()
            .expect("valid defaults normalize");
        assert_eq!(
            config.settings.audit_max_events,
            crate::audit::MAX_AUDIT_MAX_EVENTS
        );
    }

    #[test]
    fn credential_references_must_belong_to_the_connection() {
        let mut config = AppConfig::default();
        config.connections.push(ConnectionConfig {
            id: "primary_db".to_string(),
            name: "Primary".to_string(),
            kind: DbKind::Postgres,
            enabled: true,
            database: "application".to_string(),
            host: Some("localhost".to_string()),
            port: Some(5432),
            username: Some("readonly".to_string()),
            credential_ref: Some("vault://other_db".to_string()),
            ssl_mode: None,
            max_rows: 100,
            query_timeout_ms: 1_000,
            max_connections: 1,
        });
        assert!(config.normalize_and_validate().is_err());

        config.connections[0].credential_ref = Some("vault://primary_db".to_string());
        config
            .normalize_and_validate()
            .expect("matching credential reference is valid");
    }

    #[test]
    fn legacy_sqlite_credential_reference_is_cleared_on_load() {
        let input = r#"
version = 1

[server]
host = "127.0.0.1"
port = 17321
require_token = true

[[connections]]
id = "local_data"
name = "Local Data"
type = "sqlite"
enabled = true
database = "local.db"
credential_ref = "vault://local_data"
max_rows = 100
query_timeout_ms = 1000
max_connections = 1
"#;
        let mut config: AppConfig = toml::from_str(input).expect("legacy config parses");
        config
            .normalize_and_validate()
            .expect("legacy SQLite config normalizes");
        assert!(config.connections[0].credential_ref.is_none());
    }
}
