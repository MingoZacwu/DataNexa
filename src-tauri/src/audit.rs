use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

pub const MAX_AUDIT_MAX_EVENTS: usize = 5000;
const MAX_SQL_CHARS: usize = 20_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuditStatus {
    Allowed,
    Denied,
    Error,
    Timeout,
    Truncated,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub connection_id: Option<String>,
    pub tool: String,
    pub status: AuditStatus,
    pub reason: Option<String>,
    pub elapsed_ms: Option<u64>,
    pub row_count: Option<usize>,
    pub sql: Option<String>,
}

pub struct AuditLogger {
    path: PathBuf,
    events: RwLock<VecDeque<AuditEvent>>,
    persist_lock: Mutex<()>,
}

#[derive(Debug, Serialize, Deserialize)]
struct AuditLogFile {
    #[serde(default)]
    events: Vec<AuditEvent>,
}

impl AuditLogger {
    pub fn new(app: &AppHandle, max_events: usize) -> anyhow::Result<Self> {
        let dir = app.path().app_config_dir()?;
        fs::create_dir_all(&dir)?;
        let path = dir.join("audit-log.json");
        let max_events = normalize_limit(max_events);
        let (events, trimmed) = load_events(&path, max_events).unwrap_or_else(|error| {
            eprintln!("failed to load DataNexa audit log: {error}");
            (VecDeque::new(), false)
        });

        if trimmed {
            if let Err(error) = write_events(&path, &events) {
                eprintln!("failed to trim DataNexa audit log: {error}");
            }
        }

        Ok(Self {
            path,
            events: RwLock::new(events),
            persist_lock: Mutex::new(()),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn record_with_limit(
        &self,
        connection_id: Option<String>,
        tool: impl Into<String>,
        status: AuditStatus,
        reason: Option<String>,
        elapsed_ms: Option<u64>,
        row_count: Option<usize>,
        sql: Option<String>,
        max_events: usize,
    ) {
        let tool = tool.into();
        let max_events = normalize_limit(max_events);
        let mut events = self.events.write().await;
        events.push_front(AuditEvent {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            connection_id,
            tool,
            status,
            reason,
            elapsed_ms,
            row_count,
            sql: sql.map(truncate_sql),
        });

        while events.len() > max_events {
            events.pop_back();
        }

        drop(events);
        self.persist_current().await;
    }

    pub async fn trim(&self, max_events: usize) {
        let max_events = normalize_limit(max_events);
        let mut events = self.events.write().await;
        let original_len = events.len();
        while events.len() > max_events {
            events.pop_back();
        }

        if events.len() != original_len {
            drop(events);
            self.persist_current().await;
        }
    }

    pub async fn list(&self) -> Vec<AuditEvent> {
        self.events.read().await.iter().cloned().collect()
    }

    pub async fn clear(&self) {
        let mut events = self.events.write().await;
        if events.is_empty() {
            return;
        }

        events.clear();
        drop(events);
        self.persist_current().await;
    }

    async fn persist_current(&self) {
        let _guard = self.persist_lock.lock().await;
        let events = self.events.read().await;
        if let Err(error) = write_events(&self.path, &events) {
            eprintln!("failed to persist DataNexa audit log: {error}");
        }
    }
}

fn normalize_limit(max_events: usize) -> usize {
    max_events.clamp(1, MAX_AUDIT_MAX_EVENTS)
}

fn load_events(path: &Path, max_events: usize) -> anyhow::Result<(VecDeque<AuditEvent>, bool)> {
    if !path.exists() {
        return Ok((VecDeque::new(), false));
    }

    let text = fs::read_to_string(path)?;
    if text.trim().is_empty() {
        return Ok((VecDeque::new(), false));
    }

    let mut file = serde_json::from_str::<AuditLogFile>(&text)?;
    let trimmed = file.events.len() > max_events;
    file.events.truncate(max_events);
    Ok((file.events.into_iter().collect(), trimmed))
}

fn write_events(path: &Path, events: &VecDeque<AuditEvent>) -> anyhow::Result<()> {
    let file = AuditLogFile {
        events: events.iter().cloned().collect(),
    };
    let text = serde_json::to_string_pretty(&file)?;
    fs::write(path, text)?;
    Ok(())
}

fn truncate_sql(sql: String) -> String {
    let mut chars = sql.chars();
    let truncated = chars.by_ref().take(MAX_SQL_CHARS).collect::<String>();
    if chars.next().is_some() {
        format!("{truncated}\n/* truncated by DataNexa audit log */")
    } else {
        truncated
    }
}
