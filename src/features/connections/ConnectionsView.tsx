import * as Dialog from "@radix-ui/react-dialog";
import clsx from "clsx";
import { Cable, Database, MoreVertical, Power, SearchCheck, Trash2, X } from "lucide-react";
import { useState } from "react";
import type { FormEvent } from "react";
import { formatMessage, type I18nMessages } from "../../i18n";
import type { ConnectionConfig, DatabaseType, JdbcDriverBundle } from "../../types";
import { DATABASE_LOGOS } from "../../app/assets";
import { defaultPort } from "../../app/utils";
import { Field, FormSection, IconTooltip, PanelHeader, StatusPill } from "../../components/ui";

export function ConnectionsView({
  t,
  connections,
  busy,
  onEdit,
  onDelete,
  onTest,
  onDiagnose,
  onToggleEnabled,
  migrationReady
}: {
  t: I18nMessages;
  connections: ConnectionConfig[];
  busy: boolean;
  onEdit: (connection: ConnectionConfig) => void;
  onDelete: (id: string) => void;
  onTest: (id: string) => void;
  onDiagnose: (id: string) => void;
  onToggleEnabled: (id: string, enabled: boolean) => void;
  migrationReady: boolean;
}) {
  const [selectedId, setSelectedId] = useState(connections[0]?.id ?? "");
  const selected = connections.find((connection) => connection.id === selectedId) ?? connections[0];

  return (
    <section className="connections-workbench">
      <div className="panel connection-browser">
        <div className="data-list-header">
          <span>{t.connections.title}</span>
          <span>{connections.length}</span>
        </div>
        <div className="connection-list page-scroll-list">
        {connections.length === 0 ? (
          <div className="empty-state">{t.connections.empty}</div>
        ) : (
          connections.map((connection) => (
            <ConnectionRow
              t={t}
              key={connection.id}
              connection={connection}
              busy={busy}
              onEdit={onEdit}
              onDelete={onDelete}
              onTest={onTest}
              onDiagnose={onDiagnose}
              onToggleEnabled={onToggleEnabled}
              migrationReady={migrationReady}
              selected={selected?.id === connection.id}
              onSelect={() => setSelectedId(connection.id)}
            />
          ))
        )}
        </div>
      </div>
      <aside className="panel connection-inspector">
        {selected ? (
          <>
            <div className="inspector-heading">
              <ConnectionListItem t={t} connection={selected} showJdbcBadge={false} />
              <button type="button" className="button ghost" disabled={busy} onClick={() => onEdit(selected)}>{t.connections.edit}</button>
            </div>
            <dl className="inspector-grid">
              {selected.type === "jdbc" ? (
                <div className="inspector-grid-wide"><dt>{t.connectionDialog.jdbcUrl}</dt><dd><code title={selected.jdbc_url ?? undefined}>{selected.jdbc_url || "-"}</code></dd></div>
              ) : (
                <>
                  <div><dt>{t.connectionDialog.host}</dt><dd><code>{selected.type === "sqlite" ? "LOCAL" : selected.host || "-"}</code></dd></div>
                  <div><dt>{t.connectionDialog.port}</dt><dd><code>{selected.type === "sqlite" ? "-" : selected.port ?? defaultPort(selected.type)}</code></dd></div>
                  <div><dt>{t.connectionDialog.database}</dt><dd><code>{selected.database || "-"}</code></dd></div>
                </>
              )}
              <div><dt>{t.connectionDialog.username}</dt><dd><code>{selected.username || "-"}</code></dd></div>
              <div><dt>{t.connectionDialog.sslMode}</dt><dd>{selected.ssl_mode || "-"}</dd></div>
              <div><dt>{t.connectionDialog.maxRows}</dt><dd>{selected.max_rows}</dd></div>
              <div><dt>{t.connectionDialog.queryTimeoutMs}</dt><dd>{selected.query_timeout_ms} ms</dd></div>
              <div><dt>{t.connectionDialog.maxConnections}</dt><dd>{selected.max_connections}</dd></div>
              <div className={selected.type === "jdbc" ? undefined : "inspector-grid-wide"}><dt>{t.connectionDialog.maxResultBytes}</dt><dd>{Math.round(selected.max_result_bytes / 1024)} KiB</dd></div>
            </dl>
            <div className="inspector-actions">
              <button type="button" className="button soft" disabled={busy || !selected.enabled || !migrationReady} onClick={() => onTest(selected.id)}><Cable size={15} />{t.connections.test}</button>
              <button type="button" className="button ghost" disabled={busy || !selected.enabled || selected.type === "jdbc"} onClick={() => onDiagnose(selected.id)}><SearchCheck size={15} />{t.connections.diagnose}</button>
            </div>
          </>
        ) : <div className="empty-state">{t.connections.empty}</div>}
      </aside>
    </section>
  );
}

