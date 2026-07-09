use std::collections::HashSet;
use std::fs;
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
    pub streamable_http: bool,
    pub legacy_sse_compat: bool,
    pub require_token: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingsConfig {
    #[serde(default = "default_audit_max_events")]
    pub audit_max_events: usize,
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
        config.normalize();
        Ok(config)
    }

    pub fn save(&self, config: &AppConfig) -> anyhow::Result<()> {
        let text = toml::to_string_pretty(config)?;
        fs::write(&self.path, text)?;
        Ok(())
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
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 17321,
            streamable_http: true,
            legacy_sse_compat: true,
            require_token: true,
            token: None,
        }
    }
}

impl Default for SettingsConfig {
    fn default() -> Self {
        Self {
            audit_max_events: default_audit_max_events(),
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
