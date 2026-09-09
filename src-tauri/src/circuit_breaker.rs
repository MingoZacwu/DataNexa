use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;

use crate::audit::{AuditActor, AuditStatus};
use crate::i18n::backend_text;
use crate::state::AppState;

pub const CIRCUIT_BREAKER_TOOL: &str = "system.auto_circuit_breaker";

/// Best-effort circuit breaker evaluation. Failures never affect the tool-call
/// result; they are logged to stderr only.
pub async fn evaluate(app: Arc<AppState>, token_id: String) {
    if let Err(error) = evaluate_inner(app, token_id).await {
        eprintln!("circuit breaker evaluation failed: {error}");
    }
}

async fn evaluate_inner(app: Arc<AppState>, token_id: String) -> anyhow::Result<()> {
    let (breaker_enabled, require_token, window_minutes, threshold, language, retention_days) = {
        let config = app.config.read().await;
        (
            config.settings.auto_circuit_breaker,
            config.server.require_token,
            config.settings.auto_circuit_breaker_window_minutes,
            config.settings.auto_circuit_breaker_threshold,
            config.settings.language.clone(),
            config.settings.audit_retention_days,
        )
    };
    // The breaker only applies when bearer authentication is active; without
    // it requests carry no token identity at all.
    if !breaker_enabled || !require_token {
        return Ok(());
    }
    let cutoff = (Utc::now() - chrono::Duration::minutes(window_minutes as i64))
        .timestamp_millis();
    let count = app
        .audit
        .count_token_denials(&token_id, CIRCUIT_BREAKER_TOOL, cutoff)
        .await?;
    if count < threshold as u64 {
        return Ok(());
    }
    // Only the request that performs the enabled -> disabled transition
    // records the breaker audit event; concurrent requests are skipped.
    if !app.access.disable_if_enabled(&token_id).await? {
        return Ok(());
    }
    let started = Instant::now();
    let reason = backend_text(&language).circuit_breaker_reason(window_minutes, threshold, count);
    app.audit
        .record_with_actor(
            AuditActor::system_for_token(token_id),
            None,
            None,
            CIRCUIT_BREAKER_TOOL,
            AuditStatus::Denied,
            Some(reason),
            Some(started.elapsed().as_millis().try_into().unwrap_or(u64::MAX)),
            None,
            None,
            retention_days,
        )
        .await?;
    Ok(())
}
