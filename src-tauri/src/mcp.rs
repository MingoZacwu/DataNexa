use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::State;
use axum::http::{uri::Authority, HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{json, Value};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use uuid::Uuid;

use crate::audit::{AuditSql, AuditStatus};
use crate::config::{
    is_tool_enabled, ConnectionConfig, DbKind, ServerConfig, ToolConfig, MCP_TOOL_NAMES,
};
use crate::i18n::{backend_text, BackendText};
use crate::state::AppState;

#[derive(Default)]
pub struct McpRuntime {
    pub running: bool,
    pub token: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub bound_endpoint: Option<String>,
    pub shutdown: Option<oneshot::Sender<()>>,
    pub generation: u64,
    pub task: Option<JoinHandle<()>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerStatus {
    pub running: bool,
    pub endpoint: String,
    pub token: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct McpToolInfo {
    pub name: String,
    pub description: String,
    pub enabled: bool,
}

#[derive(Clone)]
struct McpHttpState {
    app: Arc<AppState>,
}

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    #[serde(default)]
    id: RpcId,
    method: String,
    #[serde(default)]
    params: Option<Value>,
}

#[derive(Debug, Clone, Default)]
enum RpcId {
    #[default]
    Missing,
    Present(Value),
}

impl<'de> Deserialize<'de> for RpcId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Value::deserialize(deserializer).map(Self::Present)
    }
}

pub async fn status(app: &Arc<AppState>) -> ServerStatus {
    let config = app.config.read().await;
    let runtime = app.mcp.read().await;
    status_from(&config.server, &runtime)
}

pub async fn start(app: Arc<AppState>) -> anyhow::Result<ServerStatus> {
    let _lifecycle = app.mcp_lifecycle.lock().await;
    start_locked(app.clone()).await
}

async fn start_locked(app: Arc<AppState>) -> anyhow::Result<ServerStatus> {
    let config = ensure_server_token(&app).await?;
    if !is_local_host(&config.host) {
        return Err(anyhow::anyhow!(
            "DataNexa v1 only allows localhost MCP binding."
        ));
    }

    if app.mcp.read().await.running {
        return Ok(status(&app).await);
    }

    let address = format!("{}:{}", config.host, config.port);
    let listener = tokio::net::TcpListener::bind(&address).await?;
    activate_listener(app, &config, listener).await
}

async fn activate_listener(
    app: Arc<AppState>,
    config: &ServerConfig,
    listener: tokio::net::TcpListener,
) -> anyhow::Result<ServerStatus> {
    let local_addr = listener.local_addr()?;
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let router = mcp_router(app.clone());

    let mut runtime = app.mcp.write().await;
    runtime.generation = runtime.generation.wrapping_add(1);
    let generation = runtime.generation;
    let cleanup_app = app.clone();
    let task = tokio::spawn(async move {
        let result = axum::serve(listener, router)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.await;
            })
            .await;

        if let Err(error) = result {
            eprintln!("DataNexa MCP server stopped with error: {error}");
        }
        let mut runtime = cleanup_app.mcp.write().await;
        if runtime.generation == generation {
            runtime.running = false;
            runtime.started_at = None;
            runtime.bound_endpoint = None;
            runtime.shutdown = None;
        }
    });

    runtime.token = config.token.clone();
    runtime.running = true;
    runtime.started_at = Some(Utc::now());
    runtime.bound_endpoint = Some(format!("http://{}:{}/mcp", config.host, local_addr.port()));
    runtime.shutdown = Some(shutdown_tx);
    runtime.task = Some(task);
    Ok(status_from(config, &runtime))
}

fn mcp_router(app: Arc<AppState>) -> Router {
    Router::new()
        .route("/mcp", get(handle_mcp_get).post(handle_mcp_post))
        .with_state(McpHttpState { app })
}

pub async fn stop(app: Arc<AppState>) -> ServerStatus {
    let _lifecycle = app.mcp_lifecycle.lock().await;
    stop_locked(&app).await;
    status(&app).await
}

async fn stop_locked(app: &Arc<AppState>) {
    let config = app.config.read().await.server.clone();
    let mut runtime = app.mcp.write().await;
    runtime.generation = runtime.generation.wrapping_add(1);
    if let Some(shutdown) = runtime.shutdown.take() {
        let _ = shutdown.send(());
    }
    runtime.running = false;
    runtime.started_at = None;
    runtime.bound_endpoint = None;
    let task = runtime.task.take();
    drop(runtime);
    if let Some(mut task) = task {
        if tokio::time::timeout(Duration::from_secs(5), &mut task)
            .await
            .is_err()
        {
            task.abort();
            let _ = task.await;
        }
    }
    let _ = config;
}

pub async fn rotate_token(app: &Arc<AppState>) -> anyhow::Result<ServerStatus> {
    let _lifecycle = app.mcp_lifecycle.lock().await;
    let token = Uuid::new_v4().to_string();
    let config = {
        let _transaction = app.config_transaction.write().await;
        let mut candidate = app.config.read().await.clone();
        candidate.server.token = Some(token.clone());
        candidate.normalize_and_validate()?;
        app.store.save(&candidate)?;
        let server = candidate.server.clone();
        *app.config.write().await = candidate;
        server
    };
    let mut runtime = app.mcp.write().await;
    runtime.token = Some(token);
    Ok(status_from(&config, &runtime))
}

