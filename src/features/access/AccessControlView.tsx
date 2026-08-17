import * as Dialog from "@radix-ui/react-dialog";
import * as Switch from "@radix-ui/react-switch";
import clsx from "clsx";
import { ChevronDown, Clipboard, KeyRound, Pencil, Power, RefreshCw, ShieldAlert, Trash2, X } from "lucide-react";
import { useEffect, useState, type FormEvent } from "react";
import type { I18nMessages } from "../../i18n";
import type { AccessTokenInfo, ConnectionConfig, McpToolInfo } from "../../types";
import { DATABASE_LOGOS } from "../../app/assets";
import { toolDisplayName } from "../../app/utils";
import { IconTooltip, StatusPill } from "../../components/ui";

type PermissionTab = "connections" | "tools";
type TokenDialogMode = "create" | "rename" | "rotate" | "delete" | null;

export function AccessControlView({
  t, tokens, connections, tools, requireToken, busy, selectedId, onSelectedIdChange,
  createRequest, onCreateRequestHandled,
  onEnableAuthentication, onCreate, onRename, onToggle, onRotate, onDelete,
  onCopyPrompt, onConnectionAllowed, onToolAllowed
}: {
  t: I18nMessages;
  tokens: AccessTokenInfo[];
  connections: ConnectionConfig[];
  tools: McpToolInfo[];
  requireToken: boolean;
  busy: boolean;
  createRequest: number;
  onCreateRequestHandled: () => void;
  selectedId: string;
  onSelectedIdChange: (id: string) => void;
  onEnableAuthentication: () => void;
  onCreate: (name: string) => Promise<boolean>;
  onRename: (id: string, name: string) => Promise<boolean>;
  onToggle: (id: string, enabled: boolean) => void;
  onRotate: (id: string) => Promise<boolean>;
  onDelete: (id: string) => Promise<boolean>;
  onCopyPrompt: (id: string) => void;
  onConnectionAllowed: (tokenId: string, connectionId: string, allowed: boolean) => void;
  onToolAllowed: (tokenId: string, toolName: string, allowed: boolean) => void;
}) {
  const [tab, setTab] = useState<PermissionTab>("connections");
  const [dialog, setDialog] = useState<TokenDialogMode>(null);
  const [name, setName] = useState("");
  const selected = tokens.find((token) => token.id === selectedId) ?? tokens[0];

  useEffect(() => {
    if (selected && selected.id !== selectedId) onSelectedIdChange(selected.id);
  }, [selected, selectedId, onSelectedIdChange]);

  useEffect(() => {
    if (createRequest > 0) {
      setName("");
      setDialog("create");
      onCreateRequestHandled();
    }
  }, [createRequest, onCreateRequestHandled]);

  const submitName = async (event: FormEvent) => {
    event.preventDefault();
    const succeeded = dialog === "create"
      ? await onCreate(name)
      : Boolean(selected && await onRename(selected.id, name));
    if (succeeded) setDialog(null);
  };

  if (!requireToken) {
    return (
      <section className="panel access-prerequisite">
        <span className="access-prerequisite-icon"><ShieldAlert size={28} /></span>
        <h2>{t.access.authenticationRequired}</h2>
        <p>{t.access.authenticationRequiredText}</p>
        <button type="button" className="button primary" onClick={onEnableAuthentication} disabled={busy}><KeyRound size={16} />{t.access.enableAuthentication}</button>
      </section>
    );
  }

  return (
    <>
      <section className="access-workbench">
        <div className="panel access-browser">
          <div className="data-list-header"><span>{t.access.tokens}</span><span>{tokens.length}</span></div>
          <div className="access-token-list page-scroll-list">
            {tokens.length === 0 ? <div className="empty-state">{t.access.empty}</div> : tokens.map((token) => (
              <div className={clsx("access-token-row", token.id === selected?.id && "selected", !token.enabled && "disabled")} key={token.id}>
                <button type="button" className="access-token-select" onClick={() => onSelectedIdChange(token.id)}>
                  <span className="token-glyph"><KeyRound size={17} /></span>
                  <span className="access-token-copy"><span><strong>{token.name}</strong><StatusPill tone={token.enabled ? "green" : "slate"} label={token.enabled ? t.access.enabled : t.access.disabled} /></span><code>{shortId(token.id)} · {token.last_used_at ? new Date(token.last_used_at).toLocaleString() : t.access.neverUsed}</code></span>
                </button>
                <div className="row-actions">
                  <IconTooltip label={token.enabled ? t.access.disable : t.access.enable}><button type="button" className={clsx("icon-button connection-toggle-button", !token.enabled && "off")} onClick={() => onToggle(token.id, !token.enabled)} disabled={busy}><Power size={16} /></button></IconTooltip>
                  <IconTooltip label={t.access.rename}><button type="button" className="icon-button" onClick={() => { onSelectedIdChange(token.id); setName(token.name); setDialog("rename"); }} disabled={busy}><Pencil size={16} /></button></IconTooltip>
                  <IconTooltip label={t.access.rotate}><button type="button" className="icon-button" onClick={() => { onSelectedIdChange(token.id); setDialog("rotate"); }} disabled={busy}><RefreshCw size={16} /></button></IconTooltip>
                  <IconTooltip label={t.access.delete}><button type="button" className="icon-button danger" onClick={() => { onSelectedIdChange(token.id); setDialog("delete"); }} disabled={busy}><Trash2 size={16} /></button></IconTooltip>
                </div>
              </div>
            ))}
          </div>
        </div>

        <aside className="panel access-inspector">
          {selected ? (
            <>
              <div className="access-inspector-heading">
                <div className="access-token-identity"><span className="token-glyph large"><KeyRound size={20} /></span><div><span><strong>{selected.name}</strong><StatusPill tone={selected.enabled ? "green" : "slate"} label={selected.enabled ? t.access.enabled : t.access.disabled} /></span><code>{shortId(selected.id)}</code></div></div>
                <div className="access-primary-actions">
                  <IconTooltip label={t.access.copyPrompt}><button type="button" className="icon-button" onClick={() => onCopyPrompt(selected.id)} disabled={busy || !selected.enabled} aria-label={t.access.copyPrompt}><Clipboard size={16} /></button></IconTooltip>
                </div>
              </div>

              <dl className="access-token-meta">
                <div><dt>{t.access.createdAt}</dt><dd>{new Date(selected.created_at).toLocaleString()}</dd></div>
                <div><dt>{t.access.lastUsed}</dt><dd>{selected.last_used_at ? new Date(selected.last_used_at).toLocaleString() : t.access.neverUsed}</dd></div>
              </dl>

              <div className="access-permission-accordion">
                <PermissionSectionButton active={tab === "connections"} label={t.access.databasePermissions} count={`${allowedConnections(selected, connections)} / ${connections.filter((item) => item.enabled).length}`} onClick={() => setTab("connections")} />
                {tab === "connections" && <div className="access-permission-list page-scroll-list">{connections.map((connection) => {
                  const allowed = connection.enabled && !selected.denied_connections.includes(connection.id);
                  return <PermissionRow key={connection.id} icon={<img src={DATABASE_LOGOS[connection.type]} alt="" />} title={connection.name} detail={connection.enabled ? connection.database : t.access.globallyDisabled} checked={allowed} disabled={busy || !connection.enabled} label={t.access.allowConnection} onChange={(checked) => onConnectionAllowed(selected.id, connection.id, checked)} />;
                })}</div>}
                <PermissionSectionButton active={tab === "tools"} label={t.access.toolPermissions} count={`${allowedTools(selected, tools)} / ${tools.filter((item) => item.enabled).length}`} onClick={() => setTab("tools")} />
                {tab === "tools" && <div className="access-permission-list page-scroll-list">{tools.map((tool) => {
                  const allowed = tool.enabled && !selected.denied_tools.includes(tool.name);
                  return <PermissionRow key={tool.name} icon={<KeyRound size={16} />} title={toolDisplayName(t, tool.name)} detail={tool.enabled ? tool.name : t.access.globallyDisabled} checked={allowed} disabled={busy || !tool.enabled} label={t.access.allowTool} onChange={(checked) => onToolAllowed(selected.id, tool.name, checked)} />;
                })}</div>}
              </div>
            </>
          ) : <div className="empty-state access-empty-inspector">{t.access.empty}</div>}
        </aside>
      </section>

      <TokenDialog t={t} mode={dialog} name={name} busy={busy} onNameChange={setName} onClose={() => setDialog(null)} onSubmit={submitName} onConfirm={async () => {
        if (!selected) return;
        const succeeded = dialog === "rotate"
          ? await onRotate(selected.id)
          : dialog === "delete" && await onDelete(selected.id);
        if (succeeded) setDialog(null);
      }} />
    </>
  );
}

