import * as Dialog from "@radix-ui/react-dialog";
import clsx from "clsx";
import { AlertTriangle, Check, ChevronDown, Database, KeyRound, PenLine, Plus, RefreshCw, Search, ShieldCheck, Table2, Trash2, X } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { formatMessage, type Locale } from "../../i18n";
import { api, isTauri } from "../../lib/tauri";
import type { AppSnapshot, ConnectionConfig, WriteGrantInput, WriteOperation, WriteSchema, WriteTable, WriteTarget } from "../../types";
import { writeErrorText, writeMessages, writeReasonDetail } from "../../write-i18n";
import { DATABASE_LOGOS } from "../../app/assets";
import { IconTooltip, StatusPill, SwitchField } from "../../components/ui";
import "./write-center.css";

type Grants = Record<string, Record<WriteOperation, boolean>>;

export type WriteDraft = {
  enabled: boolean;
  tables: WriteTable[];
  grants: Grants;
  /** Identity of the configuration this draft was made from. */
  source: string;
  dirty: boolean;
  saving: boolean;
  error: string | null;
};

export type WriteDrafts = Record<string, WriteDraft>;

const OPERATIONS: WriteOperation[] = ["insert", "update", "delete"];

/** Rows shown before the writable-table list folds behind a "show all" toggle. */
const TABLE_PREVIEW_COUNT = 5;

/** Must match `MAX_LISTED_TABLES` in src-tauri/src/write_catalog.rs. */
const TABLE_LIST_LIMIT = 500;

/**
 * How long a change must rest before it is written. Long enough that toggling a
 * few grants in a row produces one save, short enough to feel immediate.
 */
const AUTOSAVE_DELAY_MS = 500;

const connectionIdentity = (connection: ConnectionConfig) => JSON.stringify([
  connection.id, connection.type, connection.database, connection.host, connection.port,
  connection.credential_ref, connection.ssl_mode, connection.write_enabled, connection.write_tables
]);

const emptyGrants = (): Record<WriteOperation, boolean> => ({ insert: false, update: false, delete: false });

function makeDraft(snapshot: AppSnapshot, connection: ConnectionConfig): WriteDraft {
  const grants: Grants = {};
  for (const token of snapshot.access_tokens) {
    for (const operation of OPERATIONS) {
      grants[token.id] = grants[token.id] ?? emptyGrants();
      grants[token.id][operation] = token.write_grants.some(
        (grant) => grant.connection_id === connection.id && grant.operation === operation
      );
    }
  }
  return {
    enabled: connection.write_enabled,
    tables: connection.write_tables.map((table) => ({ ...table })),
    grants,
    source: connectionIdentity(connection),
    dirty: false,
    saving: false,
    error: null
  };
}

