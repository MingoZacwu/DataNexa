use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use regex::Regex;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State, WebviewWindow};
use uuid::Uuid;
use zeroize::{Zeroize, Zeroizing};

use crate::audit::AuditStatus;
use crate::config::{
    default_port, is_known_tool, AppConfig, ConnectionConfig, DbKind, ServerConfig, SettingsConfig,
};
use crate::db::ConnectionDiagnostics;
use crate::i18n::{backend_text, BackendText, ConnectionDiagnosticText};
use crate::mcp::{self, McpToolInfo, ServerStatus};
use crate::policy::{PolicyCheckResult, PolicyEngine};
use crate::state::AppState;
use crate::vault::CredentialVault;
use crate::{hide_main_window_to_tray, refresh_tray_menu};

#[derive(Debug, Clone, Serialize)]
pub struct AppSnapshot {
    pub config: AppConfig,
    pub server_status: ServerStatus,
    pub audit_events: Vec<crate::audit::AuditEvent>,
    pub tools: Vec<McpToolInfo>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConnectionInput {
    pub connection: ConnectionConfig,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub clear_password: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct ConnectionTransferFile {
    format: String,
    version: u16,
    exported_at: String,
    connections: Vec<PortableConnection>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PortableConnection {
    name: String,
    #[serde(rename = "type")]
    kind: DbKind,
    enabled: bool,
    database: String,
    #[serde(default)]
    host: Option<String>,
    #[serde(default)]
    port: Option<u16>,
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    password: Option<String>,
    #[serde(default)]
    ssl_mode: Option<String>,
    max_rows: u32,
    query_timeout_ms: u64,
    max_connections: u32,
}

impl Drop for PortableConnection {
    fn drop(&mut self) {
        if let Some(password) = self.password.as_mut() {
            password.zeroize();
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ImportConnectionsResult {
    pub snapshot: AppSnapshot,
    pub imported_count: usize,
}

const CONNECTION_TRANSFER_FORMAT: &str = "datanexa-connections";
const CONNECTION_TRANSFER_VERSION: u16 = 1;
const MAX_CONNECTION_IMPORT_BYTES: u64 = 5 * 1024 * 1024;
const MAX_CONNECTION_IMPORT_COUNT: usize = 1000;

#[tauri::command]
pub async fn get_app_snapshot(state: State<'_, Arc<AppState>>) -> Result<AppSnapshot, String> {
    snapshot(state.inner()).await.map_err(to_client_error)
}

#[tauri::command]
pub async fn save_server_config(
    state: State<'_, Arc<AppState>>,
    server: ServerConfig,
) -> Result<AppSnapshot, String> {
    let text = text_for_state(state.inner()).await;
    if !is_local_host(&server.host) {
        return Err(text.local_host_only().to_string());
    }

    mcp::reconfigure(state.inner().clone(), server)
        .await
        .map_err(to_client_error)?;

    snapshot(state.inner()).await.map_err(to_client_error)
}

#[tauri::command]
pub async fn save_settings_config(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    settings: SettingsConfig,
) -> Result<AppSnapshot, String> {
    let text = commit_config(state.inner(), |config| {
        let settings = normalize_settings(settings);
        let text = backend_text(&settings.language);
        config.settings = settings;
        Ok(text)
    })
    .await
    .map_err(to_client_error)?;
    let audit_max_events = state.config.read().await.settings.audit_max_events;
    state
        .audit
        .trim(audit_max_events)
        .await
        .map_err(to_client_error)?;

    let mcp_running = mcp::status(state.inner()).await.running;
    refresh_tray_menu(&app, text, mcp_running).map_err(to_client_error)?;

    snapshot(state.inner()).await.map_err(to_client_error)
}

#[tauri::command]
pub async fn export_connections(
    state: State<'_, Arc<AppState>>,
    path: String,
) -> Result<usize, String> {
    let path = transfer_path(&path)?;
    let _transaction = state.config_transaction.read().await;
    let connections = state.config.read().await.connections.clone();
    let mut portable_connections = Vec::with_capacity(connections.len());

    for connection in connections {
        let password = match connection.credential_ref.as_deref() {
            Some(credential_ref) => Some(
                state
                    .vault
                    .get(credential_ref)
                    .map_err(to_client_error)?
                    .ok_or_else(|| {
                        "A connection references a saved password that is missing from the credential vault."
                            .to_string()
                    })?
                    .as_str()
                    .to_owned(),
            ),
            None => None,
        };

        portable_connections.push(PortableConnection {
            name: connection.name,
            kind: connection.kind,
            enabled: connection.enabled,
            database: connection.database,
            host: connection.host,
            port: connection.port,
            username: connection.username,
            password,
            ssl_mode: connection.ssl_mode,
            max_rows: connection.max_rows,
            query_timeout_ms: connection.query_timeout_ms,
            max_connections: connection.max_connections,
        });
    }

    let exported_count = portable_connections.len();
    let transfer = ConnectionTransferFile {
        format: CONNECTION_TRANSFER_FORMAT.to_string(),
        version: CONNECTION_TRANSFER_VERSION,
        exported_at: Utc::now().to_rfc3339(),
        connections: portable_connections,
    };
    let mut contents =
        Zeroizing::new(serde_json::to_vec_pretty(&transfer).map_err(to_client_error)?);
    contents.push(b'\n');
    fs::write(path, contents.as_slice()).map_err(to_client_error)?;

    Ok(exported_count)
}

#[tauri::command]
pub async fn import_connections(
    state: State<'_, Arc<AppState>>,
    path: String,
) -> Result<ImportConnectionsResult, String> {
    let path = transfer_path(&path)?;
    let metadata = fs::metadata(path).map_err(to_client_error)?;
    if metadata.len() > MAX_CONNECTION_IMPORT_BYTES {
        return Err("The connection import file is larger than 5 MB.".to_string());
    }

    let contents = Zeroizing::new(fs::read(path).map_err(to_client_error)?);
    let mut transfer: ConnectionTransferFile =
        serde_json::from_slice(contents.as_slice()).map_err(to_client_error)?;
    if transfer.format != CONNECTION_TRANSFER_FORMAT
        || transfer.version != CONNECTION_TRANSFER_VERSION
    {
        return Err("Unsupported DataNexa connection import file.".to_string());
    }
    if transfer.connections.len() > MAX_CONNECTION_IMPORT_COUNT {
        return Err("A connection import file can contain at most 1000 connections.".to_string());
    }

    let imported_count = transfer.connections.len();
    let _transaction = state.config_transaction.write().await;
    let mut candidate = state.config.read().await.clone();
    let text = backend_text(&candidate.settings.language);
    let mut existing_ids = candidate
        .connections
        .iter()
        .map(|connection| connection.id.clone())
        .collect::<HashSet<_>>();
    let mut credentials = Vec::new();

    for mut portable in transfer.connections.drain(..) {
        let id = next_import_connection_id(&mut existing_ids);
        let password = portable
            .password
            .take()
            .filter(|password| !password.is_empty())
            .map(Zeroizing::new);
        let credential_ref = password
            .as_ref()
            .map(|_| CredentialVault::credential_ref(&id));
        let connection = ConnectionConfig {
            id,
            name: std::mem::take(&mut portable.name),
            kind: portable.kind.clone(),
            enabled: portable.enabled,
            database: std::mem::take(&mut portable.database),
            host: portable.host.take(),
            port: portable.port,
            username: portable.username.take(),
            credential_ref: credential_ref.clone(),
            ssl_mode: portable.ssl_mode.take(),
            max_rows: portable.max_rows,
            query_timeout_ms: portable.query_timeout_ms,
            max_connections: portable.max_connections,
        };
        validate_connection(&connection, &text).map_err(to_client_error)?;
        candidate.connections.push(normalize_connection(connection));
        if let (Some(credential_ref), Some(password)) = (credential_ref, password) {
            credentials.push((credential_ref, password));
        }
    }
    candidate
        .normalize_and_validate()
        .map_err(to_client_error)?;

    let mut saved_credential_refs = Vec::with_capacity(credentials.len());
    for (credential_ref, password) in credentials {
        if let Err(error) = state.vault.put_secret(&credential_ref, password) {
            return Err(import_failure_with_rollback(
                "Credential import failed",
                error,
                &state.vault,
                &saved_credential_refs,
            ));
        }
        saved_credential_refs.push(credential_ref);
    }

    if let Err(error) = persist_config_candidate(&state.store, &state.config, candidate).await {
        return Err(import_failure_with_rollback(
            "Configuration import failed",
            error,
            &state.vault,
            &saved_credential_refs,
        ));
    }
    drop(_transaction);

    Ok(ImportConnectionsResult {
        snapshot: snapshot(state.inner()).await.map_err(to_client_error)?,
        imported_count,
    })
}

#[tauri::command]
pub async fn set_mcp_tool_enabled(
    state: State<'_, Arc<AppState>>,
    name: String,
    enabled: bool,
) -> Result<AppSnapshot, String> {
    let text = text_for_state(state.inner()).await;
    if !is_known_tool(&name) {
        return Err(text.unknown_mcp_tool(&name));
    }

    commit_config(state.inner(), |config| {
        config.normalize();
        if let Some(tool) = config.tools.iter_mut().find(|tool| tool.name == name) {
            tool.enabled = enabled;
        }
        Ok(())
    })
    .await
    .map_err(to_client_error)?;

    snapshot(state.inner()).await.map_err(to_client_error)
}

#[tauri::command]
pub async fn upsert_connection(
    state: State<'_, Arc<AppState>>,
    input: ConnectionInput,
) -> Result<AppSnapshot, String> {
    validate_password_input(input.clear_password, input.password.as_deref())?;
    let text = text_for_state(state.inner()).await;
    validate_connection(&input.connection, &text).map_err(to_client_error)?;
    let clear_password = input.clear_password;
    let mut connection = normalize_connection(input.connection);
    let password = input.password.filter(|value| !value.is_empty());
    let updates_credential = password.is_some();
    {
        let _transaction = state.config_transaction.write().await;
        let mut candidate = state.config.read().await.clone();
        let existing = candidate
            .connections
            .iter()
            .find(|existing| existing.id == connection.id)
            .cloned();
        let existing_ref = existing
            .as_ref()
            .and_then(|existing| existing.credential_ref.clone());
        let trusted_ref = existing_ref
            .clone()
            .unwrap_or_else(|| CredentialVault::credential_ref(&connection.id));

        connection.credential_ref = if clear_password {
            None
        } else if password.is_some() {
            Some(trusted_ref.clone())
        } else {
            existing_ref.clone()
        };

        let previous_credential = if password.is_some() {
            state.vault.get(&trusted_ref).map_err(to_client_error)?
        } else {
            None
        };

        if let Some(existing) = candidate
            .connections
            .iter_mut()
            .find(|existing| existing.id == connection.id)
        {
            *existing = connection.clone();
        } else {
            candidate.connections.push(connection.clone());
        }
        candidate
            .normalize_and_validate()
            .map_err(to_client_error)?;
        let credential_to_delete = candidate
            .connections
            .iter()
            .find(|candidate| candidate.id == connection.id)
            .and_then(|candidate| removed_credential_ref(existing_ref.clone(), candidate));

        if let Some(password) = password {
            if let Err(error) = state.vault.put(&trusted_ref, password) {
                let rollback = if let Some(previous) = previous_credential {
                    state.vault.put_secret(&trusted_ref, previous)
                } else {
                    state.vault.delete(&trusted_ref)
                };
                return Err(match rollback {
                    Ok(()) => to_client_error(error),
                    Err(rollback_error) => format!(
                        "Credential update failed: {}; credential rollback also failed: {}",
                        to_client_error(error),
                        to_client_error(rollback_error)
                    ),
                });
            }
        }

        if let Err(error) = persist_invalidating_candidate(
            &state.store,
            &state.config,
            &state.db,
            candidate,
            std::slice::from_ref(&connection.id),
        )
        .await
        {
            let rollback = if updates_credential {
                if let Some(previous) = previous_credential {
                    state.vault.put_secret(&trusted_ref, previous)
                } else {
                    state.vault.delete(&trusted_ref)
                }
            } else {
                Ok(())
            };
            if let Err(rollback_error) = rollback {
                return Err(format!(
                    "Configuration save failed: {}; credential rollback also failed: {}",
                    to_client_error(error),
                    to_client_error(rollback_error)
                ));
            }
            return Err(to_client_error(error));
        }
        if let Some(credential_ref) = credential_to_delete {
            state.vault.delete(&credential_ref).map_err(|error| {
                format!(
                    "Connection was saved without credentials, but the orphan credential could not be deleted: {}",
                    to_client_error(error)
                )
            })?;
        }
    }
    snapshot(state.inner()).await.map_err(to_client_error)
}

#[tauri::command]
pub async fn delete_connection(
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Result<AppSnapshot, String> {
    let _transaction = state.config_transaction.write().await;
    let mut candidate = state.config.read().await.clone();
    let credential_ref = candidate
        .connections
        .iter()
        .find(|connection| connection.id == id)
        .and_then(|connection| connection.credential_ref.clone());
    candidate
        .connections
        .retain(|connection| connection.id != id);
    candidate
        .normalize_and_validate()
        .map_err(to_client_error)?;
    persist_invalidating_candidate(
        &state.store,
        &state.config,
        &state.db,
        candidate,
        std::slice::from_ref(&id),
    )
    .await
    .map_err(to_client_error)?;
    if let Some(credential_ref) = credential_ref {
        state.vault.delete(&credential_ref).map_err(|error| {
            format!(
                "Connection was deleted, but its orphan credential could not be deleted: {}",
                to_client_error(error)
            )
        })?;
    }
    drop(_transaction);

    snapshot(state.inner()).await.map_err(to_client_error)
}

#[tauri::command]
pub async fn set_connection_enabled(
    state: State<'_, Arc<AppState>>,
    id: String,
    enabled: bool,
) -> Result<AppSnapshot, String> {
    let invalidation_ids = (!enabled)
        .then(|| id.clone())
        .into_iter()
        .collect::<Vec<_>>();
    commit_config_invalidating(state.inner(), &invalidation_ids, |config| {
        let Some(connection) = config
            .connections
            .iter_mut()
            .find(|connection| connection.id == id)
        else {
            return Err(anyhow::anyhow!("connection not found"));
        };

        connection.enabled = enabled;
        Ok(())
    })
    .await
    .map_err(to_client_error)?;

    snapshot(state.inner()).await.map_err(to_client_error)
}

#[tauri::command]
pub async fn disable_all_connections(
    state: State<'_, Arc<AppState>>,
) -> Result<AppSnapshot, String> {
    let _transaction = state.config_transaction.write().await;
    let mut candidate = state.config.read().await.clone();
    let connection_ids = candidate
        .connections
        .iter()
        .map(|connection| connection.id.clone())
        .collect::<Vec<_>>();
    for connection in &mut candidate.connections {
        connection.enabled = false;
    }
    candidate
        .normalize_and_validate()
        .map_err(to_client_error)?;
    persist_invalidating_candidate(
        &state.store,
        &state.config,
        &state.db,
        candidate,
        &connection_ids,
    )
    .await
    .map_err(to_client_error)?;
    drop(_transaction);

    snapshot(state.inner()).await.map_err(to_client_error)
}

#[tauri::command]
pub async fn clear_audit_events(state: State<'_, Arc<AppState>>) -> Result<AppSnapshot, String> {
    state.audit.clear().await.map_err(to_client_error)?;
    snapshot(state.inner()).await.map_err(to_client_error)
}

#[tauri::command]
pub async fn test_connection(
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Result<String, String> {
    let _config_transaction = state.config_transaction.read().await;
    let text = text_for_state(state.inner()).await;
    let connection = find_connection(state.inner(), &id)
        .await
        .map_err(to_client_error)?;
    state.db.close(&id).await;
    let started = Instant::now();
    let max_events = audit_limit(state.inner()).await;
    match state
        .db
        .test_connection(&connection, &state.vault, &text)
        .await
    {
        Ok(duration) => {
            state
                .audit
                .record_with_limit(
                    Some(id),
                    Some(connection.name.clone()),
                    "test_connection",
                    AuditStatus::Allowed,
                    None,
                    Some(duration.as_millis().try_into().unwrap_or(u64::MAX)),
                    None,
                    None,
                    max_events,
                )
                .await
                .map_err(|error| {
                    format!(
                        "Connection test succeeded, but audit storage is unavailable: {}",
                        to_client_error(error)
                    )
                })?;
            Ok(text.connection_test_ok(duration.as_millis()))
        }
        Err(error) => {
            state
                .audit
                .record_with_limit(
                    Some(id),
                    Some(connection.name.clone()),
                    "test_connection",
                    AuditStatus::Error,
                    Some(sanitize_error(&error)),
                    Some(started.elapsed().as_millis().try_into().unwrap_or(u64::MAX)),
                    None,
                    None,
                    max_events,
                )
                .await
                .map_err(|audit_error| {
                    format!(
                        "Connection test failed and audit storage is unavailable: {}; original result: {}",
                        to_client_error(audit_error),
                        to_client_error(&error)
                    )
                })?;
            let diagnostics = state.db.diagnostics(&connection, &state.vault, &text);
            Err(format!(
                "{}\n{}",
                to_client_error(error),
                format_diagnostics_for_client(&diagnostics, &text)
            ))
        }
    }
}

#[tauri::command]
pub async fn test_connection_input(
    state: State<'_, Arc<AppState>>,
    input: ConnectionInput,
) -> Result<String, String> {
    let _config_transaction = state.config_transaction.read().await;
    let text = text_for_state(state.inner()).await;
    validate_connection(&input.connection, &text).map_err(to_client_error)?;

    let clear_password = input.clear_password;
    let password = input.password;
    let mut connection = normalize_connection(input.connection);
    if clear_password {
        connection.credential_ref = None;
    }

    let started = Instant::now();
    let max_events = audit_limit(state.inner()).await;
    let connection_id = connection.id.clone();
    let password_override = if clear_password {
        None
    } else {
        password.as_deref().filter(|value| !value.is_empty())
    };

    match state
        .db
        .test_connection_once(&connection, &state.vault, password_override, &text)
        .await
    {
        Ok(duration) => {
            state
                .audit
                .record_with_limit(
                    Some(connection_id),
                    Some(connection.name.clone()),
                    "test_connection",
                    AuditStatus::Allowed,
                    None,
                    Some(duration.as_millis().try_into().unwrap_or(u64::MAX)),
                    None,
                    None,
                    max_events,
                )
                .await
                .map_err(|error| {
                    format!(
                        "Connection test succeeded, but audit storage is unavailable: {}",
                        to_client_error(error)
                    )
                })?;
            Ok(text.connection_test_ok(duration.as_millis()))
        }
        Err(error) => {
            state
                .audit
                .record_with_limit(
                    Some(connection_id),
                    Some(connection.name.clone()),
                    "test_connection",
                    AuditStatus::Error,
                    Some(sanitize_error(&error)),
                    Some(started.elapsed().as_millis().try_into().unwrap_or(u64::MAX)),
                    None,
                    None,
                    max_events,
                )
                .await
                .map_err(|audit_error| {
                    format!(
                        "Connection test failed and audit storage is unavailable: {}; original result: {}",
                        to_client_error(audit_error),
                        to_client_error(&error)
                    )
                })?;
            Err(to_client_error(&error))
        }
    }
}

#[tauri::command]
pub async fn diagnose_connection(
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Result<ConnectionDiagnostics, String> {
    let _config_transaction = state.config_transaction.read().await;
    let text = text_for_state(state.inner()).await;
    let connection = find_connection(state.inner(), &id)
        .await
        .map_err(to_client_error)?;
    Ok(state.db.diagnostics(&connection, &state.vault, &text))
}

#[tauri::command]
pub async fn start_mcp_server(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<AppSnapshot, String> {
    mcp::start(state.inner().clone())
        .await
        .map_err(to_client_error)?;
    let text = text_for_state(state.inner()).await;
    refresh_tray_menu(&app, text, true).map_err(to_client_error)?;
    snapshot(state.inner()).await.map_err(to_client_error)
}

#[tauri::command]
pub async fn stop_mcp_server(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<AppSnapshot, String> {
    mcp::stop(state.inner().clone()).await;
    let text = text_for_state(state.inner()).await;
    refresh_tray_menu(&app, text, false).map_err(to_client_error)?;
    snapshot(state.inner()).await.map_err(to_client_error)
}

#[tauri::command]
pub async fn rotate_server_token(state: State<'_, Arc<AppState>>) -> Result<AppSnapshot, String> {
    mcp::rotate_token(state.inner())
        .await
        .map_err(to_client_error)?;
    snapshot(state.inner()).await.map_err(to_client_error)
}

#[tauri::command]
pub async fn policy_check(
    state: State<'_, Arc<AppState>>,
    kind: DbKind,
    sql: String,
    max_rows: Option<u32>,
) -> Result<PolicyCheckResult, String> {
    let text = text_for_state(state.inner()).await;
    Ok(PolicyEngine::check_with_text(
        &kind,
        &sql,
        max_rows.unwrap_or(500).clamp(1, 5000),
        &text,
    ))
}

#[tauri::command]
pub fn minimize_main_window(window: WebviewWindow) -> Result<(), String> {
    window.minimize().map_err(to_client_error)
}

#[tauri::command]
pub fn hide_main_window(window: WebviewWindow) -> Result<(), String> {
    hide_main_window_to_tray(&window).map_err(to_client_error)
}

#[tauri::command]
pub fn start_window_drag(window: WebviewWindow) -> Result<(), String> {
    window.start_dragging().map_err(to_client_error)
}

#[tauri::command]
pub fn open_project_homepage() -> Result<(), String> {
    tauri_plugin_opener::open_url("https://github.com/MingoZacwu/DataNexa", None::<&str>)
        .map_err(to_client_error)
}

async fn snapshot(state: &Arc<AppState>) -> anyhow::Result<AppSnapshot> {
    let config = state.config.read().await.clone();
    state.audit.trim(config.settings.audit_max_events).await?;
    Ok(AppSnapshot {
        server_status: mcp::status(state).await,
        audit_events: state.audit.list().await,
        tools: mcp::tool_infos(&config.tools),
        config,
    })
}

async fn commit_config<T>(
    state: &Arc<AppState>,
    update: impl FnOnce(&mut AppConfig) -> anyhow::Result<T>,
) -> anyhow::Result<T> {
    let _transaction = state.config_transaction.write().await;
    let mut candidate = state.config.read().await.clone();
    let result = update(&mut candidate)?;
    candidate.normalize_and_validate()?;
    persist_config_candidate(&state.store, &state.config, candidate).await?;
    Ok(result)
}

async fn commit_config_invalidating<T>(
    state: &Arc<AppState>,
    connection_ids: &[String],
    update: impl FnOnce(&mut AppConfig) -> anyhow::Result<T>,
) -> anyhow::Result<T> {
    let _transaction = state.config_transaction.write().await;
    let mut candidate = state.config.read().await.clone();
    let result = update(&mut candidate)?;
    candidate.normalize_and_validate()?;
    persist_invalidating_candidate(
        &state.store,
        &state.config,
        &state.db,
        candidate,
        connection_ids,
    )
    .await?;
    Ok(result)
}

async fn persist_invalidating_candidate(
    store: &crate::config::ConfigStore,
    current: &tokio::sync::RwLock<AppConfig>,
    db: &crate::db::DatabaseManager,
    candidate: AppConfig,
    connection_ids: &[String],
) -> anyhow::Result<()> {
    store.save(&candidate)?;
    for connection_id in connection_ids {
        db.close(connection_id).await;
    }
    *current.write().await = candidate;
    Ok(())
}

async fn persist_config_candidate(
    store: &crate::config::ConfigStore,
    current: &tokio::sync::RwLock<AppConfig>,
    candidate: AppConfig,
) -> anyhow::Result<()> {
    store.save(&candidate)?;
    *current.write().await = candidate;
    Ok(())
}

async fn find_connection(state: &Arc<AppState>, id: &str) -> anyhow::Result<ConnectionConfig> {
    state
        .config
        .read()
        .await
        .connections
        .iter()
        .find(|connection| connection.id == id)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("connection not found"))
}

fn transfer_path(path: &str) -> Result<&Path, String> {
    let path = path.trim();
    if path.is_empty() {
        return Err("A connection import or export file must be selected.".to_string());
    }
    Ok(Path::new(path))
}

fn next_import_connection_id(existing_ids: &mut HashSet<String>) -> String {
    loop {
        let id = format!("connection_{}", Uuid::new_v4().simple());
        if existing_ids.insert(id.clone()) {
            return id;
        }
    }
}

fn import_failure_with_rollback(
    context: &str,
    error: anyhow::Error,
    vault: &CredentialVault,
    credential_refs: &[String],
) -> String {
    let mut rollback_error = None;
    for credential_ref in credential_refs.iter().rev() {
        if let Err(error) = vault.delete(credential_ref) {
            rollback_error.get_or_insert(error);
        }
    }

    match rollback_error {
        Some(rollback_error) => format!(
            "{}: {}; credential rollback also failed: {}",
            context,
            to_client_error(error),
            to_client_error(rollback_error)
        ),
        None => format!("{}: {}", context, to_client_error(error)),
    }
}

fn validate_connection(connection: &ConnectionConfig, text: &BackendText) -> anyhow::Result<()> {
    let id_re = Regex::new(r"^[A-Za-z_][A-Za-z0-9_-]{1,63}$").expect("valid connection id regex");
    if !id_re.is_match(&connection.id) {
        return Err(anyhow::anyhow!(text.connection_id_invalid()));
    }
    if connection.name.trim().is_empty() {
        return Err(anyhow::anyhow!(text.connection_name_required()));
    }
    if connection.database.trim().is_empty() {
        return Err(anyhow::anyhow!(text.database_required()));
    }
    if connection.kind != DbKind::Sqlite
        && connection
            .host
            .as_deref()
            .unwrap_or_default()
            .trim()
            .is_empty()
    {
        return Err(anyhow::anyhow!(text.host_required()));
    }
    Ok(())
}

fn normalize_connection(mut connection: ConnectionConfig) -> ConnectionConfig {
    connection.name = connection.name.trim().to_string();
    connection.database = connection.database.trim().to_string();
    connection.max_rows = connection.max_rows.clamp(1, 5000);
    connection.query_timeout_ms = connection.query_timeout_ms.clamp(500, 60000);
    connection.max_connections = connection.max_connections.clamp(1, 3);

    if connection.kind == DbKind::Sqlite {
        connection.host = None;
        connection.port = None;
        connection.username = None;
        connection.ssl_mode = None;
    } else if connection.port.is_none() {
        connection.port = default_port(&connection.kind);
    }

    connection.host = connection
        .host
        .map(|host| host.trim().to_string())
        .filter(|host| !host.is_empty());
    connection.username = connection
        .username
        .map(|username| username.trim().to_string())
        .filter(|username| !username.is_empty());
    connection.ssl_mode = connection
        .ssl_mode
        .map(|ssl_mode| ssl_mode.trim().to_ascii_lowercase())
        .filter(|ssl_mode| !ssl_mode.is_empty());

    connection
}

fn normalize_settings(mut settings: SettingsConfig) -> SettingsConfig {
    settings.audit_max_events = settings.audit_max_events.clamp(1, 5000);
    settings.language = settings.language.trim().to_string();
    if settings.language.is_empty() {
        settings.language = "zh-CN".to_string();
    }
    settings
}

async fn audit_limit(state: &Arc<AppState>) -> usize {
    state.config.read().await.settings.audit_max_events
}

async fn text_for_state(state: &Arc<AppState>) -> BackendText {
    let language = state.config.read().await.settings.language.clone();
    backend_text(&language)
}

fn to_client_error(error: impl std::fmt::Display) -> String {
    sanitize_text(&error.to_string())
}

fn sanitize_error(error: &anyhow::Error) -> String {
    sanitize_text(&error.to_string())
}

fn sanitize_text(text: &str) -> String {
    let text = Regex::new(r"(?i)(password|token|secret)=([^&\s]+)")
        .expect("valid secret sanitizer regex")
        .replace_all(text, "$1=REDACTED")
        .to_string();
    text.replace('\n', " ")
}

fn format_diagnostics_for_client(
    diagnostics: &ConnectionDiagnostics,
    text: &BackendText,
) -> String {
    let host = diagnostics.host.as_deref().unwrap_or("-");
    let port = diagnostics
        .port
        .map(|port| port.to_string())
        .unwrap_or_else(|| "-".to_string());
    let username = diagnostics.username.as_deref().unwrap_or("-");
    let ssl_mode = diagnostics.ssl_mode.as_deref().unwrap_or("default");
    let hint = diagnostics
        .hint
        .as_deref()
        .unwrap_or_else(|| text.no_extra_hint());

    text.diagnostics_for_client(ConnectionDiagnosticText {
        database_type: &diagnostics.database_type,
        host,
        port: &port,
        database: &diagnostics.database,
        username,
        credential: &diagnostics.credential_state,
        ssl_mode,
        timeout_ms: diagnostics.query_timeout_ms,
        pool_size: diagnostics.max_connections,
        hint,
    })
}

fn is_local_host(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "localhost")
}

fn validate_password_input(clear_password: bool, password: Option<&str>) -> Result<(), String> {
    if clear_password && password.is_some_and(|password| !password.is_empty()) {
        return Err("clear_password cannot be combined with a non-empty password".to_string());
    }
    Ok(())
}

fn removed_credential_ref(
    existing_ref: Option<String>,
    candidate: &ConnectionConfig,
) -> Option<String> {
    candidate
        .credential_ref
        .is_none()
        .then_some(existing_ref)
        .flatten()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn password_clear_and_update_are_mutually_exclusive() {
        assert!(validate_password_input(true, Some("TEST_SECRET")).is_err());
        assert!(validate_password_input(true, None).is_ok());
        assert!(validate_password_input(true, Some("")).is_ok());
        assert!(validate_password_input(false, Some("TEST_SECRET")).is_ok());
    }

    #[tokio::test]
    async fn failed_config_save_does_not_publish_candidate() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let invalid_target = directory.path().join("config-target");
        fs::create_dir(&invalid_target).expect("directory target");
        let store = crate::config::ConfigStore::for_test(invalid_target);
        let current = tokio::sync::RwLock::new(AppConfig::default());
        let mut candidate = AppConfig::default();
        candidate.server.require_token = false;

        assert!(persist_config_candidate(&store, &current, candidate)
            .await
            .is_err());
        assert!(current.read().await.server.require_token);
    }

    #[tokio::test]
    async fn failed_connection_mutation_does_not_invalidate_any_pool() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let invalid_target = directory.path().join("config-target");
        fs::create_dir(&invalid_target).expect("directory target");
        let store = crate::config::ConfigStore::for_test(invalid_target);
        let current = tokio::sync::RwLock::new(AppConfig::default());
        let db = crate::db::DatabaseManager::default();
        let ids = ["upsert", "delete", "disable", "disable_all"]
            .map(str::to_string)
            .to_vec();

        assert!(
            persist_invalidating_candidate(&store, &current, &db, AppConfig::default(), &ids,)
                .await
                .is_err()
        );
        for id in ids {
            assert_eq!(db.generation_for_test(&id).await, 0);
        }
        assert!(current.read().await.server.require_token);
    }

    #[test]
    fn remote_to_sqlite_transition_schedules_old_credential_cleanup() {
        let mut config = AppConfig::default();
        config.connections.push(ConnectionConfig {
            id: "transition_db".to_string(),
            name: "Transition".to_string(),
            kind: DbKind::Sqlite,
            enabled: true,
            database: "local.db".to_string(),
            host: Some("localhost".to_string()),
            port: Some(5432),
            username: Some("readonly".to_string()),
            credential_ref: Some("vault://transition_db".to_string()),
            ssl_mode: None,
            max_rows: 100,
            query_timeout_ms: 1_000,
            max_connections: 1,
        });
        config
            .normalize_and_validate()
            .expect("SQLite transition normalizes");
        assert_eq!(
            removed_credential_ref(
                Some("vault://transition_db".to_string()),
                &config.connections[0]
            )
            .as_deref(),
            Some("vault://transition_db")
        );
    }
}
