import clsx from "clsx";
import { AlertTriangle, Clipboard, EyeOff, KeyRound, Play, RefreshCw, Server, Square } from "lucide-react";
import type { I18nMessages } from "../../i18n";
import type { AppSnapshot } from "../../types";
import { PanelHeader, PanelIconAction, StatusPill } from "../../components/ui";

export function ServerView({
  t,
  snapshot,
  busy,
  endpoint,
  onCopyAgentPrompt,
  onToggle,
  onRotate,
  startDisabled
}: {
  t: I18nMessages;
  snapshot: AppSnapshot;
  busy: boolean;
  endpoint: string;
  onCopyAgentPrompt: () => void;
  onToggle: () => void;
  onRotate: () => void;
  startDisabled: boolean;
}) {
  const requireToken = snapshot.config.server.require_token;
  const startupFailed = Boolean(snapshot.startup_error);
  const statusLabel = startupFailed ? t.overview.failed : snapshot.server_status.running ? t.overview.running : t.overview.stopped;

  return (
    <section className={clsx("server-console", startupFailed ? "error" : snapshot.server_status.running && "running")} title={snapshot.startup_error ?? undefined}>
      <div className="server-hero">
        <div className="server-identity">
          <span className="server-emblem">{startupFailed ? <AlertTriangle size={25} /> : <Server size={25} />}</span>
          <div><span className="panel-kicker">{t.overview.metricServer}</span><h2>{statusLabel}</h2></div>
        </div>
        <button type="button" className={clsx("button", snapshot.server_status.running ? "stop" : "primary")} onClick={onToggle} disabled={busy || startDisabled}>
          {snapshot.server_status.running ? <Square size={16} /> : <Play size={17} />}
          {snapshot.server_status.running ? t.server.stop : t.server.start}
        </button>
      </div>

      <div className="server-console-grid">
        <div className="server-console-section endpoint-section">
          <PanelHeader
            title={t.server.endpoint}
            action={<StatusPill tone={startupFailed ? "red" : snapshot.server_status.running ? "green" : "slate"} label={statusLabel} />}
          />
          <div className="console-value">
            <code>{endpoint}</code>
            <button type="button" className="icon-button" onClick={() => navigator.clipboard.writeText(endpoint)} aria-label={t.server.copyEndpoint}><Clipboard size={16} /></button>
          </div>
        </div>

      {requireToken ? (
        <div className="server-console-section token-section">
          <PanelHeader
            title={t.server.accessToken}
            action={<PanelIconAction icon={<RefreshCw size={16} />} label={t.server.rotateToken} onClick={onRotate} disabled={busy} />}
          />
          <div className="token-row console-value">
            <code>{snapshot.server_status.token ? "•••• •••• •••• •••• ••••" : t.server.generatedOnStart}</code>
            <button type="button" className="icon-button" onClick={() => navigator.clipboard.writeText(snapshot.server_status.token ?? "")} aria-label={t.server.copyToken}>
              <Clipboard size={16} />
            </button>
          </div>
        </div>
      ) : (
        <div className="server-console-section key-disabled-panel">
          <div className="key-disabled-icon">
            <EyeOff size={20} />
          </div>
          <h2>{t.server.tokenDisabledTitle}</h2>
          <p className="muted">{t.server.tokenDisabledText}</p>
        </div>
      )}

        <div className="server-console-section agent-copy-panel">
        <h2>{t.server.agentAccess}</h2>
        <p className="muted">{t.overview.quickAgentText}</p>
        <button type="button" className="button soft" onClick={onCopyAgentPrompt}>
          <Clipboard size={16} />
          {t.server.copyToAgent}
        </button>
      </div>
      </div>
    </section>
  );
}

