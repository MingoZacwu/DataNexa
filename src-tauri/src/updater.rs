use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::UpdaterExt;
use tokio::sync::Mutex;

use crate::state::AppState;

const UPDATE_CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const DISABLED_CHECK_POLL_INTERVAL: Duration = Duration::from_secs(60);
const STATE_FILE_NAME: &str = "updater-state.json";
const UPDATE_AVAILABLE_EVENT: &str = "updater://available";
const JRE_UPDATE_AVAILABLE_EVENT: &str = "jdbc://runtime-update-available";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct UpdaterState {
    #[serde(default)]
    last_check_at: Option<DateTime<Utc>>,
    #[serde(default)]
    available_version: Option<String>,
    #[serde(default)]
    available_for_version: Option<String>,
    #[serde(default)]
    jre_last_check_at: Option<DateTime<Utc>>,
    #[serde(default)]
    jre_available_version: Option<String>,
    #[serde(default)]
    jre_checked_for_version: Option<String>,
}

#[derive(Clone, Serialize)]
struct UpdateAvailablePayload {
    version: String,
    current_version: String,
}

#[derive(Clone, Serialize)]
struct JreUpdateAvailablePayload {
    version: String,
}

fn check_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn jre_check_is_due(state: &UpdaterState, installed_version: Option<&str>) -> bool {
    if state.jre_checked_for_version.as_deref() != installed_version {
        return true;
    }
    match state.jre_last_check_at {
        None => true,
        Some(last) => {
            let elapsed = (Utc::now() - last).to_std().unwrap_or(Duration::ZERO);
            elapsed >= UPDATE_CHECK_INTERVAL
        }
    }
}

async fn check_jre_if_due_locked(
    app: &AppHandle,
    state: &std::sync::Arc<AppState>,
    updater_state: &mut UpdaterState,
    force: bool,
) -> anyhow::Result<(Option<String>, bool)> {
    let installed_version = crate::jdbc_runtime::installed(app)?.map(|runtime| runtime.version);
    if installed_version.is_none() {
        let changed = updater_state.jre_last_check_at.take().is_some()
            || updater_state.jre_available_version.take().is_some()
            || updater_state.jre_checked_for_version.take().is_some();
        return Ok((None, changed));
    }
    if !force && !jre_check_is_due(updater_state, installed_version.as_deref()) {
        return Ok((updater_state.jre_available_version.clone(), false));
    }

    let result = state.jdbc.check_runtime_update().await?;
    updater_state.jre_last_check_at = Some(Utc::now());
    updater_state.jre_checked_for_version = installed_version;
    updater_state.jre_available_version = result.clone();
    Ok((result, true))
}

async fn check_jre_update(app: &AppHandle, force: bool) -> anyhow::Result<Option<String>> {
    let Some(state_path) = state_path(app) else {
        return Err(anyhow::anyhow!("failed to resolve updater state path"));
    };
    let _guard = check_lock().lock().await;
    let state = app.state::<std::sync::Arc<AppState>>();
    let mut updater_state = load_state(&state_path);
    let (version, checked) =
        check_jre_if_due_locked(app, state.inner(), &mut updater_state, force).await?;
    if checked {
        save_state(&state_path, &updater_state)?;
    }
    if checked {
        match version.clone() {
            Some(version) => {
                crate::debug_log::info(
                    "updater",
                    format_args!("JRE runtime update available (version={version})"),
                );
                let _ = app.emit(
                    JRE_UPDATE_AVAILABLE_EVENT,
                    JreUpdateAvailablePayload { version },
                );
            }
            None => {
                crate::debug_log::info(
                    "updater",
                    format_args!("JRE runtime is up to date"),
                );
            }
        }
    }
    Ok(version)
}

pub async fn check_jre_if_due(app: AppHandle) -> anyhow::Result<Option<String>> {
    check_jre_update(&app, false).await
}

pub async fn check_jre_now(app: AppHandle) -> anyhow::Result<Option<String>> {
    check_jre_update(&app, true).await
}

pub fn state_path(app: &AppHandle) -> Option<PathBuf> {
    let dir = app.path().app_config_dir().ok()?;
    Some(dir.join(STATE_FILE_NAME))
}

fn load_state(path: &Path) -> UpdaterState {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn save_state(path: &Path, state: &UpdaterState) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(state)?;
    std::fs::write(path, text)?;
    Ok(())
}

