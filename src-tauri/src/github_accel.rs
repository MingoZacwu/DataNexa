use tauri::{AppHandle, Manager};

use crate::state::AppState;

/// Fixed third-party accelerator used for GitHub release downloads.
pub const PROXY_BASE: &str = "https://gh-proxy.com";

const TRUSTED_HOST: &str = "github.com";

/// Whether the user opted into routing GitHub release traffic through the
/// accelerator. Read with `try_read` so a concurrent settings save cannot
/// block an update check; the direct route is the safe fallback.
pub fn enabled(app: &AppHandle) -> bool {
    let Some(state) = app.try_state::<std::sync::Arc<AppState>>() else {
        crate::debug_log::warn(
            "github_accel",
            format_args!("app state is unavailable; using the direct route"),
        );
        return false;
    };
    let enabled = match state.config.try_read() {
        Ok(config) => config.settings.github_acceleration,
        Err(_) => {
            crate::debug_log::warn(
                "github_accel",
                format_args!(
                    "settings are being saved; using the direct route for this GitHub request"
                ),
            );
            false
        }
    };
    enabled
}

/// Prefix a GitHub release URL with the accelerator when enabled. Only the
/// project's own public GitHub URLs are accepted so a malformed manifest
/// cannot turn the accelerator base into an open redirect.
pub fn accelerate(original: &str, accelerated: bool) -> anyhow::Result<String> {
    if !accelerated {
        return Ok(original.to_string());
    }
    let url = tauri::Url::parse(original)?;
    if url.scheme() != "https" {
        return Err(anyhow::anyhow!("GitHub URL must use HTTPS"));
    }
    if url.host_str() != Some(TRUSTED_HOST) {
        return Err(anyhow::anyhow!("URL host is not trusted for acceleration"));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(anyhow::anyhow!("GitHub URL must not contain credentials"));
    }
    Ok(format!("{PROXY_BASE}/{}", url.as_str()))
}

#[cfg(feature = "updater")]
pub fn accelerate_url(original: &tauri::Url, accelerated: bool) -> anyhow::Result<tauri::Url> {
    if !accelerated {
        return Ok(original.clone());
    }
    Ok(tauri::Url::parse(&accelerate(original.as_str(), true)?)?)
}
