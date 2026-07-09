use std::sync::Arc;
use std::time::Instant;

use regex::Regex;
use serde::{Deserialize, Serialize};
use tauri::menu::{Menu, MenuItem};
use tauri::{AppHandle, State, WebviewWindow};

use crate::audit::AuditStatus;
use crate::config::{
    default_port, is_known_tool, AppConfig, ConnectionConfig, DbKind, ServerConfig, SettingsConfig,
};
use crate::db::ConnectionDiagnostics;
use crate::i18n::{backend_text, BackendText};
use crate::mcp::{self, McpToolInfo, ServerStatus};
use crate::policy::{PolicyCheckResult, PolicyEngine};
use crate::state::AppState;
use crate::vault::CredentialVault;

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

    {
        let mut config = state.config.write().await;
        config.server = server;
        state.store.save(&config).map_err(to_client_error)?;
    }

    snapshot(state.inner()).await.map_err(to_client_error)
}

#[tauri::command]
pub async fn save_settings_config(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    settings: SettingsConfig,
) -> Result<AppSnapshot, String> {
    let text = {
        let mut config = state.config.write().await;
        let settings = normalize_settings(settings);
        let text = backend_text(&settings.language);
        config.settings = settings;
        state.store.save(&config).map_err(to_client_error)?;
        state.audit.trim(config.settings.audit_max_events).await;
        text
    };

    refresh_tray_menu(&app, text).map_err(to_client_error)?;

    snapshot(state.inner()).await.map_err(to_client_error)
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

    {
        let mut config = state.config.write().await;
        config.normalize();
        if let Some(tool) = config.tools.iter_mut().find(|tool| tool.name == name) {
            tool.enabled = enabled;
        }
        state.store.save(&config).map_err(to_client_error)?;
    }

    snapshot(state.inner()).await.map_err(to_client_error)
}

#[tauri::command]
pub async fn upsert_connection(
    state: State<'_, Arc<AppState>>,
    input: ConnectionInput,
) -> Result<AppSnapshot, String> {
    let text = text_for_state(state.inner()).await;
    validate_connection(&input.connection, &text).map_err(to_client_error)?;
    let clear_password = input.clear_password;
    let mut connection = normalize_connection(input.connection);

    if clear_password {
        if let Some(credential_ref) = connection.credential_ref.as_deref() {
            state
                .vault
                .delete(credential_ref)
                .map_err(to_client_error)?;
        }
        connection.credential_ref = None;
    } else if let Some(password) = input.password.filter(|value| !value.is_empty()) {
        let credential_ref = connection
            .credential_ref
            .clone()
            .unwrap_or_else(|| CredentialVault::credential_ref(&connection.id));
        state
            .vault
            .put(&credential_ref, password)
            .map_err(to_client_error)?;
        connection.credential_ref = Some(credential_ref);
    }

    {
        let mut config = state.config.write().await;
        if let Some(existing) = config
            .connections
            .iter_mut()
            .find(|existing| existing.id == connection.id)
        {
            if !clear_password && connection.credential_ref.is_none() {
                connection.credential_ref = existing.credential_ref.clone();
            }
            *existing = connection.clone();
        } else {
            config.connections.push(connection.clone());
        }
        state.store.save(&config).map_err(to_client_error)?;
    }

    state.db.close(&connection.id).await;
    snapshot(state.inner()).await.map_err(to_client_error)
}

#[tauri::command]
pub async fn delete_connection(
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Result<AppSnapshot, String> {
    let credential_ref = {
        let mut config = state.config.write().await;
        let removed = config
            .connections
            .iter()
            .find(|connection| connection.id == id)
            .and_then(|connection| connection.credential_ref.clone());
        config.connections.retain(|connection| connection.id != id);
        state.store.save(&config).map_err(to_client_error)?;
        removed
    };

    state.db.close(&id).await;
    if let Some(credential_ref) = credential_ref {
        state
            .vault
            .delete(&credential_ref)
            .map_err(to_client_error)?;
    }

    snapshot(state.inner()).await.map_err(to_client_error)
}

#[tauri::command]
pub async fn test_connection(
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Result<String, String> {
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
                    "test_connection",
                    AuditStatus::Allowed,
                    None,
                    Some(duration.as_millis().try_into().unwrap_or(u64::MAX)),
                    None,
                    None,
                    max_events,
                )
                .await;
            Ok(text.connection_test_ok(duration.as_millis()))
        }
        Err(error) => {
            state
                .audit
                .record_with_limit(
                    Some(id),
                    "test_connection",
                    AuditStatus::Error,
                    Some(sanitize_error(&error)),
                    Some(started.elapsed().as_millis().try_into().unwrap_or(u64::MAX)),
                    None,
                    None,
                    max_events,
                )
                .await;
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
pub async fn diagnose_connection(
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Result<ConnectionDiagnostics, String> {
    let text = text_for_state(state.inner()).await;
    let connection = find_connection(state.inner(), &id)
        .await
        .map_err(to_client_error)?;
    Ok(state.db.diagnostics(&connection, &state.vault, &text))
}

#[tauri::command]
pub async fn start_mcp_server(state: State<'_, Arc<AppState>>) -> Result<AppSnapshot, String> {
    mcp::start(state.inner().clone())
        .await
        .map_err(to_client_error)?;
    snapshot(state.inner()).await.map_err(to_client_error)
}

#[tauri::command]
pub async fn stop_mcp_server(state: State<'_, Arc<AppState>>) -> Result<AppSnapshot, String> {
    mcp::stop(state.inner().clone()).await;
    snapshot(state.inner()).await.map_err(to_client_error)
}

#[tauri::command]
pub async fn rotate_server_token(state: State<'_, Arc<AppState>>) -> Result<AppSnapshot, String> {
    mcp::rotate_token(state.inner()).await;
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
    window.hide().map_err(to_client_error)
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
    state.audit.trim(config.settings.audit_max_events).await;
    Ok(AppSnapshot {
        server_status: mcp::status(state).await,
        audit_events: state.audit.list().await,
        tools: mcp::tool_infos(&config.tools),
        config,
    })
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

    text.diagnostics_for_client(
        &diagnostics.database_type,
        host,
        &port,
        &diagnostics.database,
        username,
        &diagnostics.credential_state,
        ssl_mode,
        diagnostics.query_timeout_ms,
        diagnostics.max_connections,
        hint,
    )
}

fn refresh_tray_menu(app: &AppHandle, text: BackendText) -> tauri::Result<()> {
    if let Some(tray) = app.tray_by_id("main") {
        let show_item = MenuItem::with_id(app, "show", text.tray_show(), true, None::<&str>)?;
        let quit_item = MenuItem::with_id(app, "quit", text.tray_quit(), true, None::<&str>)?;
        let tray_menu = Menu::with_items(app, &[&show_item, &quit_item])?;
        tray.set_menu(Some(tray_menu))?;
    }
    Ok(())
}

fn is_local_host(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "localhost")
}