fn is_due(state: &UpdaterState) -> bool {
    match state.last_check_at {
        None => true,
        Some(last) => {
            let elapsed = (Utc::now() - last).to_std().unwrap_or(Duration::ZERO);
            elapsed >= UPDATE_CHECK_INTERVAL
        }
    }
}

fn compute_delay(state: &UpdaterState) -> Duration {
    match state.last_check_at {
        None => Duration::ZERO,
        Some(last) => {
            let elapsed = (Utc::now() - last).to_std().unwrap_or(Duration::ZERO);
            if elapsed >= UPDATE_CHECK_INTERVAL {
                Duration::ZERO
            } else {
                UPDATE_CHECK_INTERVAL - elapsed
            }
        }
    }
}

/// Spawn the background updater task. Runs forever until the Tauri runtime stops.
/// Honors `auto_check_updates` from settings on each tick. Disabled checks are
/// polled without changing the last-check timestamp so re-enabling can check
/// immediately.
pub fn spawn_updater_task(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let Some(state_path) = state_path(&app) else {
            crate::debug_log::error(
                "updater",
                format_args!("failed to resolve state path; background task exiting"),
            );
            return;
        };

        loop {
            let delay = compute_delay(&load_state(&state_path));
            tokio::time::sleep(delay).await;

            let auto_check = app
                .state::<std::sync::Arc<AppState>>()
                .inner()
                .config
                .try_read()
                .map(|config| config.settings.auto_check_updates)
                .unwrap_or(false);

            if !auto_check {
                tokio::time::sleep(DISABLED_CHECK_POLL_INTERVAL).await;
                continue;
            }

            let _guard = check_lock().lock().await;
            let state = app.state::<std::sync::Arc<AppState>>();
            let mut next_state = load_state(&state_path);
            let current_version = app.package_info().version.to_string();
            if next_state.available_for_version.as_deref() != Some(current_version.as_str()) {
                next_state.available_version = None;
                next_state.available_for_version = None;
            }

            let app_due = is_due(&next_state);
            let outcome = if app_due {
                Some(
                    async {
                        let updater = app.updater()?;
                        let update = updater.check().await?;
                        Ok::<_, anyhow::Error>(update.map(|update| update.version))
                    }
                    .await,
                )
            } else {
                None
            };
            let jre_result =
                check_jre_if_due_locked(&app, state.inner(), &mut next_state, false).await;
            if let Err(error) = &jre_result {
                crate::debug_log::error(
                    "updater",
                    format_args!("JRE update check failed: {error}"),
                );
            }

            if let Some(Ok(update)) = &outcome {
                next_state.last_check_at = Some(Utc::now());
                match update {
                    Some(version) => {
                        crate::debug_log::info(
                            "updater",
                            format_args!(
                                "update available (current={}, available={version})",
                                current_version
                            ),
                        );
                        next_state.available_version = Some(version.clone());
                        next_state.available_for_version = Some(current_version.clone());
                        let _ = app.emit(
                            UPDATE_AVAILABLE_EVENT,
                            UpdateAvailablePayload {
                                version: version.clone(),
                                current_version: current_version.clone(),
                            },
                        );
                    }
                    None => {
                        crate::debug_log::info(
                            "updater",
                            format_args!("update check completed: up to date ({current_version})"),
                        );
                        next_state.available_version = None;
                        next_state.available_for_version = None;
                    }
                }
            } else if let Some(Err(error)) = &outcome {
                next_state.last_check_at = Some(Utc::now());
                crate::debug_log::error("updater", format_args!("update check failed: {error}"));
            }

            if jre_result.is_ok() || outcome.is_some() {
                if let Err(error) = save_state(&state_path, &next_state) {
                    crate::debug_log::error(
                        "updater",
                        format_args!("failed to persist state: {error}"),
                    );
                    break;
                }
            }
            if let Ok((Some(version), true)) = jre_result {
                let _ = app.emit(
                    JRE_UPDATE_AVAILABLE_EVENT,
                    JreUpdateAvailablePayload { version },
                );
            }
        }
    });
}

