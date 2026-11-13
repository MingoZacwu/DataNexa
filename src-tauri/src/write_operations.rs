use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use futures_util::FutureExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::sqlite::{
    SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions, SqliteSynchronous,
};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::{watch, Mutex};
use tokio::time::{timeout, Instant};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::config::ConnectionConfig;
use crate::state::AppState;
use crate::write::{
    self, PendingWrite, PreparedWrite, WriteChange, WriteError, WriteOperation, WriteRequest,
    WriteValue,
};

const SUBMIT_TOOL: &str = "datanexa_submit_write";
const APPROVAL_WINDOW: &str = "write-approval";
const APPROVAL_EVENT: &str = "write-approval-changed";
const MAX_PENDING: usize = 16;
const MAX_PENDING_PER_TOKEN: usize = 4;
const MAX_JOURNAL_ENTRIES: usize = 10_000;
const MAX_JOURNAL_BYTES: i64 = 64 * 1024 * 1024;
const MAX_REQUEST_BYTES: usize = 64 * 1024;
const APPROVAL_TTL: Duration = Duration::from_secs(5 * 60);
/// How long `datanexa_submit_write` waits for the local decision before it
/// answers `pending`. Long enough that a present user is answered in one call,
/// short enough to stay below the HTTP timeout of a typical MCP client.
const SUBMIT_WAIT: Duration = Duration::from_secs(45);
const CONTROL_TIMEOUT: Duration = Duration::from_secs(10);
const JOURNAL_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WriteOperationStatus {
    Pending,
    Rejected,
    Expired,
    Cancelled,
    Executing,
    Committing,
    Committed,
    RolledBack,
    Failed,
    Unknown,
}

impl WriteOperationStatus {
    pub fn is_terminal(self) -> bool {
        !matches!(self, Self::Pending | Self::Executing | Self::Committing)
    }
}

/// Safe for MCP and audit consumers: this never contains SQL or row values.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteStatus {
    pub request_id: String,
    pub token_id: String,
    pub connection_id: String,
    pub operation: WriteOperation,
    pub schema: String,
    pub table: String,
    pub status: WriteOperationStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub revision: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub affected_rows: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    pub outcome_unknown: bool,
}

/// Local approval IPC only. Row values may contain PII; the approval window must
/// mask them by default and must never persist, log, or broadcast this DTO.
#[derive(Clone, Serialize)]
pub struct WriteApprovalDto {
    pub request: WriteStatus,
    pub connection_name: String,
    pub token_name: String,
    pub primary_key: BTreeMap<String, WriteValue>,
    pub changes: Vec<WriteChange>,
    /// Columns the database fills in itself, so the approver sees them.
    pub notes: Vec<crate::write::WriteNote>,
    /// Table-level facts that change more than the listed columns.
    pub warnings: Vec<String>,
    pub sql: String,
    pub reason: Option<String>,
    pub language: String,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WriteDecision {
    Approve,
    Reject,
    Cancel,
}

#[derive(Clone)]
pub struct WriteManager {
    inner: Arc<WriteManagerInner>,
}

struct WriteManagerInner {
    app: Option<AppHandle>,
    journal: Option<WriteJournal>,
    records: Mutex<BTreeMap<String, WriteRecord>>,
    accepting: AtomicBool,
    journal_healthy: AtomicBool,
    /// Bumped after every durable state transition. Callers waiting for a
    /// decision subscribe here instead of polling the record map.
    state_tick: watch::Sender<u64>,
}

struct WriteRecord {
    status: WriteStatus,
    identity: String,
    fingerprint: String,
    prepared: Option<PreparedWrite>,
    approval: Option<ApprovalContext>,
    cancellation: CancellationToken,
    task_running: bool,
    approval_deadline: Instant,
    /// Last revision already written to the audit projection.
    projected_revision: u64,
}

impl WriteRecord {
    fn approval_expired(&self) -> bool {
        Instant::now() >= self.approval_deadline || Utc::now() >= self.status.expires_at
    }
}

struct ApprovalContext {
    connection_fingerprint: String,
    connection_name: String,
    token_name: String,
    language: String,
}

struct WriteJournal {
    pool: SqlitePool,
    key: Zeroizing<Vec<u8>>,
}

impl WriteManager {
    pub async fn new(app: &AppHandle) -> anyhow::Result<Self> {
        let dir = app.path().app_config_dir()?;
        std::fs::create_dir_all(&dir)?;
        let (journal, records) = WriteJournal::open(&dir.join("write-operations.db"))
            .await
            .map_err(|_| anyhow::anyhow!("write journal initialization failed"))?;
        Ok(Self {
            inner: Arc::new(WriteManagerInner {
                app: Some(app.clone()),
                journal: Some(journal),
                records: Mutex::new(records),
                accepting: AtomicBool::new(true),
                journal_healthy: AtomicBool::new(true),
                state_tick: watch::channel(0_u64).0,
            }),
        })
    }

    /// Keeps readonly functionality available when the durable journal cannot
    /// be opened. Existing journal files are never replaced or reset here.
    pub fn unavailable() -> Self {
        Self {
            inner: Arc::new(WriteManagerInner {
                app: None,
                journal: None,
                records: Mutex::new(BTreeMap::new()),
                accepting: AtomicBool::new(false),
                journal_healthy: AtomicBool::new(false),
                state_tick: watch::channel(0_u64).0,
            }),
        }
    }

    /// An inert fixture for existing AppState constructors. It opens no files,
    /// starts no tasks, and rejects every write submission.
    #[cfg(test)]
    pub(crate) fn for_test() -> Self {
        Self::unavailable()
    }

    pub async fn submit(
        &self,
        app: Arc<AppState>,
        token_id: String,
        request: WriteRequest,
        authentication_fingerprint: String,
    ) -> Result<WriteStatus, WriteError> {
        let manager = self.clone();
        // Durable insertion and in-memory ownership must not be interrupted by
        // a disconnected HTTP caller, including while SQLite finishes an await.
        let submitted = tauri::async_runtime::spawn(async move {
            manager
                .submit_inner(app, token_id, request, authentication_fingerprint)
                .await
        })
        .await
        .map_err(|_| error("internal_error"))??;
        // Then wait a bounded time for the human decision. A caller that
        // disconnects while waiting only abandons its own wait: the request
        // stays owned by the manager and remains reachable by request_id.
        self.wait_for_decision(&submitted.request_id, SUBMIT_WAIT)
            .await
    }

