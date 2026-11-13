use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::RwLock as StdRwLock;

use tokio::sync::{Mutex, RwLock};
use tokio_util::sync::CancellationToken;

use crate::access_control::AccessControlStore;
use crate::audit::AuditLogger;
use crate::config::{AppConfig, ConfigLoadFailure, ConfigStore};
use crate::db::DatabaseManager;
use crate::jdbc::JdbcManager;
use crate::mcp::McpRuntime;
use crate::vault::CredentialVault;
use crate::write_operations::WriteManager;

pub struct AppState {
    pub app_handle: Option<tauri::AppHandle>,
    pub store: ConfigStore,
    pub config: RwLock<AppConfig>,
    /// Set when the persisted configuration could not be parsed. The app keeps
    /// running on defaults so the front end can show a recovery screen; every
    /// startup side effect is skipped until the file is repaired. Cleared once
    /// a reset succeeds, so a failed relaunch cannot reset twice.
    pub config_error: StdRwLock<Option<ConfigLoadFailure>>,
    pub config_transaction: RwLock<()>,
    pub snapshot_sequence: Mutex<u64>,
    pub vault: CredentialVault,
    pub audit: AuditLogger,
    pub access: AccessControlStore,
    pub db: DatabaseManager,
    pub jdbc: JdbcManager,
    pub writes: WriteManager,
    pub execution_limits: crate::mcp::ExecutionLimits,
    pub jdbc_lifecycle: Mutex<()>,
    pub mcp: RwLock<McpRuntime>,
    pub mcp_lifecycle: Mutex<()>,
    pub mcp_cancellation: RwLock<CancellationToken>,
    pub emergency_disabled: AtomicBool,
}

impl AppState {
    pub async fn new(app: tauri::AppHandle) -> anyhow::Result<Self> {
        let store = ConfigStore::new(&app)?;
        // Startup failures are exactly when the debug log is needed, so a
        // broken config force-enables logging even though the user preference
        // cannot be read at this point.
        let mut config_error = None;
        let mut config = match store.load() {
            Ok(config) => config,
            Err(error) => {
                crate::debug_log::set_enabled(true);
                let message = crate::commands::sanitize_error(&error);
                crate::debug_log::error(
                    "startup",
                    format_args!(
                        "failed to load configuration at {}: {message}",
                        store.path().display()
                    ),
                );
                // Keep the app alive on defaults. The front end renders a
                // recovery screen instead of the workspace, and the startup
                // path skips every side effect that would otherwise treat the
                // defaults as the user's real preferences.
                config_error = Some(ConfigLoadFailure {
                    message,
                    path: store.path().display().to_string(),
                });
                AppConfig::default()
            }
        };
        let audit = AuditLogger::new(&app)?;
        let access = AccessControlStore::new(&app)?;
        access.initialize(&store, &mut config).await?;
        let writes = match WriteManager::new(&app).await {
            Ok(manager) => manager,
            Err(_) => {
                crate::debug_log::error(
                    "write",
                    format_args!("write journal unavailable; restricted writes disabled"),
                );
                WriteManager::unavailable()
            }
        };

        Ok(Self {
            app_handle: Some(app.clone()),
            store,
            config: RwLock::new(config),
            config_error: StdRwLock::new(config_error),
            config_transaction: RwLock::new(()),
            snapshot_sequence: Mutex::new(0),
            vault: CredentialVault::new(),
            audit,
            access,
            db: DatabaseManager::default(),
            jdbc: JdbcManager::new(app),
            writes,
            execution_limits: crate::mcp::ExecutionLimits::default(),
            jdbc_lifecycle: Mutex::new(()),
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
        if let Err(error) = self.writes.invalidate_all().await {
            crate::debug_log::error(
                "write",
                format_args!("failed to invalidate write requests (code={})", error.code),
            );
        }
    }

    pub async fn close_database_sessions(&self) {
        self.cancel_mcp_requests().await;
        tokio::join!(self.db.close_all(), self.jdbc.shutdown_sessions());
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

    /// Snapshot of the recorded configuration failure, if any.
    pub fn config_error(&self) -> Option<ConfigLoadFailure> {
        self.config_error
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// Clears the recorded failure after a successful reset.
    pub fn clear_config_error(&self) {
        *self
            .config_error
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
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
