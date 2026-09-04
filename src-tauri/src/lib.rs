mod access_control;
mod audit;
mod commands;
mod config;
mod db;
mod i18n;
mod jdbc;
mod mcp;
mod policy;
mod startup;
mod state;
mod vault;

#[cfg(feature = "updater")]
mod updater;

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use commands::{
    check_updates_if_due, clear_audit_events, clear_legacy_audit_log, create_access_token,
    delete_access_token, delete_connection, delete_jdbc_driver, diagnose_connection,
    disable_all_connections, export_connections, get_access_token_secret, get_app_snapshot,
    get_jdbc_status, get_jdbc_storage_status, hide_main_window, import_connections,
    import_jdbc_driver, install_jdbc_driver, minimize_main_window, open_project_homepage,
    open_project_releases, open_project_site, policy_check, rename_access_token,
    retry_audit_migration, rotate_access_token, save_server_config, save_settings_config,
    set_access_token_enabled, set_connection_enabled, set_mcp_tool_enabled,
    set_token_connection_allowed, set_token_tool_allowed, set_window_material_theme,
    start_mcp_server, start_window_drag, stop_mcp_server, test_connection, test_connection_input,
    upsert_connection,
};
use i18n::{backend_text, BackendText};
use state::AppState;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, WebviewWindow, WebviewWindowBuilder, WindowEvent};

const LIGHTWEIGHT_MODE_DELAY: Duration = Duration::from_secs(5 * 60);