export function PromptTokenDialog({ t, open, tokens, connections, tools, busy, onClose, onSelect }: { t: I18nMessages; open: boolean; tokens: AccessTokenInfo[]; connections: ConnectionConfig[]; tools: McpToolInfo[]; busy: boolean; onClose: () => void; onSelect: (id: string) => void }) {
  return <Dialog.Root open={open} onOpenChange={(next) => !next && onClose()}><Dialog.Portal><Dialog.Overlay className="dialog-overlay" /><Dialog.Content className="policy-dialog prompt-token-dialog"><div className="dialog-titlebar"><div><Dialog.Title>{t.access.chooseToken}</Dialog.Title><Dialog.Description>{t.access.chooseTokenText}</Dialog.Description></div><Dialog.Close asChild><button type="button" className="icon-button" aria-label={t.common.close}><X size={18} /></button></Dialog.Close></div><div className="prompt-token-list">{tokens.filter((token) => token.enabled).map((token) => <button type="button" key={token.id} disabled={busy} onClick={() => onSelect(token.id)}><span className="token-glyph"><KeyRound size={17} /></span><span><strong>{token.name}</strong><code>{shortId(token.id)} · {t.access.allowedSummary.replace("{connections}", String(allowedConnections(token, connections))).replace("{tools}", String(allowedTools(token, tools)))} · {token.last_used_at ? new Date(token.last_used_at).toLocaleString() : t.access.neverUsed}</code></span></button>)}</div></Dialog.Content></Dialog.Portal></Dialog.Root>;
}