pub async fn reconfigure(
    app: Arc<AppState>,
    mut server: ServerConfig,
) -> anyhow::Result<ServerStatus> {
    let _lifecycle = app.mcp_lifecycle.lock().await;
    let old_config = app.config.read().await.server.clone();
    server.host = server.host.trim().to_ascii_lowercase();
    server.token = old_config.token.clone();

    let mut validation_candidate = app.config.read().await.clone();
    validation_candidate.server = server.clone();
    validation_candidate.normalize_and_validate()?;
    server = validation_candidate.server.clone();

    let running = app.mcp.read().await.running;
    let address_changed = old_config.host != server.host || old_config.port != server.port;
    if !running || !address_changed {
        persist_server_config(&app, server.clone()).await?;
        if running {
            app.mcp.write().await.token = server.token.clone();
        }
        return Ok(status(&app).await);
    }

    let candidate_address = format!("{}:{}", server.host, server.port);
    let mut stopped_for_overlap = false;
    let candidate_listener = match tokio::net::TcpListener::bind(&candidate_address).await {
        Ok(listener) => listener,
        Err(primary_error) if old_config.port == server.port => {
            stop_locked(&app).await;
            stopped_for_overlap = true;
            match tokio::net::TcpListener::bind(&candidate_address).await {
                Ok(listener) => listener,
                Err(bind_error) => {
                    let rollback_address = format!("{}:{}", old_config.host, old_config.port);
                    return match tokio::net::TcpListener::bind(&rollback_address).await {
                        Ok(listener) => {
                            activate_listener(app.clone(), &old_config, listener).await?;
                            Err(anyhow::anyhow!(
                                "MCP address change failed: {primary_error}; after stopping the old listener: {bind_error}"
                            ))
                        }
                        Err(rollback_error) => Err(anyhow::anyhow!(
                            "MCP address change failed: {primary_error}; retry failed: {bind_error}; rollback failed: {rollback_error}"
                        )),
                    };
                }
            }
        }
        Err(error) => return Err(error.into()),
    };

    if let Err(save_error) = persist_server_config(&app, server.clone()).await {
        drop(candidate_listener);
        if stopped_for_overlap {
            let rollback_address = format!("{}:{}", old_config.host, old_config.port);
            return match tokio::net::TcpListener::bind(&rollback_address).await {
                Ok(listener) => {
                    activate_listener(app.clone(), &old_config, listener).await?;
                    Err(save_error)
                }
                Err(rollback_error) => Err(anyhow::anyhow!(
                    "server config save failed: {save_error}; listener rollback failed: {rollback_error}"
                )),
            };
        }
        return Err(save_error);
    }

    if !stopped_for_overlap {
        stop_locked(&app).await;
    }
    activate_listener(app.clone(), &server, candidate_listener).await
}

async fn persist_server_config(app: &Arc<AppState>, server: ServerConfig) -> anyhow::Result<()> {
    let _transaction = app.config_transaction.write().await;
    let mut candidate = app.config.read().await.clone();
    candidate.server = server;
    candidate.normalize_and_validate()?;
    app.store.save(&candidate)?;
    *app.config.write().await = candidate;
    Ok(())
}

fn status_from(config: &ServerConfig, runtime: &McpRuntime) -> ServerStatus {
    ServerStatus {
        running: runtime.running,
        endpoint: runtime
            .bound_endpoint
            .clone()
            .unwrap_or_else(|| format!("http://{}:{}/mcp", config.host, config.port)),
        token: runtime.token.clone().or_else(|| config.token.clone()),
        started_at: runtime.started_at,
    }
}

async fn ensure_server_token(app: &Arc<AppState>) -> anyhow::Result<ServerConfig> {
    let _transaction = app.config_transaction.write().await;
    let mut candidate = app.config.read().await.clone();
    if candidate.server.token.is_none() {
        candidate.server.token = Some(Uuid::new_v4().to_string());
        candidate.normalize_and_validate()?;
        app.store.save(&candidate)?;
        *app.config.write().await = candidate.clone();
    }
    Ok(candidate.server)
}

async fn handle_mcp_get(State(state): State<McpHttpState>, headers: HeaderMap) -> Response {
    match validate_request(state.app.clone(), headers).await {
        Ok(()) => (
            StatusCode::METHOD_NOT_ALLOWED,
            "MCP GET streaming is not enabled",
        )
            .into_response(),
        Err(response) => response,
    }
}

