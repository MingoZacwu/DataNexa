use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};
use tauri::{AppHandle, Manager};

const MAX_LOG_SIZE: u64 = 4 * 1024 * 1024;
const ROTATE_KEEP: u32 = 3;
const MAX_MESSAGE_CHARS: usize = 2000;
const LOG_FILE_NAME: &str = "debug.log";

#[derive(Clone, Copy)]
enum Level {
    Info,
    Warn,
    Error,
}

impl Level {
    fn label(self) -> &'static str {
        match self {
            Level::Info => "INFO",
            Level::Warn => "WARN",
            Level::Error => "ERROR",
        }
    }
}

struct WriterState {
    file: Option<File>,
    current_size: u64,
}

/// Runtime debug log controller. Completely inert while the setting is off:
/// no log directory is created and no background work runs. Enabling the
/// setting lazily creates the log file and appends sanitized, single-line
/// entries under the "logs" folder of the application data directory, so the
/// files show up in storage management alongside the other DataNexa data.
struct DebugLogController {
    dir: PathBuf,
    enabled: AtomicBool,
    writer: Mutex<WriterState>,
}

static INSTANCE: OnceLock<DebugLogController> = OnceLock::new();

/// Resolve the log directory once at startup. Performs no filesystem IO so the
/// debug log stays completely inert until the user enables it.
pub fn init(app: &AppHandle) {
    let dir = app
        .path()
        .app_data_dir()
        .map(|dir| dir.join("logs"))
        .unwrap_or_else(|_| std::env::temp_dir().join("datanexa-logs"));
    let _ = INSTANCE.set(DebugLogController {
        dir,
        enabled: AtomicBool::new(false),
        writer: Mutex::new(WriterState {
            file: None,
            current_size: 0,
        }),
    });
}

/// Toggle debug logging at runtime. Enabling is idempotent and takes effect
/// immediately; disabling flushes and closes the writer while keeping the
/// already written files for later inspection.
pub fn set_enabled(enabled: bool) {
    let Some(controller) = INSTANCE.get() else {
        return;
    };
    if enabled {
        controller.enable();
    } else {
        controller.disable();
    }
}

pub fn info(scope: &str, args: std::fmt::Arguments) {
    log(Level::Info, scope, args);
}

pub fn warn(scope: &str, args: std::fmt::Arguments) {
    log(Level::Warn, scope, args);
}

pub fn error(scope: &str, args: std::fmt::Arguments) {
    log(Level::Error, scope, args);
}

/// Whether debug logging is currently enabled. Lets call sites skip building
/// argument state that is not lazy before reaching the log entry points.
pub fn is_enabled() -> bool {
    INSTANCE
        .get()
        .is_some_and(|controller| controller.enabled.load(Ordering::Acquire))
}

/// Delete every debug log file, including rotated ones. The active writer is
/// closed first so the current file is not locked on Windows. When logging
/// stays enabled the file is recreated immediately so logging continues; when
/// logging is off the logs directory is removed so the storage view hides
/// the category again.
pub fn clear() {
    let Some(controller) = INSTANCE.get() else {
        return;
    };
    let mut state = lock_writer(&controller.writer);
    state.file = None;
    state.current_size = 0;
    for index in 1..=ROTATE_KEEP {
        let _ = fs::remove_file(controller.rotated_path(index));
    }
    let _ = fs::remove_file(controller.dir.join(LOG_FILE_NAME));
    if controller.enabled.load(Ordering::Acquire) {
        let _ = controller.open_writer(&mut state);
    } else {
        // Best effort: an empty directory would keep the storage category
        // visible even though every log file is gone.
        let _ = fs::remove_dir(&controller.dir);
    }
}

fn log(level: Level, scope: &str, args: std::fmt::Arguments) {
    let Some(controller) = INSTANCE.get() else {
        return;
    };
    if !controller.enabled.load(Ordering::Acquire) {
        return;
    }
    controller.write(level, scope, &args.to_string());
}

