import clsx from "clsx";
import { Activity, AlertTriangle, ChevronRight, Play, Plus, Power, Square } from "lucide-react";
import quickStep1Url from "../../../resources/quickguide/step1.png";
import quickStep2Url from "../../../resources/quickguide/step2.png";
import quickStep3Url from "../../../resources/quickguide/step3.png";
import type { I18nMessages } from "../../i18n";
import type { AppSnapshot, AuditEvent } from "../../types";
import { EventList, IconTooltip, PanelHeader, PanelIconAction, QuickStep } from "../../components/ui";
import { ConnectionListItem } from "../connections/ConnectionsView";

export function OverviewView({
  t,
  snapshot,
  enabledConnections,
  recentEvents,
  onAdd,
  onOpenConnections,
  onOpenAudit,
  onSelectAudit,
  onCopyAgentPrompt,
  onToggleServer,
  onToggleEmergency,
  busy,
  startDisabled,
  mcpActivitySequence,
  mcpActivityTone
}: {
  t: I18nMessages;
  snapshot: AppSnapshot;
  enabledConnections: number;
  recentEvents: AuditEvent[];
  onAdd: () => void;
  onOpenConnections: () => void;
  onOpenAudit: () => void;
  onSelectAudit: (event: AuditEvent) => void;
  onCopyAgentPrompt: () => void;
  onToggleServer: () => void;
  onToggleEmergency: () => void;
  busy: boolean;
  startDisabled: boolean;
  mcpActivitySequence: number;
  mcpActivityTone: "success" | "error";
}) {
  const totalConnections = snapshot.config.connections.length;
  const enabledTools = snapshot.tools.filter((tool) => tool.enabled).length;
  const callsCutoff = Date.now() - 24 * 60 * 60 * 1000;
  const callsLast24Hours = snapshot.audit_events.filter((event) => (
    event.tool.startsWith("datanexa_") && new Date(event.timestamp).getTime() >= callsCutoff
  )).length;
  const oldestAuditEvent = snapshot.audit_events[snapshot.audit_events.length - 1];
  const callsPossiblyTruncated = snapshot.audit_events.length >= snapshot.config.settings.audit_max_events
    && Boolean(oldestAuditEvent && new Date(oldestAuditEvent.timestamp).getTime() >= callsCutoff);
  const startupFailed = Boolean(snapshot.startup_error);
  const emergencyDisconnect = snapshot.emergency_disconnect;
  const statusLabel = startupFailed
    ? t.overview.failed
    : emergencyDisconnect
      ? t.overview.emergencyDisconnected
      : snapshot.server_status.running
        ? t.overview.running
        : t.overview.stopped;

  return (
    <section className="overview-page">
      <section className={clsx("status-command", startupFailed ? "error" : emergencyDisconnect ? "emergency" : snapshot.server_status.running && "running")} title={snapshot.startup_error ?? undefined}>
        {mcpActivitySequence > 0 && (
          <div className={clsx("mcp-activity-effect", mcpActivityTone)} key={mcpActivitySequence} aria-hidden="true">
            <i className="mcp-activity-ripple first" />
            <i className="mcp-activity-ripple second" />
            <i className="mcp-activity-sweep" />
          </div>
        )}
        <div className="status-command-core">
          <span className="status-beacon">{startupFailed || emergencyDisconnect ? <AlertTriangle size={19} /> : <Activity size={19} />}</span>
          <div>
            <span>{t.overview.metricServer}</span>
            <strong>{statusLabel}</strong>
          </div>
        </div>
        <div className="command-metrics">
          <div><span>{t.overview.metricConnections}</span><strong>{enabledConnections}<small> / {totalConnections}</small></strong></div>
          <div><span>{t.overview.metricTools}</span><strong>{enabledTools}<small> / {snapshot.tools.length}</small></strong></div>
          <div>
            <span>{t.overview.metricCallsLast24Hours}</span>
            <strong className="call-count">
              {callsLast24Hours}
              <small> {t.overview.callsUnit}</small>
              {callsPossiblyTruncated && (
                <IconTooltip label={t.overview.callsTruncatedHint}>
                  <span className="call-count-overflow" role="img" aria-label={t.overview.callsTruncatedHint} tabIndex={0}>
                    <Plus size={10} strokeWidth={2.5} />
                  </span>
                </IconTooltip>
              )}
            </strong>
          </div>
        </div>
        <button
          type="button"
          className={clsx("button command-button", emergencyDisconnect ? "restore" : snapshot.server_status.running ? "stop" : "primary")}
          onClick={emergencyDisconnect ? onToggleEmergency : onToggleServer}
          disabled={busy || startDisabled}
        >
          {emergencyDisconnect ? <Power size={16} /> : snapshot.server_status.running ? <Square size={15} /> : <Play size={16} />}
          {emergencyDisconnect ? t.connections.restoreConnections : snapshot.server_status.running ? t.server.stop : t.server.start}
        </button>
      </section>

      <div className="overview-grid">
        <section className="panel connections-panel">
        <PanelHeader
          title={t.connections.title}
          action={(
            <div className="panel-actions">
              <PanelIconAction icon={<Plus size={16} />} label={t.overview.newConnection} onClick={onAdd} disabled={emergencyDisconnect} />
              <PanelIconAction icon={<ChevronRight size={16} />} label={t.overview.viewAllConnections} onClick={onOpenConnections} />
            </div>
          )}
        />
        <div className="compact-list">
          {snapshot.config.connections.slice(0, 5).map((connection) => (
            <ConnectionListItem t={t} key={connection.id} connection={connection} compact />
          ))}
        </div>
        </section>

        <section className="panel logs-panel">
        <PanelHeader
          title={t.overview.recentLogs}
          action={<PanelIconAction icon={<ChevronRight size={16} />} label={t.overview.viewAll} onClick={onOpenAudit} />}
        />
        <EventList t={t} events={recentEvents} onSelect={onSelectAudit} />
        </section>
      </div>

      <section className="panel quick-panel">
        <h2>{t.overview.quickStart}</h2>
        <div className="quick-steps">
          <QuickStep image={quickStep1Url} title={t.overview.quickConnectTitle} text={t.overview.quickConnectText} />
          <QuickStep image={quickStep2Url} title={t.overview.quickServerTitle} text={t.overview.quickServerText} />
          <QuickStep image={quickStep3Url} title={t.overview.quickAgentTitle} text={t.overview.quickAgentText} wide actionLabel={t.overview.copyAgentConfig} onAction={onCopyAgentPrompt} />
        </div>
      </section>
    </section>
  );
}