async fn handle_mcp_post(
    State(state): State<McpHttpState>,
    headers: HeaderMap,
    Json(request): Json<JsonRpcRequest>,
) -> Response {
    if let Err(response) = validate_request(state.app.clone(), headers).await {
        return response;
    }

    let (notification, id) = match request.id {
        RpcId::Missing => (true, Value::Null),
        RpcId::Present(id) => (false, id),
    };
    if request.jsonrpc != "2.0" {
        return rpc_error(id, -32600, "jsonrpc must be exactly 2.0");
    }

    let (result, error_code) = match request.method.as_str() {
        "initialize" => (
            Ok(json!({
                "protocolVersion": negotiated_protocol_version(request.params.as_ref()),
                "serverInfo": {
                    "name": "datanexa",
                    "version": env!("CARGO_PKG_VERSION")
                },
                "capabilities": {
                    "tools": {}
                }
            })),
            -32000,
        ),
        "notifications/initialized" => (Ok(Value::Null), -32000),
        "tools/list" => {
            let config = state.app.config.read().await;
            (Ok(json!({ "tools": tools(&config.tools) })), -32000)
        }
        "tools/call" => (
            call_tool_audited(
                state.app.clone(),
                request.params.unwrap_or_else(|| json!({})),
            )
            .await,
            -32000,
        ),
        _ => (
            Err(anyhow::anyhow!(
                "unsupported MCP method: {}",
                request.method
            )),
            -32601,
        ),
    };

    if notification {
        return StatusCode::ACCEPTED.into_response();
    }

    match result {
        Ok(value) => Json(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": value
        }))
        .into_response(),
        Err(error) => Json(json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": error_code,
                "message": sanitize_error(&error)
            }
        }))
        .into_response(),
    }
}

fn rpc_error(id: Value, code: i32, message: &str) -> Response {
    Json(json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    }))
    .into_response()
}

fn negotiated_protocol_version(params: Option<&Value>) -> &'static str {
    const LATEST: &str = "2025-11-25";
    const SUPPORTED: [&str; 4] = ["2024-11-05", "2025-03-26", "2025-06-18", LATEST];
    params
        .and_then(|params| params.get("protocolVersion"))
        .and_then(Value::as_str)
        .and_then(|version| {
            SUPPORTED
                .into_iter()
                .find(|supported| *supported == version)
        })
        .unwrap_or(LATEST)
}

async fn validate_request(app: Arc<AppState>, headers: HeaderMap) -> Result<(), Response> {
    let config = {
        let config = app.config.read().await;
        config.server.clone()
    };
    let host = headers
        .get("host")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    if !is_local_authority(host) {
        return Err((StatusCode::FORBIDDEN, "Host is not allowed").into_response());
    }

    if let Some(origin) = headers.get("origin").and_then(|value| value.to_str().ok()) {
        if !is_local_origin(origin) {
            return Err((StatusCode::FORBIDDEN, "Origin is not allowed").into_response());
        }
    }

    if config.require_token {
        let runtime_token = {
            let runtime = app.mcp.read().await;
            runtime.token.clone()
        };
        let expected = runtime_token
            .or_else(|| config.token.clone())
            .unwrap_or_default();
        let provided = headers
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
            .unwrap_or_default();
        if expected.is_empty() || provided != expected {
            return Err(
                (StatusCode::UNAUTHORIZED, "Missing or invalid bearer token").into_response(),
            );
        }
    }

    Ok(())
}

pub fn tool_infos(tool_configs: &[ToolConfig]) -> Vec<McpToolInfo> {
    MCP_TOOL_NAMES
        .iter()
        .map(|name| McpToolInfo {
            name: (*name).to_string(),
            description: tool_description(name).to_string(),
            enabled: is_tool_enabled(tool_configs, name),
        })
        .collect()
}

fn tools(tool_configs: &[ToolConfig]) -> Vec<Value> {
    let tools = vec![
        tool(
            "datanexa_list_connections",
            tool_description("datanexa_list_connections"),
            json!({ "type": "object", "properties": {} }),
        ),
        tool(
            "datanexa_get_schema",
            tool_description("datanexa_get_schema"),
            connection_schema(),
        ),
        tool(
            "datanexa_describe_table",
            tool_description("datanexa_describe_table"),
            table_schema(),
        ),
        tool(
            "datanexa_sample_rows",
            tool_description("datanexa_sample_rows"),
            sample_schema(),
        ),
        tool(
            "datanexa_execute_readonly_sql",
            tool_description("datanexa_execute_readonly_sql"),
            sql_schema(),
        ),
        tool(
            "datanexa_explain_sql",
            tool_description("datanexa_explain_sql"),
            sql_schema(),
        ),
        tool(
            "datanexa_policy_check",
            tool_description("datanexa_policy_check"),
            policy_check_schema(),
        ),
    ];

    tools
        .into_iter()
        .filter(|tool| {
            tool.get("name")
                .and_then(Value::as_str)
                .map(|name| is_tool_enabled(tool_configs, name))
                .unwrap_or(false)
        })
        .collect()
}

fn tool_description(name: &str) -> &'static str {
    match name {
        "datanexa_list_connections" => "List enabled local readonly database connections.",
        "datanexa_get_schema" => "List tables and views for a connection.",
        "datanexa_describe_table" => "Describe columns for a safe table identifier.",
        "datanexa_sample_rows" => "Read a small bounded sample from a table.",
        "datanexa_execute_readonly_sql" => {
            "Execute one read-only query. A result with truncated=true is incomplete and must not be used to infer the table's total row count."
        }
        "datanexa_explain_sql" => "Run EXPLAIN for a read-only SQL statement.",
        "datanexa_policy_check" => {
            "Validate SQL against DataNexa read-only policy without executing it."
        }
        _ => "Unknown DataNexa MCP tool.",
    }
}

fn tool(name: &str, description: &str, input_schema: Value) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": input_schema
    })
}

fn connection_schema() -> Value {
    json!({
        "type": "object",
        "required": ["connection_id"],
        "properties": {
            "connection_id": { "type": "string" }
        }
    })
}

