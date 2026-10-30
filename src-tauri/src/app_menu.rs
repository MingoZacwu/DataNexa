//! Native macOS menu bar.
//!
//! Tauri only ships an English default menu, while DataNexa's UI language is an
//! application setting that is deliberately independent of the system language.
//! So the menu is rebuilt from translated labels here: the standard AppKit
//! behaviour (about, services, hide, edit, window) is preserved through
//! predefined items, and the Help menu gets a documentation entry.
//!
//! The menu is installed during setup and reinstalled by
//! `save_settings_config` whenever the language changes, because native menu
//! labels are baked in when the menu is constructed.

/// Identifier of the documentation item; the click is handled in `lib.rs`.
pub const HELP_DOCS_MENU_ID: &str = "help_docs";

/// macOS only: the other platforms render no native menu bar (the window is
/// undecorated) and expose the documentation from the About page instead.
#[cfg(target_os = "macos")]
mod macos {
    use tauri::menu::{AboutMetadata, Menu, MenuItem, PredefinedMenuItem, Submenu};
    use tauri::{AppHandle, Runtime};

    use super::HELP_DOCS_MENU_ID;
    use crate::i18n::{BackendText, Locale};

    /// The Help item's accelerator string. Tauri parses accelerators as
    /// physical key codes, so this is the physical Cmd+Shift+/ combination;
    /// whether AppKit renders a shortcut in the menu bar is up to the OS (it
    /// currently does not on macOS 27). Opening the documentation with the
    /// keyboard is handled by the front end, so it does not depend on this
    /// string being drawn.
    const HELP_DOCS_ACCELERATOR: &str = "CmdOrCtrl+Shift+/";

    struct Labels {
        about: String,
        hide: String,
        quit: String,
        docs: String,
        services: &'static str,
        hide_others: &'static str,
        show_all: &'static str,
        file: &'static str,
        close_window: &'static str,
        edit: &'static str,
        undo: &'static str,
        redo: &'static str,
        cut: &'static str,
        copy: &'static str,
        paste: &'static str,
        select_all: &'static str,
        window: &'static str,
        minimize: &'static str,
        bring_all_to_front: &'static str,
        help: &'static str,
    }

    impl Labels {
        fn new(locale: Locale, app_name: &str) -> Self {
            match locale {
                Locale::ZhCn => Self {
                    about: format!("关于 {app_name}"),
                    hide: format!("隐藏 {app_name}"),
                    quit: format!("退出 {app_name}"),
                    docs: format!("{app_name} 帮助文档"),
                    services: "服务",
                    hide_others: "隐藏其他",
                    show_all: "全部显示",
                    file: "文件",
                    close_window: "关闭窗口",
                    edit: "编辑",
                    undo: "撤销",
                    redo: "重做",
                    cut: "剪切",
                    copy: "复制",
                    paste: "粘贴",
                    select_all: "全选",
                    window: "窗口",
                    minimize: "最小化",
                    bring_all_to_front: "前置全部窗口",
                    help: "帮助",
                },
                Locale::En => Self {
                    about: format!("About {app_name}"),
                    hide: format!("Hide {app_name}"),
                    quit: format!("Quit {app_name}"),
                    docs: format!("{app_name} Documentation"),
                    services: "Services",
                    hide_others: "Hide Others",
                    show_all: "Show All",
                    file: "File",
                    close_window: "Close Window",
                    edit: "Edit",
                    undo: "Undo",
                    redo: "Redo",
                    cut: "Cut",
                    copy: "Copy",
                    paste: "Paste",
                    select_all: "Select All",
                    window: "Window",
                    minimize: "Minimize",
                    bring_all_to_front: "Bring All to Front",
                    help: "Help",
                },
            }
        }
    }

    /// Replace the application menu with a localized one.
    pub fn install<R: Runtime>(app: &AppHandle<R>, text: BackendText) -> tauri::Result<()> {
        let package = app.package_info();
        let labels = Labels::new(text.locale(), &package.name);
        let about_metadata = AboutMetadata {
            name: Some(package.name.clone()),
            version: Some(package.version.to_string()),
            copyright: app.config().bundle.copyright.clone(),
            ..Default::default()
        };

        let app_menu = Submenu::with_items(
            app,
            package.name.clone(),
            true,
            &[
                &PredefinedMenuItem::about(app, Some(labels.about.as_str()), Some(about_metadata))?,
                &PredefinedMenuItem::separator(app)?,
                &PredefinedMenuItem::services(app, Some(labels.services))?,
                &PredefinedMenuItem::separator(app)?,
                &PredefinedMenuItem::hide(app, Some(labels.hide.as_str()))?,
                &PredefinedMenuItem::hide_others(app, Some(labels.hide_others))?,
                &PredefinedMenuItem::show_all(app, Some(labels.show_all))?,
                &PredefinedMenuItem::separator(app)?,
                &PredefinedMenuItem::quit(app, Some(labels.quit.as_str()))?,
            ],
        )?;

        let file_menu = Submenu::with_items(
            app,
            labels.file,
            true,
            &[&PredefinedMenuItem::close_window(
                app,
                Some(labels.close_window),
            )?],
        )?;

        let edit_menu = Submenu::with_items(
            app,
            labels.edit,
            true,
            &[
                &PredefinedMenuItem::undo(app, Some(labels.undo))?,
                &PredefinedMenuItem::redo(app, Some(labels.redo))?,
                &PredefinedMenuItem::separator(app)?,
                &PredefinedMenuItem::cut(app, Some(labels.cut))?,
                &PredefinedMenuItem::copy(app, Some(labels.copy))?,
                &PredefinedMenuItem::paste(app, Some(labels.paste))?,
                &PredefinedMenuItem::select_all(app, Some(labels.select_all))?,
            ],
        )?;

        let window_menu = Submenu::with_items(
            app,
            labels.window,
            true,
            &[
                &PredefinedMenuItem::minimize(app, Some(labels.minimize))?,
                &PredefinedMenuItem::separator(app)?,
                &PredefinedMenuItem::bring_all_to_front(app, Some(labels.bring_all_to_front))?,
            ],
        )?;

        let docs_item = MenuItem::with_id(
            app,
            HELP_DOCS_MENU_ID,
            labels.docs.as_str(),
            true,
            Some(HELP_DOCS_ACCELERATOR),
        )?;
        let help_menu = Submenu::with_items(app, labels.help, true, &[&docs_item])?;

        let menu = Menu::with_items(
            app,
            &[&app_menu, &file_menu, &edit_menu, &window_menu, &help_menu],
        )?;
        app.set_menu(menu)?;

        Ok(())
    }
}

#[cfg(target_os = "macos")]
pub use macos::install;