#[allow(clippy::needless_return)]
pub(crate) fn apply_system_material(
    window: &WebviewWindow,
    dark: Option<bool>,
) -> Result<bool, String> {
    #[cfg(not(target_os = "windows"))]
    let _ = dark;

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let _ = window;

    #[cfg(target_os = "windows")]
    {
        if let Err(error) = window_vibrancy::apply_mica(window, dark) {
            eprintln!("Windows Mica unavailable, using the standard CSS appearance: {error}");
            let _ = window_vibrancy::clear_acrylic(window);
            let _ = window.set_shadow(true);
            return Ok(false);
        }
        let _ = window.set_shadow(true);
        return Ok(true);
    }

    #[cfg(target_os = "macos")]
    {
        window_vibrancy::apply_vibrancy(
            window,
            window_vibrancy::NSVisualEffectMaterial::Sidebar,
            None,
            None,
        )
        .map_err(|error| error.to_string())?;
        return Ok(true);
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    Ok(false)
}

#[derive(Default)]
struct MainWindowState {
    lifecycle: tokio::sync::Mutex<()>,
    generation: AtomicU64,
    lightweight_active: AtomicBool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrayMcpStatus {
    Running,
    Stopped,
    Error,
}

fn tray_mcp_status(running: bool, startup_error: bool) -> TrayMcpStatus {
    if startup_error {
        TrayMcpStatus::Error
    } else if running {
        TrayMcpStatus::Running
    } else {
        TrayMcpStatus::Stopped
    }
}

fn tray_status_text(text: BackendText, status: TrayMcpStatus) -> &'static str {
    match status {
        TrayMcpStatus::Running => text.tray_mcp_status_running(),
        TrayMcpStatus::Stopped => text.tray_mcp_status_stopped(),
        TrayMcpStatus::Error => text.tray_mcp_status_error(),
    }
}

#[allow(clippy::needless_return)]
fn status_tray_icon(status: TrayMcpStatus) -> tauri::image::Image<'static> {
    #[cfg(target_os = "windows")]
    {
        let rgba = match status {
            TrayMcpStatus::Running => {
                include_bytes!("../../resources/tray/windows-running.rgba").to_vec()
            }
            TrayMcpStatus::Stopped => {
                include_bytes!("../../resources/tray/windows-stopped.rgba").to_vec()
            }
            TrayMcpStatus::Error => {
                include_bytes!("../../resources/tray/windows-error.rgba").to_vec()
            }
        };
        return tauri::image::Image::new_owned(rgba, 32, 32);
    }

    #[cfg(not(target_os = "windows"))]
    {
        let mut rgba = include_bytes!("../../resources/trayicon.rgba").to_vec();
        if status == TrayMcpStatus::Running {
            // Explicit mirrored rows keep the triangular border pixel-perfect.
            let outer_right = [
                17, 19, 21, 23, 25, 27, 29, 30, 30, 29, 27, 25, 23, 21, 19, 17,
            ];
            for (row, right) in outer_right.into_iter().enumerate() {
                let y = 15 + row;
                for x in 16..=right {
                    let index = (y * 32 + x) * 4;
                    rgba[index..index + 4].copy_from_slice(&[255, 255, 255, 0]);
                }
            }
            let inner_right = [20, 22, 24, 26, 28, 28, 26, 24, 22, 20];
            for (row, right) in inner_right.into_iter().enumerate() {
                let y = 18 + row;
                for x in 19..=right {
                    let index = (y * 32 + x) * 4;
                    rgba[index..index + 4].copy_from_slice(&[255, 255, 255, 255]);
                }
            }
        } else if status == TrayMcpStatus::Error {
            // Isolate a large warning triangle from the main template artwork.
            let outer_left = [
                22, 22, 21, 21, 20, 19, 19, 18, 17, 17, 16, 15, 15, 14, 14, 14,
            ];
            let outer_right = [
                23, 23, 24, 24, 25, 26, 26, 27, 28, 28, 29, 30, 30, 31, 31, 31,
            ];
            for (row, (left, right)) in outer_left.into_iter().zip(outer_right).enumerate() {
                let y = 15 + row;
                for x in left..=right {
                    let index = (y * 32 + x) * 4;
                    rgba[index..index + 4].copy_from_slice(&[255, 255, 255, 0]);
                }
            }
            let inner_left = [22, 22, 21, 21, 20, 20, 19, 18, 18, 17, 16, 16];
            let inner_right = [23, 23, 24, 24, 25, 25, 26, 27, 27, 28, 29, 29];
            for (row, (left, right)) in inner_left.into_iter().zip(inner_right).enumerate() {
                let y = 17 + row;
                for x in left..=right {
                    let index = (y * 32 + x) * 4;
                    rgba[index..index + 4].copy_from_slice(&[255, 255, 255, 255]);
                }
            }
            // Transparent cutouts create the exclamation mark in template mode.
            for y in 19..24 {
                for x in 22..24 {
                    let index = ((y * 32 + x) * 4) as usize;
                    rgba[index..index + 4].copy_from_slice(&[255, 255, 255, 0]);
                }
            }
            for y in 26..28 {
                for x in 22..24 {
                    let index = ((y * 32 + x) * 4) as usize;
                    rgba[index..index + 4].copy_from_slice(&[255, 255, 255, 0]);
                }
            }
        }
        tauri::image::Image::new_owned(rgba, 32, 32)
    }
}

fn refresh_tray_icon(
    app: &AppHandle,
    text: BackendText,
    running: bool,
    startup_error: bool,
) -> tauri::Result<()> {
    let status = tray_mcp_status(running, startup_error);
    if let Some(tray) = app.tray_by_id("main") {
        let icon = status_tray_icon(status);

        tray.set_icon_with_as_template(Some(icon), cfg!(target_os = "macos"))?;
        tray.set_tooltip(Some(format!(
            "DataNexa - {}",
            tray_status_text(text, status)
        )))?;
    }
    Ok(())
}

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
    let lightweight_active = app
        .try_state::<Arc<MainWindowState>>()
        .map(|state| state.lightweight_active.load(Ordering::Acquire))
        .unwrap_or(false);
    let lightweight_item = CheckMenuItem::with_id(
        app,
        "toggle_lightweight",
        text.tray_lightweight_mode(),
        true,
        lightweight_active,
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
            &[
                &show_item,
                &mcp_item,
                &lightweight_item,
                &separator,
                &error_item,
                &quit_item,
            ],
        )
    } else {
        Menu::with_items(
            app,
            &[
                &show_item,
                &mcp_item,
                &lightweight_item,
                &separator,
                &quit_item,
            ],
        )
    }
}