fn lock_writer(writer: &Mutex<WriterState>) -> MutexGuard<'_, WriterState> {
    // Debug logging must never panic, so a poisoned lock is recovered from.
    writer
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl DebugLogController {
    fn write(&self, level: Level, scope: &str, message: &str) {
        if !self.enabled.load(Ordering::Acquire) {
            return;
        }
        let mut state = lock_writer(&self.writer);
        // Re-check under the lock in case set_enabled flipped the switch while
        // this call was waiting for the lock.
        if !self.enabled.load(Ordering::Acquire) {
            return;
        }
        if state.file.is_none() {
            // Best-effort reopen, e.g. after a failed rotation.
            if self.open_writer(&mut state).is_err() {
                return;
            }
        }
        self.append_line(&mut state, level, scope, message);
    }

    fn enable(&self) {
        let mut state = lock_writer(&self.writer);
        if self.enabled.load(Ordering::Acquire) {
            // Already enabled; just restore the writer if a previous rotation
            // failed to reopen the file.
            if state.file.is_none() {
                let _ = self.open_writer(&mut state);
            }
            return;
        }
        if self.open_writer(&mut state).is_ok() {
            self.append_line(
                &mut state,
                Level::Info,
                "startup",
                "debug logging session started",
            );
            self.enabled.store(true, Ordering::Release);
        }
    }

    fn disable(&self) {
        let mut state = lock_writer(&self.writer);
        if !self.enabled.load(Ordering::Acquire) {
            return;
        }
        self.append_line(
            &mut state,
            Level::Info,
            "startup",
            "debug logging session ended",
        );
        if let Some(mut file) = state.file.take() {
            let _ = file.flush();
        }
        state.current_size = 0;
        self.enabled.store(false, Ordering::Release);
    }

    fn append_line(&self, state: &mut WriterState, level: Level, scope: &str, message: &str) {
        let message = truncate_chars(crate::commands::sanitize_text(message), MAX_MESSAGE_CHARS);
        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        let line = format!(
            "[{timestamp}] [{}] [{}] {}\n",
            level.label(),
            scope,
            message
        );
        if state.current_size.saturating_add(line.len() as u64) > MAX_LOG_SIZE {
            self.rotate(state);
        }
        let Some(file) = state.file.as_mut() else {
            return;
        };
        if file
            .write_all(line.as_bytes())
            .and_then(|()| file.flush())
            .is_ok()
        {
            state.current_size = state.current_size.saturating_add(line.len() as u64);
        }
    }

    fn rotate(&self, state: &mut WriterState) {
        // Close the current file before moving it on disk.
        state.file = None;
        let _ = fs::remove_file(self.rotated_path(ROTATE_KEEP));
        for index in (1..ROTATE_KEEP).rev() {
            let from = self.rotated_path(index);
            let to = self.rotated_path(index + 1);
            if from.exists() {
                let _ = fs::rename(&from, &to);
            }
        }
        let _ = fs::rename(self.dir.join(LOG_FILE_NAME), self.rotated_path(1));
        state.current_size = 0;
        // On failure the writer stays empty; the next entry retries the
        // reopen through the lazy path in write().
        let _ = self.open_writer(state);
    }

    fn open_writer(&self, state: &mut WriterState) -> std::io::Result<()> {
        fs::create_dir_all(&self.dir)?;
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.dir.join(LOG_FILE_NAME))?;
        // Resume appending across sessions so history is preserved.
        state.current_size = file.metadata()?.len();
        state.file = Some(file);
        Ok(())
    }

    fn rotated_path(&self, index: u32) -> PathBuf {
        self.dir.join(format!("{LOG_FILE_NAME}.{index}"))
    }
}

fn truncate_chars(value: String, max_chars: usize) -> String {
    match value.char_indices().nth(max_chars) {
        None => value,
        Some((index, _)) => {
            let mut truncated = value[..index].to_string();
            truncated.push_str("...(truncated)");
            truncated
        }
    }
}
