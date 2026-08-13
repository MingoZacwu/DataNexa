use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::{Mutex, RwLock};
use tokio_util::sync::CancellationToken;

use crate::access_control::AccessControlStore;
use crate::audit::AuditLogger;
use crate::config::{AppConfig, ConfigStore};
use crate::db::DatabaseManager;
use crate::mcp::McpRuntime;
use crate::vault::CredentialVault;

pub struct AppState {
    pub app_handle: Option<tauri::AppHandle>,
    pub store: ConfigStore,
    pub config: RwLock<AppConfig>,
    pub config_transaction: RwLock<()>,
    pub vault: CredentialVault,
    pub audit: AuditLogger,
    pub access: AccessControlStore,
    pub db: DatabaseManager,
    pub mcp: RwLock<McpRuntime>,
    pub mcp_lifecycle: Mutex<()>,
    pub mcp_cancellation: RwLock<CancellationToken>,
    pub emergency_disabled: AtomicBool,
}

impl AppState {
    pub async fn new(app: tauri::AppHandle) -> anyhow::Result<Self> {
        let store = ConfigStore::new(&app)?;
        let mut config = store.load()?;
        let audit = AuditLogger::new(&app, config.settings.audit_max_events)?;
        let access = AccessControlStore::new(&app)?;
        access.initialize(&store, &mut config).await?;

        Ok(Self {
            app_handle: Some(app),
            store,
            config: RwLock::new(config),
            config_transaction: RwLock::new(()),
            vault: CredentialVault::new(),
            audit,
            access,
            db: DatabaseManager::default(),
            mcp: RwLock::new(McpRuntime::default()),
            mcp_lifecycle: Mutex::new(()),
            mcp_cancellation: RwLock::new(CancellationToken::new()),
            emergency_disabled: AtomicBool::new(false),
        })
    }

    pub async fn mcp_cancellation_token(&self) -> CancellationToken {
        self.mcp_cancellation.read().await.clone()
    }

    pub async fn cancel_mcp_requests(&self) {
        self.mcp_cancellation.read().await.cancel();
    }

    pub async fn reset_mcp_cancellation(&self) {
        let mut cancellation = self.mcp_cancellation.write().await;
        if cancellation.is_cancelled() {
            *cancellation = CancellationToken::new();
        }
    }

    pub fn is_emergency_disabled(&self) -> bool {
        self.emergency_disabled.load(Ordering::Acquire)
    }

    pub async fn enter_emergency_mode(&self) {
        self.emergency_disabled.store(true, Ordering::Release);
        self.cancel_mcp_requests().await;
    }

    pub async fn exit_emergency_mode(&self) {
        self.reset_mcp_cancellation().await;
        self.emergency_disabled.store(false, Ordering::Release);
    }
}