    /// Wait until the request reaches a terminal state, or until `budget`
    /// expires. Returns the latest observable status either way.
    async fn wait_for_decision(
        &self,
        id: &str,
        budget: Duration,
    ) -> Result<WriteStatus, WriteError> {
        let deadline = Instant::now() + budget;
        // Subscribe before the first read so a transition that lands between
        // the read and the await cannot be missed.
        let mut ticks = self.inner.state_tick.subscribe();
        loop {
            let current = self
                .inner
                .records
                .lock()
                .await
                .get(id)
                .map(|record| record.status.clone())
                .ok_or_else(|| error("write_not_found"))?;
            if current.status.is_terminal() {
                return Ok(current);
            }
            tokio::select! {
                changed = ticks.changed() => {
                    // A closed channel cannot happen while the manager lives;
                    // fall back to the last observed status if it does.
                    if changed.is_err() {
                        return Ok(current);
                    }
                }
                _ = tokio::time::sleep_until(deadline) => return Ok(current),
            }
        }
    }

    async fn submit_inner(
        &self,
        app: Arc<AppState>,
        token_id: String,
        mut request: WriteRequest,
        authentication_fingerprint: String,
    ) -> Result<WriteStatus, WriteError> {
        self.expire_pending().await;
        encode_request(&token_id, &request)?;
        let journal = self
            .inner
            .journal
            .as_ref()
            .ok_or_else(|| error("write_unavailable"))?;
        let identity = journal.digest(
            b"idempotency-v1",
            &[
                token_id.as_bytes(),
                request.connection_id.as_bytes(),
                request.idempotency_key.as_bytes(),
            ],
        );
        let transaction = timeout(CONTROL_TIMEOUT, app.config_transaction.read())
            .await
            .map_err(|_| error("configuration_busy"))?;
        let server_cancellation = app.mcp_cancellation_token().await;
        if server_cancellation.is_cancelled() {
            return Err(error("write_cancelled"));
        }
        // Bind HTTP authentication to the current credential generation while
        // rotation is excluded by the configuration guard. Never retain this
        // fingerprint in the request record, approval DTO, or durable journal.
        let authentication_fingerprint = Zeroizing::new(authentication_fingerprint);
        if !is_digest(&authentication_fingerprint) {
            return Err(error("write_denied"));
        }
        let authentication_matches = timeout(
            CONTROL_TIMEOUT,
            app.access
                .authentication_matches(&token_id, &authentication_fingerprint),
        )
        .await
        .map_err(|_| error("authorization_unavailable"))?
        .map_err(|_| error("authorization_unavailable"))?;
        if !authentication_matches {
            return Err(error("write_denied"));
        }
        drop(authentication_fingerprint);
        let (connection, _, target) = timeout(
            Duration::from_secs(30),
            authorize(&app, &token_id, &request),
        )
        .await
        .map_err(|_| error("authorization_unavailable"))??;
        let (schema, table) = target;
        request.target_id = Some(crate::write_catalog::target_id(&connection, &schema, &table));
        request.schema = schema;
        request.table = table;
        let encoded = encode_request(&token_id, &request)?;
        let fingerprint = journal.digest(b"request-v2", &[&encoded]);
        drop(connection);

        let mut records = self.inner.records.lock().await;
        if server_cancellation.is_cancelled() {
            return Err(error("write_cancelled"));
        }
        if let Some(record) = records.values().find(|record| record.identity == identity) {
            return if record.fingerprint == fingerprint {
                Ok(record.status.clone())
            } else {
                Err(error("idempotency_conflict"))
            };
        }
        if !self.inner.accepting.load(Ordering::Acquire) {
            return Err(error("write_draining"));
        }
        if !self.inner.journal_healthy.load(Ordering::Acquire) {
            return Err(error("journal_error"));
        }
        let active =
            |record: &&WriteRecord| !record.status.status.is_terminal() || record.task_running;
        if records.values().filter(active).count() >= MAX_PENDING
            || records
                .values()
                .filter(active)
                .filter(|record| record.status.token_id == token_id)
                .count()
                >= MAX_PENDING_PER_TOKEN
        {
            return Err(error("pending_limit"));
        }
        let now = Utc::now();
        let status = WriteStatus {
            request_id: Uuid::new_v4().to_string(),
            token_id,
            connection_id: request.connection_id.clone(),
            operation: request.operation,
            schema: request.schema.clone(),
            table: request.table.clone(),
            status: WriteOperationStatus::Pending,
            created_at: now,
            updated_at: now,
            expires_at: now + chrono::Duration::seconds(APPROVAL_TTL.as_secs() as i64),
            revision: 1,
            affected_rows: None,
            error_code: None,
            error_message: None,
            outcome_unknown: false,
        };
        let record = WriteRecord {
            status: status.clone(),
            identity,
            fingerprint,
            prepared: None,
            approval: None,
            // Bind this request to the captured server generation. Replacing
            // the server token on restart must never revive an old approval.
            cancellation: server_cancellation.child_token(),
            task_running: true,
            approval_deadline: Instant::now() + APPROVAL_TTL,
            projected_revision: 0,
        };
        if journal.insert(&record).await.is_err() {
            self.inner.journal_healthy.store(false, Ordering::Release);
            return Err(error("journal_error"));
        }
        let cancellation = record.cancellation.clone();
        let approval_deadline = record.approval_deadline;
        records.insert(status.request_id.clone(), record);
        drop(records);
        drop(transaction);

        let manager = self.clone();
        let id = status.request_id.clone();
        let expiry_cancellation = cancellation.clone();
        tauri::async_runtime::spawn(async move {
            tokio::select! {
                _ = expiry_cancellation.cancelled() => manager.expire_pending().await,
                _ = tokio::time::sleep_until(approval_deadline) => manager.expire_pending().await,
            }
        });
        let manager = self.clone();
        tauri::async_runtime::spawn(async move {
            let outcome =
                std::panic::AssertUnwindSafe(manager.prepare_task(app, &id, request, cancellation))
                    .catch_unwind()
                    .await;
            if outcome.is_err() {
                manager
                    .finish_task(
                        &id,
                        WriteOperationStatus::Failed,
                        Some(error("internal_error")),
                        None,
                    )
                    .await;
            }
        });
        // The caller waits for the decision separately; preparation continues in
        // the background so a slow preview never blocks the submission itself.
        Ok(status)
    }

    pub async fn get_status(&self, token_id: &str, id: &str) -> Result<WriteStatus, WriteError> {
        self.expire_pending().await;
        self.inner
            .records
            .lock()
            .await
            .get(id)
            .filter(|record| record.status.token_id == token_id)
            .map(|record| record.status.clone())
            .ok_or_else(|| error("write_not_found"))
    }