fn table_schema() -> Value {
    json!({
        "type": "object",
        "required": ["connection_id", "table"],
        "properties": {
            "connection_id": { "type": "string" },
            "schema": { "type": "string" },
            "table": { "type": "string" }
        }
    })
}

fn sample_schema() -> Value {
    json!({
        "type": "object",
        "required": ["connection_id", "table"],
        "properties": {
            "connection_id": { "type": "string" },
            "schema": { "type": "string" },
            "table": { "type": "string" },
            "limit": { "type": "integer", "minimum": 1, "maximum": 5000 }
        }
    })
}

fn sql_schema() -> Value {
    json!({
        "type": "object",
        "required": ["connection_id", "sql"],
        "properties": {
            "connection_id": { "type": "string" },
            "sql": { "type": "string" }
        }
    })
}

fn policy_check_schema() -> Value {
    json!({
        "type": "object",
        "required": ["sql"],
        "anyOf": [
            { "required": ["kind"] },
            { "required": ["connection_id"] }
        ],
        "properties": {
            "connection_id": { "type": "string" },
            "kind": {
                "type": "string",
                "enum": ["sqlite", "mysql", "postgres"]
            },
            "sql": { "type": "string" },
            "max_rows": {
                "type": "integer",
                "minimum": 1,
                "maximum": 5000
            }
        }
    })
}

async fn call_tool_audited(app: Arc<AppState>, params: Value) -> anyhow::Result<Value> {
    let _config_transaction = app.config_transaction.read().await;
    let started = Instant::now();
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let audit_connection_id = optional_string(&args, "connection_id");
    let audit_connection_name = connection_name(&app, audit_connection_id.as_deref()).await;
    let audit_sql = audit_sql_for_args(&app, &args).await;
    let result = call_tool(app.clone(), params).await;
    if let Err(error) = &result {
        let max_events = audit_limit(&app).await;
        let disabled = error.to_string().contains("disabled in DataNexa");
        let timeout = error
            .chain()
            .any(|source| source.is::<tokio::time::error::Elapsed>());
        let status = if disabled {
            AuditStatus::Denied
        } else if timeout {
            AuditStatus::Timeout
        } else {
            AuditStatus::Error
        };
        app.audit
            .record_with_limit(
                audit_connection_id,
                audit_connection_name,
                name,
                status,
                Some(sanitize_error(error)),
                Some(started.elapsed().as_millis().try_into().unwrap_or(u64::MAX)),
                None,
                audit_sql,
                max_events,
            )
            .await
            .map_err(|audit_error| {
                anyhow::anyhow!("audit storage unavailable: {audit_error}; request failed")
            })?;
    }
    result
}

async fn audit_sql_for_args(app: &Arc<AppState>, args: &Value) -> Option<AuditSql> {
    let sql = optional_string(args, "sql")?;
    if !app.config.read().await.settings.audit_redact_sql_literals {
        return Some(AuditSql::raw(sql));
    }
    if let Some(kind) = optional_string(args, "kind").and_then(|kind| parse_db_kind(&kind).ok()) {
        return Some(AuditSql::new(&kind, sql));
    }
    if let Some(connection_id) = optional_string(args, "connection_id") {
        let kind = app
            .config
            .read()
            .await
            .connections
            .iter()
            .find(|connection| connection.id == connection_id)
            .map(|connection| connection.kind.clone());
        if let Some(kind) = kind {
            return Some(AuditSql::new(&kind, sql));
        }
    }
    Some(AuditSql::redacted())
}