/// Front-end command: perform an immediate update check if the 24h interval
/// has elapsed since the last attempt. Returns `Some(version)` when a newer
/// version is available, otherwise `None`. The caller (front-end) is
/// responsible for any UI state transitions.
pub async fn check_if_due(app: AppHandle) -> anyhow::Result<Option<String>> {
    let Some(state_path) = state_path(&app) else {
        return Err(anyhow::anyhow!("failed to resolve updater state path"));
    };

    let _guard = check_lock().lock().await;
    let app_state = app.state::<std::sync::Arc<AppState>>();
    let mut state = load_state(&state_path);
    let current_version = app.package_info().version.to_string();
    if state.available_for_version.as_deref() != Some(current_version.as_str()) {
        state.available_version = None;
        state.available_for_version = None;
    }

    let app_result = if is_due(&state) {
        let result = app.updater()?.check().await;
        state.last_check_at = Some(Utc::now());
        match result {
            Ok(update) => {
                match &update {
                    Some(update) => crate::debug_log::info(
                        "updater",
                        format_args!(
                            "update available (current={}, available={})",
                            current_version, update.version
                        ),
                    ),
                    None => crate::debug_log::info(
                        "updater",
                        format_args!("update check completed: up to date ({current_version})"),
                    ),
                }
                state.available_version = update.as_ref().map(|update| update.version.clone());
                state.available_for_version = update.as_ref().map(|_| current_version);
                Ok(update.map(|update| update.version))
            }
            Err(error) => Err(error.into()),
        }
    } else {
        Ok(state.available_version.clone())
    };

    let jre_result = check_jre_if_due_locked(&app, app_state.inner(), &mut state, false).await;
    if let Err(error) = &jre_result {
        crate::debug_log::error("updater", format_args!("JRE update check failed: {error}"));
    }
    match &jre_result {
        Ok((Some(version), true)) => {
            crate::debug_log::info(
                "updater",
                format_args!("JRE runtime update available (version={version})"),
            );
            let _ = app.emit(
                JRE_UPDATE_AVAILABLE_EVENT,
                JreUpdateAvailablePayload {
                    version: version.clone(),
                },
            );
        }
        Ok((None, true)) => {
            crate::debug_log::info(
                "updater",
                format_args!("JRE runtime is up to date"),
            );
        }
        _ => {}
    }
    save_state(&state_path, &state)?;
    app_result
}

#[cfg(test)]
fn jre_cache_is_due(state: &UpdaterState, installed_version: Option<&str>) -> bool {
    jre_check_is_due(state, installed_version)
}

#[cfg(test)]
mod jre_cache_tests {
    use super::*;

    #[test]
    fn jre_cache_requires_check_for_new_installed_version() {
        let state = UpdaterState {
            jre_last_check_at: Some(Utc::now()),
            jre_available_version: Some("21.0.9".to_string()),
            jre_checked_for_version: Some("21.0.8".to_string()),
            ..UpdaterState::default()
        };
        assert!(jre_cache_is_due(&state, Some("21.0.10")));
        assert!(!jre_cache_is_due(&state, Some("21.0.8")));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_due_when_never_checked() {
        assert!(is_due(&UpdaterState::default()));
    }

    #[test]
    fn is_due_after_interval() {
        let mut state = UpdaterState::default();
        state.last_check_at =
            Some(Utc::now() - chrono::Duration::from_std(UPDATE_CHECK_INTERVAL).unwrap() * 2);
        assert!(is_due(&state));
    }

    #[test]
    fn is_not_due_within_interval() {
        let mut state = UpdaterState::default();
        state.last_check_at = Some(Utc::now() - chrono::Duration::hours(1));
        assert!(!is_due(&state));
    }

    #[test]
    fn compute_delay_zero_when_never_checked() {
        assert_eq!(compute_delay(&UpdaterState::default()), Duration::ZERO);
    }

    #[test]
    fn compute_delay_zero_after_interval() {
        let mut state = UpdaterState::default();
        state.last_check_at =
            Some(Utc::now() - chrono::Duration::from_std(UPDATE_CHECK_INTERVAL).unwrap() * 2);
        assert_eq!(compute_delay(&state), Duration::ZERO);
    }

    #[test]
    fn state_roundtrip_preserves_last_check_at() {
        let temp = tempfile::tempdir().expect("temp dir");
        let path = temp.path().join("updater-state.json");
        let mut state = UpdaterState::default();
        state.last_check_at = Some(Utc::now());
        save_state(&path, &state).expect("save");
        let loaded = load_state(&path);
        assert_eq!(loaded.last_check_at, state.last_check_at);
    }

    #[test]
    fn load_state_returns_default_when_missing() {
        let temp = tempfile::tempdir().expect("temp dir");
        let path = temp.path().join("missing.json");
        let state = load_state(&path);
        assert!(state.last_check_at.is_none());
    }
}
