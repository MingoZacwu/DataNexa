import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { Check, ChevronRight, Code2, Database, KeyRound, Pencil, Plus, Trash2, X } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import type { MouseEvent } from "react";
import brandLogoUrl from "../../../resources/datanexa.png";
import { detectLocale, normalizeLocale } from "../../i18n";
import { detectThemeMode, resolveTheme, systemTheme } from "../../app/theme";
import { api } from "../../lib/tauri";
import type { WriteApprovalDto, WriteStatus, WriteValue } from "../../types";
import { writeErrorText, writeMessages, writeWarningText } from "../../write-i18n";
import { auditReasonText } from "../audit/audit-write-text";

function previewApproval(): WriteApprovalDto {
  const now = Date.now();
  return {
    request: {
      request_id: "preview-only-request", connection_id: "preview-only-connection", token_id: "preview-only-token",
      operation: "update", schema: "public", table: "inventory_demo", status: "pending",
      created_at: new Date(now).toISOString(), expires_at: new Date(now + 5 * 60 * 1000).toISOString(), revision: 1
    },
    connection_name: "Demo Inventory", token_name: "Preview Agent",
    primary_key: { item_id: { type: "text", value: "DEMO-042" } },
    changes: [
      { column: "stock_limit", before: { type: "number", value: "20" }, after: { type: "number", value: "30" } },
      { column: "reorder_level", before: { type: "number", value: "8" }, after: { type: "number", value: "12" } }
    ],
    notes: [{ column: "updated_at", code: "auto_generated" }],
    warnings: ["triggers_present"],
    sql: "UPDATE public.inventory_demo SET stock_limit = $1, reorder_level = $2 WHERE item_id = $3",
    reason: "Adjust inventory thresholds for demonstration data.", language: detectLocale()
  };
}

function valueText(value: WriteValue | null) {
  if (value === null) return "—";
  if (value.type === "null") return "NULL";
  return String(value.value);
}

function diagnosticCodes(message?: string | null) {
  return message?.match(/SQLSTATE(?:\s*[:=]?\s*|\[)[A-Z0-9]{5}\]?/gi)?.join(" · ") ?? "";
}

const activeStatuses = new Set(["pending", "executing", "committing"]);

