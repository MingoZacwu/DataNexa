import * as Tooltip from "@radix-ui/react-tooltip";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import clsx from "clsx";
import { AlertTriangle, Database, Filter, Home, Logs, Plus, RefreshCw, Server, Settings, Trash2, Wrench, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { FormEvent } from "react";
import brandLogoUrl from "../resources/datanexa.png";
import { detectLocale, formatMessage, messages, normalizeLocale, persistLocale } from "./i18n";
import type { Locale } from "./i18n";
import { api } from "./lib/tauri";
import { useAppUpdater } from "./lib/updater";
import type { AppSnapshot, AuditEvent, ConnectionConfig, DatabaseType, PolicyCheckResult, ServerConfig, SettingsConfig } from "./types";
import { detectThemeMode, persistThemeMode, resolveTheme, systemTheme } from "./app/theme";
import type { AuditFilters, EffectiveTheme, SettingsTab, ThemeMode, ToastMessage, ToastTone, View } from "./app/types";
import { buildAgentPrompt, compactConnectionError, formatConnectionTest, formatDiagnostics, toolDisplayName, updateScrollFade, viewTitle } from "./app/utils";
import { AuditMigrationDialog, AuditMigrationReminder, NavButton, SidebarFooter, SidebarUpdateReminder, WindowControls, WindowDragRegion } from "./components/chrome";
import { IconTooltip, ToastViewport } from "./components/ui";
import { OverviewView } from "./features/overview/OverviewView";
import { ConnectionDialog, ConnectionsView } from "./features/connections/ConnectionsView";
import { ServerView } from "./features/server/ServerView";
import { ToolsView } from "./features/tools/ToolsView";
import { AuditDetailDialog, AuditView } from "./features/audit/AuditView";
import { SettingsView } from "./features/settings/SettingsView";

type McpActivityTone = "success" | "error";
type McpToolCallCompletedPayload = { failed: boolean };

const MCP_ACTIVITY_DURATION_MS = 1750;
const MCP_ACTIVITY_REDUCED_DURATION_MS = 200;

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
  max_connections: 1,
  max_result_bytes: 1048576
});