export function ConnectionDialog({
  t,
  editing,
  busy,
  password,
  clearPassword,
  onPasswordChange,
  onClearPasswordChange,
  onEditingChange,
  onTest,
  migrationReady,
  jdbcDrivers,
  onSubmit,
  onClose
}: {
  t: I18nMessages;
  editing: ConnectionConfig | null;
  busy: boolean;
  password: string;
  clearPassword: boolean;
  onPasswordChange: (value: string) => void;
  onClearPasswordChange: (checked: boolean) => void;
  onEditingChange: (connection: ConnectionConfig) => void;
  onTest: () => void;
  migrationReady: boolean;
  jdbcDrivers: JdbcDriverBundle[];
  onSubmit: (event: FormEvent<HTMLFormElement>) => void;
  onClose: () => void;
}) {
  if (!editing) return null;

  return (
    <Dialog.Root open onOpenChange={(open) => !open && onClose()}>
      <Dialog.Portal>
        <Dialog.Overlay className="dialog-overlay" />
        <Dialog.Content className="connection-dialog">
          <div className="dialog-titlebar">
            <div>
              <Dialog.Title>{editing.id.startsWith("connection_") ? t.connectionDialog.addTitle : t.connectionDialog.editTitle}</Dialog.Title>
              <Dialog.Description>{t.connectionDialog.description}</Dialog.Description>
            </div>
            <Dialog.Close asChild>
              <button type="button" className="icon-button" aria-label={t.common.close}>
                <X size={18} />
              </button>
            </Dialog.Close>
          </div>

          <form className="connection-form" onSubmit={onSubmit}>
            <div className="connection-form-scroll">
            <FormSection title={t.connectionDialog.basicInfo}>
              <Field label={t.connectionDialog.name} span>
                <input value={editing.name} onChange={(event) => onEditingChange({ ...editing, name: event.target.value })} required />
              </Field>
              <Field label={t.connectionDialog.databaseType}>
                <select
                  value={editing.type}
                  onChange={(event) => {
                    const type = event.target.value as DatabaseType;
                    onEditingChange({
                      ...editing,
                      type,
                      database: type === "jdbc" ? "" : editing.database,
                      host: type === "jdbc" ? null : editing.host,
                      port: defaultPort(type),
                      ssl_mode: type === "sqlite" || type === "jdbc" ? null : editing.ssl_mode ?? "prefer",
                      jdbc_bundle_id: type === "jdbc" ? editing.jdbc_bundle_id ?? jdbcDrivers[0]?.bundle_id ?? null : null,
                      jdbc_url: type === "jdbc" ? editing.jdbc_url ?? "" : null,
                      jdbc_driver_class: type === "jdbc" ? editing.jdbc_driver_class ?? null : null
                    });
                  }}
                >
                  <option value="sqlite">SQLite</option>
                  <option value="mysql">MySQL</option>
                  <option value="postgres">PostgreSQL</option>
                  <option value="jdbc">JDBC</option>
                </select>
              </Field>
            </FormSection>

            <FormSection title={t.connectionDialog.address}>
              {editing.type === "sqlite" ? (
                <Field label={t.connectionDialog.databaseFile} span>
                  <input value={editing.database} onChange={(event) => onEditingChange({ ...editing, database: event.target.value })} placeholder="E:/data/app.db" required />
                </Field>
              ) : editing.type === "jdbc" ? (
                <>
                  <Field label={t.connectionDialog.jdbcDriver} span>
                    <select
                      value={editing.jdbc_bundle_id ?? ""}
                      onChange={(event) => onEditingChange({ ...editing, jdbc_bundle_id: event.target.value || null })}
                      required
                    >
                      <option value="">{t.connectionDialog.jdbcDriverPlaceholder}</option>
                      {jdbcDrivers.map((driver) => (
                        <option key={driver.bundle_id} value={driver.bundle_id}>{driver.display_name} · {driver.source === "local" ? t.settings.localDriver : driver.maven_coordinate}</option>
                      ))}
                    </select>
                  </Field>
                  <Field label={t.connectionDialog.jdbcUrl} span>
                    <input
                      value={editing.jdbc_url ?? ""}
                      onChange={(event) => onEditingChange({ ...editing, jdbc_url: event.target.value })}
                      placeholder="jdbc:vendor:..."
                      autoComplete="off"
                      required
                    />
                  </Field>
                  <Field label={t.connectionDialog.username}>
                    <input value={editing.username ?? ""} onChange={(event) => onEditingChange({ ...editing, username: event.target.value })} />
                  </Field>
                  <Field label={t.connectionDialog.jdbcDriverClass}>
                    <input
                      value={editing.jdbc_driver_class ?? ""}
                      onChange={(event) => onEditingChange({ ...editing, jdbc_driver_class: event.target.value || null })}
                      placeholder={t.connectionDialog.jdbcDriverClassPlaceholder}
                    />
                  </Field>
                  <p className="field-note span-all">{t.connectionDialog.jdbcPreviewNotice}</p>
                </>
              ) : (
                <>
                  <Field label={t.connectionDialog.host}>
                    <input value={editing.host ?? ""} onChange={(event) => onEditingChange({ ...editing, host: event.target.value })} required />
                  </Field>
                  <Field label={t.connectionDialog.port}>
                    <input type="number" value={editing.port ?? defaultPort(editing.type) ?? ""} onChange={(event) => onEditingChange({ ...editing, port: Number(event.target.value) })} required />
                  </Field>
                  <Field label={t.connectionDialog.database}>
                    <input value={editing.database} onChange={(event) => onEditingChange({ ...editing, database: event.target.value })} required />
                  </Field>
                  <Field label={t.connectionDialog.username}>
                    <input value={editing.username ?? ""} onChange={(event) => onEditingChange({ ...editing, username: event.target.value })} />
                  </Field>
                  <Field label={t.connectionDialog.sslMode} span>
                    <select value={editing.ssl_mode ?? "prefer"} onChange={(event) => onEditingChange({ ...editing, ssl_mode: event.target.value })}>
                      <option value="disable">{t.connectionDialog.sslDisable}</option>
                      <option value="prefer">{t.connectionDialog.sslPrefer}</option>
                      <option value="require">{t.connectionDialog.sslRequire}</option>
                    </select>
                  </Field>
                </>
              )}
            </FormSection>

            <FormSection title={t.connectionDialog.credentialsAndLimits}>
              <Field label={t.connectionDialog.password}>
                <input
                  type="password"
                  value={password}
                  onChange={(event) => onPasswordChange(event.target.value)}
                  disabled={clearPassword}
                  placeholder={editing.credential_ref ? t.connectionDialog.keepExistingPassword : t.connectionDialog.saveToVault}
                />
              </Field>
              <div className="credential-action-field">
                <button
                  type="button"
                  className={clsx("button ghost credential-clear-button", clearPassword && "pending")}
                  disabled={!editing.credential_ref || busy || clearPassword}
                  onClick={() => onClearPasswordChange(true)}
                >
                  <Trash2 size={15} />
                  {t.connectionDialog.clearSavedCredential}
                </button>
              </div>
              <Field label={t.connectionDialog.maxRows}>
                <input type="number" min={1} max={5000} value={editing.max_rows} onChange={(event) => onEditingChange({ ...editing, max_rows: Number(event.target.value) })} />
              </Field>
              <Field label={t.connectionDialog.queryTimeoutMs}>
                <input type="number" min={500} value={editing.query_timeout_ms} onChange={(event) => onEditingChange({ ...editing, query_timeout_ms: Number(event.target.value) })} />
              </Field>
              <Field label={t.connectionDialog.maxConnections}>
                <input type="number" min={1} max={3} value={editing.max_connections} onChange={(event) => onEditingChange({ ...editing, max_connections: Number(event.target.value) })} />
              </Field>
              <Field label={t.connectionDialog.maxResultBytes}>
                <input type="number" min={64} max={8192} step={64} value={Math.round(editing.max_result_bytes / 1024)} onChange={(event) => onEditingChange({ ...editing, max_result_bytes: Number(event.target.value) * 1024 })} />
              </Field>
              <p className="field-note span-all">
                {formatMessage(t.connectionDialog.currentCredential, { credential: editing.credential_ref ?? t.connectionDialog.credentialNotSaved })}
              </p>
            </FormSection>
            </div>

            <footer>
              <button type="button" className="button soft" disabled={busy || !migrationReady || (editing.type === "jdbc" && !editing.jdbc_bundle_id)} onClick={onTest}>
                <Cable size={16} />
                {t.connections.test}
              </button>
              <Dialog.Close asChild>
                <button type="button" className="button ghost">{t.common.cancel}</button>
              </Dialog.Close>
              <button type="submit" className="button primary" disabled={busy || (editing.type === "jdbc" && !editing.jdbc_bundle_id)}>{t.connectionDialog.save}</button>
            </footer>
          </form>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

function ConnectionRow({
  t,
  connection,
  busy,
  onEdit,
  onDelete,
  onTest,
  onDiagnose,
  onToggleEnabled,
  migrationReady,
  selected,
  onSelect
}: {
  t: I18nMessages;
  connection: ConnectionConfig;
  busy: boolean;
  onEdit: (connection: ConnectionConfig) => void;
  onDelete: (id: string) => void;
  onTest: (id: string) => void;
  onDiagnose: (id: string) => void;
  onToggleEnabled: (id: string, enabled: boolean) => void;
  migrationReady: boolean;
  selected?: boolean;
  onSelect?: () => void;
}) {
  return (
    <div className={clsx("connection-row", !connection.enabled && "disabled", selected && "selected")}>
      <button type="button" className="connection-select" onClick={onSelect}>
        <ConnectionListItem t={t} connection={connection} />
      </button>
      <div className="row-actions">
        <IconTooltip label={formatMessage(t.connections.toggleEnabled, { name: connection.name })}>
          <button
            type="button"
            className={clsx("icon-button connection-toggle-button", !connection.enabled && "off")}
            onClick={() => onToggleEnabled(connection.id, !connection.enabled)}
            disabled={busy}
            aria-label={formatMessage(t.connections.toggleEnabled, { name: connection.name })}
          >
            <Power size={17} />
          </button>
        </IconTooltip>
        <IconTooltip label={t.connections.test}>
          <button type="button" className="icon-button" onClick={() => onTest(connection.id)} disabled={busy || !connection.enabled || !migrationReady}>
            <Cable size={17} />
          </button>
        </IconTooltip>
        <IconTooltip label={t.connections.diagnose}>
          <button type="button" className="icon-button" onClick={() => onDiagnose(connection.id)} disabled={busy || !connection.enabled || connection.type === "jdbc"}>
            <SearchCheck size={17} />
          </button>
        </IconTooltip>
        <IconTooltip label={t.connections.edit}>
          <button type="button" className="icon-button" onClick={() => onEdit(connection)} disabled={busy}>
            <MoreVertical size={17} />
          </button>
        </IconTooltip>
        <IconTooltip label={t.connections.delete}>
          <button type="button" className="icon-button danger" onClick={() => onDelete(connection.id)} disabled={busy}>
            <Trash2 size={17} />
          </button>
        </IconTooltip>
      </div>
    </div>
  );
}

export function ConnectionListItem({ t, connection, compact = false, showJdbcBadge = true }: { t: I18nMessages; connection: ConnectionConfig; compact?: boolean; showJdbcBadge?: boolean }) {
  return (
    <div className={clsx("connection-item", compact && "compact")}>
      <div className={clsx("db-badge", connection.type)}>
        {DATABASE_LOGOS[connection.type]
          ? <img src={DATABASE_LOGOS[connection.type]} alt="" aria-hidden="true" />
          : <Database size={24} aria-hidden="true" />}
      </div>
      <div className="connection-info">
        <div>
          <strong>{connection.name}</strong>
          {showJdbcBadge && connection.type === "jdbc" && <StatusPill tone="blue" label="JDBC" />}
          <StatusPill tone={connection.enabled ? "green" : "slate"} label={connection.enabled ? t.connections.enabled : t.connections.paused} />
        </div>
        <p title={connection.type === "jdbc" ? connection.jdbc_url ?? undefined : undefined}>
          {connection.type === "sqlite"
            ? connection.database || t.connections.noDatabaseFile
            : connection.type === "jdbc"
              ? connection.jdbc_url || "-"
              : `${connection.host || "-"}:${connection.port ?? defaultPort(connection.type)}`}
        </p>
      </div>
    </div>
  );
}
