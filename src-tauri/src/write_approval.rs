use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tokio::sync::Mutex;

use crate::state::AppState;
use crate::write_operations::{WriteApprovalDto, WriteDecision, WriteOperationStatus, WriteStatus};

pub const WINDOW_LABEL: &str = "write-approval";
pub const PREVIEW_WINDOW_LABEL: &str = "write-approval-preview";
const CHANGED_EVENT: &str = "write-approval-changed";

#[derive(Default)]
pub struct ApprovalWindowState {
    session: Mutex<ApprovalSession>,
}

#[derive(Default)]
struct ApprovalSession {
    bound: Option<WriteApprovalDto>,
    ready: bool,
    closing: bool,
    window_open: bool,
}

#[derive(Default)]
pub struct PreviewWindowState {
    session: Mutex<PreviewSession>,
}

#[derive(Default)]
struct PreviewSession {
    ready: bool,
    closing: bool,
    window_open: bool,
}

fn trusted_window_url(url: &tauri::Url, dev_url: Option<&tauri::Url>) -> bool {
    let local_asset = url.port().is_none()
        && ((url.scheme() == "tauri" && url.host_str() == Some("localhost"))
            || (matches!(url.scheme(), "http" | "https")
                && url.host_str() == Some("tauri.localhost")));
    let development_origin =
        cfg!(debug_assertions) && dev_url.is_some_and(|dev| dev.origin() == url.origin());
    (local_asset || development_origin)
        && matches!(url.path(), "/" | "/index.html")
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
}

fn validate_caller(window: &WebviewWindow, expected_label: &str) -> Result<(), String> {
    if window.label() != expected_label {
        return Err("Only the matching native approval window may use this command.".to_string());
    }
    let url = window
        .url()
        .map_err(|_| "Unable to verify the approval window.".to_string())?;
    if !trusted_window_url(&url, window.app_handle().config().build.dev_url.as_ref()) {
        return Err("The approval window origin is not trusted.".to_string());
    }
    Ok(())
}

fn build_window(app: &AppHandle, label: &str) -> Result<WebviewWindow, String> {
    let dev_url = app.config().build.dev_url.clone();
    WebviewWindowBuilder::new(app, label, WebviewUrl::App("index.html".into()))
        .on_navigation(move |url| trusted_window_url(url, dev_url.as_ref()))
        .title("DataNexa Write Approval")
        // Wide enough for a before/after diff of two full values per row and for
        // readable SQL; the layout itself is unchanged.
        .inner_size(720.0, 540.0)
        .min_inner_size(720.0, 460.0)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .decorations(false)
        .transparent(true)
        .shadow(true)
        .visible(false)
        .center()
        .build()
        .map_err(|_| "Unable to create the native approval window.".to_string())
}

fn pending(status: &WriteStatus) -> bool {
    status.status == WriteOperationStatus::Pending
}

pub fn is_visible(app: &AppHandle) -> bool {
    [WINDOW_LABEL, PREVIEW_WINDOW_LABEL].iter().any(|label| {
        app.get_webview_window(label)
            .is_some_and(|window| window.is_visible().unwrap_or(true))
    })
}

fn reveal_ready_window(window: &WebviewWindow) -> Result<(), String> {
    window
        .show()
        .map_err(|_| "Unable to show the approval window.".to_string())?;
    crate::sync_dock_visibility(window.app_handle());
    window
        .set_focus()
        .map_err(|_| "Unable to focus the approval window.".to_string())
}

fn destroy_window(window: &WebviewWindow, session: &mut ApprovalSession) -> Result<(), String> {
    if session.closing {
        return Err("The approval window is still closing; wait for it to finish.".to_string());
    }
    session.closing = true;
    if window.destroy().is_err() {
        // Keep the visible window and its binding available for a close retry.
        session.closing = false;
        return Err("Unable to destroy the approval window; close it again to retry.".to_string());
    }
    session.ready = false;
    Ok(())
}

pub async fn show_write_approval(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<Arc<AppState>>().inner().clone();
    if state
        .writes
        .current_approval()
        .await
        .map_err(|error| error.message)?
        .is_none()
    {
        return Ok(());
    }
    let controller = app.state::<ApprovalWindowState>();
    let mut session = controller.session.lock().await;
    if session.closing {
        return Err(
            "The approval window is still closing; query the accepted request before retrying."
                .to_string(),
        );
    }
    if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
        window
            .emit_to(WINDOW_LABEL, CHANGED_EVENT, ())
            .map_err(|_| "Unable to refresh the approval window.".to_string())?;
        if session.ready && session.bound.is_some() {
            reveal_ready_window(&window)?;
        }
        return Ok(());
    }
    if session.window_open {
        return Err("The previous approval window is being removed; query the accepted request before retrying.".to_string());
    }
    build_window(app, WINDOW_LABEL)?;
    session.window_open = true;
    Ok(())
}