export function WriteCenterView({ locale, snapshot, drafts, onDraftsChange, selectedId, onSelectedIdChange, onSnapshot, onOpenAccess, onOpenTools }: {
  locale: Locale;
  snapshot: AppSnapshot;
  drafts: WriteDrafts;
  onDraftsChange: (update: (current: WriteDrafts) => WriteDrafts) => void;
  selectedId: string;
  onSelectedIdChange: (id: string) => void;
  onSnapshot: (snapshot: AppSnapshot) => void;
  onOpenAccess: () => void;
  onOpenTools: () => void;
}) {
  const t = writeMessages[locale];
  const connections = snapshot.config.connections;
  const selected = connections.find((connection) => connection.id === selectedId)
    ?? connections.find((connection) => connection.type !== "jdbc")
    ?? connections[0];
  const [pickerOpen, setPickerOpen] = useState(false);
  const [approvalError, setApprovalError] = useState<string | null>(null);
  const [tablesExpanded, setTablesExpanded] = useState(false);
  // One in-flight save per connection; a request that arrives while a save is
  // running marks the draft dirty again and the timer picks it up afterwards.
  const savingRef = useRef<Record<string, boolean>>({});
  const timerRef = useRef<Record<string, number>>({});
  // Edit counter per connection. A response may only replace a draft that has
  // not been edited since its save started.
  const revisionRef = useRef<Record<string, number>>({});

  // Keep a draft per connection. Background snapshot polls never overwrite an
  // edit in progress; only a successful save replaces the draft.
  useEffect(() => {
    onDraftsChange((current) => {
      let next = current;
      for (const connection of snapshot.config.connections) {
        const fresh = makeDraft(snapshot, connection);
        const existing = current[connection.id];
        if (!existing || (!existing.dirty && !existing.saving && existing.source !== fresh.source)) {
          next = { ...next, [connection.id]: fresh };
        }
      }
      return next;
    });
  }, [snapshot, onDraftsChange]);

  useEffect(() => {
    if (selected && selected.id !== selectedId) onSelectedIdChange(selected.id);
  }, [selected?.id, selectedId, onSelectedIdChange]);

  // The fold belongs to the connection being edited, so switching connections
  // starts from the collapsed list again.
  useEffect(() => {
    setTablesExpanded(false);
  }, [selected?.id]);

  // Autosave timers must not fire after the view is gone.
  useEffect(() => () => {
    for (const timer of Object.values(timerRef.current)) window.clearTimeout(timer);
  }, []);

  const submitToolEnabled = snapshot.tools.some(
    (tool) => tool.name === "datanexa_submit_write" && tool.enabled
  );
  const authenticationReady = snapshot.config.server.require_token;
  const jdbc = selected?.type === "jdbc";
  const pendingApprovals = snapshot.pending_write_approvals;
  const draftSource = selected;
  const draft = draftSource ? drafts[selected.id] ?? makeDraft(snapshot, draftSource) : undefined;

  /** Writes the current draft. A save in flight does not block editing: the
   * edit sequence decides whether the draft is still newer than the response,
   * and a newer edit schedules the next save. Failures stay on the draft so the
   * panel can offer a retry instead of discarding the change. */
  const saveDraft = useCallback(async (connectionId: string) => {
    if (savingRef.current[connectionId]) return;
    const pending = draftsRef.current[connectionId];
    if (!pending || !pending.dirty) return;
    const target = snapshotRef.current.config.connections.find((connection) => connection.id === connectionId);
    if (!target) return;
    const sequence = (revisionRef.current[connectionId] ?? 0);
    savingRef.current[connectionId] = true;
    const grants: WriteGrantInput[] = snapshotRef.current.access_tokens.map((token) => ({
      token_id: token.id,
      ...(pending.grants[token.id] ?? emptyGrants())
    }));
    onDraftsChangeRef.current((current) => {
      const entry = current[connectionId];
      if (!entry) return current;
      return { ...current, [connectionId]: { ...entry, saving: true, error: null } };
    });
    try {
      const response = await api.saveWriteConfiguration(connectionId, pending.enabled, pending.tables, grants);
      onSnapshotRef.current(response);
      const saved = response.config.connections.find((entry) => entry.id === connectionId);
      const superseded = (revisionRef.current[connectionId] ?? 0) !== sequence;
      onDraftsChangeRef.current((current) => {
        const entry = current[connectionId];
        if (!entry) return current;
        if (superseded) {
          // The user changed something while this save was in flight. Keep the
          // newer state and let the follow-up save persist it; do not reset.
          return { ...current, [connectionId]: { ...entry, saving: false } };
        }
        return { ...current, [connectionId]: saved ? makeDraft(response, saved) : { ...entry, saving: false } };
      });
      savingRef.current[connectionId] = false;
      if (superseded) {
        scheduleSaveRef.current(connectionId, AUTOSAVE_DELAY_MS);
      }
    } catch (error) {
      onDraftsChangeRef.current((current) => {
        const entry = current[connectionId];
        if (!entry) return current;
        return { ...current, [connectionId]: { ...entry, saving: false, error: writeErrorText(error) } };
      });
      savingRef.current[connectionId] = false;
    }
  }, []);

  // Refs keep the timer callback stable while still reading the latest props.
  const draftsRef = useRef(drafts);
  const snapshotRef = useRef(snapshot);
  const onDraftsChangeRef = useRef(onDraftsChange);
  const onSnapshotRef = useRef(onSnapshot);
  draftsRef.current = drafts;
  snapshotRef.current = snapshot;
  onDraftsChangeRef.current = onDraftsChange;
  onSnapshotRef.current = onSnapshot;

  function scheduleSave(connectionId: string, delay: number) {
    window.clearTimeout(timerRef.current[connectionId]);
    timerRef.current[connectionId] = window.setTimeout(() => {
      void saveDraftRef.current(connectionId);
    }, delay);
  }
  const scheduleSaveRef = useRef(scheduleSave);
  scheduleSaveRef.current = scheduleSave;

  function updateDraft(connectionId: string, update: (draft: WriteDraft) => WriteDraft) {
    revisionRef.current[connectionId] = (revisionRef.current[connectionId] ?? 0) + 1;
    onDraftsChange((current) => {
      const existing = current[connectionId];
      if (!existing) return current;
      return { ...current, [connectionId]: { ...update(existing), dirty: true, error: null } };
    });
    // Debounce: every edit restarts the window, so one action burst saves once.
    scheduleSave(connectionId, AUTOSAVE_DELAY_MS);
  }
  const saveDraftRef = useRef(saveDraft);
  saveDraftRef.current = saveDraft;

  function setTable(connectionId: string, target: WriteTarget | WriteTable, present: boolean) {
    updateDraft(connectionId, (current) => {
      const matches = (entry: WriteTable) => entry.schema === target.schema && entry.table === target.table;
      const next = present
        ? [...current.tables.filter((entry) => !matches(entry)), { schema: target.schema, table: target.table, target_id: "target_id" in target ? target.target_id : null }]
        : current.tables.filter((entry) => !matches(entry));
      return { ...current, tables: next };
    });
  }

  async function openApproval() {
    setApprovalError(null);
    try {
      await api.openWriteApproval();
    } catch (error) {
      const message = writeErrorText(error);
      setApprovalError(message.includes("no_pending_approval") ? t.noPendingApproval : message);
    }
  }

  if (!authenticationReady) {
    return (
      <section className="panel access-prerequisite">
        <span className="access-prerequisite-icon"><ShieldCheck size={28} /></span>
        <h2>{t.authenticationRequired}</h2>
        <p>{t.authenticationRequiredText}</p>
        <button type="button" className="button primary" onClick={onOpenAccess}>
          <KeyRound size={16} />{t.enableAuthentication}
        </button>
      </section>
    );
  }

  return (
    <>
      <section className="connections-workbench write-workbench">
        <div className="panel connection-browser">
          <div className="data-list-header">
            <span>{t.connections}</span>
            <span>{connections.length}</span>
          </div>
          <div className="connection-list page-scroll-list">
            {connections.length === 0
              ? <div className="empty-state">{t.noConnections}</div>
              : connections.map((connection) => {
                const subtitle = connection.type === "jdbc" ? connection.jdbc_url ?? "" : connection.database;
                const active = connection.id === selected?.id;
                return (
                  <button
                    type="button"
                    key={connection.id}
                    className={clsx("write-connection", active && "selected", !connection.enabled && "disabled")}
                    aria-current={active ? "true" : undefined}
                    onClick={() => onSelectedIdChange(connection.id)}
                  >
                    <span className={clsx("db-badge", connection.type, connection.write_enabled && "write-enabled")}>
                      <img src={DATABASE_LOGOS[connection.type]} alt="" />
                      {connection.write_enabled && (
                        <IconTooltip label={t.writeEnabledBadge}>
                          <span className="write-badge-mark" aria-label={t.writeEnabledBadge}>
                            <PenLine size={9} />
                          </span>
                        </IconTooltip>
                      )}
                    </span>
                    <span className="write-connection-copy">
                      <span className="write-connection-title">
                        <strong>{connection.name}</strong>
                        {connection.type === "jdbc"
                          ? <StatusPill tone="slate" label="JDBC" />
                          : !connection.enabled
                            ? <StatusPill tone="slate" label={t.paused} />
                            : null}
                      </span>
                      <small title={subtitle || undefined}>{subtitle}</small>
                    </span>
                  </button>
                );
              })}
          </div>
        </div>

        <aside className="panel connection-inspector write-inspector">
          {selected && draft ? (
            <>
              <header className="write-head">
                <div className="write-identity">
                  <span className="db-badge"><img src={DATABASE_LOGOS[selected.type]} alt="" /></span>
                  <div className="write-identity-copy">
                    <strong>{selected.name}</strong>
                    <code>{selected.database}</code>
                  </div>
                </div>
                <div className="write-actions">
                  {pendingApprovals > 0 && isTauri && (
                    <button type="button" className="button ghost" onClick={() => void openApproval()}>
                      <ShieldCheck size={15} />{t.openApproval}
                      <span className="write-pending-count">{pendingApprovals}</span>
                    </button>
                  )}
                </div>
              </header>

              {jdbc
                ? (
                  <div className="write-unsupported">
                    <span className="write-art" aria-hidden="true">
                      <span className="write-art-card write-art-back" />
                      <span className="write-art-card write-art-front">
                        <span className="write-art-heading" />
                        <span className="write-art-line" />
                        <span className="write-art-line short" />
                        <span className="write-art-line" />
                      </span>
                      <span className="write-art-badge"><AlertTriangle size={13} /></span>
                    </span>
                    <strong>{t.jdbcUnsupported}</strong>
                  </div>
                )
                : (
                  <div className="write-scroll page-scroll-list">
                    <div className="write-body">
                    <section className="write-section">
                      <h3>{t.writeSection}</h3>
                      <SwitchField
                        label={t.allowWrites}
                        tooltip={t.allowWritesHint}
                        checked={draft.enabled}
                        // Turning writes off must stay possible even when the
                        // connection is paused or the submit tool is disabled;
                        // only granting new access needs those preconditions.
                        disabled={(!selected.enabled || !submitToolEnabled) && !draft.enabled}
                        onCheckedChange={(checked) => updateDraft(selected.id, (current) => ({ ...current, enabled: checked }))}
                      />
                      {!selected.enabled && <p className="field-note">{t.paused}</p>}
                      {!submitToolEnabled && (
                        <p className="field-note">
                          {t.submitToolDisabled}
                          <button type="button" className="write-inline-link" onClick={onOpenTools}>{t.openTools}</button>
                        </p>
                      )}
                    </section>

                    <section className="write-section">
                      <div className="write-section-head">
                        <h3>{t.tablesSection}</h3>
                        <button type="button" className="button ghost" disabled={!selected.enabled} onClick={() => setPickerOpen(true)}>
                          <Plus size={15} />{t.addTable}
                        </button>
                      </div>
                      {draft.tables.length === 0
                        ? <p className="field-note">{t.noTables}</p>
                        : (
                          <>
                            <div className="write-table-list">
                              {(tablesExpanded ? draft.tables : draft.tables.slice(0, TABLE_PREVIEW_COUNT)).map((table) => (
                                <div className="write-table-row" key={`${table.schema}.${table.table}`}>
                                  <Table2 size={15} />
                                  <code>{table.schema}.{table.table}</code>
                                  <IconTooltip label={t.removeTable}>
                                    <button
                                      type="button"
                                      className="icon-button danger"
                                      aria-label={`${t.removeTable}: ${table.schema}.${table.table}`}
                                      onClick={() => setTable(selected.id, table, false)}
                                    >
                                      <Trash2 size={15} />
                                    </button>
                                  </IconTooltip>
                                </div>
                              ))}
                            </div>
                            {draft.tables.length > TABLE_PREVIEW_COUNT && (
                              <button
                                type="button"
                                className="write-table-toggle"
                                aria-expanded={tablesExpanded}
                                onClick={() => setTablesExpanded((current) => !current)}
                              >
                                {tablesExpanded
                                  ? t.collapseTables
                                  : formatMessage(t.expandTables, { count: draft.tables.length })}
                                <ChevronDown size={14} />
                              </button>
                            )}
                          </>
                        )}
                    </section>

                    <section className="write-section">
                      <h3>{t.grantsSection}</h3>
                      {snapshot.access_tokens.length === 0
                        ? <p className="field-note">{t.noTokens}</p>
                        : (
                          <div className="write-token-list">
                            {snapshot.access_tokens.map((token) => {
                              const grant = draft.grants[token.id] ?? emptyGrants();
                              const denied = !token.enabled
                                || token.denied_connections.includes(selected.id)
                                || token.denied_tools.includes("datanexa_submit_write");
                              return (
                                <div className="write-token" key={token.id}>
                                  <span className="write-token-name">
                                    <KeyRound size={15} />
                                    <strong>{token.name}</strong>
                                  </span>
                                  <TokenGrants
                                    operations={OPERATIONS}
                                    labels={t.operationNames}
                                    onLabel={t.grantOn}
                                    offLabel={t.grantOff}
                                    grant={grant}
                                    tokenName={token.name}
                                    blocked={denied}
                                    onToggle={(operation, next) => updateDraft(selected.id, (current) => ({
                                      ...current,
                                      grants: {
                                        ...current.grants,
                                        [token.id]: { ...(current.grants[token.id] ?? emptyGrants()), [operation]: next }
                                      }
                                    }))}
                                  />
                                </div>
                              );
                            })}
                          </div>
                        )}
                    </section>

                    {draft.error && (
                      <div className="wc-notice wc-error" role="alert">
                        <strong>{t.saveFailed}</strong>
                        <p>{draft.error}</p>
                        <button type="button" className="write-inline-link" onClick={() => void saveDraft(selected.id)}>{t.retrySave}</button>
                      </div>
                    )}
                    {approvalError && (
                      <div className="wc-notice" role="status">{approvalError}</div>
                    )}
                    </div>
                  </div>
                )}
            </>
          ) : <div className="empty-state">{t.noConnections}</div>}
        </aside>
      </section>

      {selected && draft && (
        <TablePicker
          key={selected.id}
          locale={locale}
          connection={selected}
          selected={draft.tables}
          open={pickerOpen}
          onClose={() => setPickerOpen(false)}
          onChange={(target, present) => setTable(selected.id, target, present)}
        />
      )}
    </>
  );
}

