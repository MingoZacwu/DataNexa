use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use async_stream::stream;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use futures_util::Stream;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::audit::AuditStatus;
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
    pub shutdown: Option<oneshot::Sender<()>>,
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
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Option<Value>,
}

pub async fn status(app: &Arc<AppState>) -> ServerStatus {
    let config = app.config.read().await;
    let runtime = app.mcp.read().await;
    status_from(&config.server, &runtime)
}

pub async fn start(app: Arc<AppState>) -> anyhow::Result<ServerStatus> {
    let config = ensure_server_token(&app).await?;
    if !is_local_host(&config.host) {
        return Err(anyhow::anyhow!(
            "DataNexa v1 only allows localhost MCP binding."
        ));
    }

    let mut runtime = app.mcp.write().await;
    if runtime.running {
        return Ok(status_from(&config, &runtime));
    }

    runtime.token = config.token.clone();

    let address = format!("{}:{}", config.host, config.port);
    let listener = tokio::net::TcpListener::bind(&address).await?;
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let http_state = McpHttpState { app: app.clone() };
    let router = Router::new()
        .route("/mcp", get(sse_stream).post(handle_mcp_post))
        .route("/sse", get(sse_stream))
        .with_state(http_state);

    tokio::spawn(async move {
        let result = axum::serve(listener, router)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.await;
            })
            .await;

        if let Err(error) = result {
            eprintln!("DataNexa MCP server stopped with error: {error}");
        }
    });

    runtime.running = true;
    runtime.started_at = Some(Utc::now());
    runtime.shutdown = Some(shutdown_tx);
    Ok(status_from(&config, &runtime))
}

pub async fn stop(app: Arc<AppState>) -> ServerStatus {
    let config = app.config.read().await.server.clone();
    let mut runtime = app.mcp.write().await;
    if let Some(shutdown) = runtime.shutdown.take() {
        if shutdown.send(()).is_err() {
            runtime.running = false;
        }
    }
    runtime.running = false;
    runtime.started_at = None;
    status_from(&config, &runtime)
}

pub async fn rotate_token(app: &Arc<AppState>) -> ServerStatus {
    let token = Uuid::new_v4().to_string();
    let config = {
        let mut config = app.config.write().await;
        config.server.token = Some(token.clone());
        if let Err(error) = app.store.save(&config) {
            eprintln!("failed to persist DataNexa MCP token: {error}");
        }
        config.server.clone()
    };
    let mut runtime = app.mcp.write().await;
    runtime.token = Some(token);
    status_from(&config, &runtime)
}

fn status_from(config: &ServerConfig, runtime: &McpRuntime) -> ServerStatus {
    ServerStatus {
        running: runtime.running,
        endpoint: format!("http://{}:{}/mcp", config.host, config.port),
        token: runtime.token.clone().or_else(|| config.token.clone()),
        started_at: runtime.started_at.clone(),
    }
}

async fn ensure_server_token(app: &Arc<AppState>) -> anyhow::Result<ServerConfig> {
    let mut config = app.config.write().await;
    if config.server.token.is_none() {
        config.server.token = Some(Uuid::new_v4().to_string());
        app.store.save(&config)?;
    }
    Ok(config.server.clone())
}

async fn sse_stream(State(state): State<McpHttpState>, headers: HeaderMap) -> Response {
    match validate_request(state.app.clone(), headers).await {
        Ok(()) => {
            let stream = heartbeat_stream();
            Sse::new(stream)
                .keep_alive(
                    KeepAlive::new()
                        .interval(Duration::from_secs(15))
                        .text("ping"),
                )
                .into_response()
        }
        Err(response) => response,
    }
}

