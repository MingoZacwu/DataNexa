import * as Dialog from "@radix-ui/react-dialog";
import * as Switch from "@radix-ui/react-switch";
import * as Tooltip from "@radix-ui/react-tooltip";
import clsx from "clsx";
import {
  Activity,
  AlertTriangle,
  Cable,
  CheckCircle2,
  ChevronRight,
  Clipboard,
  Clock3,
  Database,
  EyeOff,
  FileText,
  Github,
  Home,
  KeyRound,
  ListChecks,
  Logs,
  Minus,
  Moon,
  Monitor,
  MoreVertical,
  Play,
  Plus,
  Power,
  RefreshCw,
  SearchCheck,
  Server,
  Settings,
  ShieldCheck,
  Square,
  Sun,
  Trash2,
  Wrench,
  X
} from "lucide-react";
import { useEffect, useState } from "react";
import { createPortal } from "react-dom";
import type { FormEvent, MouseEvent as ReactMouseEvent, ReactNode } from "react";
import appConfig from "../app.config.json";
import appIconUrl from "../resources/icon.png";
import brandLogoUrl from "../resources/datanexa.png";
import mysqlLogoUrl from "../resources/db-logo/mysql.png";
import postgresLogoUrl from "../resources/db-logo/postgres.png";
import sqliteLogoUrl from "../resources/db-logo/sqlite.png";
import quickStep1Url from "../resources/quickguide/step1.png";
import quickStep2Url from "../resources/quickguide/step2.png";
import quickStep3Url from "../resources/quickguide/step3.png";
import {
  detectLocale,
  formatMessage,
  languageOptions,
  messages,
  normalizeLocale,
  persistLocale
} from "./i18n";
import type { I18nMessages, Locale } from "./i18n";
import { api } from "./lib/tauri";
import type {
  AppSnapshot,
  AuditEvent,
  ConnectionConfig,
  ConnectionDiagnostics,
  DatabaseType,
  McpToolInfo,
  PolicyCheckResult,
  ServerConfig,
  SettingsConfig
} from "./types";

type View = "overview" | "connections" | "server" | "tools" | "audit" | "settings";
type SettingsTab = "general" | "about";
type ThemeMode = "system" | "light" | "dark";
type EffectiveTheme = "light" | "dark";
type ToastTone = "success" | "error" | "info";

const APP_VERSION = appConfig.version;
const THEME_STORAGE_KEY = "datanexa.theme";
const AUDIT_PAGE_SIZE = 50;
const DATABASE_LOGOS: Record<DatabaseType, string> = {
  mysql: mysqlLogoUrl,
  postgres: postgresLogoUrl,
  sqlite: sqliteLogoUrl
};

function isThemeMode(value: string | null): value is ThemeMode {
  return value === "system" || value === "light" || value === "dark";
}

