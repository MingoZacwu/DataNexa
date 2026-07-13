use tokio::sync::{Mutex, RwLock};

use crate::audit::AuditLogger;
use crate::config::{AppConfig, ConfigStore};
use crate::db::DatabaseManager;
use crate::mcp::McpRuntime;
use crate::vault::CredentialVault;

pub struct AppState {
    pub store: ConfigStore,
    pub config: RwLock<AppConfig>,
    pub config_transaction: RwLock<()>,
    pub vault: CredentialVault,
    pub audit: AuditLogger,
    pub db: DatabaseManager,
    pub mcp: RwLock<McpRuntime>,
    pub mcp_lifecycle: Mutex<()>,
}

impl AppState {
    pub fn new(app: tauri::AppHandle) -> anyhow::Result<Self> {
        let store = ConfigStore::new(&app)?;
        let config = store.load()?;
        let audit = AuditLogger::new(&app, config.settings.audit_max_events)?;

        Ok(Self {
            store,
            config: RwLock::new(config),
            config_transaction: RwLock::new(()),
            vault: CredentialVault::new(),
            audit,
            db: DatabaseManager::default(),
            mcp: RwLock::new(McpRuntime::default()),
            mcp_lifecycle: Mutex::new(()),
        })
    }
}
