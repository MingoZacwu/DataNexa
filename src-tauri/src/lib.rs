mod audit;
mod commands;
mod config;
mod db;
mod i18n;
mod mcp;
mod policy;
mod state;
mod vault;

use std::sync::Arc;

use commands::{
    clear_audit_events, delete_connection, diagnose_connection, disable_all_connections,
    get_app_snapshot, hide_main_window, minimize_main_window, open_project_homepage, policy_check,
    rotate_server_token, save_server_config, save_settings_config, set_connection_enabled,
    set_mcp_tool_enabled, start_mcp_server, start_window_drag, stop_mcp_server, test_connection,
    test_connection_input, upsert_connection,
};
use i18n::{backend_text, BackendText};
use state::AppState;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, WebviewWindow, WindowEvent};

fn create_tray_menu(
    app: &AppHandle,
    text: BackendText,
    mcp_running: bool,
) -> tauri::Result<Menu<tauri::Wry>> {
    let show_item = MenuItem::with_id(app, "show", text.tray_show(), true, None::<&str>)?;
    let mcp_item = CheckMenuItem::with_id(
        app,
        "toggle_mcp",
        text.tray_mcp_server(),
        true,
        mcp_running,
        None::<&str>,
    )?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit_item = MenuItem::with_id(app, "quit", text.tray_quit(), true, None::<&str>)?;
    Menu::with_items(app, &[&show_item, &mcp_item, &separator, &quit_item])
}

pub(crate) fn refresh_tray_menu(
    app: &AppHandle,
    text: BackendText,
    mcp_running: bool,
) -> tauri::Result<()> {
    if let Some(tray) = app.tray_by_id("main") {
        tray.set_menu(Some(create_tray_menu(app, text, mcp_running)?))?;
    }
    Ok(())
}

fn set_dock_visibility(app: &AppHandle, visible: bool) -> tauri::Result<()> {
    #[cfg(target_os = "macos")]
    app.set_dock_visibility(visible)?;

    #[cfg(not(target_os = "macos"))]
    let _ = (app, visible);

    Ok(())
}

fn show_main_window(app: &AppHandle) {
    let _ = set_dock_visibility(app, true);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

pub(crate) fn hide_main_window_to_tray(window: &WebviewWindow) -> tauri::Result<()> {
    window.hide()?;
    set_dock_visibility(window.app_handle(), false)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let mut state = AppState::new(app.handle().clone())?;
            let tray_text = backend_text(&state.config.get_mut().settings.language);
            let state = Arc::new(state);
            app.manage(state);

            #[cfg(target_os = "windows")]
            let tray_icon = app.default_window_icon().cloned().unwrap_or_else(|| {
                tauri::image::Image::new(include_bytes!("../../resources/trayicon.rgba"), 32, 32)
            });

            #[cfg(not(target_os = "windows"))]
            let tray_icon =
                tauri::image::Image::new(include_bytes!("../../resources/trayicon.rgba"), 32, 32);

            let tray_menu = create_tray_menu(app.handle(), tray_text, false)?;
            TrayIconBuilder::with_id("main")
                .icon(tray_icon)
                .tooltip("DataNexa")
                .menu(&tray_menu)
                .show_menu_on_left_click(false)
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main_window(tray.app_handle());
                    }
                })
                .build(app)?;

            Ok(())
        })
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => {
                show_main_window(app);
            }
            "toggle_mcp" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let state = app.state::<Arc<AppState>>().inner().clone();
                    let running = state.mcp.read().await.running;
                    let result = if running {
                        mcp::stop(state.clone()).await;
                        Ok(())
                    } else {
                        mcp::start(state.clone()).await.map(|_| ())
                    };

                    if let Err(error) = result {
                        eprintln!("failed to toggle MCP server from tray: {error}");
                    }

                    let running = mcp::status(&state).await.running;
                    let language = state.config.read().await.settings.language.clone();
                    if let Err(error) = refresh_tray_menu(&app, backend_text(&language), running) {
                        eprintln!("failed to refresh tray menu: {error}");
                    }
                });
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                if window.hide().is_ok() {
                    let _ = set_dock_visibility(window.app_handle(), false);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_app_snapshot,
            save_server_config,
            save_settings_config,
            set_mcp_tool_enabled,
            upsert_connection,
            delete_connection,
            set_connection_enabled,
            disable_all_connections,
            clear_audit_events,
            test_connection,
            test_connection_input,
            diagnose_connection,
            start_mcp_server,
            stop_mcp_server,
            rotate_server_token,
            minimize_main_window,
            hide_main_window,
            start_window_drag,
            open_project_homepage,
            policy_check
        ])
        .build(tauri::generate_context!())
        .expect("failed to build DataNexa")
        .run(|app, event| {
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen {
                has_visible_windows,
                ..
            } = event
            {
                if !has_visible_windows {
                    show_main_window(app);
                }
            }

            #[cfg(not(target_os = "macos"))]
            let _ = (app, event);
        });
}