function systemTheme(): EffectiveTheme {
  if (typeof window === "undefined") return "light";
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

function resolveTheme(mode: ThemeMode, fallback: EffectiveTheme): EffectiveTheme {
  return mode === "system" ? fallback : mode;
}

function detectThemeMode(): ThemeMode {
  if (typeof window === "undefined") return "system";
  const stored = window.localStorage.getItem(THEME_STORAGE_KEY);
  return isThemeMode(stored) ? stored : "system";
}

function persistThemeMode(mode: ThemeMode) {
  if (typeof window !== "undefined") {
    window.localStorage.setItem(THEME_STORAGE_KEY, mode);
  }
}

interface ToastMessage {
  id: string;
  message: string;
  tone: ToastTone;
  leaving?: boolean;
}

const defaultConnection = (name: string): ConnectionConfig => ({
  id: `connection_${crypto.randomUUID().slice(0, 8)}`,
  name,
  type: "mysql",
  enabled: true,
  database: "",
  host: "localhost",
  port: 3306,
  username: "",
  credential_ref: null,
  ssl_mode: "prefer",
  max_rows: 500,
  query_timeout_ms: 8000,
  max_connections: 1
});

function App() {
  const [snapshot, setSnapshot] = useState<AppSnapshot | null>(null);
  const [activeView, setActiveView] = useState<View>("overview");
  const [editing, setEditing] = useState<ConnectionConfig | null>(null);
  const [password, setPassword] = useState("");
  const [clearPassword, setClearPassword] = useState(false);
  const [busy, setBusy] = useState(false);
  const [toasts, setToasts] = useState<ToastMessage[]>([]);
  const [policySql, setPolicySql] = useState("SELECT * FROM users");
  const [policyKind, setPolicyKind] = useState<DatabaseType>("mysql");
  const [policyResult, setPolicyResult] = useState<PolicyCheckResult | null>(null);
  const [selectedAudit, setSelectedAudit] = useState<AuditEvent | null>(null);
  const [settingsTab, setSettingsTab] = useState<SettingsTab>("general");
  const [locale, setLocale] = useState<Locale>(detectLocale);
  const [theme, setTheme] = useState<ThemeMode>(detectThemeMode);
  const [systemThemeMode, setSystemThemeMode] = useState<EffectiveTheme>(systemTheme);
  const effectiveTheme = resolveTheme(theme, systemThemeMode);
  const t = messages[locale];

  useEffect(() => {
    void refresh();
  }, []);

  useEffect(() => {
    const preventContextMenu = (event: globalThis.MouseEvent) => event.preventDefault();
    document.addEventListener("contextmenu", preventContextMenu);
    return () => document.removeEventListener("contextmenu", preventContextMenu);
  }, []);

  useEffect(() => {
    const query = window.matchMedia("(prefers-color-scheme: dark)");
    const updateSystemTheme = () => setSystemThemeMode(query.matches ? "dark" : "light");

    updateSystemTheme();
    query.addEventListener("change", updateSystemTheme);
    return () => query.removeEventListener("change", updateSystemTheme);
  }, []);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    document.documentElement.classList.toggle("dark", effectiveTheme === "dark");
    persistThemeMode(theme);
  }, [theme, effectiveTheme]);

  useEffect(() => {
    document.documentElement.lang = locale;
  }, [locale]);

  useEffect(() => {
    const configuredLanguage = snapshot?.config.settings.language;
    if (!configuredLanguage) return;
    const nextLocale = normalizeLocale(configuredLanguage);
    setLocale((current) => (current === nextLocale ? current : nextLocale));
    persistLocale(nextLocale);
  }, [snapshot?.config.settings.language]);

  useEffect(() => {
    void refresh({ quiet: true });
  }, [activeView]);

  useEffect(() => {
    const timer = window.setInterval(() => {
      void refresh({ quiet: true });
    }, 2500);
    return () => window.clearInterval(timer);
  }, []);

  async function refresh(options: { quiet?: boolean } = {}) {
    if (!options.quiet) setBusy(true);
    try {
      setSnapshot(await api.snapshot());
    } catch (error) {
      showError(error);
    } finally {
      if (!options.quiet) setBusy(false);
    }
  }

  function pushToast(message: string, tone: ToastTone = "success") {
    const id = crypto.randomUUID();
    setToasts((items) => [{ id, message, tone }, ...items].slice(0, 4));
    window.setTimeout(() => {
      dismissToast(id);
    }, tone === "error" ? 4200 : 2400);
  }

  function dismissToast(id: string) {
    setToasts((items) => items.map((item) => item.id === id ? { ...item, leaving: true } : item));
    window.setTimeout(() => {
      setToasts((items) => items.filter((item) => item.id !== id));
    }, 180);
  }

  function showError(error: unknown) {
    pushToast(error instanceof Error ? error.message : String(error), "error");
  }

  function openNewConnection() {
    setPassword("");
    setClearPassword(false);
    setEditing(defaultConnection(t.connections.newConnectionName));
  }

  function openExistingConnection(connection: ConnectionConfig) {
    setPassword("");
    setClearPassword(false);
    setEditing({ ...connection });
  }

  async function saveConnection(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!editing) return;
    setBusy(true);
    try {
      const next = await api.upsertConnection({
        connection: editing,
        password: password.length > 0 ? password : null,
        clear_password: clearPassword
      });
      setSnapshot(next);
      setEditing(null);
      setPassword("");
      setClearPassword(false);
      pushToast(t.toast.connectionSaved);
    } catch (error) {
      showError(error);
    } finally {
      setBusy(false);
    }
  }

  async function deleteConnection(id: string) {
    setBusy(true);
    try {
      setSnapshot(await api.deleteConnection(id));
      pushToast(t.toast.connectionDeleted);
    } catch (error) {
      showError(error);
    } finally {
      setBusy(false);
    }
  }

  async function setConnectionEnabled(id: string, enabled: boolean) {
    setBusy(true);
    try {
      const connectionName = snapshot?.config.connections.find((connection) => connection.id === id)?.name ?? id;
      setSnapshot(await api.setConnectionEnabled(id, enabled));
      pushToast(
        formatMessage(enabled ? t.toast.connectionEnabled : t.toast.connectionDisabled, {
          connection: connectionName
        }),
        "info"
      );
    } catch (error) {
      showError(error);
    } finally {
      setBusy(false);
    }
  }

  async function disableAllConnections() {
    setBusy(true);
    try {
      setSnapshot(await api.disableAllConnections());
      pushToast(t.toast.allConnectionsDisabled, "info");
    } catch (error) {
      showError(error);
    } finally {
      setBusy(false);
    }
  }

  async function clearAuditEvents() {
    setBusy(true);
    try {
      setSnapshot(await api.clearAuditEvents());
      setSelectedAudit(null);
      pushToast(t.toast.auditCleared, "info");
    } catch (error) {
      showError(error);
    } finally {
      setBusy(false);
    }
  }

  async function testConnection(id: string) {
    setBusy(true);
    try {
      pushToast(formatConnectionTest(t, await api.testConnection(id)), "info");
      await refresh({ quiet: true });
    } catch (error) {
      pushToast(compactConnectionError(error), "error");
    } finally {
      setBusy(false);
    }
  }

  async function testEditingConnection() {
    if (!editing) return;
    setBusy(true);
    try {
      pushToast(
        formatConnectionTest(t, await api.testConnectionInput({
          connection: editing,
          password: password.length > 0 ? password : null,
          clear_password: clearPassword
        })),
        "info"
      );
      await refresh({ quiet: true });
    } catch (error) {
      pushToast(compactConnectionError(error), "error");
    } finally {
      setBusy(false);
    }
  }

  async function diagnoseConnection(id: string) {
    setBusy(true);
    try {
      pushToast(formatDiagnostics(t, await api.diagnoseConnection(id)), "info");
      await refresh({ quiet: true });
    } catch (error) {
      pushToast(compactConnectionError(error), "error");
    } finally {
      setBusy(false);
    }
  }

  async function toggleServer() {
    setBusy(true);
    try {
      setSnapshot(snapshot?.server_status.running ? await api.stopServer() : await api.startServer());
    } catch (error) {
      showError(error);
    } finally {
      setBusy(false);
    }
  }

  async function rotateToken() {
    setBusy(true);
    try {
      setSnapshot(await api.rotateToken());
      pushToast(t.toast.tokenRotated);
    } catch (error) {
      showError(error);
    } finally {
      setBusy(false);
    }
  }

  async function saveServer(server: ServerConfig) {
    setBusy(true);
    try {
      setSnapshot(await api.saveServerConfig(server));
      pushToast(t.toast.serverSaved, "info");
    } catch (error) {
      showError(error);
    } finally {
      setBusy(false);
    }
  }

  async function saveSettings(settings: SettingsConfig) {
    setBusy(true);
    try {
      const nextLocale = normalizeLocale(settings.language);
      setLocale(nextLocale);
      persistLocale(nextLocale);
      setSnapshot(await api.saveSettingsConfig({ ...settings, language: nextLocale }));
      pushToast(messages[nextLocale].toast.settingsSaved, "info");
    } catch (error) {
      showError(error);
    } finally {
      setBusy(false);
    }
  }

  async function runPolicyCheck() {
    setBusy(true);
    try {
      setPolicyResult(await api.policyCheck(policyKind, policySql, 500));
    } catch (error) {
      showError(error);
    } finally {
      setBusy(false);
    }
  }

  async function setToolEnabled(name: string, enabled: boolean) {
    setBusy(true);
    try {
      setSnapshot(await api.setMcpToolEnabled(name, enabled));
      pushToast(
        formatMessage(enabled ? t.toast.toolEnabled : t.toast.toolDisabled, {
          tool: toolDisplayName(t, name)
        }),
        "info"
      );
    } catch (error) {
      showError(error);
    } finally {
      setBusy(false);
    }
  }

  function copyAgentPrompt(endpoint: string, requireToken: boolean, token?: string | null) {
    const prompt = buildAgentPrompt(t, endpoint, requireToken, token);
    void navigator.clipboard
      .writeText(prompt)
      .then(() => pushToast(t.toast.agentCopied, "info"))
      .catch(showError);
  }

  const connections = snapshot?.config.connections ?? [];
  const enabledConnections = connections.filter((connection) => connection.enabled).length;
  const serverEndpoint = snapshot?.server_status.endpoint ?? "http://127.0.0.1:17321/mcp";
  const requireToken = snapshot?.config.server.require_token ?? true;
  const serverToken = snapshot?.server_status.token ?? null;
  const recentEvents = snapshot?.audit_events.slice(0, 8) ?? [];

  return (
    <Tooltip.Provider delayDuration={180}>
      <div className="app-shell">
        <div className="ambient-grid" aria-hidden="true" />
        <WindowChrome t={t} />

        <div className="app-body">
          <aside className="sidebar">
            <div className="brand">
              <div className="brand-mark"><img src={brandLogoUrl} alt="DataNexa" /></div>
              <div>
                <strong>DataNexa</strong>
                <span>MCP DATABASE GATEWAY</span>
              </div>
            </div>

            <nav className="nav-list">
              <NavButton icon={<Home />} label={t.nav.overview} active={activeView === "overview"} onClick={() => setActiveView("overview")} />
              <NavButton icon={<Database />} label={t.nav.connections} active={activeView === "connections"} onClick={() => setActiveView("connections")} />
              <NavButton icon={<Server />} label={t.nav.server} active={activeView === "server"} onClick={() => setActiveView("server")} />
              <NavButton icon={<Wrench />} label={t.nav.tools} active={activeView === "tools"} onClick={() => setActiveView("tools")} />
              <NavButton icon={<Logs />} label={t.nav.audit} active={activeView === "audit"} onClick={() => setActiveView("audit")} />
              <NavButton icon={<Settings />} label={t.nav.settings} active={activeView === "settings"} onClick={() => setActiveView("settings")} />
            </nav>

            <SidebarFooter
              t={t}
              running={Boolean(snapshot?.server_status.running)}
              port={snapshot?.config.server.port ?? 17321}
              busy={busy}
              disabled={!snapshot}
              onToggle={toggleServer}
            />
          </aside>

          <main className="workspace">
            <header className="topbar">
              <div className="page-title-block">
                <h1>{viewTitle(t, activeView)}</h1>
              </div>
              <div className="top-actions">
                <div className="top-icon-actions">
                  {snapshot && activeView === "connections" && (
                    <IconTooltip label={t.connections.emergencyDisable}>
                      <button type="button" className="icon-button danger" onClick={disableAllConnections} disabled={busy || connections.length === 0} aria-label={t.connections.emergencyDisable}>
                        <AlertTriangle size={17} />
                      </button>
                    </IconTooltip>
                  )}
                  {snapshot && activeView === "audit" && (
                    <IconTooltip label={t.audit.clear}>
                      <button type="button" className="icon-button danger" onClick={clearAuditEvents} disabled={busy || snapshot.audit_events.length === 0} aria-label={t.audit.clear}>
                        <Trash2 size={17} />
                      </button>
                    </IconTooltip>
                  )}
                  <button type="button" className={clsx("icon-button", busy && "is-spinning")} onClick={() => refresh()} disabled={busy} aria-label={t.common.refresh}>
                    <RefreshCw size={17} />
                  </button>
                </div>
                {snapshot && activeView === "connections" && (
                  <button type="button" className="button primary" onClick={openNewConnection} disabled={busy}>
                    <Plus size={16} />
                    {t.overview.newConnection}
                  </button>
                )}
              </div>
            </header>

            {!snapshot ? (
              <div className="loading-panel">{t.overview.loading}</div>
            ) : (
              <div className={clsx("view-stage", `view-${activeView}`)} key={activeView}>
                {activeView === "overview" && (
                  <OverviewView
                    t={t}
                    snapshot={snapshot}
                    enabledConnections={enabledConnections}
                    recentEvents={recentEvents}
                    onAdd={openNewConnection}
                    onOpenConnections={() => setActiveView("connections")}
                    onOpenAudit={() => setActiveView("audit")}
                    onSelectAudit={setSelectedAudit}
                    onCopyAgentPrompt={() => copyAgentPrompt(serverEndpoint, requireToken, serverToken)}
                    onToggleServer={toggleServer}
                  />
                )}
                {activeView === "connections" && (
                  <ConnectionsView
                    t={t}
                    connections={connections}
                    busy={busy}
                    onEdit={openExistingConnection}
                    onDelete={deleteConnection}
                    onTest={testConnection}
                    onDiagnose={diagnoseConnection}
                    onToggleEnabled={setConnectionEnabled}
                  />
                )}
                {activeView === "server" && (
                  <ServerView
                    t={t}
                    snapshot={snapshot}
                    busy={busy}
                    endpoint={serverEndpoint}
                    onCopyAgentPrompt={() => copyAgentPrompt(serverEndpoint, snapshot.config.server.require_token, snapshot.server_status.token)}
                    onToggle={toggleServer}
                    onRotate={rotateToken}
                  />
                )}
                {activeView === "tools" && <ToolsView t={t} tools={snapshot.tools} busy={busy} onToggle={setToolEnabled} />}
                {activeView === "audit" && <AuditView t={t} events={snapshot.audit_events} onSelect={setSelectedAudit} />}
                {activeView === "settings" && (
                  <SettingsView
                    t={t}
                    locale={locale}
                    theme={theme}
                    effectiveTheme={effectiveTheme}
                    server={snapshot.config.server}
                    settings={snapshot.config.settings}
                    busy={busy}
                    tab={settingsTab}
                    policySql={policySql}
                    policyKind={policyKind}
                    policyResult={policyResult}
                    onTabChange={setSettingsTab}
                    onThemeChange={setTheme}
                    onPolicyKindChange={setPolicyKind}
                    onSqlChange={setPolicySql}
                    onPolicyCheck={runPolicyCheck}
                    onSaveServer={saveServer}
                    onSaveSettings={saveSettings}
                    onOpenProjectHomepage={() => void api.openProjectHomepage().catch(showError)}
                  />
                )}
              </div>
            )}
          </main>
        </div>

        <ToastViewport t={t} toasts={toasts} onDismiss={dismissToast} />

        <ConnectionDialog
          t={t}
          editing={editing}
          busy={busy}
          password={password}
          clearPassword={clearPassword}
          onPasswordChange={setPassword}
          onClearPasswordChange={(checked) => {
            setClearPassword(checked);
            if (checked) setPassword("");
          }}
          onEditingChange={setEditing}
          onTest={testEditingConnection}
          onSubmit={saveConnection}
          onClose={() => setEditing(null)}
        />
        <AuditDetailDialog t={t} event={selectedAudit} onClose={() => setSelectedAudit(null)} />
      </div>
    </Tooltip.Provider>
  );
}

