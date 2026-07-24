mod audit;
mod commands;
mod config;
mod db;
mod i18n;
mod mcp;
mod policy;
mod startup;
mod state;
mod vault;

#[cfg(feature = "updater")]
mod updater;

use std::sync::Arc;

use commands::{
    check_updates_if_due, clear_audit_events, clear_legacy_audit_log, delete_connection,
    diagnose_connection, disable_all_connections, export_connections, get_app_snapshot,
    hide_main_window, import_connections, minimize_main_window, open_project_homepage,
    open_project_releases, open_project_site, policy_check, retry_audit_migration,
    rotate_server_token,
    save_server_config, save_settings_config, set_connection_enabled, set_mcp_tool_enabled,
    start_mcp_server, start_window_drag, stop_mcp_server, test_connection, test_connection_input,
    upsert_connection,
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
    startup_error: bool,
    audit_ready: bool,
) -> tauri::Result<Menu<tauri::Wry>> {
    let show_item = MenuItem::with_id(app, "show", text.tray_show(), true, None::<&str>)?;
    let mcp_item = CheckMenuItem::with_id(
        app,
        "toggle_mcp",
        text.tray_mcp_server(),
        audit_ready,
        mcp_running,
        None::<&str>,
    )?;
    let separator = PredefinedMenuItem::separator(app)?;
    let error_item = MenuItem::with_id(
        app,
        "mcp_error",
        text.tray_mcp_startup_error(),
        false,
        None::<&str>,
    )?;
    let quit_item = MenuItem::with_id(app, "quit", text.tray_quit(), true, None::<&str>)?;
    if startup_error {
        Menu::with_items(
            app,
            &[&show_item, &mcp_item, &separator, &error_item, &quit_item],
        )
    } else {
        Menu::with_items(app, &[&show_item, &mcp_item, &separator, &quit_item])
    }
}

pub(crate) fn refresh_tray_menu(
    app: &AppHandle,
    text: BackendText,
    mcp_running: bool,
    startup_error: bool,
    audit_ready: bool,
) -> tauri::Result<()> {
    if let Some(tray) = app.tray_by_id("main") {
        tray.set_menu(Some(create_tray_menu(
            app,
            text,
            mcp_running,
            startup_error,
            audit_ready,
        )?))?;
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
    let _ = startup::set_activation_policy(true);
    let _ = set_dock_visibility(app, true);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

pub(crate) fn hide_main_window_to_tray(window: &WebviewWindow) -> tauri::Result<()> {
    window.hide()?;
    let _ = startup::set_activation_policy(false);
    set_dock_visibility(window.app_handle(), false)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main_window(app);
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init());

    #[cfg(feature = "updater")]
    let builder = builder.plugin({
        let updater = tauri_plugin_updater::Builder::new();
        #[cfg(target_os = "macos")]
        let updater = updater.target("darwin-universal");
        updater.build()
    });

    builder
        .setup(|app| {
            let mut state =
                tauri::async_runtime::block_on(async { AppState::new(app.handle().clone()) })?;
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

            let tray_menu = create_tray_menu(app.handle(), tray_text, false, false, false)?;
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

            let state = app.state::<Arc<AppState>>().inner().clone();
            let autostart = std::env::args().any(|arg| arg == "--autostart");
            let login_launch = autostart || startup::launched_at_login();
            let configured = state
                .config
                .try_read()
                .map(|config| config.settings.auto_start_mcp)
                .unwrap_or(false);
            // Reconcile the Run registry value with the persistent preference so
            // that manual reinstalls (which force uninstall-then-install on
            // Windows NSIS) do not leave the auto-start entry missing after
            // upgrade. Windows-only: macOS login items are owned by
            // launchd/SMAppService and are not affected by NSIS.
            #[cfg(target_os = "windows")]
            {
                if configured {
                    if let Err(error) = startup::enable() {
                        eprintln!("failed to restore auto-start registry: {error}");
                    }
                } else {
                    if let Err(error) = startup::disable() {
                        eprintln!("failed to clear auto-start registry: {error}");
                    }
                }
            }
            let app_handle = app.handle().clone();
            let state_for_task = state.clone();
            tauri::async_runtime::spawn(async move {
                let max_events = state_for_task.config.read().await.settings.audit_max_events;
                let migration_result = state_for_task.audit.initialize(max_events).await;
                if migration_result.is_ok() && configured && login_launch {
                    let started = std::time::Instant::now();
                    if let Err(error) = mcp::start(state_for_task.clone()).await {
                        let reason = error.to_string();
                        state_for_task.mcp.write().await.startup_error = Some(reason.clone());
                        commands::record_startup_event(
                            &state_for_task,
                            "system.auto_start_mcp",
                            reason,
                            started.elapsed(),
                        )
                        .await;
                    }
                }
                let running = mcp::status(&state_for_task).await.running;
                let error = state_for_task.mcp.read().await.startup_error.is_some();
                let language = state_for_task.config.read().await.settings.language.clone();
                let ready = state_for_task.audit.is_ready().await;
                let _ =
                    refresh_tray_menu(&app_handle, backend_text(&language), running, error, ready);
            });
            if login_launch {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = hide_main_window_to_tray(&window);
                }
            } else {
                show_main_window(app.handle());
            }

            #[cfg(feature = "updater")]
            updater::spawn_updater_task(app.handle().clone());

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
                        let started = std::time::Instant::now();
                        match mcp::start(state.clone()).await {
                            Ok(_) => Ok(()),
                            Err(error) => {
                                let reason = error.to_string();
                                commands::record_startup_event(
                                    &state,
                                    "system.start_mcp",
                                    reason,
                                    started.elapsed(),
                                )
                                .await;
                                Err(error)
                            }
                        }
                    };

                    if let Err(error) = result {
                        eprintln!("failed to toggle MCP server from tray: {error}");
                    }

                    let running = mcp::status(&state).await.running;
                    let language = state.config.read().await.settings.language.clone();
                    let startup_error = state.mcp.read().await.startup_error.is_some();
                    if let Err(error) = refresh_tray_menu(
                        &app,
                        backend_text(&language),
                        running,
                        startup_error,
                        state.audit.is_ready().await,
                    ) {
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
            export_connections,
            import_connections,
            set_mcp_tool_enabled,
            upsert_connection,
            delete_connection,
            set_connection_enabled,
            disable_all_connections,
            clear_audit_events,
            retry_audit_migration,
            clear_legacy_audit_log,
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
            open_project_releases,
            open_project_site,
            policy_check,
            check_updates_if_due
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