/**
 * One control per operation: a glass bead that stretches into a labelled button
 * while the pointer is over the group. The same element performs both states, so
 * the width, height and corner radius interpolate into each other instead of one
 * control covering another. The dots are the resting state, which keeps a long
 * token list glanceable.
 */
function TokenGrants({ operations, labels, onLabel, offLabel, grant, tokenName, blocked, onToggle }: {
  operations: WriteOperation[];
  labels: Record<WriteOperation, string>;
  onLabel: string;
  offLabel: string;
  grant: Record<WriteOperation, boolean>;
  tokenName: string;
  blocked: boolean;
  onToggle: (operation: WriteOperation, next: boolean) => void;
}) {
  return (
    <div className="write-grants" role="group" aria-label={tokenName}>
      {operations.map((operation) => {
        const on = grant[operation];
        // Revoking always works; granting a new operation needs the token to
        // still be allowed this connection and the submit tool.
        const locked = blocked && !on;
        return (
          <button
            type="button"
            key={operation}
            className={clsx("write-grant", operation, on && "on")}
            aria-pressed={on}
            aria-label={`${labels[operation]} · ${on ? onLabel : offLabel}`}
            title={`${labels[operation]} · ${on ? onLabel : offLabel}`}
            disabled={locked}
            onClick={() => onToggle(operation, !on)}
          >
            <span className="write-grant-label">{labels[operation]}</span>
          </button>
        );
      })}
    </div>
  );
}

