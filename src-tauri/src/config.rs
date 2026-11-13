use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

/// Describes why the persisted configuration could not be loaded. The
/// application still starts with defaults so the front end can render a
/// recovery screen instead of exiting without any window.
#[derive(Debug, Clone, Serialize)]
pub struct ConfigLoadFailure {
    pub message: String,
    pub path: String,
}

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
    #[serde(default = "default_audit_retention_days")]
    pub audit_retention_days: u32,
    // Enable this explicitly when audit logs must not retain SQL literal values.
    #[serde(default)]
    pub audit_redact_sql_literals: bool,
    #[serde(default = "default_true")]
    pub auto_check_updates: bool,
    // Route update checks plus installer and Java runtime downloads through
    // the fixed gh-proxy.com accelerator. Off by default so existing installs
    // keep their direct connection behavior.
    #[serde(default)]
    pub github_acceleration: bool,
    #[serde(default)]
    pub auto_start_mcp: bool,
    // Start the MCP server on every application launch, including manual
    // launches. Launch-at-login implies this behavior regardless of the stored
    // value, which the startup path and the UI derive with a logical OR.
    #[serde(default)]
    pub auto_run_mcp: bool,
    #[serde(default)]
    pub auto_lightweight_mode: bool,
    #[serde(default = "default_true")]
    pub mcp_activity_effects: bool,
    #[serde(default = "default_language")]
    pub language: String,
    /// Explicitly selected external Java home. When unset, the bundled runtime is always used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jdbc_java_home: Option<String>,
    // Auto-disable an access token once it accumulates enough denied audit
    // events within the configured window. Requires bearer authentication.
    #[serde(default)]
    pub auto_circuit_breaker: bool,
    #[serde(default = "default_circuit_breaker_window_minutes")]
    pub auto_circuit_breaker_window_minutes: u32,
    #[serde(default = "default_circuit_breaker_threshold")]
    pub auto_circuit_breaker_threshold: u32,
    // Runtime debug logging writes sanitized internal diagnostics to a
    // rotating file under the app log directory. Disabled by default so no
    // log files are created for regular usage.
    #[serde(default)]
    pub debug_logging_enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolConfig {
    pub name: String,
    pub enabled: bool,
}