    /// Native controller only; never register this as an MCP operation.
    pub async fn approval_status(&self, id: &str) -> Result<WriteStatus, WriteError> {
        self.expire_pending().await;
        self.inner
            .records
            .lock()
            .await
            .get(id)
            .map(|record| record.status.clone())
            .ok_or_else(|| error("write_not_found"))
    }

    /// Native controller only. Each approval window reviews exactly one request;
    /// a later request gets its own window rather than replacing the visible one.
    pub async fn current_approval(&self) -> Result<Option<WriteApprovalDto>, WriteError> {
        self.expire_pending().await;
        let records = self.inner.records.lock().await;
        let record = records
            .values()
            .filter(|record| {
                record.status.status == WriteOperationStatus::Pending && !record.approval_expired()
            })
            .filter(|record| record.prepared.is_some() && record.approval.is_some())
            .min_by_key(|record| (record.status.created_at, record.status.request_id.as_str()));
        Ok(record.and_then(|record| {
            let prepared = record.prepared.as_ref()?;
            let approval = record.approval.as_ref()?;
            Some(WriteApprovalDto {
                request: record.status.clone(),
                connection_name: approval.connection_name.clone(),
                token_name: approval.token_name.clone(),
                primary_key: prepared.preview.primary_key.clone(),
                changes: prepared.preview.changes.clone(),
                notes: prepared.preview.notes.clone(),
                warnings: prepared.preview.warnings.clone(),
                sql: prepared.preview.sql.clone(),
                reason: prepared.request.reason.clone(),
                language: approval.language.clone(),
            })
        }))
    }

    pub async fn decide(
        &self,
        app: Arc<AppState>,
        id: String,
        revision: u64,
        decision: WriteDecision,
    ) -> Result<WriteStatus, WriteError> {
        let manager = self.clone();
        // Native IPC cancellation must not interrupt the atomic decision.
        tauri::async_runtime::spawn(async move {
            manager.decide_inner(app, id, revision, decision).await
        })
        .await
        .map_err(|_| error("internal_error"))?
    }

    async fn decide_inner(
        &self,
        app: Arc<AppState>,
        id: String,
        revision: u64,
        decision: WriteDecision,
    ) -> Result<WriteStatus, WriteError> {
        self.expire_pending().await;
        let mut records = self.inner.records.lock().await;
        let record = records
            .get_mut(&id)
            .ok_or_else(|| error("write_not_found"))?;
        if record.status.status == WriteOperationStatus::Pending && record.approval_expired() {
            record.cancellation.cancel();
            let next = next_status(
                &record.status,
                WriteOperationStatus::Expired,
                Some(error("approval_expired")),
                None,
            );
            self.inner.persist(record, next).await;
            drop(records);
            self.inner.notify();
            return Err(error("approval_expired"));
        }
        if record.status.revision != revision
            || record.status.status != WriteOperationStatus::Pending
        {
            return Err(error("stale_approval"));
        }
        match decision {
            WriteDecision::Reject | WriteDecision::Cancel => {
                record.cancellation.cancel();
                let state = if matches!(decision, WriteDecision::Reject) {
                    WriteOperationStatus::Rejected
                } else {
                    WriteOperationStatus::Cancelled
                };
                let next = next_status(&record.status, state, None, None);
                self.inner.persist(record, next).await;
                let status = record.status.clone();
                drop(records);
                self.inner.notify();
                Ok(status)
            }
            WriteDecision::Approve => {
                if !self.inner.accepting.load(Ordering::Acquire)
                    || !self.inner.journal_healthy.load(Ordering::Acquire)
                    || record.cancellation.is_cancelled()
                {
                    return Err(error("write_unavailable"));
                }
                if record.prepared.is_none() || record.approval.is_none() || record.task_running {
                    return Err(error("approval_not_ready"));
                }
                let prepared = record
                    .prepared
                    .take()
                    .ok_or_else(|| error("approval_not_ready"))?;
                let approval = record
                    .approval
                    .take()
                    .ok_or_else(|| error("approval_not_ready"))?;
                let next = next_status(&record.status, WriteOperationStatus::Executing, None, None);
                if !self.inner.persist(record, next).await {
                    record.cancellation.cancel();
                    record.status = next_status(
                        &record.status,
                        WriteOperationStatus::Failed,
                        Some(error("journal_error")),
                        None,
                    );
                    let status = record.status.clone();
                    drop(records);
                    self.inner.notify();
                    return Ok(status);
                }
                record.task_running = true;
                let cancellation = record.cancellation.clone();
                let status = record.status.clone();
                drop(records);
                self.inner.notify();
                let manager = self.clone();
                tauri::async_runtime::spawn(async move {
                    let outcome = std::panic::AssertUnwindSafe(manager.execute_task(
                        app,
                        &id,
                        prepared,
                        approval,
                        cancellation,
                    ))
                    .catch_unwind()
                    .await;
                    if outcome.is_err() {
                        manager
                            .finish_task(
                                &id,
                                WriteOperationStatus::Unknown,
                                Some(unknown_error("internal_error")),
                                None,
                            )
                            .await;
                    }
                });
                Ok(status)
            }
        }
    }

    pub async fn invalidate_all(&self) -> Result<(), WriteError> {
        self.invalidate(|_| true).await
    }

    pub async fn invalidate_token(&self, token_id: &str) -> Result<(), WriteError> {
        self.invalidate(|record| record.status.token_id == token_id)
            .await
    }

    pub async fn invalidate_connection(&self, connection_id: &str) -> Result<(), WriteError> {
        self.invalidate(|record| record.status.connection_id == connection_id)
            .await
    }

    async fn invalidate(&self, matches: impl Fn(&WriteRecord) -> bool) -> Result<(), WriteError> {
        let mut records = self.inner.records.lock().await;
        // Cancel every matching task before waiting on the first durable update.
        for record in records.values().filter(|record| matches(record)) {
            if !record.status.status.is_terminal() || record.task_running {
                record.cancellation.cancel();
            }
        }
        let mut success = true;
        for record in records.values_mut().filter(|record| matches(record)) {
            if record.status.status == WriteOperationStatus::Pending {
                let next = next_status(
                    &record.status,
                    WriteOperationStatus::Cancelled,
                    Some(error("authorization_changed")),
                    None,
                );
                success &= self.inner.persist(record, next).await;
            }
        }
        drop(records);
        self.inner.notify();
        if success {
            Ok(())
        } else {
            Err(error("journal_error"))
        }
    }