pub(crate) fn refresh_tray_menu(
    app: &AppHandle,
    text: BackendText,
    mcp_running: bool,
    startup_error: bool,
    audit_ready: bool,
) -> tauri::Result<()> {
    refresh_tray_icon(app, text, mcp_running, startup_error)?;
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

async fn refresh_tray_from_state(app: &AppHandle, state: &Arc<AppState>) -> tauri::Result<()> {
    let running = mcp::status(state).await.running;
    let startup_error = state.mcp.read().await.startup_error.is_some();
    let language = state.config.read().await.settings.language.clone();
    refresh_tray_menu(
        app,
        backend_text(&language),
        running,
        startup_error,
        state.audit.is_ready().await,
    )
}

fn reveal_main_window(window: &WebviewWindow) {
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();
}

fn show_main_window(app: &AppHandle) {
    let _ = startup::set_activation_policy(true);
    let _ = set_dock_visibility(app, true);
    let app_state = app.state::<Arc<AppState>>().inner().clone();
    let window_state = app.state::<Arc<MainWindowState>>().inner().clone();
    window_state.generation.fetch_add(1, Ordering::AcqRel);

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let _lifecycle = window_state.lifecycle.lock().await;
        if !window_state.lightweight_active.load(Ordering::Acquire) {
            if let Some(window) = app.get_webview_window("main") {
                reveal_main_window(&window);
                return;
            }
        }

        for _ in 0..40 {
            if app.get_webview_window("main").is_none() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }

        if app.get_webview_window("main").is_some() {
            eprintln!("failed to recreate main window: previous window is still being destroyed");
            return;
        }

        let Some(config) = app
            .config()
            .app
            .windows
            .iter()
            .find(|config| config.label == "main")
        else {
            eprintln!("failed to recreate main window: main window config is missing");
            return;
        };

        match WebviewWindowBuilder::from_config(&app, config).and_then(|builder| builder.build()) {
            Ok(window) => {
                if let Err(error) = apply_system_material(&window, None) {
                    eprintln!("failed to apply system window material: {error}");
                }
                window_state
                    .lightweight_active
                    .store(false, Ordering::Release);
                reveal_main_window(&window);
                if let Err(error) = refresh_tray_from_state(&app, &app_state).await {
                    eprintln!("failed to refresh tray after recreating main window: {error}");
                }
            }
            Err(error) => eprintln!("failed to recreate main window: {error}"),
        }
    });
}

async fn activate_lightweight_mode(app: AppHandle, generation: u64, automatic: bool) {
    let app_state = app.state::<Arc<AppState>>().inner().clone();
    let window_state = app.state::<Arc<MainWindowState>>().inner().clone();
    let _lifecycle = window_state.lifecycle.lock().await;
    if window_state.generation.load(Ordering::Acquire) != generation {
        return;
    }
    if automatic && !app_state.config.read().await.settings.auto_lightweight_mode {
        return;
    }
    if window_state.generation.load(Ordering::Acquire) != generation {
        return;
    }

    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
        let _ = startup::set_activation_policy(false);
        let _ = set_dock_visibility(&app, false);
        if let Err(error) = window.destroy() {
            eprintln!("failed to activate lightweight mode: {error}");
            return;
        }
    }

    window_state
        .lightweight_active
        .store(true, Ordering::Release);
    if let Err(error) = refresh_tray_from_state(&app, &app_state).await {
        eprintln!("failed to refresh tray after activating lightweight mode: {error}");
    }
}

fn request_lightweight_mode(app: &AppHandle, automatic: bool) {
    let state = app.state::<Arc<MainWindowState>>().inner().clone();
    let generation = state.generation.fetch_add(1, Ordering::AcqRel) + 1;
    let app = app.clone();
    tauri::async_runtime::spawn(activate_lightweight_mode(app, generation, automatic));
}