function WindowChrome({ t }: { t: I18nMessages }) {
  const isMacos = typeof navigator !== "undefined" && /Mac|iPhone|iPad|iPod/i.test(navigator.userAgent);

  function handleDragStart(event: ReactMouseEvent<HTMLDivElement>) {
    if (event.button !== 0) return;
    if ((event.target as HTMLElement).closest("button")) return;
    void api.startWindowDrag().catch(() => undefined);
  }

  return (
    <div className="window-chrome" onMouseDown={handleDragStart} data-tauri-drag-region>
      <div className="window-title" data-tauri-drag-region>
        DataNexa
      </div>
      {!isMacos && (
        <div className="window-controls">
          <button type="button" className="window-control minimize" onClick={() => void api.minimizeWindow().catch(() => undefined)} aria-label={t.common.minimize}>
            <Minus size={13} />
          </button>
          <button type="button" className="window-control close" onClick={() => void api.hideWindow().catch(() => undefined)} aria-label={t.common.close}>
            <X size={13} />
          </button>
        </div>
      )}
    </div>
  );
}

function SidebarFooter({
  t,
  running,
  port,
  busy,
  disabled,
  onToggle
}: {
  t: I18nMessages;
  running: boolean;
  port: number;
  busy: boolean;
  disabled: boolean;
  onToggle: () => void;
}) {
  const toggleLabel = running ? t.server.stop : t.server.start;
  return (
    <div className="sidebar-footer">
      <div className={clsx("sidebar-status-line", running && "running")}>
        <span className="status-orb" />
        <span>{running ? formatMessage(t.sidebar.serverRunning, { port }) : t.sidebar.serverStopped}</span>
      </div>
      <span className="footer-divider" aria-hidden="true" />
      <IconTooltip label={toggleLabel}>
        <button
          type="button"
          className={clsx("sidebar-service-button", running && "running")}
          onClick={onToggle}
          disabled={busy || disabled}
          aria-label={toggleLabel}
        >
          {running ? <Square size={16} /> : <Play size={16} />}
        </button>
      </IconTooltip>
    </div>
  );
}