fn heartbeat_stream() -> impl Stream<Item = Result<Event, Infallible>> {
    stream! {
        loop {
            yield Ok(Event::default().event("message").data("{\"jsonrpc\":\"2.0\",\"method\":\"notifications/ping\"}"));
            tokio::time::sleep(Duration::from_secs(15)).await;
        }
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

    let id = request.id.clone();
    let result = match request.method.as_str() {
        "initialize" => Ok(json!({
            "protocolVersion": "2025-11-25",
            "serverInfo": {
                "name": "datanexa",
                "version": env!("CARGO_PKG_VERSION")
            },
            "capabilities": {
                "tools": {}
            }
        })),
        "notifications/initialized" => Ok(Value::Null),
        "tools/list" => {
            let config = state.app.config.read().await;
            Ok(json!({ "tools": tools(&config.tools) }))
        }
        "tools/call" => {
            call_tool(
                state.app.clone(),
                request.params.unwrap_or_else(|| json!({})),
            )
            .await
        }
        _ => Err(anyhow::anyhow!(
            "unsupported MCP method: {}",
            request.method
        )),
    };

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
                "code": -32000,
                "message": sanitize_error(&error)
            }
        }))
        .into_response(),
    }
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
            "Execute a single read-only SELECT/WITH/EXPLAIN statement after policy validation."
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
                    name,
                    AuditStatus::Allowed,
                    None,
                    None,
                    Some(connections.len()),
                    None,
                    max_events,
                )
                .await;
            json!({ "connections": connections })
        }
        "datanexa_get_schema" => {
            let connection_id = required_string(&args, "connection_id")?;
            let connection = connection(app.clone(), connection_id.clone()).await?;
            let schema = app.db.list_schema(&connection, &app.vault, &text).await?;
            app.audit
                .record_with_limit(
                    Some(connection_id),
                    name,
                    AuditStatus::Allowed,
                    None,
                    None,
                    Some(schema.len()),
                    None,
                    max_events,
                )
                .await;
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
                    name,
                    AuditStatus::Allowed,
                    None,
                    None,
                    Some(columns.len()),
                    None,
                    max_events,
                )
                .await;
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
                    name,
                    AuditStatus::Allowed,
                    None,
                    Some(result.elapsed_ms),
                    Some(result.row_count),
                    Some(result.rewritten_sql.clone()),
                    max_events,
                )
                .await;
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
                        name,
                        AuditStatus::Denied,
                        Some(policy.reason.clone()),
                        None,
                        None,
                        Some(sql),
                        max_events,
                    )
                    .await;
                json!({ "policy": policy, "result": null })
            } else {
                let result = result.ok_or_else(|| anyhow::anyhow!("missing query result"))?;
                app.audit
                    .record_with_limit(
                        Some(connection_id),
                        name,
                        AuditStatus::Allowed,
                        None,
                        Some(result.elapsed_ms),
                        Some(result.row_count),
                        Some(result.rewritten_sql.clone()),
                        max_events,
                    )
                    .await;
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
                        name,
                        AuditStatus::Denied,
                        Some(policy.reason.clone()),
                        None,
                        None,
                        Some(sql),
                        max_events,
                    )
                    .await;
                json!({ "policy": policy, "result": null })
            } else {
                let result = result.ok_or_else(|| anyhow::anyhow!("missing query result"))?;
                app.audit
                    .record_with_limit(
                        Some(connection_id),
                        name,
                        AuditStatus::Allowed,
                        None,
                        Some(result.elapsed_ms),
                        Some(result.row_count),
                        Some(result.rewritten_sql.clone()),
                        max_events,
                    )
                    .await;
                json!({ "policy": policy, "result": result })
            }
        }
        "datanexa_policy_check" => {
            let sql = required_string(&args, "sql")?;
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
    authority.starts_with("127.0.0.1:")
        || authority.starts_with("localhost:")
        || authority == "127.0.0.1"
        || authority == "localhost"
}

fn is_local_origin(origin: &str) -> bool {
    origin.starts_with("http://127.0.0.1:")
        || origin.starts_with("http://localhost:")
        || origin.starts_with("https://127.0.0.1:")
        || origin.starts_with("https://localhost:")
        || origin.starts_with("tauri://localhost")
}
