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
    delete_connection, diagnose_connection, get_app_snapshot, hide_main_window,
    minimize_main_window, open_project_homepage, policy_check, rotate_server_token,
    save_server_config, save_settings_config, set_mcp_tool_enabled, start_mcp_server,
    start_window_drag, stop_mcp_server, test_connection, upsert_connection,
};
use i18n::backend_text;
use state::AppState;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, TrayIconBuilder, TrayIconEvent};
use tauri::{Manager, WindowEvent};

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

            let show_item =
                MenuItem::with_id(app, "show", tray_text.tray_show(), true, None::<&str>)?;
            let quit_item =
                MenuItem::with_id(app, "quit", tray_text.tray_quit(), true, None::<&str>)?;
            let tray_menu = Menu::with_items(app, &[&show_item, &quit_item])?;
            TrayIconBuilder::with_id("main")
                .icon(tray_icon)
                .tooltip("DataNexa")
                .menu(&tray_menu)
                .show_menu_on_left_click(false)
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::DoubleClick {
                        button: MouseButton::Left,
                        ..
                    } = event
                    {
                        if let Some(window) = tray.app_handle().get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app)?;

            Ok(())
        })
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_app_snapshot,
            save_server_config,
            save_settings_config,
            set_mcp_tool_enabled,
            upsert_connection,
            delete_connection,
            test_connection,
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
        .run(tauri::generate_context!())
        .expect("failed to run DataNexa");
}