fn schedule_lightweight_mode(app: &AppHandle) {
    let app_state = app.state::<Arc<AppState>>().inner().clone();
    let window_state = app.state::<Arc<MainWindowState>>().inner().clone();
    let generation = window_state.generation.fetch_add(1, Ordering::AcqRel) + 1;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if !app_state.config.read().await.settings.auto_lightweight_mode {
            return;
        }
        tokio::time::sleep(LIGHTWEIGHT_MODE_DELAY).await;
        activate_lightweight_mode(app, generation, true).await;
    });
}

pub(crate) fn hide_main_window_to_tray(window: &WebviewWindow) -> tauri::Result<()> {
    window.hide()?;
    let _ = startup::set_activation_policy(false);
    let dock_result = set_dock_visibility(window.app_handle(), false);
    schedule_lightweight_mode(window.app_handle());
    dock_result
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
            if let Some(window) = app.get_webview_window("main") {
                if let Err(error) = apply_system_material(&window, None) {
                    eprintln!("failed to apply system window material: {error}");
                }
            }

            let mut state = tauri::async_runtime::block_on(AppState::new(app.handle().clone()))?;
            let tray_text = backend_text(&state.config.get_mut().settings.language);
            let state = Arc::new(state);
            app.manage(state);
            app.manage(Arc::new(MainWindowState::default()));

            let tray_icon = status_tray_icon(TrayMcpStatus::Stopped);

            let tray_menu = create_tray_menu(app.handle(), tray_text, false, false, false)?;
            TrayIconBuilder::with_id("main")
                .icon(tray_icon)
                .icon_as_template(cfg!(target_os = "macos"))
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
            let (configured, auto_lightweight_mode) = state
                .config
                .try_read()
                .map(|config| {
                    (
                        config.settings.auto_start_mcp,
                        config.settings.auto_lightweight_mode,
                    )
                })
                .unwrap_or((false, false));
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
                if auto_lightweight_mode {
                    request_lightweight_mode(app.handle(), true);
                } else if let Some(window) = app.get_webview_window("main") {
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
            "toggle_lightweight" => {
                let state = app.state::<Arc<MainWindowState>>();
                if state.lightweight_active.load(Ordering::Acquire) {
                    show_main_window(app);
                } else {
                    request_lightweight_mode(app, false);
                }
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
            if window.label() == "main" {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    if let Some(main_window) = window.app_handle().get_webview_window("main") {
                        if let Err(error) = hide_main_window_to_tray(&main_window) {
                            eprintln!("failed to hide main window: {error}");
                        }
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_app_snapshot,
            get_jdbc_status,
            get_jdbc_storage_status,
            install_jdbc_driver,
            import_jdbc_driver,
            delete_jdbc_driver,
            create_access_token,
            rename_access_token,
            set_access_token_enabled,
            rotate_access_token,
            delete_access_token,
            get_access_token_secret,
            set_token_connection_allowed,
            set_token_tool_allowed,
            save_server_config,
            save_settings_config,
            export_connections,
            import_connections,
            set_mcp_tool_enabled,
            set_window_material_theme,
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
            if let tauri::RunEvent::ExitRequested { code, api, .. } = &event {
                if code.is_none() {
                    api.prevent_exit();
                }
            }

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

#[cfg(test)]
mod tests {
    use super::{tray_mcp_status, TrayMcpStatus};

    #[test]
    fn startup_error_has_priority_over_running() {
        assert_eq!(tray_mcp_status(true, true), TrayMcpStatus::Error);
    }

    #[test]
    fn running_and_stopped_states_are_distinct() {
        assert_eq!(tray_mcp_status(true, false), TrayMcpStatus::Running);
        assert_eq!(tray_mcp_status(false, false), TrayMcpStatus::Stopped);
    }
}