export function WriteApprovalWindow({ previewMode: preview }: { previewMode: boolean }) {
  const [approval, setApproval] = useState<WriteApprovalDto | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [available, setAvailable] = useState(false);
  const [themeReady, setThemeReady] = useState(false);
  const [readyRequestId, setReadyRequestId] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [now, setNow] = useState(Date.now());
  const [error, setError] = useState<"fetchError" | "decisionError" | "closeError" | "closeAfterDecisionError" | "dragError" | "readyError" | null>(null);
  const [errorDetail, setErrorDetail] = useState<string | null>(null);
  const [fetchFailed, setFetchFailed] = useState(false);
  const [previewResult, setPreviewResult] = useState<"previewApproved" | "previewClosed" | null>(null);
  const [readyRetry, setReadyRetry] = useState(0);
  const currentRef = useRef<WriteApprovalDto | null>(null);
  const actionRef = useRef(false);
  const epochRef = useRef(0);
  const mountedRef = useRef(false);
  const bodyRef = useRef<HTMLDivElement>(null);
  const locale = approval?.language ? normalizeLocale(approval.language) : detectLocale();
  const t = writeMessages[locale];
  const request = approval?.request;
  const operation = request?.operation;
  const heading = operation === "delete" ? t.deleteTitle : operation === "insert" ? t.insertTitle : t.updateTitle;
  const approveLabel = operation === "delete" ? t.approveDelete : operation === "insert" ? t.approveInsert : t.approveUpdate;
  // Unknown state falls back to the neutral update illustration.
  const illustration = operation === "insert" ? "insert" : operation === "delete" ? "delete" : "update";
  const expiresAt = request ? Date.parse(request.expires_at) : NaN;
  const expired = Boolean(request?.status === "pending" && (!Number.isFinite(expiresAt) || now >= expiresAt));
  const canDecide = Boolean(request?.status === "pending" && available && !expired && !busy && !fetchFailed && (preview || readyRequestId === request.request_id));
  const displayStatus = expired ? "expired" : request?.status;

  const setCurrent = useCallback((next: WriteApprovalDto | null) => {
    currentRef.current = next;
    setApproval(next);
  }, []);

  const acceptFetched = useCallback((next: WriteApprovalDto | null) => {
    const current = currentRef.current;
    if (!current) {
      setCurrent(next);
      setAvailable(Boolean(next));
    } else if (next?.request.request_id === current.request.request_id) {
      if (next.request.revision >= current.request.revision) {
        setCurrent(next);
        setAvailable(true);
      }
    } else {
      // A window is bound to one request for its whole lifetime; fail closed if
      // the native side ever reports a different one.
      setAvailable(false);
    }
    setLoaded(true);
    setFetchFailed(false);
    setError((currentError) => currentError === "fetchError" || currentError === "decisionError" ? null : currentError);
  }, [setCurrent]);

  useEffect(() => {
    mountedRef.current = true;
    return () => { mountedRef.current = false; };
  }, []);

  useEffect(() => {
    document.documentElement.lang = locale;
    document.title = t.title;
  }, [locale, t.title]);

  useEffect(() => {
    let cancelled = false;
    let sequence = 0;
    const query = window.matchMedia("(prefers-color-scheme: dark)");
    const applyTheme = async () => {
      const call = ++sequence;
      const mode = detectThemeMode();
      const dark = resolveTheme(mode, systemTheme()) === "dark";
      document.documentElement.classList.toggle("dark", dark);
      document.documentElement.dataset.theme = mode;
      try {
        const [nativeDark, material] = await api.setWindowMaterialTheme(mode === "system" ? null : dark);
        if (cancelled || call !== sequence) return;
        document.documentElement.dataset.systemMaterial = material ? "enabled" : "fallback";
        document.documentElement.classList.toggle("dark", material ? nativeDark : dark);
      } catch {
        // A solid sidebar is the supported fallback when material is unavailable.
        if (cancelled || call !== sequence) return;
        document.documentElement.dataset.systemMaterial = "fallback";
      }
      setThemeReady(true);
    };
    const onStorage = (event: StorageEvent) => {
      if (event.key === "datanexa.theme") void applyTheme();
    };
    void applyTheme();
    query.addEventListener("change", applyTheme);
    window.addEventListener("storage", onStorage);
    return () => {
      cancelled = true;
      query.removeEventListener("change", applyTheme);
      window.removeEventListener("storage", onStorage);
    };
  }, []);

  useEffect(() => {
    if (preview) {
      acceptFetched(previewApproval());
      return;
    }
    let cancelled = false;
    let fetching = false;
    let unlisten: UnlistenFn | undefined;
    const fetchApproval = async () => {
      if (cancelled || fetching || actionRef.current) return;
      fetching = true;
      const epoch = epochRef.current;
      try {
        const next = await api.getWriteApproval();
        if (!cancelled && epoch === epochRef.current) acceptFetched(next);
      } catch (cause) {
        if (!cancelled && epoch === epochRef.current) {
          setErrorDetail(writeErrorText(cause));
          setFetchFailed(true);
          setError("fetchError");
        }
      } finally {
        fetching = false;
      }
    };
    void fetchApproval();
    void listen("write-approval-changed", () => {
      // Never read values from event payloads or the main-window snapshot.
      void fetchApproval();
    }).then((dispose) => {
      if (cancelled) dispose();
      else unlisten = dispose;
    }).catch(() => {
      // Polling remains active if native events are unavailable.
    });
    const timer = window.setInterval(() => void fetchApproval(), 2000);
    return () => {
      cancelled = true;
      unlisten?.();
      window.clearInterval(timer);
    };
  }, [acceptFetched, preview]);

  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, []);

  useEffect(() => {
    bodyRef.current?.scrollTo({ top: 0 });
    // Focus the scroll container, never the close button: Enter must not reject
    // a request by accident. The close button stays reachable with Tab.
    bodyRef.current?.focus();
  }, [request?.request_id]);

  useEffect(() => {
    if (!loaded || !themeReady) return;
    let cancelled = false;
    // Effects run after the DTO and theme commit. Hidden WebViews may suspend
    // animation frames, so native visibility must not depend on a paint.
    void (preview ? api.readyWriteApprovalPreview() : api.readyWriteApproval()).then(() => {
      if (!cancelled) setReadyRequestId(request?.request_id ?? null);
    }).catch((cause) => {
      if (!cancelled) {
        setErrorDetail(writeErrorText(cause));
        setReadyRequestId(null);
        setError("readyError");
      }
    });
    return () => {
      cancelled = true;
    };
  }, [loaded, themeReady, preview, readyRetry, request?.request_id]);

  const close = useCallback(async () => {
    if (actionRef.current) return;
    actionRef.current = true;
    epochRef.current += 1;
    setBusy(true);
    try {
      if (preview) {
        const current = currentRef.current;
        if (current?.request.status === "pending") setCurrent({ ...current, request: { ...current.request, status: "rejected", revision: current.request.revision + 1 } });
        setPreviewResult("previewClosed");
        setAvailable(false);
        await api.closeWriteApprovalPreview();
      } else {
        await api.closeWriteApproval();
        if (mountedRef.current) {
          setCurrent(null);
            setAvailable(false);
        }
      }
    } catch (cause) {
      if (mountedRef.current) { setErrorDetail(writeErrorText(cause)); setError("closeError"); }
    } finally {
      actionRef.current = false;
      epochRef.current += 1;
      if (mountedRef.current) setBusy(false);
    }
  }, [preview, setCurrent]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        void close();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [close]);

  async function decide(approve: boolean) {
    const current = currentRef.current;
    if (actionRef.current || !current || current.request.status !== "pending" || !canDecide || Date.now() >= Date.parse(current.request.expires_at)) return;
    actionRef.current = true;
    epochRef.current += 1;
    setBusy(true);
    setError(null);
    let decided = false;
    try {
      const status: WriteStatus = preview
        ? { ...current.request, status: approve ? "committed" : "rejected", revision: current.request.revision + 1, affected_rows: approve ? 1 : 0 }
        : await api.decideWriteApproval(current.request.request_id, current.request.revision, approve);
      decided = true;
      if (!mountedRef.current) return;
      setCurrent({ ...current, request: status });
      if (preview) {
        if (approve) setPreviewResult("previewApproved");
        return;
      }
      // A decision ends this window. Any later request opens its own window;
      // nothing about it belongs to this review.
      await api.closeWriteApproval();
      if (mountedRef.current) {
        setCurrent(null);
        setAvailable(false);
      }
    } catch (cause) {
      if (!mountedRef.current) return;
      setErrorDetail(writeErrorText(cause));
      if (decided) {
        // The decision is already recorded; only the window close failed.
        setError("closeAfterDecisionError");
      } else {
        setError("decisionError");
        setFetchFailed(true);
        // Do not retry a decision; only re-read the trusted native state.
      }
    } finally {
      actionRef.current = false;
      epochRef.current += 1;
      if (mountedRef.current) setBusy(false);
    }
  }

  function startDrag(event: MouseEvent<HTMLElement>) {
    if (event.button !== 0 || event.detail !== 1 || (event.target as HTMLElement).closest("button")) return;
    void (preview ? api.dragWriteApprovalPreview() : api.dragWriteApproval()).catch((cause) => { setErrorDetail(writeErrorText(cause)); setError("dragError"); });
  }

  const statusHint = expired || request?.status === "expired" ? t.expiredHint
    : request?.status === "cancelled" ? t.cancelledHint
      : request?.status === "unknown" ? t.unknownHint
        : request?.status === "pending" && !available ? t.unavailableHint : null;

  return (
    <section className={`write-approval-window${operation === "delete" ? " is-delete" : ""}`} aria-label={t.title}>
      <aside className="wa-context" onMouseDown={startDrag}>
        <div className="wa-illustration" aria-hidden="true">
          <div className="wa-record wa-record-back" />
          <div className="wa-record wa-record-front">
            <div className="wa-record-heading" />
            <div className="wa-record-line" />
            <div className={`wa-record-line wa-record-short${illustration === "delete" ? " wa-record-removed" : ""}`} />
            {illustration === "insert"
              ? <div className="wa-record-newrow"><Plus size={8} /><span /></div>
              : <div className="wa-record-edit"><span /><span /></div>}
          </div>
          <div className={`wa-record-badge${illustration === "delete" ? " wa-record-badge-delete" : ""}`}>
            {illustration === "delete" ? <Trash2 size={13} /> : illustration === "insert" ? <Plus size={14} /> : <Pencil size={14} />}
          </div>
        </div>
        <div className="wa-identity"><div className="wa-meta-label"><Database size={13} />{t.connection}</div><div className="wa-identity-name">{approval?.connection_name ?? "—"}</div></div>
        <div className="wa-identity"><div className="wa-meta-label"><KeyRound size={13} />{t.token}</div><div className="wa-identity-name">{approval?.token_name ?? "—"}</div></div>
        <div className="wa-brand"><img src={brandLogoUrl} alt="" /><span>DataNexa</span></div>
      </aside>
      <div className="wa-main">
        <header className="wa-heading" onMouseDown={startDrag}>
          <h1>{heading}</h1>
          <button type="button" className="wa-close" aria-label={canDecide ? t.closeReject : t.close} disabled={busy} onClick={() => void close()}><X size={17} /></button>
        </header>
        <div className="wa-body" ref={bodyRef} tabIndex={-1}>
          {preview && <p className="wa-notice">{t.preview}</p>}
          {!loaded ? <p className="wa-meta" role="status">{t.loading}</p> : !approval ? <p className="wa-meta" role="status">{t.empty}</p> : (
            <>
              <div className="wa-target"><code>{request?.schema}.{request?.table}</code>
                {Object.keys(approval.primary_key).length > 0 && <div className="wa-predicate">{t.primaryKey} {Object.entries(approval.primary_key).map(([column, value]) => <code key={column}>{column} = {valueText(value)}</code>)}</div>}
              </div>
              {operation === "delete" ? (
                // A delete is identified by the complete primary key only, so the
                // diff lists those columns instead of a before/after pair.
                <table className="wa-diff wa-diff-key" aria-label={t.deleteTitle}>
                  <thead><tr><th scope="col">{t.column}</th><th scope="col">{t.before}</th></tr></thead>
                  <tbody>{Object.entries(approval.primary_key).map(([column, value]) => <tr key={column}><td><code>{column}</code></td><td className="wa-old"><code>{valueText(value)}</code></td></tr>)}</tbody>
                </table>
              ) : (
                <table className="wa-diff" aria-label={operation === "insert" ? t.insertTitle : t.updateTitle}>
                  <thead><tr><th scope="col">{t.column}</th>{operation === "update" && <th scope="col">{t.before}</th>}<th scope="col">{t.after}</th></tr></thead>
                  <tbody>{approval.changes.map((change) => <tr key={change.column}><td><code>{change.column}</code></td>{operation === "update" && <td className="wa-old"><code>{valueText(change.before)}</code></td>}<td><span className="wa-new"><code>{valueText(change.after)}</code></span></td></tr>)}</tbody>
                </table>
              )}
              {operation === "delete" && <p className="wa-read-note">{t.deleteNote}</p>}
              {operation === "update" && <p className="wa-read-note">{t.previewNote}</p>}
              {approval.notes && approval.notes.length > 0 && (
                <div className="wa-notes">
                  <div className="wa-meta-label">{t.notes}</div>
                  <ul className="wa-note-list">{approval.notes.map((note) => <li key={note.column}><code>{note.column}</code><span>{note.code === "uses_default" ? t.noteUsesDefault : t.noteAutoGenerated}</span></li>)}</ul>
                </div>
              )}
              {approval.warnings?.map((code) => <p className="wa-notice wa-warning" key={code}>{writeWarningText(locale, code)}</p>)}
              <details className="wa-details" key={request?.request_id}>
                <summary><ChevronRight className="wa-chevron" size={12} /><Code2 size={14} />{t.details}</summary>
                <div className="wa-detail-content">
                  <div className="wa-meta-label">{t.agentReason}</div>
                  <p>{approval.reason || t.noReason}</p>
                  <pre>{approval.sql}</pre>
                  <div className="wa-meta">{t.requestId}: <code>{request?.request_id}</code></div>
                </div>
              </details>
              <p className="wa-meta wa-expiry">{t.expires}: {Number.isFinite(expiresAt) ? new Date(expiresAt).toLocaleString(locale) : "—"}</p>
              {statusHint && <p className="wa-notice" role="status">{statusHint}</p>}
              {(request?.error_code || request?.error_message) && <div className="wa-notice" role="status"><code>{request.error_code}</code><p>{diagnosticCodes(request.error_message)}</p>{request.error_message && <p>{auditReasonText(locale, { reason: request.error_message, reason_code: request.error_code })}</p>}</div>}
              {request?.affected_rows != null && <p className="wa-meta">{t.affectedRows}: {request.affected_rows}</p>}
            </>
          )}
          {error && <div className="wa-notice" role="alert"><p>{t[error]}{error === "readyError" && <button type="button" className="wa-link" onClick={() => { setError(null); setReadyRetry((value) => value + 1); }}>{t.retry}</button>}</p>{errorDetail && <><code>{diagnosticCodes(errorDetail)}</code><details><summary>{t.serverDetail}</summary><pre>{auditReasonText(locale, { reason: errorDetail })}</pre></details></>}</div>}
        </div>
        <footer className="wa-footer">
          <div className="wa-result" role="status" aria-live="polite">{previewResult ? t[previewResult] : displayStatus ? t.statusLabels[displayStatus] : t.empty}</div>
          {request?.status === "pending" && <div className="wa-actions">
            <button type="button" className="wa-button" disabled={!canDecide} onClick={() => void decide(false)}><X size={14} />{t.reject}</button>
            <button type="button" className="wa-button wa-primary" disabled={!canDecide} onClick={() => void decide(true)}><Check size={14} />{approveLabel}</button>
          </div>}
          {preview && request && !activeStatuses.has(request.status) && <button type="button" className="wa-button" onClick={() => { setCurrent(previewApproval()); setAvailable(true); setPreviewResult(null); }}>{t.previewReset}</button>}
        </footer>
      </div>
    </section>
  );
}