function NavButton({ icon, label, active, onClick }: { icon: ReactNode; label: string; active: boolean; onClick: () => void }) {
  return (
    <button type="button" className={clsx("nav-button", active && "active")} onClick={onClick}>
      {icon}
      <span>{label}</span>
    </button>
  );
}

function ThemeModeControl({
  t,
  theme,
  effectiveTheme,
  labelledBy,
  disabled,
  onChange
}: {
  t: I18nMessages;
  theme: ThemeMode;
  effectiveTheme: EffectiveTheme;
  labelledBy?: string;
  disabled?: boolean;
  onChange: (theme: ThemeMode) => void;
}) {
  const options: Array<{ value: ThemeMode; label: string; icon: ReactNode }> = [
    { value: "system", label: t.settings.themeSystem, icon: <Monitor size={15} /> },
    { value: "light", label: t.settings.themeLight, icon: <Sun size={15} /> },
    { value: "dark", label: t.settings.themeDark, icon: <Moon size={15} /> }
  ];

  return (
    <div
      className="theme-mode-control"
      role="radiogroup"
      aria-label={labelledBy ? undefined : t.settings.theme}
      aria-labelledby={labelledBy}
      data-effective-theme={effectiveTheme}
    >
      {options.map((option) => (
        <button
          key={option.value}
          type="button"
          role="radio"
          aria-checked={theme === option.value}
          className={clsx(theme === option.value && "active")}
          disabled={disabled}
          onClick={() => onChange(option.value)}
        >
          {option.icon}
          <span>{option.label}</span>
        </button>
      ))}
    </div>
  );
}