/**
 * Table picker. Tables arrive with their capability already resolved, so a row
 * only shows what the table supports — no per-table probe.
 */
function TablePicker({ locale, connection, selected, open, onClose, onChange }: {
  locale: Locale;
  connection: ConnectionConfig;
  selected: WriteTable[];
  open: boolean;
  onClose: () => void;
  onChange: (target: WriteTarget, present: boolean) => void;
}) {
  const t = writeMessages[locale];
  const [schemas, setSchemas] = useState<WriteSchema[]>([]);
  const [schema, setSchema] = useState("");
  const [targets, setTargets] = useState<WriteTarget[]>([]);
  const [search, setSearch] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [reload, setReload] = useState(0);
  const [truncated, setTruncated] = useState(false);
  const requestId = useRef(0);

  // A different connection must never keep showing the previous catalogue, even
  // for one frame: the schema and table state are cleared before the first read.
  useEffect(() => {
    setSchemas([]);
    setSchema("");
    setTargets([]);
    setSearch("");
    setError(null);
  }, [connection.id]);

  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    const id = ++requestId.current;
    setLoading(true);
    setError(null);
    void api.getWriteSchemas(connection.id)
      .then((result) => {
        if (cancelled || requestId.current !== id) return;
        setSchemas(result);
        setSchema((current) => current && result.some((entry) => entry.name === current)
          ? current
          : result[0]?.name ?? "");
      })
      .catch((cause) => {
        if (!cancelled && requestId.current === id) setError(writeErrorText(cause));
      })
      .finally(() => {
        if (!cancelled && requestId.current === id) setLoading(false);
      });
    return () => { cancelled = true; };
  }, [open, connection.id, reload]);

  // The schema belongs to the connection; switching connections restarts from
  // the first schema the new connection reports.
  useEffect(() => {
    setSchema((current) => (schemas.some((entry) => entry.name === current) ? current : schemas[0]?.name ?? ""));
  }, [schemas]);

  useEffect(() => {
    if (!open || !schema) return;
    let cancelled = false;
    const id = ++requestId.current;
    setLoading(true);
    setError(null);
    void api.getWriteTables(connection.id, schema)
      .then((result) => {
        if (cancelled || requestId.current !== id) return;
        setTargets(result);
        // The backend caps one listing; say so rather than pretending the list
        // is complete.
        setTruncated(result.length >= TABLE_LIST_LIMIT);
      })
      .catch((cause) => {
        if (!cancelled && requestId.current === id) setError(writeErrorText(cause));
      })
      .finally(() => {
        if (!cancelled && requestId.current === id) setLoading(false);
      });
    return () => { cancelled = true; };
  }, [open, connection.id, schema, reload]);

  const isSelected = (target: WriteTarget) =>
    selected.some((entry) => entry.schema === target.schema && entry.table === target.table);
  const visible = useMemo(() => {
    const term = search.trim().toLocaleLowerCase();
    if (!term) return targets;
    return targets.filter((target) => target.table.toLocaleLowerCase().includes(term));
  }, [targets, search]);

  return (
    <Dialog.Root open={open} onOpenChange={(next) => !next && onClose()}>
      <Dialog.Portal>
        <Dialog.Overlay className="dialog-overlay" />
        <Dialog.Content className="policy-dialog write-picker-dialog">
          <div className="dialog-titlebar">
            <div>
              <Dialog.Title>{t.pickerTitle}</Dialog.Title>
              <Dialog.Description>{`${connection.name} · ${schema || t.pickerSchema}`}</Dialog.Description>
            </div>
            <Dialog.Close asChild>
              <button type="button" className="icon-button" aria-label={t.pickerClose}><X size={18} /></button>
            </Dialog.Close>
          </div>

          <div className="write-picker-toolbar">
            <div className="write-picker-schemas">
              {schemas.map((entry) => (
                <button
                  type="button"
                  key={entry.name}
                  className={clsx("write-schema-chip", entry.name === schema && "active")}
                  onClick={() => setSchema(entry.name)}
                >
                  <Database size={14} />{entry.name}
                </button>
              ))}
            </div>
            <div className="write-picker-search">
              <Search size={15} />
              <input aria-label={t.pickerSearch} placeholder={t.pickerSearch} value={search} onChange={(event) => setSearch(event.target.value)} />
              <IconTooltip label={t.reload}>
                <button type="button" className={clsx("icon-button", loading && "is-spinning")} disabled={loading} onClick={() => setReload((value) => value + 1)} aria-label={t.reload}>
                  <RefreshCw size={15} />
                </button>
              </IconTooltip>
            </div>
          </div>

          <div className="write-picker-list page-scroll-list">
            {error && <div className="wc-notice wc-error" role="alert">{error}</div>}
            {!error && loading && targets.length === 0 && <p className="field-note">{t.pickerLoading}</p>}
            {!error && !loading && visible.length === 0 && <p className="field-note">{schemas.length === 0 ? t.pickerNoSchemas : t.pickerEmpty}</p>}
            {visible.map((target) => {
              const checked = isSelected(target);
              const writable = target.operations.length > 0;
              return (
                <button
                  type="button"
                  key={`${target.schema}.${target.table}`}
                  className={clsx("write-picker-row", checked && "selected", !writable && "disabled")}
                  disabled={!writable && !checked}
                  aria-pressed={checked}
                  onClick={() => onChange(target, !checked)}
                >
                  <span className={clsx("write-picker-check", checked && "checked")} aria-hidden="true">
                    {checked && <Check size={12} />}
                  </span>
                  <code>{target.table}</code>
                  <span className="write-picker-ops">
                    {writable
                      ? target.operations.map((operation) => (
                        <span key={operation} className={clsx("write-op-pill", operation)}>{t.operationNames[operation]}</span>
                      ))
                      : (
                        <IconTooltip label={writeReasonDetail(locale, target)}>
                          <span className="write-unwritable" title={writeReasonDetail(locale, target)}>
                            <StatusPill tone="slate" label={t.notWritable} />
                          </span>
                        </IconTooltip>
                      )}
                  </span>
                </button>
              );
            })}
            {truncated && !error && <p className="field-note">{formatMessage(t.pickerTruncated, { count: TABLE_LIST_LIMIT })}</p>}
          </div>

          <footer className="write-picker-footer">
            <span>{`${t.pickerSelected} ${selected.filter((entry) => entry.schema === schema).length}`}</span>
            <button type="button" className="button primary" onClick={onClose}>{t.pickerClose}</button>
          </footer>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