async fn call_tool(app: Arc<AppState>, params: Value) -> anyhow::Result<Value> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("tools/call requires a tool name"))?;
    let name = name.to_string();
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    if !tool_enabled(&app, &name).await {
        return Err(anyhow::anyhow!("MCP tool is disabled in DataNexa: {name}"));
    }
    let max_events = audit_limit(&app).await;
    let text = text_for_app(&app).await;

    let payload = match name.as_str() {
        "datanexa_list_connections" => {
            let connections = {
                let config = app.config.read().await;
                config
                    .connections
                    .iter()
                    .filter(|connection| connection.enabled)
                    .map(public_connection)
                    .collect::<Vec<Value>>()
            };
            app.audit
                .record_with_limit(
                    None,
                    None,
                    name,
                    AuditStatus::Allowed,
                    None,
                    None,
                    Some(connections.len()),
                    None,
                    max_events,
                )
                .await?;
            json!({ "connections": connections })
        }
        "datanexa_get_schema" => {
            let connection_id = required_string(&args, "connection_id")?;
            let connection = connection(app.clone(), connection_id.clone()).await?;
            let schema = app.db.list_schema(&connection, &app.vault, &text).await?;
            app.audit
                .record_with_limit(
                    Some(connection_id),
                    Some(connection.name.clone()),
                    name,
                    AuditStatus::Allowed,
                    None,
                    None,
                    Some(schema.len()),
                    None,
                    max_events,
                )
                .await?;
            json!({ "schema": schema })
        }
        "datanexa_describe_table" => {
            let connection_id = required_string(&args, "connection_id")?;
            let table = required_string(&args, "table")?;
            let schema = optional_string(&args, "schema");
            let connection = connection(app.clone(), connection_id.clone()).await?;
            let columns = app
                .db
                .describe_table(&connection, &app.vault, schema.as_deref(), &table, &text)
                .await?;
            app.audit
                .record_with_limit(
                    Some(connection_id),
                    Some(connection.name.clone()),
                    name,
                    AuditStatus::Allowed,
                    None,
                    None,
                    Some(columns.len()),
                    None,
                    max_events,
                )
                .await?;
            json!({ "columns": columns })
        }
        "datanexa_sample_rows" => {
            let connection_id = required_string(&args, "connection_id")?;
            let table = required_string(&args, "table")?;
            let schema = optional_string(&args, "schema");
            let limit = args
                .get("limit")
                .and_then(Value::as_u64)
                .map(|value| value as u32);
            let connection = connection(app.clone(), connection_id.clone()).await?;
            let result = app
                .db
                .sample_rows(
                    &connection,
                    &app.vault,
                    schema.as_deref(),
                    &table,
                    limit,
                    &text,
                )
                .await?;
            app.audit
                .record_with_limit(
                    Some(connection_id),
                    Some(connection.name.clone()),
                    name,
                    if result.truncated {
                        AuditStatus::Truncated
                    } else {
                        AuditStatus::Allowed
                    },
                    None,
                    Some(result.elapsed_ms),
                    Some(result.row_count),
                    Some(audit_sql(&app, &connection.kind, &result.rewritten_sql).await),
                    max_events,
                )
                .await?;
            json!(result)
        }
        "datanexa_execute_readonly_sql" => {
            let connection_id = required_string(&args, "connection_id")?;
            let sql = required_string(&args, "sql")?;
            let connection = connection(app.clone(), connection_id.clone()).await?;
            let (policy, result) = app
                .db
                .execute_readonly(&connection, &app.vault, &sql, &text)
                .await?;
            if !policy.allowed {
                app.audit
                    .record_with_limit(
                        Some(connection_id),
                        Some(connection.name.clone()),
                        name,
                        AuditStatus::Denied,
                        Some(policy.reason.clone()),
                        None,
                        None,
                        Some(audit_sql(&app, &connection.kind, &sql).await),
                        max_events,
                    )
                    .await?;
                json!({ "policy": policy, "result": null })
            } else {
                let result = result.ok_or_else(|| anyhow::anyhow!("missing query result"))?;
                app.audit
                    .record_with_limit(
                        Some(connection_id),
                        Some(connection.name.clone()),
                        name,
                        if result.truncated {
                            AuditStatus::Truncated
                        } else {
                            AuditStatus::Allowed
                        },
                        None,
                        Some(result.elapsed_ms),
                        Some(result.row_count),
                        Some(audit_sql(&app, &connection.kind, &result.rewritten_sql).await),
                        max_events,
                    )
                    .await?;
                json!({ "policy": policy, "result": result })
            }
        }
        "datanexa_explain_sql" => {
            let connection_id = required_string(&args, "connection_id")?;
            let sql = required_string(&args, "sql")?;
            let connection = connection(app.clone(), connection_id.clone()).await?;
            let (policy, result) = app
                .db
                .explain_sql(&connection, &app.vault, &sql, &text)
                .await?;
            if !policy.allowed {
                app.audit
                    .record_with_limit(
                        Some(connection_id),
                        Some(connection.name.clone()),
                        name,
                        AuditStatus::Denied,
                        Some(policy.reason.clone()),
                        None,
                        None,
                        Some(audit_sql(&app, &connection.kind, &sql).await),
                        max_events,
                    )
                    .await?;
                json!({ "policy": policy, "result": null })
            } else {
                let result = result.ok_or_else(|| anyhow::anyhow!("missing query result"))?;
                app.audit
                    .record_with_limit(
                        Some(connection_id),
                        Some(connection.name.clone()),
                        name,
                        if result.truncated {
                            AuditStatus::Truncated
                        } else {
                            AuditStatus::Allowed
                        },
                        None,
                        Some(result.elapsed_ms),
                        Some(result.row_count),
                        Some(audit_sql(&app, &connection.kind, &result.rewritten_sql).await),
                        max_events,
                    )
                    .await?;
                json!({ "policy": policy, "result": result })
            }
        }
        "datanexa_policy_check" => {
            let sql = required_string(&args, "sql")?;
            let audit_connection_id = optional_string(&args, "connection_id");
            let audit_connection_name = connection_name(&app, audit_connection_id.as_deref()).await;
            let (kind, max_rows) = if let Some(kind) = optional_string(&args, "kind") {
                let max_rows = args
                    .get("max_rows")
                    .and_then(Value::as_u64)
                    .map(|value| value as u32)
                    .unwrap_or(500)
                    .clamp(1, 5000);
                (parse_db_kind(&kind)?, max_rows)
            } else {
                let connection_id = required_string(&args, "connection_id")?;
                let connection = connection(app.clone(), connection_id).await?;
                (connection.kind, connection.max_rows)
            };
            let policy = crate::policy::PolicyEngine::check_with_text(&kind, &sql, max_rows, &text);
            app.audit
                .record_with_limit(
                    audit_connection_id,
                    audit_connection_name,
                    name,
                    if policy.allowed {
                        AuditStatus::Allowed
                    } else {
                        AuditStatus::Denied
                    },
                    (!policy.allowed).then(|| policy.reason.clone()),
                    None,
                    None,
                    Some(audit_sql(&app, &kind, &sql).await),
                    max_events,
                )
                .await?;
            json!({ "policy": policy })
        }
        _ => return Err(anyhow::anyhow!("unknown DataNexa tool: {name}")),
    };

    Ok(json!({
        "content": [
            {
                "type": "text",
                "text": serde_json::to_string_pretty(&payload)?
            }
        ],
        "isError": false
    }))
}

