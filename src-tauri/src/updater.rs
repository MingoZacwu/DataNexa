use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::UpdaterExt;

use crate::state::AppState;

const UPDATE_CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const DISABLED_CHECK_POLL_INTERVAL: Duration = Duration::from_secs(60);
const STATE_FILE_NAME: &str = "updater-state.json";
const UPDATE_AVAILABLE_EVENT: &str = "updater://available";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct UpdaterState {
    #[serde(default)]
    last_check_at: Option<DateTime<Utc>>,
    #[serde(default)]
    available_version: Option<String>,
    #[serde(default)]
    available_for_version: Option<String>,
}

#[derive(Clone, Serialize)]
struct UpdateAvailablePayload {
    version: String,
    current_version: String,
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
            eprintln!("DataNexa updater: failed to resolve state path; background task exiting");
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

            let outcome = async {
                let updater = app.updater()?;
                let update = updater.check().await?;
                Ok::<_, anyhow::Error>(update.map(|update| update.version))
            }
            .await;

            let mut next_state = load_state(&state_path);
            next_state.last_check_at = Some(Utc::now());

            match outcome {
                Ok(Some(version)) => {
                    let current_version = app.package_info().version.to_string();
                    next_state.available_version = Some(version.clone());
                    next_state.available_for_version = Some(current_version.clone());
                    if let Err(error) = save_state(&state_path, &next_state) {
                        eprintln!("DataNexa updater: failed to persist state: {error}");
                        break;
                    }
                    let _ = app.emit(
                        UPDATE_AVAILABLE_EVENT,
                        UpdateAvailablePayload {
                            version,
                            current_version,
                        },
                    );
                }
                Ok(None) => {
                    next_state.available_version = None;
                    next_state.available_for_version = None;
                    if let Err(error) = save_state(&state_path, &next_state) {
                        eprintln!("DataNexa updater: failed to persist state: {error}");
                        break;
                    }
                }
                Err(error) => {
                    if let Err(save_error) = save_state(&state_path, &next_state) {
                        eprintln!("DataNexa updater: failed to persist state: {save_error}");
                        break;
                    }
                    eprintln!("DataNexa updater check failed: {error}");
                }
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

    let mut state = load_state(&state_path);
    let current_version = app.package_info().version.to_string();
    if state.available_for_version.as_deref() != Some(current_version.as_str()) {
        state.available_version = None;
        state.available_for_version = None;
    }

    if !is_due(&state) {
        return Ok(state.available_version);
    }

    let result = app.updater()?.check().await;
    state.last_check_at = Some(Utc::now());
    match result {
        Ok(update) => {
            state.available_version = update.as_ref().map(|update| update.version.clone());
            state.available_for_version = update.as_ref().map(|_| current_version);
            save_state(&state_path, &state)?;
            Ok(update.map(|update| update.version))
        }
        Err(error) => {
            save_state(&state_path, &state)?;
            Err(error.into())
        }
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