/// Opens (or re-focuses) the approval window for the oldest request awaiting a
/// decision. Used when the automatic window creation failed and by Write Center
/// to look at a pending request again.
#[tauri::command]
pub async fn open_write_approval(app: AppHandle) -> Result<(), String> {
    let state = app.state::<Arc<AppState>>().inner().clone();
    if state.writes.pending_approvals().await == 0 {
        // Stable sentinel so the desktop UI can localize it.
        return Err("no_pending_approval".to_string());
    }
    show_write_approval(&app).await
}

#[tauri::command]
pub async fn preview_write_approval(app: AppHandle) -> Result<(), String> {
    let controller = app.state::<PreviewWindowState>();
    let mut session = controller.session.lock().await;
    if session.closing {
        return Err("The approval preview is still closing; wait for it to finish.".to_string());
    }
    if let Some(window) = app.get_webview_window(PREVIEW_WINDOW_LABEL) {
        if session.ready {
            reveal_ready_window(&window)?;
        }
        return Ok(());
    }
    if session.window_open {
        return Err(
            "The previous approval preview is still being removed; retry shortly.".to_string(),
        );
    }
    // Preview data and simulated decisions remain entirely in the local UI.
    build_window(&app, PREVIEW_WINDOW_LABEL)?;
    session.window_open = true;
    Ok(())
}

#[tauri::command]
pub async fn ready_write_approval_preview(window: WebviewWindow) -> Result<(), String> {
    validate_caller(&window, PREVIEW_WINDOW_LABEL)?;
    let controller = window.app_handle().state::<PreviewWindowState>();
    let mut session = controller.session.lock().await;
    if session.closing || !session.window_open {
        return Err("The approval preview is closing or unavailable.".to_string());
    }
    reveal_ready_window(&window)?;
    session.ready = true;
    Ok(())
}

async fn close_preview(app: &AppHandle) -> Result<(), String> {
    let controller = app.state::<PreviewWindowState>();
    let mut session = controller.session.lock().await;
    if session.closing {
        return Err("The approval preview is still closing; wait for it to finish.".to_string());
    }
    if let Some(window) = app.get_webview_window(PREVIEW_WINDOW_LABEL) {
        session.closing = true;
        if window.destroy().is_err() {
            session.closing = false;
            return Err(
                "Unable to destroy the approval preview; close it again to retry.".to_string(),
            );
        }
    } else {
        crate::sync_dock_visibility(app);
    }
    session.ready = false;
    Ok(())
}

#[tauri::command]
pub async fn close_write_approval_preview(window: WebviewWindow) -> Result<(), String> {
    validate_caller(&window, PREVIEW_WINDOW_LABEL)?;
    close_preview(window.app_handle()).await
}

#[tauri::command]
pub fn drag_write_approval_preview(window: WebviewWindow) -> Result<(), String> {
    validate_caller(&window, PREVIEW_WINDOW_LABEL)?;
    window
        .start_dragging()
        .map_err(|_| "Unable to drag the approval preview.".to_string())
}

pub fn native_preview_destroyed(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let controller = app.state::<PreviewWindowState>();
        let mut session = controller.session.lock().await;
        // RunEvent observes removal from the registry before allowing a reopen.
        *session = PreviewSession::default();
        drop(session);
        crate::sync_dock_visibility(&app);
    });
}

pub fn native_preview_close_requested(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if close_preview(&app).await.is_err() {
            crate::debug_log::warn(
                "write_approval",
                format_args!("Unable to close the approval preview"),
            );
        }
    });
}

#[tauri::command]
pub async fn get_write_approval(
    state: State<'_, Arc<AppState>>,
    window: WebviewWindow,
) -> Result<Option<WriteApprovalDto>, String> {
    validate_caller(&window, WINDOW_LABEL)?;
    let controller = window.app_handle().state::<ApprovalWindowState>();
    let mut session = controller.session.lock().await;
    if session.closing {
        return Ok(None);
    }
    if let Some(bound) = session.bound.as_mut() {
        // One window reviews exactly one request; it closes after the decision.
        bound.request = state
            .writes
            .approval_status(&bound.request.request_id)
            .await
            .map_err(|error| error.message)?;
        return Ok(session.bound.clone());
    }
    session.bound = state
        .writes
        .current_approval()
        .await
        .map_err(|error| error.message)?;
    session.ready = false;
    if session.bound.is_none() {
        destroy_window(&window, &mut session)?;
    }
    Ok(session.bound.clone())
}

#[tauri::command]
pub async fn ready_write_approval(window: WebviewWindow) -> Result<(), String> {
    validate_caller(&window, WINDOW_LABEL)?;
    let controller = window.app_handle().state::<ApprovalWindowState>();
    let mut session = controller.session.lock().await;
    if session.closing {
        return Err("The approval window is closing.".to_string());
    }
    if session.bound.is_none() {
        return Err("No approval is bound to this window.".to_string());
    }
    reveal_ready_window(&window)?;
    session.ready = true;
    Ok(())
}