    /// Stops admission for exit/update. A false result means the caller must not
    /// exit/install: a transaction may still be in flight. The caller must call
    /// [`Self::resume_admission`] when it decides to stay running (a failed
    /// update install, a blocked exit), otherwise writes stay closed.
    pub async fn drain(&self, budget: Duration) -> bool {
        self.inner.accepting.store(false, Ordering::Release);
        timeout(budget, async {
            if self.invalidate_all().await.is_err() {
                crate::debug_log::warn(
                    "write",
                    format_args!("write journal unavailable during drain"),
                );
            }
            while self.has_active().await {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .is_ok()
    }

    /// Re-open admission after a drain that did not end in process exit.
    pub fn resume_admission(&self) {
        if self.inner.journal.is_some() {
            self.inner.accepting.store(true, Ordering::Release);
        }
    }

    pub async fn has_active(&self) -> bool {
        self.inner
            .records
            .lock()
            .await
            .values()
            .any(|record| !record.status.status.is_terminal() || record.task_running)
    }

    async fn expire_pending(&self) {
        let mut records = self.inner.records.lock().await;
        let mut changed = false;
        for record in records.values_mut() {
            if record.status.status == WriteOperationStatus::Pending
                && (record.cancellation.is_cancelled() || record.approval_expired())
            {
                let (state, code) = if record.cancellation.is_cancelled() {
                    (WriteOperationStatus::Cancelled, "write_cancelled")
                } else {
                    (WriteOperationStatus::Expired, "approval_expired")
                };
                record.cancellation.cancel();
                let next = next_status(&record.status, state, Some(error(code)), None);
                self.inner.persist(record, next).await;
                changed = true;
            }
        }
        drop(records);
        if changed {
            self.inner.notify();
        }
    }

    async fn prepare_task(
        &self,
        app: Arc<AppState>,
        id: &str,
        request: WriteRequest,
        cancellation: CancellationToken,
    ) {
        let result = self.prepare_inner(&app, id, request, &cancellation).await;
        match result {
            Err(failure) => {
                let state = if cancellation.is_cancelled() {
                    WriteOperationStatus::Cancelled
                } else {
                    WriteOperationStatus::Failed
                };
                self.finish_task(id, state, Some(failure), None).await;
            }
            Ok((prepared, approval)) => {
                let mut records = self.inner.records.lock().await;
                let Some(record) = records.get_mut(id) else {
                    return;
                };
                record.task_running = false;
                if record.status.status != WriteOperationStatus::Pending
                    || cancellation.is_cancelled()
                {
                    return;
                }
                if record.approval_expired() {
                    record.cancellation.cancel();
                    let next = next_status(
                        &record.status,
                        WriteOperationStatus::Expired,
                        Some(error("approval_expired")),
                        None,
                    );
                    self.inner.persist(record, next).await;
                    drop(records);
                    self.inner.notify();
                    return;
                }
                let next = next_status(&record.status, WriteOperationStatus::Pending, None, None);
                if !self.inner.persist(record, next).await {
                    record.cancellation.cancel();
                    record.status = next_status(
                        &record.status,
                        WriteOperationStatus::Failed,
                        Some(error("journal_error")),
                        None,
                    );
                    drop(records);
                    self.inner.notify();
                    return;
                }
                record.prepared = Some(prepared);
                record.approval = Some(approval);
                drop(records);
                self.inner.notify();
                let shown = match &self.inner.app {
                    Some(handle) => timeout(
                        CONTROL_TIMEOUT,
                        crate::write_approval::show_write_approval(handle),
                    )
                    .await
                    .map(|result| result.is_ok())
                    .unwrap_or(false),
                    None => false,
                };
                if !shown {
                    // Keep the request pending: the approval window can be opened
                    // again from Write Center. Silently cancelling here loses the
                    // user's request because of a transient windowing failure.
                    crate::debug_log::warn(
                        "write",
                        format_args!("approval window could not be shown; request stays pending"),
                    );
                }
            }
        }
    }

    async fn prepare_inner(
        &self,
        app: &Arc<AppState>,
        id: &str,
        request: WriteRequest,
        cancellation: &CancellationToken,
    ) -> Result<(PreparedWrite, ApprovalContext), WriteError> {
        let token_id = self
            .inner
            .records
            .lock()
            .await
            .get(id)
            .map(|record| record.status.token_id.clone())
            .ok_or_else(|| error("write_not_found"))?;
        let _permits = tokio::select! {
            _ = cancellation.cancelled() => return Err(error("write_cancelled")),
            result = timeout(CONTROL_TIMEOUT, crate::mcp::acquire_write_permits(app, &request.connection_id)) => {
                result.map_err(|_| error("capacity_exceeded"))?.map_err(|_| error("capacity_exceeded"))?
            }
        };
        let _transaction = tokio::select! {
            _ = cancellation.cancelled() => return Err(error("write_cancelled")),
            result = timeout(CONTROL_TIMEOUT, app.config_transaction.read()) => result.map_err(|_| error("configuration_busy"))?,
        };
        let (config, language, _) =
            timeout(Duration::from_secs(30), authorize(app, &token_id, &request))
                .await
                .map_err(|_| error("authorization_unavailable"))??;
        if !self.inner.journal_healthy.load(Ordering::Acquire) {
            return Err(error("journal_error"));
        }
        let tokens = timeout(CONTROL_TIMEOUT, app.access.list())
            .await
            .map_err(|_| error("authorization_unavailable"))?
            .map_err(|_| error("authorization_unavailable"))?;
        let token_name = tokens
            .into_iter()
            .find(|token| token.id == token_id && token.enabled)
            .map(|token| token.name)
            .ok_or_else(|| error("write_denied"))?;
        let approval = ApprovalContext {
            connection_fingerprint: self.connection_fingerprint(&config)?,
            connection_name: config.name.clone(),
            token_name,
            language,
        };
        let prepared = tokio::select! {
            _ = cancellation.cancelled() => return Err(error("write_cancelled")),
            result = timeout(execution_budget(&config), write::prepare(&config, &app.vault, request)) => {
                result.map_err(|_| error("prepare_timeout"))?.map_err(|failure| {
                    if failure.code == "timeout" { error("prepare_timeout") } else { sanitize_write_error(failure) }
                })?
            }
        };
        Ok((prepared, approval))
    }

    /// Count of requests that are prepared and waiting for a human decision.
    pub async fn pending_approvals(&self) -> usize {
        self.expire_pending().await;
        self.inner
            .records
            .lock()
            .await
            .values()
            .filter(|record| {
                record.status.status == WriteOperationStatus::Pending
                    && !record.approval_expired()
                    && record.prepared.is_some()
                    && record.approval.is_some()
            })
            .count()
    }

    async fn execute_task(
        &self,
        app: Arc<AppState>,
        id: &str,
        prepared: PreparedWrite,
        approval: ApprovalContext,
        cancellation: CancellationToken,
    ) {
        let outcome = self
            .execute_inner(&app, id, &prepared, &approval, &cancellation)
            .await;
        self.finish_task(id, outcome.0, outcome.1, outcome.2).await;
    }

    async fn execute_inner(
        &self,
        app: &Arc<AppState>,
        id: &str,
        prepared: &PreparedWrite,
        approval: &ApprovalContext,
        cancellation: &CancellationToken,
    ) -> (WriteOperationStatus, Option<WriteError>, Option<u64>) {
        let before_write = self
            .execution_context(app, id, prepared, approval, cancellation)
            .await;
        let (config, _permits, _transaction) = match before_write {
            Ok(context) => context,
            Err(failure) => {
                // Only a real cancellation is reported as such. A busy
                // configuration, a missing permit, or an unreachable journal is
                // an execution failure, and the audit trail must say so.
                let state = if failure.code == "write_cancelled" {
                    WriteOperationStatus::Cancelled
                } else {
                    WriteOperationStatus::Failed
                };
                return (state, Some(sanitize_write_error(failure)), None);
            }
        };
        // The executor first attempts explicit rollback on timeout/cancellation.
        // This larger watchdog is only a last resort: dropping it means unknown.
        let pending = match timeout(
            execution_budget(&config),
            write::execute(&config, &app.vault, prepared, cancellation.clone()),
        )
        .await
        {
            Ok(Ok(pending)) => pending,
            Ok(Err(failure)) => {
                let failure = sanitize_write_error(failure);
                let state = if failure.outcome_unknown {
                    WriteOperationStatus::Unknown
                } else if failure.code == "write_cancelled" {
                    WriteOperationStatus::Cancelled
                } else {
                    WriteOperationStatus::Failed
                };
                return (state, Some(failure), None);
            }
            Err(_) => {
                cancellation.cancel();
                return (
                    WriteOperationStatus::Unknown,
                    Some(unknown_error("execution_timeout")),
                    None,
                );
            }
        };
        // Keep the read guard while rechecking immediately before the durable
        // commit gate. This also observes emergency mode, which can be raised
        // before its configuration mutation obtains the exclusive guard.
        let token_id = self
            .inner
            .records
            .lock()
            .await
            .get(id)
            .map(|record| record.status.token_id.clone());
        let revalidated = match token_id {
            Some(token_id) => timeout(
                CONTROL_TIMEOUT,
                authorize_connection(app, &token_id, &prepared.request),
            )
            .await
            .map_err(|_| error("authorization_unavailable"))
            .and_then(|result| result),
            None => Err(error("write_not_found")),
        };
        if let Err(failure) = revalidated {
            return rollback(pending, execution_budget(&config), failure).await;
        }
        let commit_gate = {
            let mut records = self.inner.records.lock().await;
            match records.get_mut(id) {
                Some(record)
                    if record.status.status == WriteOperationStatus::Executing
                        && !cancellation.is_cancelled()
                        && self.inner.journal_healthy.load(Ordering::Acquire) =>
                {
                    let next =
                        next_status(&record.status, WriteOperationStatus::Committing, None, None);
                    self.inner.persist(record, next).await
                }
                _ => false,
            }
        };
        self.inner.notify();
        if !commit_gate || cancellation.is_cancelled() {
            let failure = if self.inner.journal_healthy.load(Ordering::Acquire) {
                error("write_cancelled")
            } else {
                error("journal_error")
            };
            return rollback(pending, execution_budget(&config), failure).await;
        }
        // The config transaction read guard remains held through commit. A
        // revocation must acquire its write guard before it is acknowledged.
        match timeout(execution_budget(&config), pending.commit()).await {
            Ok(Ok(affected_rows)) => (WriteOperationStatus::Committed, None, Some(affected_rows)),
            Ok(Err(failure)) => {
                let failure = sanitize_write_error(failure);
                let state = if failure.outcome_unknown {
                    WriteOperationStatus::Unknown
                } else {
                    WriteOperationStatus::Failed
                };
                (state, Some(failure), None)
            }
            Err(_) => (
                WriteOperationStatus::Unknown,
                Some(unknown_error("commit_timeout")),
                None,
            ),
        }
    }

    async fn execution_context<'a>(
        &self,
        app: &'a Arc<AppState>,
        id: &str,
        prepared: &PreparedWrite,
        approval: &ApprovalContext,
        cancellation: &CancellationToken,
    ) -> Result<
        (
            ConnectionConfig,
            crate::mcp::ExecutionPermits,
            tokio::sync::RwLockReadGuard<'a, ()>,
        ),
        WriteError,
    > {
        let token_id = self
            .inner
            .records
            .lock()
            .await
            .get(id)
            .map(|record| record.status.token_id.clone())
            .ok_or_else(|| error("write_not_found"))?;
        let permits = tokio::select! {
            _ = cancellation.cancelled() => return Err(error("write_cancelled")),
            result = timeout(CONTROL_TIMEOUT, crate::mcp::acquire_write_permits(app, &prepared.request.connection_id)) => {
                result.map_err(|_| error("capacity_exceeded"))?.map_err(|_| error("capacity_exceeded"))?
            }
        };
        let transaction = tokio::select! {
            _ = cancellation.cancelled() => return Err(error("write_cancelled")),
            result = timeout(CONTROL_TIMEOUT, app.config_transaction.read()) => result.map_err(|_| error("configuration_busy"))?,
        };
        let (config, _, _) = timeout(
            Duration::from_secs(30),
            authorize(app, &token_id, &prepared.request),
        )
        .await
        .map_err(|_| error("authorization_unavailable"))??;
        if cancellation.is_cancelled() {
            return Err(error("write_cancelled"));
        }
        if !self.inner.journal_healthy.load(Ordering::Acquire) {
            return Err(error("journal_error"));
        }
        if self.connection_fingerprint(&config)? != approval.connection_fingerprint {
            return Err(error("configuration_changed"));
        }
        Ok((config, permits, transaction))
    }

    fn connection_fingerprint(&self, config: &ConnectionConfig) -> Result<String, WriteError> {
        let bytes =
            Zeroizing::new(serde_json::to_vec(config).map_err(|_| error("configuration_changed"))?);
        Ok(self
            .inner
            .journal
            .as_ref()
            .ok_or_else(|| error("journal_error"))?
            .digest(b"connection-v1", &[&bytes]))
    }

    async fn finish_task(
        &self,
        id: &str,
        state: WriteOperationStatus,
        failure: Option<WriteError>,
        affected_rows: Option<u64>,
    ) {
        let mut records = self.inner.records.lock().await;
        if let Some(record) = records.get_mut(id) {
            record.task_running = false;
            // A cancelled/expired preparation must not overwrite that decision.
            if !record.status.status.is_terminal() {
                let next = next_status(&record.status, state, failure, affected_rows);
                self.inner.persist(record, next).await;
            }
            record.prepared = None;
            record.approval = None;
            record.cancellation.cancel();
        }
        drop(records);
        self.inner.notify();
    }
}

impl WriteManagerInner {
    async fn persist(&self, record: &mut WriteRecord, next: WriteStatus) -> bool {
        // Publish the conservative in-memory transition before awaiting disk.
        // If a status/expiry caller disconnects during that await, it must not
        // leave a cancelled approval in Pending with its sensitive values held.
        if next.status.is_terminal() {
            record.prepared = None;
            record.approval = None;
        }
        record.status = next;
        let success = match &self.journal {
            Some(journal) => journal.update(&record.status).await.is_ok(),
            None => false,
        };
        if !success {
            self.journal_healthy.store(false, Ordering::Release);
            record.status.error_code = Some("journal_error".to_string());
            record.status.error_message = Some(error_message("journal_error").to_string());
        }
        // Wake a caller waiting for this request's decision. The value itself is
        // irrelevant; it only carries a monotonically increasing tick.
        let tick = self.state_tick.borrow().wrapping_add(1);
        let _ = self.state_tick.send(tick);
        // In particular, confirmed commit + journal failure stays COMMITTED.
        // The durable committing marker instead recovers as UNKNOWN on restart.
        success
    }

    /// Project one record into the ordinary audit log and signal both windows.
    /// Project the operation state into the ordinary audit log and signal both
    /// windows. Only a snapshot of safe metadata crosses this task boundary.
    ///
    /// Only records that moved since the previous projection are written: an
    /// approval click used to rewrite the whole journal, which turns into
    /// thousands of audit transactions once history accumulates.
    fn notify(&self) {
        if let Some(app) = &self.app {
            let handle = app.clone();
            tauri::async_runtime::spawn(async move {
                let Some(state) = handle.try_state::<std::sync::Arc<AppState>>() else {
                    return;
                };
                let config = state.config.read().await.clone();
                let changed = {
                    let mut records = state.writes.inner.records.lock().await;
                    let mut changed = Vec::new();
                    for record in records.values_mut() {
                        if record.projected_revision != record.status.revision {
                            record.projected_revision = record.status.revision;
                            changed.push(record.status.clone());
                        }
                    }
                    changed
                };
                for status in changed {
                    let name = config
                        .connections
                        .iter()
                        .find(|connection| connection.id == status.connection_id)
                        .map(|connection| connection.name.clone());
                    if state
                        .audit
                        .sync_write_status(&status, name, config.settings.audit_retention_days)
                        .await
                        .is_err()
                    {
                        crate::debug_log::error(
                            "write",
                            format_args!("write audit projection could not be saved"),
                        );
                        break;
                    }
                }
            });
            // Both recipients receive only an empty invalidation signal. The
            // sensitive approval DTO remains local to the approval controller.
            for window in [APPROVAL_WINDOW, "main"] {
                if app.emit_to(window, APPROVAL_EVENT, ()).is_err() {
                    crate::debug_log::warn(
                        "write",
                        format_args!("write state notification could not be delivered"),
                    );
                }
            }
        }
    }
}

impl WriteJournal {
    async fn open(path: &Path) -> anyhow::Result<(Self, BTreeMap<String, WriteRecord>)> {
        secure_journal_file(path)?;
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(false)
            .journal_mode(SqliteJournalMode::Delete)
            .synchronous(SqliteSynchronous::Full)
            .busy_timeout(Duration::from_secs(2));
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .acquire_timeout(JOURNAL_TIMEOUT)
            .connect_with(options)
            .await?;
        let page_size: i64 = sqlx::query_scalar("PRAGMA page_size")
            .fetch_one(&pool)
            .await?;
        anyhow::ensure!(page_size > 0, "invalid journal page size");
        let pages = MAX_JOURNAL_BYTES / page_size;
        let applied: i64 = sqlx::query_scalar(&format!("PRAGMA max_page_count={pages}"))
            .fetch_one(&pool)
            .await?;
        anyhow::ensure!(applied <= pages, "write journal exceeds its size limit");
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS write_meta (key TEXT PRIMARY KEY, value BLOB NOT NULL)",
        )
        .execute(&pool)
        .await?;
        sqlx::query("CREATE TABLE IF NOT EXISTS write_operations (request_id TEXT PRIMARY KEY, identity_digest TEXT NOT NULL UNIQUE, request_fingerprint TEXT NOT NULL, status_json TEXT NOT NULL)").execute(&pool).await?;
        let mut tx = pool.begin().await?;
        let version: Option<Vec<u8>> =
            sqlx::query_scalar("SELECT value FROM write_meta WHERE key='version'")
                .fetch_optional(&mut *tx)
                .await?;
        let existing: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM write_operations")
            .fetch_one(&mut *tx)
            .await?;
        anyhow::ensure!(
            existing >= 0 && existing <= MAX_JOURNAL_ENTRIES as i64,
            "write journal entry limit exceeded"
        );
        if let Some(version) = version {
            anyhow::ensure!(version == b"1", "unsupported write journal version");
        } else {
            anyhow::ensure!(existing == 0, "write journal metadata is missing");
            sqlx::query("INSERT INTO write_meta (key,value) VALUES ('version',?)")
                .bind(b"1".as_slice())
                .execute(&mut *tx)
                .await?;
        }
        let key: Option<Vec<u8>> =
            sqlx::query_scalar("SELECT value FROM write_meta WHERE key='fingerprint_key'")
                .fetch_optional(&mut *tx)
                .await?;
        let key = match key {
            Some(key) => {
                anyhow::ensure!(key.len() == 32, "invalid write journal key");
                key
            }
            None => {
                anyhow::ensure!(existing == 0, "write journal key is missing");
                let mut key = Vec::with_capacity(32);
                key.extend_from_slice(Uuid::new_v4().as_bytes());
                key.extend_from_slice(Uuid::new_v4().as_bytes());
                sqlx::query("INSERT INTO write_meta (key,value) VALUES ('fingerprint_key',?)")
                    .bind(&key)
                    .execute(&mut *tx)
                    .await?;
                key
            }
        };
        let rows = sqlx::query_as::<_, (String, String, String, String)>("SELECT request_id,identity_digest,request_fingerprint,status_json FROM write_operations ORDER BY rowid DESC").fetch_all(&mut *tx).await?;
        let mut records = BTreeMap::new();
        for (id, identity, fingerprint, json) in rows {
            let mut status: WriteStatus = serde_json::from_str(&json)?;
            anyhow::ensure!(
                status.request_id == id && is_digest(&identity) && is_digest(&fingerprint),
                "invalid write journal record"
            );
            // Evict the oldest records once the journal is over its entry
            // budget. A full journal used to reject every later write for good.
            if records.len() >= MAX_JOURNAL_ENTRIES {
                sqlx::query("DELETE FROM write_operations WHERE request_id=?")
                    .bind(&id)
                    .execute(&mut *tx)
                    .await?;
                continue;
            }
            if !status.status.is_terminal() {
                let (state, failure) = match status.status {
                    WriteOperationStatus::Pending => (
                        WriteOperationStatus::Cancelled,
                        error("application_restarted"),
                    ),
                    _ => (
                        WriteOperationStatus::Unknown,
                        unknown_error("application_restarted"),
                    ),
                };
                status = next_status(&status, state, Some(failure), None);
                sqlx::query("UPDATE write_operations SET status_json=? WHERE request_id=?")
                    .bind(serde_json::to_string(&status)?)
                    .bind(&id)
                    .execute(&mut *tx)
                    .await?;
            }
            records.insert(
                id,
                WriteRecord {
                    status,
                    identity,
                    fingerprint,
                    prepared: None,
                    approval: None,
                    cancellation: CancellationToken::new(),
                    task_running: false,
                    approval_deadline: Instant::now(),
                    projected_revision: 0,
                },
            );
        }
        tx.commit().await?;
        Ok((
            Self {
                pool,
                key: Zeroizing::new(key),
            },
            records,
        ))
    }

    async fn insert(&self, record: &WriteRecord) -> anyhow::Result<()> {
        let json = serde_json::to_string(&record.status)?;
        timeout(JOURNAL_TIMEOUT, sqlx::query("INSERT INTO write_operations (request_id,identity_digest,request_fingerprint,status_json) VALUES (?,?,?,?)")
            .bind(&record.status.request_id).bind(&record.identity).bind(&record.fingerprint).bind(json)
            .execute(&self.pool)).await??;
        Ok(())
    }

    async fn update(&self, status: &WriteStatus) -> anyhow::Result<()> {
        let changed = timeout(
            JOURNAL_TIMEOUT,
            sqlx::query("UPDATE write_operations SET status_json=? WHERE request_id=?")
                .bind(serde_json::to_string(status)?)
                .bind(&status.request_id)
                .execute(&self.pool),
        )
        .await??;
        anyhow::ensure!(
            changed.rows_affected() == 1,
            "write journal record is missing"
        );
        Ok(())
    }

    // HMAC-SHA256 with domain separation and length-prefixed parts. A local
    // random key prevents low-entropy row values and idempotency keys from
    // becoming bare hashes; neither the serialized request nor its values are
    // written to this journal. Never export the local key alongside summaries.
    fn digest(&self, domain: &[u8], parts: &[&[u8]]) -> String {
        let mut inner_pad = [0x36_u8; 64];
        let mut outer_pad = [0x5c_u8; 64];
        for (index, byte) in self.key.iter().enumerate() {
            inner_pad[index] ^= byte;
            outer_pad[index] ^= byte;
        }
        let mut inner = Sha256::new();
        inner.update(inner_pad);
        inner.update((domain.len() as u64).to_be_bytes());
        inner.update(domain);
        for part in parts {
            inner.update((part.len() as u64).to_be_bytes());
            inner.update(part);
        }
        let mut outer = Sha256::new();
        outer.update(outer_pad);
        outer.update(inner.finalize());
        format!("{:x}", outer.finalize())
    }
}

async fn authorize(
    app: &AppState,
    token_id: &str,
    request: &WriteRequest,
) -> Result<(ConnectionConfig, String, (String, String)), WriteError> {
    let (connection, language) = authorize_connection(app, token_id, request).await?;
    let target = crate::write_catalog::resolve_allowed(
        &app.db,
        &connection,
        &app.vault,
        request.target_id.as_deref(),
        &request.schema,
        &request.table,
    )
    .await?;
    Ok((connection, language, target))
}

async fn authorize_connection(
    app: &AppState,
    token_id: &str,
    request: &WriteRequest,
) -> Result<(ConnectionConfig, String), WriteError> {
    if token_id.is_empty() || app.is_emergency_disabled() || app.config_error().is_some() {
        return Err(error("write_denied"));
    }
    if app.mcp_cancellation_token().await.is_cancelled() {
        return Err(error("write_cancelled"));
    }
    let config = app.config.read().await;
    if !config.server.require_token
        || !config
            .tools
            .iter()
            .any(|tool| tool.name == SUBMIT_TOOL && tool.enabled)
    {
        return Err(error("write_disabled"));
    }
    let connection = config
        .connections
        .iter()
        .find(|connection| connection.id == request.connection_id)
        .filter(|connection| connection.enabled && connection.write_enabled)
        .ok_or_else(|| error("write_denied"))?;
    let connection = connection.clone();
    let language = config.settings.language.clone();
    drop(config);
    let allowed = app
        .access
        .allows_write(token_id, &request.connection_id, request.operation.as_str())
        .await
        .map_err(|_| error("authorization_unavailable"))?;
    if !allowed {
        return Err(error("write_denied"));
    }
    Ok((connection, language))
}

/// Binds an idempotency identity to the exact request content. Structural
/// validation belongs to `write::validate_request`; this only rejects what the
/// journal would be unable to represent faithfully.
fn encode_request(
    token_id: &str,
    request: &WriteRequest,
) -> Result<Zeroizing<Vec<u8>>, WriteError> {
    if token_id.is_empty()
        || token_id.len() > 256
        || request.connection_id.is_empty()
        || request.connection_id.len() > 128
        || request
            .target_id
            .as_ref()
            .is_some_and(|id| !crate::write_catalog::valid_target_id(id))
    {
        return Err(error("invalid_write_request"));
    }
    let bytes =
        Zeroizing::new(serde_json::to_vec(request).map_err(|_| error("invalid_write_request"))?);
    if bytes.len() > MAX_REQUEST_BYTES {
        return Err(error("request_too_large"));
    }
    Ok(bytes)
}

fn next_status(
    current: &WriteStatus,
    state: WriteOperationStatus,
    failure: Option<WriteError>,
    affected_rows: Option<u64>,
) -> WriteStatus {
    let mut next = current.clone();
    next.status = state;
    next.updated_at = Utc::now();
    next.revision = current.revision.saturating_add(1);
    next.affected_rows = affected_rows;
    next.outcome_unknown = state == WriteOperationStatus::Unknown;
    next.error_code = failure.as_ref().map(|failure| failure.code.to_string());
    next.error_message = failure.map(|failure| failure.message);
    next
}

fn execution_budget(config: &ConnectionConfig) -> Duration {
    Duration::from_millis(config.query_timeout_ms.clamp(500, 60_000) * 3 + 10_000)
}

async fn rollback(
    pending: PendingWrite,
    budget: Duration,
    failure: WriteError,
) -> (WriteOperationStatus, Option<WriteError>, Option<u64>) {
    match timeout(budget, pending.rollback()).await {
        Ok(Ok(())) => (WriteOperationStatus::RolledBack, Some(failure), None),
        Ok(Err(_)) | Err(_) => (
            WriteOperationStatus::Unknown,
            Some(unknown_error("rollback_unknown")),
            None,
        ),
    }
}

fn sanitize_write_error(failure: WriteError) -> WriteError {
    // Every message that reaches this function is produced by this crate, and
    // database server text never enters a `WriteError` (see `write::database_error`).
    // The only thing a sanitizer has to guarantee is that database details do not
    // leak, so stable codes are mapped to stable categories and the explanation is
    // preserved: a caller that is told why a table is not writable can correct the
    // request instead of guessing.
    let code = match failure.code.as_ref() {
        "write_cancelled" | "cancelled" => "write_cancelled",
        "prepare_timeout" => "prepare_timeout",
        "execution_timeout" | "timeout" => "execution_timeout",
        "commit_timeout" => "commit_timeout",
        "rollback_unknown" | "rollback_failed" => "rollback_unknown",
        "stale_preview" | "row_changed" | "concurrent_change" => "stale_preview",
        "invalid_write_request"
        | "invalid_request"
        | "invalid_identifier"
        | "invalid_column"
        | "invalid_value"
        | "invalid_number"
        | "invalid_primary_key"
        | "primary_key_immutable"
        | "missing_required_column"
        | "explicit_values_required"
        | "request_limit"
        | "row_exists" => "invalid_write_request",
        "unsupported_database" | "unsupported_driver" | "unsupported_capability"
        | "unsupported_column_type" | "connection_unavailable" | "connection_options" => {
            "unsupported_database"
        }
        "row_count_mismatch" | "row_count" => "row_count_mismatch",
        "outcome_unknown" => "outcome_unknown",
        "database_error" => "write_failed",
        _ => "write_failed",
    };
    let mut sanitized = error(code);
    // Preserve the explanation only for codes this crate owns; anything else
    // keeps the generic text. `database_error` is safe: its text is built by
    // `write::database_error` and carries a vendor error code, never a server
    // message.
    if code != "write_failed"
        || matches!(
            failure.code.as_str(),
            "preview_cleanup" | "preview_limit" | "database_error"
        )
    {
        sanitized.message = failure.message;
    }
    sanitized.code = code.to_string();
    sanitized.outcome_unknown = failure.outcome_unknown;
    sanitized
}

fn error(code: &str) -> WriteError {
    WriteError {
        code: code.to_string(),
        message: error_message(code).to_string(),
        outcome_unknown: false,
    }
}

fn unknown_error(code: &str) -> WriteError {
    let mut failure = error(code);
    failure.outcome_unknown = true;
    failure
}

fn error_message(code: &str) -> &'static str {
    match code {
        "write_unavailable" => "Write operations are unavailable.",
        "write_denied" => "Write permission is not available for this request.",
        "write_disabled" => "Authenticated write submission is disabled.",
        "write_draining" => "Write operations are shutting down.",
        "table_not_allowed" => "The requested table is not enabled for writes.",
        "authorization_unavailable" => "Write authorization could not be verified.",
        "authorization_changed" => "Write authorization or configuration changed.",
        "configuration_busy" => "Configuration is currently being changed.",
        "configuration_changed" => "The approved connection configuration changed.",
        "idempotency_conflict" => "This idempotency key belongs to a different request.",
        "pending_limit" => "The pending write request limit has been reached.",
        "journal_full" => "The write journal is full; existing idempotency records were retained.",
        "journal_error" => {
            "The write journal could not be persisted. Do not retry with a new idempotency key."
        }
        "write_not_found" => "The write request was not found.",
        "stale_approval" => "This approval revision is no longer pending.",
        "approval_not_ready" => "The write preview is not ready for approval.",
        "approval_expired" => "The local approval request expired.",
        "approval_unavailable" => "The local approval window could not be opened.",
        "capacity_exceeded" => "The connection execution limit has been reached.",
        "write_cancelled" => "The write request was cancelled.",
        "prepare_timeout" => "Preparing the write preview timed out.",
        "execution_timeout" => "The write execution timed out.",
        "commit_timeout" => "The commit outcome could not be confirmed.",
        "rollback_unknown" => "The rollback outcome could not be confirmed.",
        "outcome_unknown" => {
            "The transaction outcome could not be confirmed. Do not retry automatically."
        }
        "application_restarted" => "The application restarted before the operation was finalized.",
        "invalid_write_request" => "The structured write request is invalid.",
        "request_too_large" => "The structured write request exceeds the size limit.",
        "stale_preview" => "The target row changed after the preview was prepared.",
        "unsupported_database" => "This connection does not support approved writes.",
        "row_count_mismatch" => "The operation did not affect exactly one row.",
        "internal_error" => "The write operation could not be completed safely.",
        _ => "The write operation failed. Database details were not exposed.",
    }
}

fn is_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn secure_journal_file(path: &Path) -> anyhow::Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    match options.open(path) {
        Ok(file) => file.sync_all()?,
        Err(failure) if failure.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(failure) => return Err(failure.into()),
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}