async fn audit_limit(app: &Arc<AppState>) -> usize {
    app.config.read().await.settings.audit_max_events
}

async fn connection_name(app: &Arc<AppState>, connection_id: Option<&str>) -> Option<String> {
    let connection_id = connection_id?;
    app.config
        .read()
        .await
        .connections
        .iter()
        .find(|connection| connection.id == connection_id)
        .map(|connection| connection.name.clone())
}

async fn audit_sql(app: &Arc<AppState>, kind: &DbKind, sql: &str) -> AuditSql {
    if app.config.read().await.settings.audit_redact_sql_literals {
        AuditSql::new(kind, sql)
    } else {
        AuditSql::raw(sql)
    }
}

async fn text_for_app(app: &Arc<AppState>) -> BackendText {
    let language = app.config.read().await.settings.language.clone();
    backend_text(&language)
}

async fn tool_enabled(app: &Arc<AppState>, name: &str) -> bool {
    let config = app.config.read().await;
    is_tool_enabled(&config.tools, name)
}

async fn connection(app: Arc<AppState>, connection_id: String) -> anyhow::Result<ConnectionConfig> {
    let config = app.config.read().await;
    let connection = config
        .connections
        .iter()
        .find(|connection| connection.id == connection_id)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("connection not found"))?;
    if !connection.enabled {
        return Err(anyhow::anyhow!("connection is disabled"));
    }
    Ok(connection)
}

fn public_connection(connection: &ConnectionConfig) -> Value {
    json!({
        "id": connection.id,
        "name": connection.name,
        "type": db_kind(&connection.kind),
        "enabled": connection.enabled,
        "max_rows": connection.max_rows,
        "query_timeout_ms": connection.query_timeout_ms
    })
}

fn db_kind(kind: &DbKind) -> &'static str {
    match kind {
        DbKind::Sqlite => "sqlite",
        DbKind::Mysql => "mysql",
        DbKind::Postgres => "postgres",
    }
}

fn required_string(args: &Value, key: &str) -> anyhow::Result<String> {
    args.get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.to_string())
        .ok_or_else(|| anyhow::anyhow!("missing required argument: {key}"))
}

fn optional_string(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.to_string())
}

fn parse_db_kind(kind: &str) -> anyhow::Result<DbKind> {
    match kind.to_ascii_lowercase().as_str() {
        "sqlite" => Ok(DbKind::Sqlite),
        "mysql" => Ok(DbKind::Mysql),
        "postgres" | "postgresql" => Ok(DbKind::Postgres),
        _ => Err(anyhow::anyhow!("unsupported database kind: {kind}")),
    }
}

fn sanitize_error(error: &anyhow::Error) -> String {
    let text = error.to_string();
    let text = regex::Regex::new(r"(?i)(password|token|secret)=([^&\s]+)")
        .expect("valid secret sanitizer regex")
        .replace_all(&text, "$1=REDACTED")
        .to_string();
    text.replace('\n', " ")
}

fn is_local_host(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "localhost")
}

fn is_local_authority(authority: &str) -> bool {
    Authority::from_str(authority)
        .map(|authority| matches!(authority.host(), "127.0.0.1" | "localhost"))
        .unwrap_or(false)
}