#[tauri::command]
pub async fn decide_write_approval(
    request_id: String,
    revision: u64,
    approve: bool,
    state: State<'_, Arc<AppState>>,
    window: WebviewWindow,
) -> Result<WriteStatus, String> {
    validate_caller(&window, WINDOW_LABEL)?;
    let controller = window.app_handle().state::<ApprovalWindowState>();
    let mut session = controller.session.lock().await;
    let bound = session
        .bound
        .as_ref()
        .ok_or_else(|| "No approval is bound to this window.".to_string())?;
    if !session.ready
        || session.closing
        || !window.is_visible().unwrap_or(false)
        || bound.request.request_id != request_id
        || bound.request.revision != revision
        || !pending(&bound.request)
    {
        return Err("The approval changed; refresh it before deciding.".to_string());
    }
    let decision = if approve {
        WriteDecision::Approve
    } else {
        WriteDecision::Reject
    };
    let status = state
        .writes
        .decide(state.inner().clone(), request_id, revision, decision)
        .await
        .map_err(|error| error.message)?;
    if let Some(bound) = session.bound.as_mut() {
        bound.request = status.clone();
    }
    Ok(status)
}

async fn close_bound(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<Arc<AppState>>().inner().clone();
    let controller = app.state::<ApprovalWindowState>();
    let mut session = controller.session.lock().await;
    if session.closing {
        return Err("The approval window is still closing; wait for it to finish.".to_string());
    }
    if let Some(bound) = session.bound.as_mut() {
        bound.request = state
            .writes
            .approval_status(&bound.request.request_id)
            .await
            .map_err(|error| error.message)?;
        if pending(&bound.request) {
            bound.request = state
                .writes
                .decide(
                    state.clone(),
                    bound.request.request_id.clone(),
                    bound.request.revision,
                    WriteDecision::Cancel,
                )
                .await
                .map_err(|error| error.message)?;
        }
    }
    if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
        destroy_window(&window, &mut session)?;
    } else {
        session.bound = None;
        session.ready = false;
        session.closing = false;
        crate::sync_dock_visibility(app);
    }
    Ok(())
}

#[tauri::command]
pub async fn close_write_approval(window: WebviewWindow) -> Result<(), String> {
    validate_caller(&window, WINDOW_LABEL)?;
    close_bound(window.app_handle()).await
}

#[tauri::command]
pub fn drag_write_approval(window: WebviewWindow) -> Result<(), String> {
    validate_caller(&window, WINDOW_LABEL)?;
    window
        .start_dragging()
        .map_err(|_| "Unable to drag the approval window.".to_string())
}

pub fn native_destroyed(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let controller = app.state::<ApprovalWindowState>();
        let mut session = controller.session.lock().await;
        // Called from RunEvent after Tauri removes the destroyed window from
        // its registry. No timer can leave the controller permanently closing.
        let previous = session.bound.take();
        session.ready = false;
        session.closing = false;
        session.window_open = false;
        drop(session);
        crate::sync_dock_visibility(&app);

        let state = app.state::<Arc<AppState>>().inner().clone();
        if let Some(previous) = previous {
            let result = async {
                let status = state
                    .writes
                    .approval_status(&previous.request.request_id)
                    .await?;
                if pending(&status) {
                    state
                        .writes
                        .decide(
                            state.clone(),
                            status.request_id,
                            status.revision,
                            WriteDecision::Cancel,
                        )
                        .await?;
                }
                Ok::<(), crate::write::WriteError>(())
            }
            .await;
            if result.is_err() {
                crate::debug_log::warn(
                    "write_approval",
                    format_args!(
                        "Unable to cancel the request bound to a destroyed approval window"
                    ),
                );
            }
        }
        if show_write_approval(&app).await.is_err() {
            // Cancel only requests still awaiting a usable approval window;
            // never interrupt an already approved transaction during recovery.
            let cancellation = async {
                for _ in 0..16 {
                    let Some(approval) = state.writes.current_approval().await? else {
                        break;
                    };
                    state
                        .writes
                        .decide(
                            state.clone(),
                            approval.request.request_id,
                            approval.request.revision,
                            WriteDecision::Cancel,
                        )
                        .await?;
                }
                Ok::<(), crate::write::WriteError>(())
            }
            .await;
            if cancellation.is_err() {
                crate::debug_log::warn(
                    "write_approval",
                    format_args!("Unable to cancel queued approvals after window recovery failed"),
                );
            }
            crate::debug_log::warn(
                "write_approval",
                format_args!("Unable to recreate the approval window"),
            );
        }
    });
}

pub fn native_close_requested(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if close_bound(&app).await.is_err() {
            crate::debug_log::warn(
                "write_approval",
                format_args!("Unable to cancel or close the current write approval"),
            );
        }
    });
}