function OverviewView({
  t,
  snapshot,
  enabledConnections,
  recentEvents,
  onAdd,
  onOpenConnections,
  onOpenAudit,
  onSelectAudit,
  onCopyAgentPrompt,
  onToggleServer
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
}) {
  const totalConnections = snapshot.config.connections.length;
  const enabledTools = snapshot.tools.filter((tool) => tool.enabled).length;
  const uptime = snapshot.server_status.started_at ? relativeDuration(t, snapshot.server_status.started_at) : t.overview.notStarted;

  const onboardingComplete = totalConnections > 0;

  return (
    <section className="overview-page">
      <section className={clsx("status-command", snapshot.server_status.running && "running")}>
        <div className="status-command-core">
          <span className="status-beacon"><Activity size={19} /></span>
          <div>
            <span>{t.overview.metricServer}</span>
            <strong>{snapshot.server_status.running ? t.overview.running : t.overview.stopped}</strong>
          </div>
        </div>
        <div className="command-metrics">
          <div><span>{t.overview.metricConnections}</span><strong>{enabledConnections}<small> / {totalConnections}</small></strong></div>
          <div><span>{t.overview.metricTools}</span><strong>{enabledTools}<small> / {snapshot.tools.length}</small></strong></div>
          <div><span>{t.overview.metricUptime}</span><strong>{uptime}</strong></div>
        </div>
        <button type="button" className={clsx("button command-button", snapshot.server_status.running ? "stop" : "primary")} onClick={onToggleServer}>
          {snapshot.server_status.running ? <Square size={15} /> : <Play size={16} />}
          {snapshot.server_status.running ? t.server.stop : t.server.start}
        </button>
      </section>

      <div className="overview-grid">
        <section className="panel connections-panel">
        <PanelHeader
          title={t.connections.title}
          action={(
            <div className="panel-actions">
              <PanelIconAction icon={<Plus size={16} />} label={t.overview.newConnection} onClick={onAdd} />
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

      <section className={clsx("panel quick-panel", onboardingComplete && "is-compact")}>
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

function ConnectionsView({
  t,
  connections,
  busy,
  onEdit,
  onDelete,
  onTest,
  onDiagnose,
  onToggleEnabled
}: {
  t: I18nMessages;
  connections: ConnectionConfig[];
  busy: boolean;
  onEdit: (connection: ConnectionConfig) => void;
  onDelete: (id: string) => void;
  onTest: (id: string) => void;
  onDiagnose: (id: string) => void;
  onToggleEnabled: (id: string, enabled: boolean) => void;
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
              <ConnectionListItem t={t} connection={selected} />
              <button type="button" className="button ghost" onClick={() => onEdit(selected)}>{t.connections.edit}</button>
            </div>
            <dl className="inspector-grid">
              <div><dt>{t.connectionDialog.host}</dt><dd><code>{selected.type === "sqlite" ? "LOCAL" : selected.host || "-"}</code></dd></div>
              <div><dt>{t.connectionDialog.port}</dt><dd><code>{selected.type === "sqlite" ? "-" : selected.port ?? defaultPort(selected.type)}</code></dd></div>
              <div><dt>{t.connectionDialog.database}</dt><dd><code>{selected.database || "-"}</code></dd></div>
              <div><dt>{t.connectionDialog.username}</dt><dd><code>{selected.username || "-"}</code></dd></div>
              <div><dt>{t.connectionDialog.sslMode}</dt><dd>{selected.ssl_mode || "-"}</dd></div>
              <div><dt>{t.connectionDialog.maxRows}</dt><dd>{selected.max_rows}</dd></div>
              <div><dt>{t.connectionDialog.queryTimeoutMs}</dt><dd>{selected.query_timeout_ms} ms</dd></div>
              <div><dt>{t.connectionDialog.maxConnections}</dt><dd>{selected.max_connections}</dd></div>
            </dl>
            <div className="inspector-actions">
              <button type="button" className="button soft" disabled={busy || !selected.enabled} onClick={() => onTest(selected.id)}><Cable size={15} />{t.connections.test}</button>
              <button type="button" className="button ghost" disabled={busy || !selected.enabled} onClick={() => onDiagnose(selected.id)}><SearchCheck size={15} />{t.connections.diagnose}</button>
            </div>
          </>
        ) : <div className="empty-state">{t.connections.empty}</div>}
      </aside>
    </section>
  );
}

function ToolsView({
  t,
  tools,
  busy,
  onToggle
}: {
  t: I18nMessages;
  tools: McpToolInfo[];
  busy: boolean;
  onToggle: (name: string, enabled: boolean) => void;
}) {
  const enabledCount = tools.filter((tool) => tool.enabled).length;
  const groups = [
    { key: "discovery" as const, tools: tools.filter((tool) => ["datanexa_list_connections", "datanexa_get_schema", "datanexa_describe_table"].includes(tool.name)) },
    { key: "access" as const, tools: tools.filter((tool) => ["datanexa_sample_rows", "datanexa_execute_readonly_sql"].includes(tool.name)) },
    { key: "analysis" as const, tools: tools.filter((tool) => ["datanexa_explain_sql", "datanexa_policy_check"].includes(tool.name)) }
  ];

  return (
    <section className="tools-page">
      <div className="panel tools-summary">
        <div>
          <h2>{formatMessage(t.tools.summary, { enabled: enabledCount, total: tools.length })}</h2>
        </div>
      </div>

      <div className="tools-list">
        {groups.map((group) => (
          <section className="tool-group" key={group.key}>
            <header><span>{t.tools.groups[group.key]}</span><small>{group.tools.filter((tool) => tool.enabled).length} / {group.tools.length}</small></header>
            {group.tools.map((tool) => (
              <article className={clsx("tool-card", !tool.enabled && "disabled")} key={tool.name}>
                <span className="tool-signal" />
                <div className="tool-body">
                  <div className="tool-title-row">
                    <div>
                      <strong>{toolDisplayName(t, tool.name)}</strong>
                      <code>{tool.name}</code>
                    </div>
                  </div>
                  <p>{toolIntro(t, tool)}</p>
                </div>
                <Switch.Root className="switch" checked={tool.enabled} disabled={busy} onCheckedChange={(checked) => onToggle(tool.name, checked)} aria-label={formatMessage(t.tools.toggle, { name: tool.name })}>
                  <Switch.Thumb className="switch-thumb" />
                </Switch.Root>
              </article>
            ))}
          </section>
        ))}
      </div>
    </section>
  );
}

function ServerView({
  t,
  snapshot,
  busy,
  endpoint,
  onCopyAgentPrompt,
  onToggle,
  onRotate
}: {
  t: I18nMessages;
  snapshot: AppSnapshot;
  busy: boolean;
  endpoint: string;
  onCopyAgentPrompt: () => void;
  onToggle: () => void;
  onRotate: () => void;
}) {
  const requireToken = snapshot.config.server.require_token;

  return (
    <section className={clsx("server-console", snapshot.server_status.running && "running")}>
      <div className="server-hero">
        <div className="server-identity">
          <span className="server-emblem"><Server size={25} /></span>
          <div><span className="panel-kicker">{t.overview.metricServer}</span><h2>{snapshot.server_status.running ? t.overview.running : t.overview.stopped}</h2></div>
        </div>
        <button type="button" className={clsx("button", snapshot.server_status.running ? "stop" : "primary")} onClick={onToggle} disabled={busy}>
          {snapshot.server_status.running ? <Square size={16} /> : <Play size={17} />}
          {snapshot.server_status.running ? t.server.stop : t.server.start}
        </button>
      </div>

      <div className="server-console-grid">
        <div className="server-console-section endpoint-section">
          <PanelHeader title={t.server.endpoint} />
          <div className="console-value">
            <code>{endpoint}</code>
            <button type="button" className="icon-button" onClick={() => navigator.clipboard.writeText(endpoint)} aria-label={t.server.copyEndpoint}><Clipboard size={16} /></button>
          </div>
          <StatusPill tone={snapshot.server_status.running ? "green" : "slate"} label={snapshot.server_status.running ? t.overview.running : t.overview.stopped} />
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

function AuditView({ t, events, onSelect }: { t: I18nMessages; events: AuditEvent[]; onSelect: (event: AuditEvent) => void }) {
  const [page, setPage] = useState(1);
  const totalPages = Math.max(1, Math.ceil(events.length / AUDIT_PAGE_SIZE));
  const currentPage = Math.min(page, totalPages);
  const pageStart = (currentPage - 1) * AUDIT_PAGE_SIZE;
  const pageEvents = events.slice(pageStart, pageStart + AUDIT_PAGE_SIZE);

  useEffect(() => {
    setPage((current) => Math.min(current, totalPages));
  }, [totalPages]);

  return (
    <section className="panel page-panel list-page-panel audit-page-panel">
      <div className="audit-table">
        <div className="audit-row header">
          <span>{t.audit.time}</span>
          <span>{t.audit.tool}</span>
          <span>{t.audit.connection}</span>
          <span>{t.audit.status}</span>
          <span>{t.audit.detail}</span>
        </div>
        <div className="audit-table-body">
          {events.length === 0 ? (
            <div className="empty-state">{t.audit.empty}</div>
          ) : (
            pageEvents.map((event) => (
              <button type="button" className="audit-row audit-button" key={event.id} onClick={() => onSelect(event)}>
                <span>{new Date(event.timestamp).toLocaleString()}</span>
                <span>{event.tool}</span>
                <span>{event.connection_id ?? t.common.system}</span>
                <span>
                  <StatusPill tone={statusTone(event.status)} label={statusLabel(t, event.status)} />
                </span>
                <span>{event.reason ?? formatMessage(t.common.rowsElapsed, { rows: event.row_count ?? 0, elapsed: event.elapsed_ms ?? 0 })}</span>
              </button>
            ))
          )}
        </div>
        {events.length > 0 && (
          <div className="pagination-footer">
            <span>{formatMessage(t.audit.pageInfo, { page: currentPage, totalPages, total: events.length })}</span>
            <div className="pagination-actions">
              <button type="button" className="button ghost" disabled={currentPage <= 1} onClick={() => setPage((value) => Math.max(1, value - 1))}>
                {t.common.previous}
              </button>
              <button type="button" className="button ghost" disabled={currentPage >= totalPages} onClick={() => setPage((value) => Math.min(totalPages, value + 1))}>
                {t.common.next}
              </button>
            </div>
          </div>
        )}
      </div>
    </section>
  );
}

function SettingsView({
  t,
  locale,
  theme,
  effectiveTheme,
  server,
  settings,
  busy,
  tab,
  policySql,
  policyKind,
  policyResult,
  onTabChange,
  onThemeChange,
  onPolicyKindChange,
  onSqlChange,
  onPolicyCheck,
  onSaveServer,
  onSaveSettings,
  onOpenProjectHomepage
}: {
  t: I18nMessages;
  locale: Locale;
  theme: ThemeMode;
  effectiveTheme: EffectiveTheme;
  server: ServerConfig;
  settings: SettingsConfig;
  busy: boolean;
  tab: SettingsTab;
  policySql: string;
  policyKind: DatabaseType;
  policyResult: PolicyCheckResult | null;
  onTabChange: (tab: SettingsTab) => void;
  onThemeChange: (theme: ThemeMode) => void;
  onPolicyKindChange: (kind: DatabaseType) => void;
  onSqlChange: (sql: string) => void;
  onPolicyCheck: () => void;
  onSaveServer: (server: ServerConfig) => void;
  onSaveSettings: (settings: SettingsConfig) => void;
  onOpenProjectHomepage: () => void;
}) {
  const [serverDraft, setServerDraft] = useState(server);
  const [settingsDraft, setSettingsDraft] = useState(settings);
  const [policyDialogOpen, setPolicyDialogOpen] = useState(false);

  useEffect(() => setServerDraft(server), [server]);
  useEffect(() => setSettingsDraft(settings), [settings]);
  useEffect(() => {
    setSettingsDraft((current) => ({ ...current, language: locale }));
  }, [locale]);

  return (
    <section className="settings-page">
      <div className="settings-tabs">
        <button type="button" className={clsx(tab === "general" && "active")} onClick={() => onTabChange("general")}>
          {t.settings.general}
        </button>
        <button type="button" className={clsx(tab === "about" && "active")} onClick={() => onTabChange("about")}>
          {t.settings.about}
        </button>
      </div>

      {tab === "general" ? (
        <div className="settings-stack">
          <section className="panel">
            <h2>{t.settings.servicePolicy}</h2>
            <div className="form-grid settings-grid">
              <Field label={t.settings.listenHost}>
                <input
                  value={serverDraft.host}
                  onChange={(event) => setServerDraft({ ...serverDraft, host: event.target.value })}
                  onBlur={(event) => onSaveServer({ ...serverDraft, host: event.currentTarget.value })}
                />
              </Field>
              <Field label={t.settings.port}>
                <input
                  type="number"
                  value={serverDraft.port}
                  onChange={(event) => setServerDraft({ ...serverDraft, port: Number(event.target.value) })}
                  onBlur={(event) => onSaveServer({ ...serverDraft, port: Number(event.currentTarget.value) })}
                />
              </Field>
              <SwitchField label={t.settings.requireBearer} checked={serverDraft.require_token} disabled={busy} onCheckedChange={(checked) => {
                const next = { ...serverDraft, require_token: checked };
                setServerDraft(next);
                onSaveServer(next);
              }} />
              <SwitchField label={t.settings.legacySse} checked={serverDraft.legacy_sse_compat} disabled={busy} onCheckedChange={(checked) => {
                const next = { ...serverDraft, legacy_sse_compat: checked };
                setServerDraft(next);
                onSaveServer(next);
              }} />
            </div>
          </section>

          <section className="panel">
            <h2>{t.settings.display}</h2>
            <div className="form-grid settings-grid">
              <Field label={t.settings.language}>
                <select
                  value={locale}
                  onChange={(event) => {
                    const language = normalizeLocale(event.target.value);
                    const next = { ...settingsDraft, language };
                    setSettingsDraft(next);
                    onSaveSettings(next);
                  }}
                  disabled={busy}
                >
                  {languageOptions.map((option) => (
                    <option key={option.value} value={option.value}>
                      {option.nativeLabel}
                    </option>
                  ))}
                </select>
              </Field>
              <div className="field span-all">
                <span id="settings-theme-mode-label">{t.settings.theme}</span>
                <ThemeModeControl
                  t={t}
                  theme={theme}
                  effectiveTheme={effectiveTheme}
                  labelledBy="settings-theme-mode-label"
                  disabled={busy}
                  onChange={onThemeChange}
                />
              </div>
            </div>
          </section>

          <section className="panel">
            <h2>{t.settings.auditLog}</h2>
            <div className="form-grid settings-grid">
              <Field label={t.settings.auditMaxEvents}>
                <input
                  type="number"
                  min={1}
                  max={5000}
                  value={settingsDraft.audit_max_events}
                  onChange={(event) => setSettingsDraft({ ...settingsDraft, audit_max_events: Number(event.target.value) })}
                  onBlur={(event) => onSaveSettings({ ...settingsDraft, audit_max_events: Number(event.currentTarget.value) })}
                />
              </Field>
            </div>
          </section>

          <Dialog.Root open={policyDialogOpen} onOpenChange={setPolicyDialogOpen}>
            <section className="panel safety-panel">
              <div className="panel-header">
                <h2>{t.settings.securityPosture}</h2>
                <IconTooltip label={t.settings.checkSql}>
                  <Dialog.Trigger asChild>
                    <button type="button" className="policy-check-button" disabled={busy} aria-label={t.settings.checkSql}>
                      <SearchCheck size={17} />
                    </button>
                  </Dialog.Trigger>
                </IconTooltip>
              </div>
              <ul className="security-list">
                <li><ShieldCheck size={16} /> {t.settings.securityAst}</li>
                <li><ListChecks size={16} /> {t.settings.securityReadonly}</li>
                <li><KeyRound size={16} /> {t.settings.securityVault}</li>
                <li><FileText size={16} /> {t.settings.securityAudit}</li>
                <li className="security-warning"><AlertTriangle size={16} /> {t.settings.securityWarning}</li>
              </ul>
            </section>
            <Dialog.Portal>
              <Dialog.Overlay className="dialog-overlay" />
              <Dialog.Content className="policy-dialog">
                <div className="dialog-titlebar">
                  <div>
                    <Dialog.Title>{t.settings.policyConsole}</Dialog.Title>
                    <Dialog.Description>{t.settings.policyDescription}</Dialog.Description>
                  </div>
                  <Dialog.Close asChild>
                    <button type="button" className="icon-button" aria-label={t.common.close}>
                      <X size={18} />
                    </button>
                  </Dialog.Close>
                </div>
                <div className="policy-panel">
                  <div className="policy-toolbar">
                    <Field label={t.settings.sqlDialect}>
                      <select value={policyKind} onChange={(event) => onPolicyKindChange(event.target.value as DatabaseType)}>
                        <option value="mysql">MySQL</option>
                        <option value="postgres">PostgreSQL</option>
                        <option value="sqlite">SQLite</option>
                      </select>
                    </Field>
                    <button type="button" className="button primary" disabled={busy} onClick={onPolicyCheck}>
                      <SearchCheck size={17} />
                      {t.settings.checkSql}
                    </button>
                  </div>
                  <textarea value={policySql} onChange={(event) => onSqlChange(event.target.value)} spellCheck={false} />
                  {policyResult && (
                    <div className={clsx("policy-result", policyResult.allowed ? "allowed" : "denied")}>
                      {policyResult.allowed ? <CheckCircle2 size={18} /> : <AlertTriangle size={18} />}
                      <div>
                        <strong>{policyResult.allowed ? t.settings.allowed : t.settings.denied}</strong>
                        <p>{policyResult.reason}</p>
                        {policyResult.rewritten_sql && <code>{policyResult.rewritten_sql}</code>}
                      </div>
                    </div>
                  )}
                </div>
              </Dialog.Content>
            </Dialog.Portal>
          </Dialog.Root>
        </div>
      ) : (
        <div className="settings-stack">
          <section className="panel about-panel">
            <div className="about-hero">
              <img src={appIconUrl} alt="DataNexa" />
              <div>
                <h2>DataNexa <span className="version-badge">v{APP_VERSION}</span></h2>
                <p>{t.settings.aboutText}</p>
              </div>
            </div>
            <footer className="about-footer">
              <a
                className="github-link"
                href="https://github.com/MingoZacwu/DataNexa"
                onClick={(event) => {
                  event.preventDefault();
                  onOpenProjectHomepage();
                }}
              >
                <Github size={16} />
                GitHub
              </a>
              <p>(C) 2026 Zachary Wu All Rights Reserved.</p>
            </footer>
          </section>
        </div>
      )}
    </section>
  );
}

function ConnectionDialog({
  t,
  editing,
  busy,
  password,
  clearPassword,
  onPasswordChange,
  onClearPasswordChange,
  onEditingChange,
  onTest,
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
                    onEditingChange({ ...editing, type, port: defaultPort(type), ssl_mode: type === "sqlite" ? null : editing.ssl_mode ?? "prefer" });
                  }}
                >
                  <option value="sqlite">SQLite</option>
                  <option value="mysql">MySQL</option>
                  <option value="postgres">PostgreSQL</option>
                </select>
              </Field>
            </FormSection>

            <FormSection title={t.connectionDialog.address}>
              {editing.type === "sqlite" ? (
                <Field label={t.connectionDialog.databaseFile} span>
                  <input value={editing.database} onChange={(event) => onEditingChange({ ...editing, database: event.target.value })} placeholder="E:/data/app.db" required />
                </Field>
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
              <p className="field-note span-all">
                {formatMessage(t.connectionDialog.currentCredential, { credential: editing.credential_ref ?? t.connectionDialog.credentialNotSaved })}
              </p>
            </FormSection>
            </div>

            <footer>
              <button type="button" className="button soft" disabled={busy} onClick={onTest}>
                <Cable size={16} />
                {t.connections.test}
              </button>
              <Dialog.Close asChild>
                <button type="button" className="button ghost">{t.common.cancel}</button>
              </Dialog.Close>
              <button type="submit" className="button primary" disabled={busy}>{t.connectionDialog.save}</button>
            </footer>
          </form>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

function AuditDetailDialog({ t, event, onClose }: { t: I18nMessages; event: AuditEvent | null; onClose: () => void }) {
  return (
    <Dialog.Root open={Boolean(event)} onOpenChange={(open) => !open && onClose()}>
      <Dialog.Portal>
        <Dialog.Overlay className="dialog-overlay" />
        <Dialog.Content className="audit-dialog">
          {event && (
            <>
              <div className="dialog-titlebar">
                <div>
                  <Dialog.Title>{t.audit.detailTitle}</Dialog.Title>
                  <Dialog.Description>{new Date(event.timestamp).toLocaleString()}</Dialog.Description>
                </div>
                <Dialog.Close asChild>
                  <button type="button" className="icon-button" aria-label={t.common.close}>
                    <X size={18} />
                  </button>
                </Dialog.Close>
              </div>

              <dl className="detail-grid">
                <div>
                  <dt>{t.audit.tool}</dt>
                  <dd>{event.tool}</dd>
                </div>
                <div>
                  <dt>{t.audit.connection}</dt>
                  <dd>{event.connection_id ?? t.common.system}</dd>
                </div>
                <div>
                  <dt>{t.audit.status}</dt>
                  <dd><StatusPill tone={statusTone(event.status)} label={statusLabel(t, event.status)} /></dd>
                </div>
                <div>
                  <dt>{t.audit.elapsedRows}</dt>
                  <dd>{formatMessage(t.audit.elapsedRowsValue, { elapsed: event.elapsed_ms ?? 0, rows: event.row_count ?? 0 })}</dd>
                </div>
              </dl>

              {event.reason && (
                <div className="detail-section">
                  <h3>{t.audit.reason}</h3>
                  <p>{event.reason}</p>
                </div>
              )}

              <div className="detail-section">
                <div className="detail-section-title">
                  <h3>SQL</h3>
                  {event.sql && (
                    <button type="button" className="button ghost" onClick={() => navigator.clipboard.writeText(event.sql ?? "")}>
                      <Clipboard size={15} />
                      {t.common.copy}
                    </button>
                  )}
                </div>
                {event.sql ? <pre>{event.sql}</pre> : <p className="muted">{t.audit.noSql}</p>}
              </div>
            </>
          )}
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
          <button type="button" className="icon-button" onClick={() => onTest(connection.id)} disabled={busy || !connection.enabled}>
            <Cable size={17} />
          </button>
        </IconTooltip>
        <IconTooltip label={t.connections.diagnose}>
          <button type="button" className="icon-button" onClick={() => onDiagnose(connection.id)} disabled={busy || !connection.enabled}>
            <SearchCheck size={17} />
          </button>
        </IconTooltip>
        <IconTooltip label={t.connections.edit}>
          <button type="button" className="icon-button" onClick={() => onEdit(connection)}>
            <MoreVertical size={17} />
          </button>
        </IconTooltip>
        <IconTooltip label={t.connections.delete}>
          <button type="button" className="icon-button danger" onClick={() => onDelete(connection.id)}>
            <Trash2 size={17} />
          </button>
        </IconTooltip>
      </div>
    </div>
  );
}

function ConnectionListItem({ t, connection, compact = false }: { t: I18nMessages; connection: ConnectionConfig; compact?: boolean }) {
  return (
    <div className={clsx("connection-item", compact && "compact")}>
      <div className={clsx("db-badge", connection.type)}>
        <img src={DATABASE_LOGOS[connection.type]} alt="" aria-hidden="true" />
      </div>
      <div className="connection-info">
        <div>
          <strong>{connection.name}</strong>
          <StatusPill tone={connection.enabled ? "green" : "slate"} label={connection.enabled ? t.connections.enabled : t.connections.paused} />
          <span className={clsx("type-tag", connection.type)}>{dbTypeLabel(connection.type)}</span>
        </div>
        <p>
          {connection.type === "sqlite"
            ? connection.database || t.connections.noDatabaseFile
            : `${connection.host}:${connection.port ?? defaultPort(connection.type)} / ${connection.username ?? "-"} / ${connection.database}`}
        </p>
      </div>
    </div>
  );
}

function EventList({ t, events, onSelect }: { t: I18nMessages; events: AuditEvent[]; onSelect?: (event: AuditEvent) => void }) {
  if (events.length === 0) {
    return <div className="empty-state compact">{t.audit.emptyCompact}</div>;
  }

  return (
    <div className="event-list">
      {events.map((event) => (
        <button type="button" className="event-item" key={event.id} onClick={() => onSelect?.(event)}>
          <span className={clsx("event-dot", statusTone(event.status))} />
          <time>{new Date(event.timestamp).toLocaleTimeString()}</time>
          <span>{event.reason ?? event.tool}</span>
          <StatusPill tone={statusTone(event.status)} label={statusLabel(t, event.status)} />
        </button>
      ))}
    </div>
  );
}

function ToastViewport({ t, toasts, onDismiss }: { t: I18nMessages; toasts: ToastMessage[]; onDismiss: (id: string) => void }) {
  if (toasts.length === 0) return null;

  return createPortal(
    <div className="toast-viewport" role="status" aria-live="polite">
      {toasts.map((toast) => (
        <div className={clsx("toast", toast.tone, toast.leaving && "leaving")} key={toast.id}>
          {toast.tone === "error" ? <AlertTriangle size={18} /> : <CheckCircle2 size={18} />}
          <span>{toast.message}</span>
          <button type="button" onClick={() => onDismiss(toast.id)} aria-label={t.common.closeNotice}>
            <X size={15} />
          </button>
        </div>
      ))}
    </div>,
    document.body
  );
}

function PanelHeader({ title, action, onAction, disabled }: { title: string; action?: ReactNode; onAction?: () => void; disabled?: boolean }) {
  return (
    <div className="panel-header">
      <h2>{title}</h2>
      {typeof action === "string" && (
        <button type="button" className="button primary" onClick={onAction} disabled={disabled}>
          <Plus size={16} />
          {action}
        </button>
      )}
      {action && typeof action !== "string" && action}
    </div>
  );
}

function PanelIconAction({ icon, label, className, onClick, disabled }: { icon: ReactNode; label: string; className?: string; onClick: () => void; disabled?: boolean }) {
  return (
    <IconTooltip label={label}>
      <button type="button" className={clsx("panel-icon-action", className)} onClick={onClick} disabled={disabled} aria-label={label}>
        {icon}
      </button>
    </IconTooltip>
  );
}

function FormSection({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="form-section">
      <h3>{title}</h3>
      <div className="form-grid">{children}</div>
    </section>
  );
}

function Field({ label, span, children }: { label: string; span?: boolean; children: ReactNode }) {
  return (
    <label className={clsx("field", span && "span-all")}>
      <span>{label}</span>
      {children}
    </label>
  );
}

function SwitchField({
  label,
  checked,
  disabled,
  onCheckedChange
}: {
  label: string;
  checked: boolean;
  disabled?: boolean;
  onCheckedChange: (checked: boolean) => void;
}) {
  return (
    <label className="switch-row">
      <span>{label}</span>
      <Switch.Root className="switch" checked={checked} disabled={disabled} onCheckedChange={onCheckedChange}>
        <Switch.Thumb className="switch-thumb" />
      </Switch.Root>
    </label>
  );
}

function IconTooltip({ label, children }: { label: string; children: ReactNode }) {
  return (
    <Tooltip.Root>
      <Tooltip.Trigger asChild>{children}</Tooltip.Trigger>
      <Tooltip.Portal>
        <Tooltip.Content className="tooltip" side="top">
          {label}
          <Tooltip.Arrow className="tooltip-arrow" />
        </Tooltip.Content>
      </Tooltip.Portal>
    </Tooltip.Root>
  );
}

function MetricIcon({ icon }: { icon: ReactNode }) {
  return <div className="metric-icon">{icon}</div>;
}

function MetricValue({ value, suffix }: { value: ReactNode; suffix?: string }) {
  return (
    <div className="metric-value">
      <strong>{value}</strong>
      {suffix && <small>{suffix}</small>}
    </div>
  );
}

function QuickStep({ image, title, text, wide, actionLabel, onAction }: { image: string; title: string; text: string; wide?: boolean; actionLabel?: string; onAction?: () => void }) {
  return (
    <div className={clsx("quick-step", wide && "wide")}>
      <img src={image} alt="" />
      <div className="quick-step-body">
        <strong>{title}</strong>
        <span>{text}</span>
      </div>
      {onAction && actionLabel && (
        <PanelIconAction icon={<Clipboard size={16} />} label={actionLabel} onClick={onAction} />
      )}
    </div>
  );
}

function StatusPill({ tone, label }: { tone: "green" | "blue" | "amber" | "red" | "slate"; label: string }) {
  return <span className={clsx("status-pill", tone)}>{label}</span>;
}

function viewTitle(t: I18nMessages, view: View) {
  switch (view) {
    case "overview":
      return t.nav.overview;
    case "connections":
      return t.nav.connections;
    case "server":
      return t.nav.server;
    case "tools":
      return t.nav.tools;
    case "audit":
      return t.nav.audit;
    case "settings":
      return t.nav.settings;
  }
}

function toolDisplayName(t: I18nMessages, name: string) {
  const names: Record<string, string> = t.tools.names;
  return names[name] ?? name;
}

function toolIntro(t: I18nMessages, tool: McpToolInfo) {
  const intros: Record<string, string> = t.tools.intros;
  return intros[tool.name] ?? tool.description;
}

function dbTypeLabel(type: DatabaseType) {
  if (type === "postgres") return "PostgreSQL";
  if (type === "mysql") return "MySQL";
  return "SQLite";
}

function defaultPort(type: DatabaseType) {
  if (type === "postgres") return 5432;
  if (type === "mysql") return 3306;
  return null;
}

function statusTone(status: AuditEvent["status"]): "green" | "blue" | "amber" | "red" | "slate" {
  if (status === "allowed") return "green";
  if (status === "denied") return "red";
  if (status === "timeout") return "amber";
  if (status === "truncated") return "blue";
  if (status === "error") return "red";
  return "slate";
}

function statusLabel(t: I18nMessages, status: AuditEvent["status"]) {
  if (status === "allowed") return t.status.allowed;
  if (status === "denied") return t.status.denied;
  if (status === "timeout") return t.status.timeout;
  if (status === "truncated") return t.status.truncated;
  return t.status.error;
}

function formatDiagnostics(t: I18nMessages, diagnostics: ConnectionDiagnostics) {
  const summary = formatMessage(t.diagnostics.summary, {
    name: diagnostics.name,
    type: dbTypeLabel(diagnostics.database_type),
    credential: credentialStateLabel(t, diagnostics.credential_state)
  });
  return diagnostics.hint ? `${summary}\n${diagnostics.hint.trim()}` : summary;
}

function formatConnectionTest(t: I18nMessages, message: string) {
  const elapsed = message.match(/(\d+)\s*ms/i)?.[1] ?? "-";
  return formatMessage(t.toast.connectionTestPassed, { elapsed });
}

function compactConnectionError(error: unknown) {
  const message = error instanceof Error ? error.message : String(error);
  const lines = message.split(/\r?\n/).map((line) => line.trim()).filter(Boolean);
  return [...new Set(lines)].join("\n") || message;
}

function credentialStateLabel(t: I18nMessages, state: string) {
  if (state === "not_required") return t.diagnostics.notRequired;
  if (state === "not_saved") return t.diagnostics.notSaved;
  if (state === "saved_empty") return t.diagnostics.savedEmpty;
  if (state === "saved") return t.diagnostics.saved;
  if (state === "missing_in_vault") return t.diagnostics.missingInVault;
  if (state === "vault_error") return t.diagnostics.vaultError;
  return state;
}

function buildAgentPrompt(t: I18nMessages, endpoint: string, requireToken: boolean, token?: string | null) {
  const datanexa: Record<string, unknown> = {
    transport: "streamable-http",
    url: endpoint
  };
  if (requireToken) {
    datanexa.headers = {
      Authorization: `Bearer ${token ?? "TOKEN"}`
    };
  }

  return [
    t.agentPrompt.intro,
    t.agentPrompt.configIntro,
    JSON.stringify({ mcpServers: { datanexa } }, null, 2)
  ].join("\n\n");
}

function relativeDuration(t: I18nMessages, timestamp: string) {
  const elapsedMs = Date.now() - new Date(timestamp).getTime();
  if (elapsedMs < 0) return t.common.justNow;
  const minutes = Math.floor(elapsedMs / 60000);
  const hours = Math.floor(minutes / 60);
  if (hours > 0) return `${hours}h ${minutes % 60}m`;
  if (minutes > 0) return `${minutes}m`;
  return t.common.justNow;
}

export default App;