fn is_local_origin(origin: &str) -> bool {
    Uri::from_str(origin)
        .ok()
        .and_then(|uri| {
            let allowed_scheme = matches!(uri.scheme_str(), Some("http" | "https" | "tauri"));
            uri.authority().map(|authority| {
                allowed_scheme && matches!(authority.host(), "127.0.0.1" | "localhost")
            })
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use axum::body::{to_bytes, Body};
    use axum::http::Request;
    use tower::ServiceExt;

    use super::*;
    use crate::audit::AuditLogger;
    use crate::config::{AppConfig, ConfigStore};
    use crate::db::DatabaseManager;
    use crate::state::AppState;
    use crate::vault::CredentialVault;

    fn test_state(root: &Path, port: u16, audit_path: &Path) -> Arc<AppState> {
        let mut config = AppConfig::default();
        config.server.port = port;
        config.server.token = Some("TEST_TOKEN".to_string());
        let store = ConfigStore::for_test(root.join("config.toml"));
        store.save(&config).expect("test config saves");
        Arc::new(AppState {
            store,
            config: tokio::sync::RwLock::new(config),
            config_transaction: tokio::sync::RwLock::new(()),
            vault: CredentialVault::new(),
            audit: AuditLogger::for_test(audit_path.to_path_buf()),
            db: DatabaseManager::default(),
            mcp: tokio::sync::RwLock::new(McpRuntime::default()),
            mcp_lifecycle: tokio::sync::Mutex::new(()),
        })
    }

    fn test_state_with_failing_store(root: &Path) -> Arc<AppState> {
        let mut config = AppConfig::default();
        config.server.token = Some("TEST_TOKEN".to_string());
        let invalid_target = root.join("config-target");
        std::fs::create_dir(&invalid_target).expect("directory target");
        Arc::new(AppState {
            store: ConfigStore::for_test(invalid_target),
            config: tokio::sync::RwLock::new(config),
            config_transaction: tokio::sync::RwLock::new(()),
            vault: CredentialVault::new(),
            audit: AuditLogger::for_test(root.join("audit.json")),
            db: DatabaseManager::default(),
            mcp: tokio::sync::RwLock::new(McpRuntime::default()),
            mcp_lifecycle: tokio::sync::Mutex::new(()),
        })
    }

    fn mcp_request(body: Value) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri("/mcp")
            .header("host", "127.0.0.1:17321")
            .header("authorization", "Bearer TEST_TOKEN")
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .expect("request builds")
    }

    async fn response_json(response: Response) -> Value {
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body reads");
        serde_json::from_slice(&body).expect("response body is JSON")
    }

    async fn free_port() -> u16 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("ephemeral listener binds");
        listener.local_addr().expect("local address").port()
    }

    #[test]
    fn rpc_id_distinguishes_notification_from_explicit_null() {
        let notification: JsonRpcRequest = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        }))
        .expect("notification parses");
        assert!(matches!(notification.id, RpcId::Missing));

        let explicit_null: JsonRpcRequest = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": null,
            "method": "tools/list"
        }))
        .expect("explicit null id parses");
        assert!(matches!(explicit_null.id, RpcId::Present(Value::Null)));
    }

    #[test]
    fn host_and_origin_validation_use_exact_parsing() {
        assert!(is_local_authority("localhost:17321"));
        assert!(is_local_authority("127.0.0.1:17321"));
        assert!(!is_local_authority("localhost.attacker:17321"));
        assert!(!is_local_authority("example.test"));

        assert!(is_local_origin("tauri://localhost"));
        assert!(is_local_origin("http://127.0.0.1:17321"));
        assert!(!is_local_origin("tauri://localhost.attacker"));
        assert!(!is_local_origin("https://example.test"));
    }

    #[test]
    fn protocol_negotiation_preserves_supported_client_versions() {
        let supported = json!({ "protocolVersion": "2025-06-18" });
        let unsupported = json!({ "protocolVersion": "1900-01-01" });
        assert_eq!(negotiated_protocol_version(Some(&supported)), "2025-06-18");
        assert_eq!(
            negotiated_protocol_version(Some(&unsupported)),
            "2025-11-25"
        );
    }

    #[tokio::test]
    async fn http_notification_null_id_validation_and_removed_sse_behave_correctly() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let state = test_state(
            directory.path(),
            17321,
            &directory.path().join("audit.json"),
        );
        let router = mcp_router(state);

        let notification = router
            .clone()
            .oneshot(mcp_request(json!({
                "jsonrpc": "2.0",
                "method": "notifications/initialized"
            })))
            .await
            .expect("notification response");
        assert_eq!(notification.status(), StatusCode::ACCEPTED);
        assert!(to_bytes(notification.into_body(), usize::MAX)
            .await
            .expect("notification body reads")
            .is_empty());

        let null_id = router
            .clone()
            .oneshot(mcp_request(json!({
                "jsonrpc": "2.0",
                "id": null,
                "method": "tools/list"
            })))
            .await
            .expect("null id response");
        assert_eq!(null_id.status(), StatusCode::OK);
        assert_eq!(response_json(null_id).await.get("id"), Some(&Value::Null));

        let bad_host = Request::builder()
            .method("POST")
            .uri("/mcp")
            .header("host", "localhost.attacker:17321")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}).to_string(),
            ))
            .expect("bad host request");
        assert_eq!(
            router
                .clone()
                .oneshot(bad_host)
                .await
                .expect("bad host response")
                .status(),
            StatusCode::FORBIDDEN
        );

        let mut bad_origin = mcp_request(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/list"
        }));
        bad_origin.headers_mut().insert(
            "origin",
            "tauri://localhost.attacker".parse().expect("origin header"),
        );
        assert_eq!(
            router
                .clone()
                .oneshot(bad_origin)
                .await
                .expect("bad origin response")
                .status(),
            StatusCode::FORBIDDEN
        );

        let sse = Request::builder()
            .uri("/sse")
            .body(Body::empty())
            .expect("SSE request");
        assert_eq!(
            router.oneshot(sse).await.expect("SSE response").status(),
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn audit_failure_closes_successful_tool_response() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let invalid_audit_target = directory.path().join("audit-target");
        std::fs::create_dir(&invalid_audit_target).expect("audit directory target");
        let state = test_state(directory.path(), 17321, &invalid_audit_target);
        let response = mcp_router(state)
            .oneshot(mcp_request(json!({
                "jsonrpc": "2.0",
                "id": 7,
                "method": "tools/call",
                "params": {
                    "name": "datanexa_list_connections",
                    "arguments": {}
                }
            })))
            .await
            .expect("tool response");
        let body = response_json(response).await;
        assert!(body.get("result").is_none());
        assert!(body
            .pointer("/error/message")
            .and_then(Value::as_str)
            .is_some_and(|message| message.contains("audit storage unavailable")));
    }

    #[tokio::test]
    async fn authentication_initialize_tool_errors_and_disabled_audit_are_enforced() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let state = test_state(
            directory.path(),
            17321,
            &directory.path().join("audit.json"),
        );
        let router = mcp_router(state.clone());
        let body = json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}).to_string();

        let missing_token = Request::builder()
            .method("POST")
            .uri("/mcp")
            .header("host", "127.0.0.1:17321")
            .header("content-type", "application/json")
            .body(Body::from(body.clone()))
            .expect("missing token request");
        assert_eq!(
            router
                .clone()
                .oneshot(missing_token)
                .await
                .expect("missing token response")
                .status(),
            StatusCode::UNAUTHORIZED
        );

        let wrong_token = Request::builder()
            .method("POST")
            .uri("/mcp")
            .header("host", "127.0.0.1:17321")
            .header("authorization", "Bearer WRONG_TEST_TOKEN")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .expect("wrong token request");
        assert_eq!(
            router
                .clone()
                .oneshot(wrong_token)
                .await
                .expect("wrong token response")
                .status(),
            StatusCode::UNAUTHORIZED
        );

        let initialize = router
            .clone()
            .oneshot(mcp_request(json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "initialize",
                "params": { "protocolVersion": "2025-06-18" }
            })))
            .await
            .expect("initialize response");
        assert_eq!(
            response_json(initialize)
                .await
                .pointer("/result/protocolVersion"),
            Some(&Value::String("2025-06-18".to_string()))
        );

        let tool_call = router
            .clone()
            .oneshot(mcp_request(json!({
                "jsonrpc": "2.0",
                "id": 3,
                "method": "tools/call",
                "params": { "name": "datanexa_list_connections", "arguments": {} }
            })))
            .await
            .expect("tool call response");
        assert!(response_json(tool_call).await.get("result").is_some());

        let unknown = router
            .clone()
            .oneshot(mcp_request(json!({
                "jsonrpc": "2.0",
                "id": 4,
                "method": "unknown/method"
            })))
            .await
            .expect("unknown method response");
        assert_eq!(
            response_json(unknown).await.pointer("/error/code"),
            Some(&Value::Number((-32601).into()))
        );

        state
            .config
            .write()
            .await
            .tools
            .iter_mut()
            .find(|tool| tool.name == "datanexa_list_connections")
            .expect("tool config")
            .enabled = false;
        let disabled = router
            .oneshot(mcp_request(json!({
                "jsonrpc": "2.0",
                "id": 5,
                "method": "tools/call",
                "params": { "name": "datanexa_list_connections", "arguments": {} }
            })))
            .await
            .expect("disabled tool response");
        assert!(response_json(disabled).await.get("error").is_some());
        assert!(state
            .audit
            .list()
            .await
            .iter()
            .any(|event| matches!(event.status, AuditStatus::Denied)));
    }

    #[tokio::test]
    async fn failed_auth_and_token_saves_do_not_change_runtime() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let state = test_state_with_failing_store(directory.path());
        {
            let mut runtime = state.mcp.write().await;
            runtime.running = true;
            runtime.token = Some("TEST_TOKEN".to_string());
            runtime.bound_endpoint = Some("http://127.0.0.1:17321/mcp".to_string());
        }

        let mut server = state.config.read().await.server.clone();
        server.require_token = false;
        assert!(reconfigure(state.clone(), server).await.is_err());
        assert!(state.config.read().await.server.require_token);
        assert_eq!(state.mcp.read().await.token.as_deref(), Some("TEST_TOKEN"));

        assert!(rotate_token(&state).await.is_err());
        assert_eq!(
            state.config.read().await.server.token.as_deref(),
            Some("TEST_TOKEN")
        );
        assert_eq!(state.mcp.read().await.token.as_deref(), Some("TEST_TOKEN"));
    }

    #[tokio::test]
    async fn lifecycle_start_stop_reconfigure_and_failed_candidate_preserve_state() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let initial_port = free_port().await;
        let state = test_state(
            directory.path(),
            initial_port,
            &directory.path().join("audit.json"),
        );

        let started = start(state.clone()).await.expect("server starts");
        assert!(started.running);
        assert!(started.endpoint.contains(&initial_port.to_string()));
        tokio::net::TcpStream::connect(("127.0.0.1", initial_port))
            .await
            .expect("initial endpoint accepts connections");

        let occupied = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("occupied listener binds");
        let occupied_port = occupied.local_addr().expect("occupied address").port();
        let mut rejected = state.config.read().await.server.clone();
        rejected.port = occupied_port;
        assert!(reconfigure(state.clone(), rejected).await.is_err());
        let after_rejection = status(&state).await;
        assert!(after_rejection.running);
        assert!(after_rejection.endpoint.contains(&initial_port.to_string()));
        drop(occupied);

        let replacement_port = free_port().await;
        let mut replacement = state.config.read().await.server.clone();
        replacement.port = replacement_port;
        let replaced = reconfigure(state.clone(), replacement)
            .await
            .expect("server reconfigures");
        assert!(replaced.running);
        assert!(replaced.endpoint.contains(&replacement_port.to_string()));
        tokio::net::TcpStream::connect(("127.0.0.1", replacement_port))
            .await
            .expect("replacement endpoint accepts connections");
        assert!(tokio::net::TcpStream::connect(("127.0.0.1", initial_port))
            .await
            .is_err());

        let stopped = stop(state).await;
        assert!(!stopped.running);
        assert!(
            tokio::net::TcpStream::connect(("127.0.0.1", replacement_port))
                .await
                .is_err()
        );
    }
}