function PermissionRow({ icon, title, detail, checked, disabled, label, onChange }: { icon: React.ReactNode; title: string; detail: string; checked: boolean; disabled: boolean; label: string; onChange: (checked: boolean) => void }) {
  return <div className={clsx("access-permission-row", disabled && "disabled")}><span className="permission-icon">{icon}</span><span className="permission-copy"><strong>{title}</strong><code>{detail}</code></span><Switch.Root className="switch" checked={checked} disabled={disabled} onCheckedChange={onChange} aria-label={`${label}: ${title}`}><Switch.Thumb className="switch-thumb" /></Switch.Root></div>;
}

function PermissionSectionButton({ active, label, count, onClick }: { active: boolean; label: string; count: string; onClick: () => void }) {
  return <button type="button" className={clsx("access-permission-section-button", active && "active")} onClick={onClick} aria-expanded={active}><span>{label}</span><span><small>{count}</small><ChevronDown size={15} /></span></button>;
}

function TokenDialog({ t, mode, name, busy, onNameChange, onClose, onSubmit, onConfirm }: { t: I18nMessages; mode: TokenDialogMode; name: string; busy: boolean; onNameChange: (value: string) => void; onClose: () => void; onSubmit: (event: FormEvent) => void; onConfirm: () => void }) {
  if (!mode) return null;
  const nameMode = mode === "create" || mode === "rename";
  return <Dialog.Root open onOpenChange={(open) => !open && onClose()}><Dialog.Portal><Dialog.Overlay className="dialog-overlay" /><Dialog.Content className={clsx("policy-dialog access-dialog", !nameMode && "compact")}><div className="dialog-titlebar"><div><Dialog.Title>{t.access.dialogTitles[mode]}</Dialog.Title><Dialog.Description>{t.access.dialogDescriptions[mode]}</Dialog.Description></div><Dialog.Close asChild><button type="button" className="icon-button" aria-label={t.common.close}><X size={18} /></button></Dialog.Close></div>{nameMode ? <form onSubmit={onSubmit}><label className="field"><span>{t.access.tokenName}</span><input autoFocus maxLength={80} value={name} onChange={(event) => onNameChange(event.target.value)} required /></label><footer><button type="button" className="button ghost" onClick={onClose}>{t.common.cancel}</button><button type="submit" className="button primary" disabled={busy || !name.trim()}>{mode === "create" ? t.access.create : t.access.save}</button></footer></form> : <footer><button type="button" className="button ghost" onClick={onClose}>{t.common.cancel}</button><button type="button" className={clsx("button", mode === "delete" ? "danger-solid" : "primary")} disabled={busy} onClick={onConfirm}>{mode === "delete" ? t.access.confirmDelete : t.access.confirmRotate}</button></footer>}</Dialog.Content></Dialog.Portal></Dialog.Root>;
}

function shortId(id: string) { return id.replace(/-/g, "").slice(0, 8).toUpperCase(); }
function allowedConnections(token: AccessTokenInfo, connections: ConnectionConfig[]) { return connections.filter((item) => item.enabled && !token.denied_connections.includes(item.id)).length; }
function allowedTools(token: AccessTokenInfo, tools: McpToolInfo[]) { return tools.filter((item) => item.enabled && !token.denied_tools.includes(item.name)).length; }