impl<'de> Deserialize<'de> for ToolConfig {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Input {
            name: String,
            enabled: Option<bool>,
        }
        let input = Input::deserialize(deserializer)?;
        Ok(Self {
            enabled: input
                .enabled
                .unwrap_or_else(|| default_tool_enabled(&input.name)),
            name: input.name,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WriteTable {
    pub schema: String,
    pub table: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DbKind {
    Sqlite,
    Mysql,
    Postgres,
    Jdbc,
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
    #[serde(default)]
    pub jdbc_bundle_id: Option<String>,
    #[serde(default)]
    pub jdbc_url: Option<String>,
    #[serde(default)]
    pub jdbc_driver_class: Option<String>,
    #[serde(default = "default_max_rows")]
    pub max_rows: u32,
    #[serde(default = "default_query_timeout_ms")]
    pub query_timeout_ms: u64,
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
    #[serde(default = "default_max_result_bytes")]
    pub max_result_bytes: usize,
    #[serde(default)]
    pub write_enabled: bool,
    #[serde(default)]
    pub write_tables: Vec<WriteTable>,
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
            crate::debug_log::info(
                "config",
                format_args!("no configuration found, writing defaults"),
            );
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

    /// Absolute path of the persisted configuration file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Copies the unreadable configuration next to itself before a reset so the
    /// user can still recover hand-written content. Returns `None` when there
    /// is no file to preserve (for example an unreadable directory entry).
    pub fn backup_corrupt(&self) -> anyhow::Result<Option<PathBuf>> {
        if !self.path.exists() {
            return Ok(None);
        }
        let parent = self
            .path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("configuration path has no parent directory"))?;
        let stem = self
            .path
            .file_stem()
            .map(|value| value.to_string_lossy().to_string())
            .unwrap_or_else(|| "config".to_string());
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");

        // A second reset within the same second must not overwrite the first
        // backup, so fall back to a numeric suffix.
        for attempt in 0..100 {
            let name = if attempt == 0 {
                format!("{stem}.corrupt-{stamp}.toml")
            } else {
                format!("{stem}.corrupt-{stamp}-{attempt}.toml")
            };
            let candidate = parent.join(name);
            if candidate.exists() {
                continue;
            }
            fs::copy(&self.path, &candidate)?;
            return Ok(Some(candidate));
        }
        Err(anyhow::anyhow!(
            "unable to allocate a unique backup file name"
        ))
    }

    /// Replaces the persisted configuration with defaults. Only called after a
    /// failed load, and only after the caller has backed the original up.
    pub fn reset_to_default(&self) -> anyhow::Result<AppConfig> {
        let config = AppConfig::default();
        self.save(&config)?;
        Ok(config)
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
        self.settings.audit_retention_days =
            crate::audit::normalize_retention_days(self.settings.audit_retention_days);

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
            audit_retention_days: default_audit_retention_days(),
            audit_redact_sql_literals: false,
            auto_check_updates: true,
            github_acceleration: false,
            auto_start_mcp: false,
            auto_run_mcp: false,
            auto_lightweight_mode: false,
            mcp_activity_effects: true,
            language: default_language(),
            jdbc_java_home: None,
            auto_circuit_breaker: false,
            auto_circuit_breaker_window_minutes: default_circuit_breaker_window_minutes(),
            auto_circuit_breaker_threshold: default_circuit_breaker_threshold(),
            debug_logging_enabled: false,
        }
    }
}

pub fn default_port(kind: &DbKind) -> Option<u16> {
    match kind {
        DbKind::Sqlite => None,
        DbKind::Mysql => Some(3306),
        DbKind::Postgres => Some(5432),
        DbKind::Jdbc => None,
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

pub const MIN_RESULT_BYTES: usize = 64 * 1024;
pub const MAX_RESULT_BYTES: usize = 8 * 1024 * 1024;

pub fn default_max_result_bytes() -> usize {
    1024 * 1024
}

fn default_audit_retention_days() -> u32 {
    crate::audit::DEFAULT_AUDIT_RETENTION_DAYS
}

fn default_language() -> String {
    // Empty means "follow the system locale". The frontend resolves it on
    // startup and persists an explicit choice once the user picks one.
    String::new()
}

fn default_circuit_breaker_window_minutes() -> u32 {
    10
}

fn default_circuit_breaker_threshold() -> u32 {
    5
}

fn normalize_settings(settings: &mut SettingsConfig) {
    // Only an explicit choice is persisted; anything else means "follow the
    // system", which the frontend resolves from the OS locale.
    settings.language = settings.language.trim().to_string();
    if settings.language != "zh-CN" && settings.language != "en" {
        settings.language.clear();
    }
    settings.jdbc_java_home = settings
        .jdbc_java_home
        .take()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    settings.auto_circuit_breaker_window_minutes =
        settings.auto_circuit_breaker_window_minutes.clamp(1, 60);
    settings.auto_circuit_breaker_threshold = settings.auto_circuit_breaker_threshold.clamp(1, 50);
}

pub const WRITE_SUBMIT_TOOL: &str = "datanexa_submit_write";
pub const WRITE_STATUS_TOOL: &str = "datanexa_get_write_status";
pub const WRITE_TARGETS_TOOL: &str = "datanexa_list_write_targets";

pub const MCP_TOOL_NAMES: [&str; 10] = [
    "datanexa_list_connections",
    "datanexa_get_schema",
    "datanexa_describe_table",
    "datanexa_sample_rows",
    "datanexa_execute_readonly_sql",
    "datanexa_explain_sql",
    "datanexa_policy_check",
    WRITE_SUBMIT_TOOL,
    WRITE_STATUS_TOOL,
    WRITE_TARGETS_TOOL,
];

pub fn default_tool_enabled(name: &str) -> bool {
    is_known_tool(name) && name != WRITE_SUBMIT_TOOL
}

pub fn is_known_tool(name: &str) -> bool {
    MCP_TOOL_NAMES.contains(&name)
}

pub fn is_tool_enabled(tools: &[ToolConfig], name: &str) -> bool {
    tools
        .iter()
        .find(|tool| tool.name == name)
        .map(|tool| tool.enabled)
        .unwrap_or_else(|| default_tool_enabled(name))
}

pub fn normalize_tool_configs(tools: &mut Vec<ToolConfig>) {
    let known = MCP_TOOL_NAMES.iter().copied().collect::<HashSet<&str>>();
    let mut seen = HashSet::new();
    tools.retain(|tool| known.contains(tool.name.as_str()) && seen.insert(tool.name.clone()));

    for name in MCP_TOOL_NAMES {
        if !tools.iter().any(|tool| tool.name == name) {
            tools.push(ToolConfig {
                name: name.to_string(),
                enabled: default_tool_enabled(name),
            });
        }
    }
}

fn default_tool_configs() -> Vec<ToolConfig> {
    MCP_TOOL_NAMES
        .iter()
        .map(|name| ToolConfig {
            name: (*name).to_string(),
            enabled: default_tool_enabled(name),
        })
        .collect()
}

fn default_true() -> bool {
    true
}

/// Applies a persisted numeric limit and records any correction, so "why did
/// the value I entered change" is answerable from the debug log.
pub(crate) fn clamp_with_log<T>(id: &str, field: &str, requested: T, min: T, max: T) -> T
where
    T: PartialOrd + PartialEq + Copy + std::fmt::Display,
{
    let applied = if requested < min {
        min
    } else if requested > max {
        max
    } else {
        requested
    };
    if applied != requested {
        crate::debug_log::warn(
            "config",
            format_args!(
                "connection setting clamped (id={id}, field={field}, requested={requested}, applied={applied})"
            ),
        );
    }
    applied
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
    connection.jdbc_bundle_id = connection
        .jdbc_bundle_id
        .take()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    connection.jdbc_url = connection
        .jdbc_url
        .take()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    connection.jdbc_driver_class = connection
        .jdbc_driver_class
        .take()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    connection.max_rows = clamp_with_log(&connection.id, "max_rows", connection.max_rows, 1, 5000);
    connection.query_timeout_ms = clamp_with_log(
        &connection.id,
        "query_timeout_ms",
        connection.query_timeout_ms,
        500,
        60_000,
    );
    connection.max_connections = clamp_with_log(
        &connection.id,
        "max_connections",
        connection.max_connections,
        1,
        3,
    );
    connection.max_result_bytes = clamp_with_log(
        &connection.id,
        "max_result_bytes",
        connection.max_result_bytes,
        MIN_RESULT_BYTES,
        MAX_RESULT_BYTES,
    );

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
    if connection.name.is_empty() {
        return Err(anyhow::anyhow!("connection name is required"));
    }
    if connection.kind == DbKind::Jdbc {
        connection.write_enabled = false;
        connection.write_tables.clear();
    }
    if connection.write_tables.len() > 64 {
        return Err(anyhow::anyhow!(
            "at most 64 writable tables may be configured"
        ));
    }
    let mut write_tables = HashSet::new();
    for target in &mut connection.write_tables {
        target.schema = target.schema.trim().to_string();
        target.table = target.table.trim().to_string();
        // Names come from server metadata. They only have to be quotable and
        // bounded here; refusing non-ASCII names would reject valid tables.
        let valid = |value: &str| {
            !value.is_empty()
                && value.chars().count() <= 128
                && !value.chars().any(|character| character.is_control())
        };
        if !valid(&target.schema) || !valid(&target.table) {
            return Err(anyhow::anyhow!(
                "writable table schema and name must be non-empty identifiers"
            ));
        }
        if connection.kind == DbKind::Sqlite && target.schema != "main" {
            return Err(anyhow::anyhow!(
                "SQLite writes only support the main schema"
            ));
        }
        if target.target_id.as_ref().is_some_and(|id| {
            id.len() != 67
                || !id.starts_with("wt_")
                || !id[3..].bytes().all(|byte| byte.is_ascii_hexdigit())
        }) {
            return Err(anyhow::anyhow!("invalid writable target identity"));
        }
        if !write_tables.insert((target.schema.clone(), target.table.clone())) {
            return Err(anyhow::anyhow!("duplicate writable table"));
        }
    }

    match connection.kind {
        DbKind::Sqlite => {
            if connection.database.is_empty() {
                return Err(anyhow::anyhow!("connection database is required"));
            }
            connection.credential_ref = None;
            connection.host = None;
            connection.port = None;
            connection.username = None;
            connection.ssl_mode = None;
            connection.jdbc_bundle_id = None;
            connection.jdbc_url = None;
            connection.jdbc_driver_class = None;
        }
        DbKind::Mysql | DbKind::Postgres => {
            if connection.database.is_empty() {
                return Err(anyhow::anyhow!("connection database is required"));
            }
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
                DbKind::Sqlite | DbKind::Jdbc => true,
            };
            if !valid_ssl {
                return Err(anyhow::anyhow!("unsupported database ssl_mode"));
            }
            connection.jdbc_bundle_id = None;
            connection.jdbc_url = None;
            connection.jdbc_driver_class = None;
        }
        DbKind::Jdbc => {
            if let Some(credential_ref) = connection.credential_ref.as_deref() {
                let expected = crate::vault::CredentialVault::credential_ref(&connection.id);
                if credential_ref != expected {
                    return Err(anyhow::anyhow!(
                        "credential_ref must match the connection id"
                    ));
                }
            }
            if connection.jdbc_bundle_id.is_none() {
                return Err(anyhow::anyhow!("JDBC driver bundle is required"));
            }
            if !connection
                .jdbc_url
                .as_deref()
                .is_some_and(|value| value.starts_with("jdbc:"))
            {
                return Err(anyhow::anyhow!("a valid JDBC URL is required"));
            }
            connection.database.clear();
            connection.host = None;
            connection.port = None;
            connection.ssl_mode = None;
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
        config.settings.audit_retention_days = 10;
        config
            .normalize_and_validate()
            .expect("valid defaults normalize");
        assert_eq!(
            config.settings.audit_retention_days,
            crate::audit::DEFAULT_AUDIT_RETENTION_DAYS
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
            jdbc_bundle_id: None,
            jdbc_url: None,
            jdbc_driver_class: None,
            max_rows: 100,
            query_timeout_ms: 1_000,
            max_connections: 1,
            max_result_bytes: default_max_result_bytes(),
            write_enabled: false,
            write_tables: Vec::new(),
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
