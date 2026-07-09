use std::collections::VecDeque;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
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

#[derive(Default)]
pub struct AuditLogger {
    events: RwLock<VecDeque<AuditEvent>>,
}

impl AuditLogger {
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
    }

    pub async fn trim(&self, max_events: usize) {
        let max_events = normalize_limit(max_events);
        let mut events = self.events.write().await;
        while events.len() > max_events {
            events.pop_back();
        }
    }

    pub async fn list(&self) -> Vec<AuditEvent> {
        self.events.read().await.iter().cloned().collect()
    }
}

fn normalize_limit(max_events: usize) -> usize {
    max_events.clamp(1, MAX_AUDIT_MAX_EVENTS)
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