function App() {
  const isMacos = typeof navigator !== "undefined" && /Mac|iPhone|iPad|iPod/i.test(navigator.userAgent);
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
  const [auditFilterOpen, setAuditFilterOpen] = useState(false);
  const [auditFilters, setAuditFilters] = useState<AuditFilters>({ from: "", to: "", tool: "", connection: "", status: "" });
  const [showAuditClear, setShowAuditClear] = useState(false);
  const [settingsTab, setSettingsTab] = useState<SettingsTab>("general");
  const [locale, setLocale] = useState<Locale>(detectLocale);
  const [theme, setTheme] = useState<ThemeMode>(detectThemeMode);
  const [systemThemeMode, setSystemThemeMode] = useState<EffectiveTheme>(systemTheme);
  const [dismissedUpdateVersion, setDismissedUpdateVersion] = useState<string | null>(null);
  const [migrationDialogOpen, setMigrationDialogOpen] = useState(false);
  const [migrationRecoveryBusy, setMigrationRecoveryBusy] = useState(false);
  const [mcpActivitySequence, setMcpActivitySequence] = useState(0);
  const [mcpActivityTone, setMcpActivityTone] = useState<McpActivityTone>("success");
  const [confirmClearLegacy, setConfirmClearLegacy] = useState(false);
  const effectiveTheme = resolveTheme(theme, systemThemeMode);
  const t = messages[locale];
  const hasAuditFilters = Object.values(auditFilters).some(Boolean);
  const activeViewRef = useRef(activeView);
  const mcpActivityEffectsEnabled = snapshot?.config.settings.mcp_activity_effects ?? true;
  const mcpActivityEffectsEnabledRef = useRef(mcpActivityEffectsEnabled);
  const mcpActivityPlayingRef = useRef(false);
  const mcpActivityTimerRef = useRef<number | undefined>(undefined);

  useEffect(() => {
    activeViewRef.current = activeView;
    if (activeView !== "overview") {
      mcpActivityPlayingRef.current = false;
      window.clearTimeout(mcpActivityTimerRef.current);
      mcpActivityTimerRef.current = undefined;
      setMcpActivitySequence(0);
    }
  }, [activeView]);

  useEffect(() => {
    mcpActivityEffectsEnabledRef.current = mcpActivityEffectsEnabled;
    if (!mcpActivityEffectsEnabled) {
      mcpActivityPlayingRef.current = false;
      window.clearTimeout(mcpActivityTimerRef.current);
      mcpActivityTimerRef.current = undefined;
      setMcpActivitySequence(0);
    }
  }, [mcpActivityEffectsEnabled]);

  const updater = useAppUpdater(
    snapshot?.updater_enabled ?? null,
    snapshot?.config.settings.auto_check_updates ?? false
  );
  useEffect(() => {
    void refresh();
  }, []);

  useEffect(() => {
    if (hasAuditFilters) {
      setShowAuditClear(true);
      return;
    }
    if (!showAuditClear) return;
    const timer = window.setTimeout(() => setShowAuditClear(false), 240);
    return () => window.clearTimeout(timer);
  }, [hasAuditFilters, showAuditClear]);

  useEffect(() => {
    const preventContextMenu = (event: globalThis.MouseEvent) => event.preventDefault();
    document.addEventListener("contextmenu", preventContextMenu);
    return () => document.removeEventListener("contextmenu", preventContextMenu);
  }, []);

  useEffect(() => {
    const query = window.matchMedia("(prefers-color-scheme: dark)");
    const updateSystemTheme = () => {
      if (theme === "system") {
        setSystemThemeMode(query.matches ? "dark" : "light");
      }
    };

    updateSystemTheme();
    query.addEventListener("change", updateSystemTheme);
    return () => query.removeEventListener("change", updateSystemTheme);
  }, [theme]);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    document.documentElement.classList.toggle("dark", effectiveTheme === "dark");
    persistThemeMode(theme);
    void api.setWindowMaterialTheme(theme === "system" ? null : effectiveTheme === "dark")
      .then(([nativeDark, micaEnabled]) => {
        if (theme === "system") {
          setSystemThemeMode(nativeDark ? "dark" : "light");
        }
        document.documentElement.dataset.systemMaterial = micaEnabled ? "enabled" : "fallback";
      })
      .catch(() => {
        document.documentElement.dataset.systemMaterial = "fallback";
      });
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

  const shownStartupError = useRef<string | null>(null);
  useEffect(() => {
    const error = snapshot?.startup_error;
    if (!error || shownStartupError.current === error) return;
    shownStartupError.current = error;
    pushToast(error, "error");
  }, [snapshot?.startup_error]);

  useEffect(() => {
    if (snapshot?.audit_migration.status === "ready") {
      setMigrationDialogOpen(false);
      setConfirmClearLegacy(false);
    }
  }, [snapshot?.audit_migration.status]);

  useEffect(() => {
    const timer = window.setInterval(() => {
      void refresh({ quiet: true });
    }, 15000);
    return () => window.clearInterval(timer);
  }, []);

  useEffect(() => {
    let unlistenCompleted: UnlistenFn | undefined;
    let refreshTimer: number | undefined;
    let cancelled = false;

    const triggerActivity = (failed: boolean) => {
      if (!mcpActivityEffectsEnabledRef.current
        || activeViewRef.current !== "overview"
        || mcpActivityPlayingRef.current) return;

      mcpActivityPlayingRef.current = true;
      setMcpActivityTone(failed ? "error" : "success");
      setMcpActivitySequence((sequence) => sequence + 1);
      const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
      const duration = reducedMotion ? MCP_ACTIVITY_REDUCED_DURATION_MS : MCP_ACTIVITY_DURATION_MS;
      window.clearTimeout(mcpActivityTimerRef.current);
      mcpActivityTimerRef.current = window.setTimeout(() => {
        mcpActivityPlayingRef.current = false;
        mcpActivityTimerRef.current = undefined;
      }, duration);
    };

    void (async () => {
      try {
        unlistenCompleted = await listen<McpToolCallCompletedPayload>("mcp://tool-call-completed", (event) => {
          triggerActivity(event.payload.failed);
          window.clearTimeout(refreshTimer);
          refreshTimer = window.setTimeout(() => void refresh({ quiet: true }), 120);
        });
        if (cancelled) {
          unlistenCompleted();
          unlistenCompleted = undefined;
        }
      } catch {
        // The low-frequency snapshot refresh remains available if event delivery is unavailable.
      }
    })();

    return () => {
      cancelled = true;
      unlistenCompleted?.();
      mcpActivityPlayingRef.current = false;
      window.clearTimeout(mcpActivityTimerRef.current);
      mcpActivityTimerRef.current = undefined;
      window.clearTimeout(refreshTimer);
    };
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
    const wasEmergencyDisconnected = Boolean(snapshot?.emergency_disconnect);
    setBusy(true);
    try {
      setSnapshot(await api.disableAllConnections());
      pushToast(
        wasEmergencyDisconnected
          ? t.toast.emergencyDisconnectRestored
          : t.toast.allConnectionsDisabled,
        "info"
      );
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
    if (!snapshot || snapshot.audit_migration.status !== "ready") return;
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
    if (!editing || !snapshot || snapshot.audit_migration.status !== "ready") return;
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
    if (!snapshot?.server_status.running && snapshot?.audit_migration.status !== "ready") return;
    setBusy(true);
    try {
      setSnapshot(snapshot?.server_status.running ? await api.stopServer() : await api.startServer());
    } catch (error) {
      showError(error);
      await refresh({ quiet: true });
    } finally {
      setBusy(false);
    }
  }

  async function retryAuditMigration() {
    setMigrationRecoveryBusy(true);
    try {
      setSnapshot(await api.retryAuditMigration());
    } catch (error) {
      showError(error);
      await refresh({ quiet: true });
    } finally {
      setMigrationRecoveryBusy(false);
    }
  }

  async function clearLegacyAuditLog() {
    setMigrationRecoveryBusy(true);
    try {
      setSnapshot(await api.clearLegacyAuditLog());
    } catch (error) {
      showError(error);
      await refresh({ quiet: true });
    } finally {
      setMigrationRecoveryBusy(false);
      setConfirmClearLegacy(false);
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

  async function saveServer(server: ServerConfig): Promise<boolean> {
    setBusy(true);
    try {
      setSnapshot(await api.saveServerConfig(server));
      pushToast(t.toast.serverSaved, "info");
      return true;
    } catch (error) {
      showError(error);
      await refresh({ quiet: true });
      return false;
    } finally {
      setBusy(false);
    }
  }

  async function saveSettings(settings: SettingsConfig, applyAutoStart = false) {
    setBusy(true);
    try {
      const nextLocale = normalizeLocale(settings.language);
      setLocale(nextLocale);
      persistLocale(nextLocale);
      const nextSnapshot = await api.saveSettingsConfig({ ...settings, language: nextLocale }, applyAutoStart);
      setSnapshot(nextSnapshot);
      if (applyAutoStart && settings.auto_start_mcp && nextSnapshot.auto_start_status === "requires_approval") {
        pushToast(messages[nextLocale].toast.autoStartRequiresApproval, "error");
      } else {
        pushToast(messages[nextLocale].toast.settingsSaved, "info");
      }
    } catch (error) {
      showError(error);
    } finally {
      setBusy(false);
    }
  }

  async function exportConnections() {
    setBusy(true);
    try {
      const exportedCount = await api.exportConnections(locale);
      if (exportedCount !== null) {
        pushToast(formatMessage(t.toast.connectionsExported, { count: exportedCount }), "info");
      }
    } catch (error) {
      showError(error);
    } finally {
      setBusy(false);
    }
  }

  async function importConnections() {
    setBusy(true);
    try {
      const result = await api.importConnections(locale);
      if (result) {
        setSnapshot(result.snapshot);
        pushToast(formatMessage(t.toast.connectionsImported, { count: result.imported_count }));
      }
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
  const availableUpdateVersion = updater.state.kind === "available" ? updater.state.version : null;
  const showUpdateReminder = availableUpdateVersion !== null && dismissedUpdateVersion !== availableUpdateVersion;
  const migrationReady = snapshot?.audit_migration.status === "ready";
  const emergencyDisconnect = Boolean(snapshot?.emergency_disconnect);

  return (
    <Tooltip.Provider delayDuration={180}>
      <div className="app-shell">
        <div className="ambient-grid" aria-hidden="true" />
        <WindowDragRegion />
        {!isMacos && <WindowControls t={t} />}

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

            <div className="sidebar-bottom">
              {snapshot && snapshot.audit_migration.status !== "ready" ? (
                <AuditMigrationReminder t={t} state={snapshot.audit_migration} onOpen={() => setMigrationDialogOpen(true)} />
              ) : showUpdateReminder && availableUpdateVersion ? (
                <SidebarUpdateReminder
                  t={t}
                  version={availableUpdateVersion}
                  onOpenAbout={() => {
                    setActiveView("settings");
                    setSettingsTab("about");
                  }}
                  onDismiss={() => setDismissedUpdateVersion(availableUpdateVersion)}
                />
              ) : null}
              <SidebarFooter
                t={t}
                running={Boolean(snapshot?.server_status.running)}
                startupFailed={Boolean(snapshot?.startup_error)}
                port={snapshot?.config.server.port ?? 17321}
                busy={busy}
                disabled={!snapshot || (!snapshot.server_status.running && !migrationReady)}
                onToggle={toggleServer}
              />
            </div>
          </aside>

          <main className="workspace">
            <header className="topbar">
              <div className="page-title-block">
                <h1>{viewTitle(t, activeView)}</h1>
              </div>
              <div className="top-actions">
                {snapshot && activeView === "audit" && (
                  <div className={clsx("top-filter-actions", hasAuditFilters && "active", !hasAuditFilters && showAuditClear && "leaving")}>
                    <IconTooltip label={t.audit.filter}>
                      <button type="button" className={clsx("icon-button", hasAuditFilters && "active-filter")} onClick={() => setAuditFilterOpen(true)} aria-label={t.audit.filter}>
                        <Filter size={17} />
                      </button>
                    </IconTooltip>
                    {showAuditClear && (
                      <span className={clsx("audit-clear-action", !hasAuditFilters && "leaving")}>
                        <IconTooltip label={t.audit.clearFilter}>
                          <button type="button" className="icon-button" onClick={() => setAuditFilters({ from: "", to: "", tool: "", connection: "", status: "" })} aria-label={t.audit.clearFilter}>
                            <X size={17} />
                          </button>
                        </IconTooltip>
                      </span>
                    )}
                  </div>
                )}
                <div className={clsx("top-icon-actions", activeView === "connections" && emergencyDisconnect && "emergency-active")}>
                  {snapshot && activeView === "connections" && (
                    <IconTooltip label={emergencyDisconnect ? t.connections.emergencyRestore : t.connections.emergencyDisable}>
                      <button
                        type="button"
                        className={clsx("icon-button danger", emergencyDisconnect && "emergency-toggle-active")}
                        onClick={disableAllConnections}
                        disabled={busy || !snapshot.server_status.running}
                        aria-pressed={emergencyDisconnect}
                        aria-label={emergencyDisconnect ? t.connections.emergencyRestore : t.connections.emergencyDisable}
                      >
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
                  <button type="button" className="button primary" onClick={openNewConnection} disabled={busy || emergencyDisconnect}>
                    <Plus size={16} />
                    {t.overview.newConnection}
                  </button>
                )}
              </div>
            </header>

            {!snapshot ? (
              <div className="loading-panel">{t.overview.loading}</div>
            ) : (
              <div className={clsx("view-stage", `view-${activeView}`)} key={activeView} onScroll={updateScrollFade}>
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
                    onToggleEmergency={disableAllConnections}
                    busy={busy}
                    startDisabled={!snapshot.server_status.running && !migrationReady}
                    mcpActivitySequence={mcpActivitySequence}
                    mcpActivityTone={mcpActivityTone}
                  />
                )}
                {activeView === "connections" && (
                  <ConnectionsView
                    t={t}
                    connections={connections}
                    busy={busy || emergencyDisconnect}
                    onEdit={openExistingConnection}
                    onDelete={deleteConnection}
                    onTest={testConnection}
                    onDiagnose={diagnoseConnection}
                    onToggleEnabled={setConnectionEnabled}
                    migrationReady={migrationReady}
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
                    startDisabled={!snapshot.server_status.running && !migrationReady}
                    onRotate={rotateToken}
                  />
                )}
                {activeView === "tools" && <ToolsView t={t} tools={snapshot.tools} busy={busy} onToggle={setToolEnabled} />}
                {activeView === "audit" && <AuditView t={t} events={snapshot.audit_events} tools={snapshot.tools} connections={snapshot.config.connections} filters={auditFilters} onFiltersChange={setAuditFilters} filterOpen={auditFilterOpen} onFilterOpenChange={setAuditFilterOpen} onSelect={setSelectedAudit} />}
                {activeView === "settings" && (
                  <SettingsView
                    t={t}
                    locale={locale}
                    theme={theme}
                    effectiveTheme={effectiveTheme}
                    server={snapshot.config.server}
                    settings={snapshot.config.settings}
                    autoStartStatus={snapshot.auto_start_status}
                    busy={busy}
                    tab={settingsTab}
                    policySql={policySql}
                    policyKind={policyKind}
                    policyResult={policyResult}
                    updaterEnabled={snapshot.updater_enabled}
                    updateState={updater.state}
                    onCheckUpdate={() => void updater.checkForUpdates()}
                    onUpdate={() => void updater.installUpdate()}
                    onOpenProjectReleases={() => void api.openProjectReleases().catch(showError)}
                    onTabChange={setSettingsTab}
                    onThemeChange={setTheme}
                    onPolicyKindChange={setPolicyKind}
                    onSqlChange={setPolicySql}
                    onPolicyCheck={runPolicyCheck}
                    onSaveServer={saveServer}
                    onSaveSettings={saveSettings}
                    onExportConnections={() => void exportConnections()}
                    onImportConnections={() => void importConnections()}
                    onOpenProjectHomepage={() => void api.openProjectHomepage().catch(showError)}
                    onOpenProjectSite={() => void api.openProjectSite().catch(showError)}
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
          migrationReady={migrationReady}
          onSubmit={saveConnection}
          onClose={() => setEditing(null)}
        />
        <AuditDetailDialog t={t} event={selectedAudit} onClose={() => setSelectedAudit(null)} />
        {snapshot && snapshot.audit_migration.status === "failed" && (
          <AuditMigrationDialog
            t={t}
            state={snapshot.audit_migration}
            open={migrationDialogOpen}
            busy={migrationRecoveryBusy}
            confirmClear={confirmClearLegacy}
            onOpenChange={setMigrationDialogOpen}
            onRetry={() => void retryAuditMigration()}
            onRequestClear={() => setConfirmClearLegacy(true)}
            onCancelClear={() => setConfirmClearLegacy(false)}
            onConfirmClear={() => void clearLegacyAuditLog()}
          />
        )}
      </div>
    </Tooltip.Provider>
  );
}


export default App;
