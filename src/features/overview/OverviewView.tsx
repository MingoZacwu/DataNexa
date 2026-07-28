import clsx from "clsx";
import { Activity, AlertTriangle, ChevronRight, Play, Plus, Power, Square } from "lucide-react";
import quickStep1Url from "../../../resources/quickguide/step1.png";
import quickStep2Url from "../../../resources/quickguide/step2.png";
import quickStep3Url from "../../../resources/quickguide/step3.png";
import type { I18nMessages } from "../../i18n";
import type { AppSnapshot, AuditEvent } from "../../types";
import { relativeDuration } from "../../app/utils";
import { EventList, PanelHeader, PanelIconAction, QuickStep } from "../../components/ui";
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
  startDisabled
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
}) {
  const totalConnections = snapshot.config.connections.length;
  const enabledTools = snapshot.tools.filter((tool) => tool.enabled).length;
  const uptime = snapshot.server_status.started_at ? relativeDuration(t, snapshot.server_status.started_at) : t.overview.notStarted;
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
          <div><span>{t.overview.metricUptime}</span><strong>{uptime}</strong></div>
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


