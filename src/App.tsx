import * as Tooltip from "@radix-ui/react-tooltip";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import clsx from "clsx";
import { AlertTriangle, Coffee, Database, Filter, Home, Logs, Plus, RefreshCw, Settings, ShieldCheck, Trash2, Wrench, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { FormEvent } from "react";
import brandLogoUrl from "../resources/datanexa.png";
import { detectLocale, formatList, formatMessage, messages, normalizeLocale, persistLocale } from "./i18n";
import type { Locale } from "./i18n";
import { api, WRONG_PASSWORD_ERROR } from "./lib/tauri";
import { useAppUpdater } from "./lib/updater";
import type { AppSnapshot, AuditEvent, ConnectionConfig, DatabaseType, ImportJdbcDriverInput, InstallJdbcDriverInput, JdbcCacheSelection, JdbcInstallProgress, JdbcRuntimeInstallProgress, JdbcStatus, JdbcStorageStatus, PolicyCheckResult, ServerConfig, SettingsConfig } from "./types";
import { detectThemeMode, persistThemeMode, resolveTheme, systemTheme } from "./app/theme";
import { hasSeenBackgroundRunTip, markBackgroundRunTip } from "./app/tips";
import type { AuditFilters, EffectiveTheme, SettingsTab, ThemeMode, ToastMessage, ToastTone, View } from "./app/types";
import { buildAgentPrompt, compactConnectionError, formatConnectionTest, formatDiagnostics, toolDisplayName, updateScrollFade, useScrollFade, viewTitle } from "./app/utils";
import { AuditMigrationDialog, AuditMigrationReminder, LanguageSwitcher, NavButton, SidebarFooter, SidebarUpdateReminder, WindowControls, WindowDragRegion } from "./components/chrome";
import { IconTooltip, ToastViewport } from "./components/ui";
import { OverviewView } from "./features/overview/OverviewView";
import { ConnectionDialog, ConnectionsView } from "./features/connections/ConnectionsView";
import { AccessControlView, PromptTokenDialog } from "./features/access/AccessControlView";
import { ToolsView } from "./features/tools/ToolsView";
import { AuditDetailDialog, AuditView } from "./features/audit/AuditView";
import { SettingsView } from "./features/settings/SettingsView";
import { EncryptionPasswordDialog } from "./features/settings/EncryptionPasswordDialog";

type McpActivityTone = "success" | "error";
type McpToolCallCompletedPayload = { failed: boolean };
type JreUpdateAvailablePayload = { version: string };

// Let a start/stop transition settle before the tip covers the hero, so the
// status card does not change the moment the user clicks the toggle.
const BACKGROUND_TIP_DELAY_MS = 900;
const BACKGROUND_TIP_LEAVE_MS = 280;
const BACKGROUND_TIP_SUCCESS_MS = 1800;

// A scope names the controls that must be disabled while a request runs. Choose
// the scope that owns the buttons being acted on, not the command being called:
// unrelated views have to stay usable while an operation is in flight.
type BusyScope = "refresh" | "server" | "connections" | "access" | "tools" | "settings" | "jdbc" | "jreCheck" | "audit";

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
  jdbc_bundle_id: null,
  jdbc_url: null,
  jdbc_driver_class: null,
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
  // Counts in-flight requests per scope so overlapping ones keep that scope busy
  // until the last of them settles.
  const [busyScopes, setBusyScopes] = useState<ReadonlyMap<BusyScope, number>>(() => new Map());
  const [toasts, setToasts] = useState<ToastMessage[]>([]);
  const [policySql, setPolicySql] = useState("SELECT * FROM users");
  const [policyKind, setPolicyKind] = useState<DatabaseType>("mysql");
  const [policyResult, setPolicyResult] = useState<PolicyCheckResult | null>(null);
  const [selectedAudit, setSelectedAudit] = useState<AuditEvent | null>(null);
  const [auditFilterOpen, setAuditFilterOpen] = useState(false);
  const [auditFilters, setAuditFilters] = useState<AuditFilters>({ from: "", to: "", tool: "", connection: "", status: "", token: "" });
  const [selectedTokenId, setSelectedTokenId] = useState("");
  const [createTokenRequest, setCreateTokenRequest] = useState(0);
  const [promptTokenDialogOpen, setPromptTokenDialogOpen] = useState(false);
  const [importRequest, setImportRequest] = useState<{ path: string; exportedAt: string | null } | null>(null);
  const [importError, setImportError] = useState<string | null>(null);
  const [showAuditClear, setShowAuditClear] = useState(false);
  const [settingsTab, setSettingsTab] = useState<SettingsTab>("general");
  const [jdbcStatus, setJdbcStatus] = useState<JdbcStatus | null>(null);
  const [jdbcStorageStatus, setJdbcStorageStatus] = useState<JdbcStorageStatus | null>(null);
  const [jdbcInstallProgress, setJdbcInstallProgress] = useState<JdbcInstallProgress | null>(null);
  const [jdbcRuntimeProgress, setJdbcRuntimeProgress] = useState<JdbcRuntimeInstallProgress | null>(null);
  const [locale, setLocale] = useState<Locale>(detectLocale);
  const [theme, setTheme] = useState<ThemeMode>(detectThemeMode);
  const [systemThemeMode, setSystemThemeMode] = useState<EffectiveTheme>(systemTheme);
  const [dismissedUpdateVersion, setDismissedUpdateVersion] = useState<string | null>(null);
  const [jreUpdateVersion, setJreUpdateVersion] = useState<string | null>(null);
  const [dismissedJreUpdateVersion, setDismissedJreUpdateVersion] = useState<string | null>(null);
  const [migrationDialogOpen, setMigrationDialogOpen] = useState(false);
  const [migrationRecoveryBusy, setMigrationRecoveryBusy] = useState(false);
  const [mcpActivitySequence, setMcpActivitySequence] = useState(0);
  const [mcpActivityTone, setMcpActivityTone] = useState<McpActivityTone>("success");
  const [backgroundTipVisible, setBackgroundTipVisible] = useState(false);
  const [backgroundTipLeaving, setBackgroundTipLeaving] = useState(false);
  const [backgroundTipSuccess, setBackgroundTipSuccess] = useState(false);
  const [windowVisible, setWindowVisible] = useState(() => document.visibilityState === "visible");
  const [confirmClearLegacy, setConfirmClearLegacy] = useState(false);
  const effectiveTheme = resolveTheme(theme, systemThemeMode);
  const t = messages[locale];
  const hasAuditFilters = Object.values(auditFilters).some(Boolean);
  const activeViewRef = useRef(activeView);
  const backgroundTipShownRef = useRef(false);
  const serverRunning = snapshot?.server_status.running ?? false;
  const autoStartStatus = snapshot?.auto_start_status;
  const mcpActivityEffectsEnabled = snapshot?.config.settings.mcp_activity_effects ?? true;
  const mcpActivityEffectsEnabledRef = useRef(mcpActivityEffectsEnabled);
  const mcpActivityPlayingRef = useRef(false);
  const mcpActivityTimerRef = useRef<number | undefined>(undefined);
  const stageScrollFadeRef = useScrollFade();

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

  useEffect(() => {
    const handleVisibility = () => setWindowVisible(document.visibilityState === "visible");
    document.addEventListener("visibilitychange", handleVisibility);
    return () => document.removeEventListener("visibilitychange", handleVisibility);
  }, []);

  useEffect(() => {
    // Tell users once, when keeping the MCP service available becomes relevant:
    // the server is running on the Overview and launch-at-login is not set yet.
    // The window check matters on login launches, where the webview keeps
    // running while hidden in the tray.
    if (activeView !== "overview" || !windowVisible) return;
    if (!serverRunning) return;
    if (autoStartStatus === "enabled" || autoStartStatus === "requires_approval") return;
    if (backgroundTipShownRef.current || hasSeenBackgroundRunTip()) return;
    const timer = window.setTimeout(() => {
      backgroundTipShownRef.current = true;
      markBackgroundRunTip("shown");
      setBackgroundTipSuccess(false);
      setBackgroundTipVisible(true);
    }, BACKGROUND_TIP_DELAY_MS);
    return () => window.clearTimeout(timer);
  }, [serverRunning, autoStartStatus, activeView, windowVisible]);

  const updater = useAppUpdater(
    snapshot?.updater_enabled ?? null,
    snapshot?.config.settings.auto_check_updates ?? false
  );
  useEffect(() => {
    void refresh();
    void refreshJdbcStatus();
  }, []);

  useEffect(() => {
    if (snapshot?.config.settings.auto_check_updates !== true) return;

    const checkJreUpdateIfDue = () => {
      void api.checkJdbcRuntimeUpdateIfDue()
        .then((version) => {
          setJreUpdateVersion(version);
          if (version) void refreshJdbcStatus();
        })
        .catch(() => undefined);
    };

    checkJreUpdateIfDue();
    const handleVisibilityChange = () => {
      if (document.visibilityState === "visible") checkJreUpdateIfDue();
    };
    document.addEventListener("visibilitychange", handleVisibilityChange);
    return () => document.removeEventListener("visibilitychange", handleVisibilityChange);
  }, [snapshot?.config.settings.auto_check_updates]);

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    let cancelled = false;
    void (async () => {
      try {
        unlisten = await listen<JdbcRuntimeInstallProgress>("jdbc://runtime-install-progress", (event) => {
          setJdbcRuntimeProgress(event.payload);
        });
        if (cancelled) {
          unlisten();
          unlisten = undefined;
        }
      } catch {
        // Preview mode and older runtimes may not provide event delivery.
      }
    })();
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    let cancelled = false;
    void (async () => {
      try {
        unlisten = await listen<JdbcInstallProgress>("jdbc://driver-install-progress", (event) => {
          setJdbcInstallProgress(event.payload);
        });
        if (cancelled) {
          unlisten();
          unlisten = undefined;
        }
      } catch {
        // Preview mode and older runtimes may not provide event delivery.
      }
    })();
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    let cancelled = false;
    void (async () => {
      try {
        unlisten = await listen<JreUpdateAvailablePayload>("jdbc://runtime-update-available", (event) => {
          setJreUpdateVersion(event.payload.version);
          void refreshJdbcStatus();
        });
        if (cancelled) {
          unlisten();
          unlisten = undefined;
        }
      } catch {
        // Event delivery is best-effort; the runtime settings can still check manually.
      }
    })();
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [t]);

  useEffect(() => {
    if (activeView === "settings" && (settingsTab === "drivers" || settingsTab === "storage")) {
      void refreshJdbcStatus();
      if (settingsTab === "storage") void refreshJdbcStorageStatus();
    }
  }, [activeView, settingsTab]);

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
    // An empty setting means the user has never chosen a language, so the
    // system-detected locale stays in effect.
    if (!configuredLanguage) return;
    const nextLocale = normalizeLocale(configuredLanguage);
    setLocale((current) => (current === nextLocale ? current : nextLocale));
    persistLocale(nextLocale);
  }, [snapshot?.config.settings.language]);

  useEffect(() => {
    void refresh({ quiet: true });
  }, [activeView]);

  const shownStartupError = useRef<string | null>(null);
  const jdbcStorageRefreshRef = useRef(false);
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

  function beginBusy(scope: BusyScope) {
    setBusyScopes((current) => {
      const next = new Map(current);
      next.set(scope, (next.get(scope) ?? 0) + 1);
      return next;
    });
  }

  function endBusy(scope: BusyScope) {
    setBusyScopes((current) => {
      const count = current.get(scope) ?? 0;
      if (count === 0) return current;
      const next = new Map(current);
      if (count === 1) {
        next.delete(scope);
      } else {
        next.set(scope, count - 1);
      }
      return next;
    });
  }

  function isBusy(...scopes: BusyScope[]) {
    return scopes.some((scope) => (busyScopes.get(scope) ?? 0) > 0);
  }

  function applySnapshot(next: AppSnapshot) {
    setSnapshot((current) => current && current.version >= next.version ? current : next);
  }

  async function refresh(options: { quiet?: boolean } = {}) {
    if (!options.quiet) beginBusy("refresh");
    try {
      applySnapshot(await api.snapshot());
    } catch (error) {
      showError(error);
    } finally {
      if (!options.quiet) endBusy("refresh");
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
    beginBusy("connections");
    try {
      const next = await api.upsertConnection({
        connection: editing,
        password: password.length > 0 ? password : null,
        clear_password: clearPassword
      });
      applySnapshot(next);
      setEditing(null);
      setPassword("");
      setClearPassword(false);
      pushToast(t.toast.connectionSaved);
    } catch (error) {
      showError(error);
    } finally {
      endBusy("connections");
    }
  }

  async function copyConnectionName(name: string) {
    try {
      await navigator.clipboard.writeText(name);
      pushToast(t.connections.nameCopied, "info");
    } catch (error) {
      showError(error);
    }
  }

  async function deleteConnection(id: string) {
    beginBusy("connections");
    try {
      applySnapshot(await api.deleteConnection(id));
      pushToast(t.toast.connectionDeleted);
    } catch (error) {
      showError(error);
    } finally {
      endBusy("connections");
    }
  }

  async function setConnectionEnabled(id: string, enabled: boolean) {
    beginBusy("connections");
    try {
      const connectionName = snapshot?.config.connections.find((connection) => connection.id === id)?.name ?? id;
      applySnapshot(await api.setConnectionEnabled(id, enabled));
      pushToast(
        formatMessage(enabled ? t.toast.connectionEnabled : t.toast.connectionDisabled, {
          connection: connectionName
        }),
        "info"
      );
    } catch (error) {
      showError(error);
    } finally {
      endBusy("connections");
    }
  }

  async function disableAllConnections() {
    const wasEmergencyDisconnected = Boolean(snapshot?.emergency_disconnect);
    beginBusy("connections");
    try {
      applySnapshot(await api.disableAllConnections());
      pushToast(
        wasEmergencyDisconnected
          ? t.toast.emergencyDisconnectRestored
          : t.toast.allConnectionsDisabled,
        "info"
      );
    } catch (error) {
      showError(error);
    } finally {
      endBusy("connections");
    }
  }

  async function clearAuditEvents() {
    beginBusy("audit");
    try {
      applySnapshot(await api.clearAuditEvents());
      setSelectedAudit(null);
      pushToast(t.toast.auditCleared, "info");
    } catch (error) {
      showError(error);
    } finally {
      endBusy("audit");
    }
  }

  async function testConnection(id: string) {
    if (!snapshot || snapshot.audit_migration.status !== "ready") return;
    beginBusy("connections");
    try {
      pushToast(formatConnectionTest(t, await api.testConnection(id)), "info");
      await refresh({ quiet: true });
    } catch (error) {
      pushToast(compactConnectionError(error), "error");
    } finally {
      endBusy("connections");
    }
  }

  async function testEditingConnection() {
    if (!editing || !snapshot || snapshot.audit_migration.status !== "ready") return;
    beginBusy("connections");
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
      endBusy("connections");
    }
  }

  async function diagnoseConnection(id: string) {
    beginBusy("connections");
    try {
      pushToast(formatDiagnostics(t, await api.diagnoseConnection(id)), "info");
      await refresh({ quiet: true });
    } catch (error) {
      pushToast(compactConnectionError(error), "error");
    } finally {
      endBusy("connections");
    }
  }

  async function toggleServer() {
    if (!snapshot?.server_status.running && snapshot?.audit_migration.status !== "ready") return;
    beginBusy("server");
    try {
      applySnapshot(snapshot?.server_status.running ? await api.stopServer() : await api.startServer());
    } catch (error) {
      showError(error);
      await refresh({ quiet: true });
    } finally {
      endBusy("server");
    }
  }

  async function retryAuditMigration() {
    setMigrationRecoveryBusy(true);
    try {
      applySnapshot(await api.retryAuditMigration());
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
      applySnapshot(await api.clearLegacyAuditLog());
    } catch (error) {
      showError(error);
      await refresh({ quiet: true });
    } finally {
      setMigrationRecoveryBusy(false);
      setConfirmClearLegacy(false);
    }
  }

  async function createAccessToken(name: string): Promise<boolean> {
    beginBusy("access");
    try {
      const result = await api.createAccessToken(name);
      setSelectedTokenId(result.token_id);
      await refresh({ quiet: true });
      try {
        await navigator.clipboard.writeText(result.secret);
        pushToast(t.access.tokenCreated);
      } catch (error) {
        showError(error);
      }
      return true;
    } catch (error) {
      showError(error);
      return false;
    } finally {
      endBusy("access");
    }
  }

  async function refreshJdbcStatus() {
    try {
      setJdbcStatus(await api.jdbcStatus());
    } catch (error) {
      showError(error);
    }
  }

  async function installJdbcRuntime(): Promise<boolean> {
    beginBusy("jdbc");
    setJdbcRuntimeProgress({ phase: "preparing", downloaded_bytes: 0, total_bytes: null, progress: 0 });
    try {
      await api.installJdbcRuntime();
      await refreshJdbcStatus();
      setJreUpdateVersion(null);
      pushToast(t.toast.jdbcRuntimeInstalled, "info");
      return true;
    } catch (error) {
      showError(error);
      return false;
    } finally {
      setJdbcRuntimeProgress(null);
      endBusy("jdbc");
    }
  }

  async function removeJdbcRuntime(): Promise<boolean> {
    beginBusy("jdbc");
    try {
      await api.removeJdbcRuntime();
      await refreshJdbcStatus();
      await refreshJdbcStorageStatus();
      setJreUpdateVersion(null);
      pushToast(t.toast.jdbcRuntimeRemoved, "info");
      return true;
    } catch (error) {
      showError(error);
      return false;
    } finally {
      endBusy("jdbc");
    }
  }

  async function checkJdbcRuntimeUpdate(): Promise<string | null> {
    // A runtime update check only reads local metadata and fetches a remote
    // manifest, so it gets its own scope: it must not disable driver installs,
    // storage housekeeping or anything else.
    beginBusy("jreCheck");
    try {
      const version = await api.checkJdbcRuntimeUpdate();
      setJreUpdateVersion(version);
      if (version) {
        pushToast(formatMessage(t.toast.jdbcRuntimeUpdateAvailable, { version }), "info");
      } else if (jdbcStatus?.runtime.source === "managed") {
        pushToast(t.toast.jdbcRuntimeUpToDate, "info");
      }
      await refreshJdbcStatus();
      return version;
    } catch (error) {
      showError(error);
      return null;
    } finally {
      endBusy("jreCheck");
    }
  }

  async function checkAllUpdates() {
    await updater.checkForUpdates();
    await checkJdbcRuntimeUpdate();
  }

  async function refreshJdbcStorageStatus() {
    // A slow JDBC operation makes the backend slow to answer too. Skipping the
    // overlapping polls keeps them from queueing up behind it.
    if (jdbcStorageRefreshRef.current) return;
    jdbcStorageRefreshRef.current = true;
    try {
      setJdbcStorageStatus(await api.jdbcStorageStatus());
    } catch (error) {
      showError(error);
    } finally {
      jdbcStorageRefreshRef.current = false;
    }
  }

  async function clearJdbcCache(selection: JdbcCacheSelection): Promise<boolean> {
    beginBusy("jdbc");
    try {
      await api.clearJdbcCache(selection);
      await refreshJdbcStorageStatus();
      const clearedItems = formatList(locale, [
        selection.maven ? t.settings.clearMavenCacheOption : null,
        selection.old_runtimes ? t.settings.clearOldRuntimeOption : null,
        selection.debug_logs ? t.settings.clearDebugLogsOption : null
      ].filter((label): label is string => label !== null));
      pushToast(formatMessage(t.toast.cacheCleared, { items: clearedItems }), "info");
      return true;
    } catch (error) {
      showError(error);
      return false;
    } finally {
      endBusy("jdbc");
    }
  }

  async function stopJdbcDriverRuntime(bundleId: string): Promise<boolean> {
    beginBusy("jdbc");
    try {
      await api.stopJdbcDriverRuntime(bundleId);
      await refreshJdbcStorageStatus();
      return true;
    } catch (error) {
      showError(error);
      return false;
    } finally {
      endBusy("jdbc");
    }
  }

  async function installJdbcDriver(input: InstallJdbcDriverInput): Promise<boolean> {
    beginBusy("jdbc");
    setJdbcInstallProgress({ operation: "install", phase: "preparing", progress: 0 });
    try {
      await api.installJdbcDriver(input);
      setJdbcStatus(await api.jdbcStatus());
      pushToast(t.toast.jdbcDriverInstalled);
      return true;
    } catch (error) {
      showError(error);
      return false;
    } finally {
      endBusy("jdbc");
      setJdbcInstallProgress(null);
    }
  }

  async function importJdbcDriver(input: ImportJdbcDriverInput): Promise<boolean> {
    beginBusy("jdbc");
    setJdbcInstallProgress({ operation: "import", phase: "preparing", progress: 0 });
    try {
      await api.importJdbcDriver(input);
      setJdbcStatus(await api.jdbcStatus());
      pushToast(t.toast.jdbcDriverInstalled);
      return true;
    } catch (error) {
      showError(error);
      return false;
    } finally {
      endBusy("jdbc");
      setJdbcInstallProgress(null);
    }
  }

  async function deleteJdbcDriver(bundleId: string) {
    beginBusy("jdbc");
    try {
      setJdbcStatus(await api.deleteJdbcDriver(bundleId));
      pushToast(t.toast.jdbcDriverDeleted);
    } catch (error) {
      showError(error);
    } finally {
      endBusy("jdbc");
    }
  }

  async function updateAccess(action: () => Promise<AppSnapshot>, message: string): Promise<boolean> {
    beginBusy("access");
    try {
      applySnapshot(await action());
      pushToast(message, "info");
      return true;
    } catch (error) {
      showError(error);
      return false;
    } finally {
      endBusy("access");
    }
  }

  async function renameAccessToken(id: string, name: string) {
    return updateAccess(() => api.renameAccessToken(id, name), t.access.tokenRenamed);
  }

  async function rotateAccessToken(id: string): Promise<boolean> {
    beginBusy("access");
    try {
      const result = await api.rotateAccessToken(id);
      await refresh({ quiet: true });
      try {
        await navigator.clipboard.writeText(result.secret);
        pushToast(t.access.tokenRotated);
      } catch (error) {
        showError(error);
      }
      return true;
    } catch (error) {
      showError(error);
      return false;
    } finally {
      endBusy("access");
    }
  }

  async function deleteAccessToken(id: string) {
    return updateAccess(() => api.deleteAccessToken(id), t.access.tokenDeleted);
  }

  // Enabling authentication rewrites the server config, so the access view waits
  // for it too even though the save itself belongs to the settings scope.
  async function enableTokenAuthentication() {
    if (!snapshot) return;
    beginBusy("access");
    try {
      await saveServer({ ...snapshot.config.server, require_token: true });
    } finally {
      endBusy("access");
    }
  }
  async function copyPromptForToken(id: string) { try { const result = await api.getAccessTokenSecret(id); await navigator.clipboard.writeText(buildAgentPrompt(t, serverEndpoint, true, result.secret)); setPromptTokenDialogOpen(false); pushToast(t.toast.agentCopied, "info"); } catch (error) { showError(error); } }

  async function saveServer(server: ServerConfig): Promise<boolean> {
    beginBusy("settings");
    try {
      applySnapshot(await api.saveServerConfig(server));
      pushToast(t.toast.serverSaved, "info");
      return true;
    } catch (error) {
      showError(error);
      await refresh({ quiet: true });
      return false;
    } finally {
      endBusy("settings");
    }
  }

  async function changeLocale(next: Locale) {
    if (next === locale) return;
    // Switch the interface immediately, then persist; a failed save keeps the
    // local choice so the user is never stuck in an unreadable language.
    setLocale(next);
    persistLocale(next);
    const settings = snapshot?.config.settings;
    if (!settings) return;
    beginBusy("settings");
    try {
      const nextSnapshot = await api.saveSettingsConfig({ ...settings, language: next }, false);
      applySnapshot(nextSnapshot);
    } catch (error) {
      showError(error);
    } finally {
      endBusy("settings");
    }
  }

  async function saveSettings(settings: SettingsConfig, applyAutoStart = false) {
    beginBusy("settings");
    try {
      const nextLocale = normalizeLocale(settings.language);
      setLocale(nextLocale);
      persistLocale(nextLocale);
      const nextSnapshot = await api.saveSettingsConfig({ ...settings, language: nextLocale }, applyAutoStart);
      applySnapshot(nextSnapshot);
      if (applyAutoStart && settings.auto_start_mcp && nextSnapshot.auto_start_status === "requires_approval") {
        pushToast(messages[nextLocale].toast.autoStartRequiresApproval, "error");
      }
    } catch (error) {
      showError(error);
    } finally {
      endBusy("settings");
    }
  }

  function hideBackgroundTip() {
    setBackgroundTipLeaving(true);
    window.setTimeout(() => {
      setBackgroundTipVisible(false);
      setBackgroundTipLeaving(false);
      setBackgroundTipSuccess(false);
    }, BACKGROUND_TIP_LEAVE_MS);
  }

  function dismissBackgroundTip() {
    hideBackgroundTip();
  }

  async function enableAutoStartFromTip() {
    if (!snapshot || backgroundTipSuccess) return;
    setBackgroundTipSuccess(true);
    try {
      const nextSnapshot = await api.saveSettingsConfig({ ...snapshot.config.settings, auto_start_mcp: true }, true);
      applySnapshot(nextSnapshot);
      markBackgroundRunTip("enabled");
      if (nextSnapshot.auto_start_status === "requires_approval") {
        pushToast(messages[locale].toast.autoStartRequiresApproval, "error");
      }
      window.setTimeout(() => hideBackgroundTip(), BACKGROUND_TIP_SUCCESS_MS);
    } catch (error) {
      setBackgroundTipSuccess(false);
      showError(error);
    }
  }

  async function exportConnections(password: string | null) {
    // Pick the destination before taking the scope: the native save dialog stays
    // open until the user decides, and holding the scope across it would leave
    // every settings control disabled while they browse.
    let path: string | null;
    try {
      path = await api.pickConnectionExportPath(locale, password !== null);
    } catch (error) {
      showError(error);
      return;
    }
    if (!path) return;
    beginBusy("settings");
    try {
      const exportedCount = await api.exportConnections(path, password);
      pushToast(formatMessage(
        password === null ? t.toast.connectionsExported : t.toast.connectionsExportedEncrypted,
        { count: exportedCount }
      ), "info");
    } catch (error) {
      showError(error);
    } finally {
      endBusy("settings");
    }
  }

  async function applyConnectionsImport(path: string, password: string | null) {
    const result = await api.importConnections(path, password);
    applySnapshot(result.snapshot);
    pushToast(formatMessage(
      result.skipped_count > 0 ? t.toast.connectionsImportedPartial : t.toast.connectionsImported,
      { count: result.imported_count, skipped: result.skipped_count }
    ));
  }

  async function importConnections() {
    // Same ordering rationale as export: the picker runs before the scope is held.
    let path: string | null;
    try {
      path = await api.pickConnectionImportFile(locale);
    } catch (error) {
      showError(error);
      return;
    }
    if (!path) return;
    beginBusy("settings");
    try {
      const inspection = await api.inspectConnectionFile(path);
      if (!inspection.encrypted) {
        await applyConnectionsImport(path, null);
        return;
      }
      // Ask for the password before importing anything. The import command reads the file
      // again once the password is known.
      setImportError(null);
      setImportRequest({ path, exportedAt: inspection.exported_at });
    } catch (error) {
      showError(error);
    } finally {
      endBusy("settings");
    }
  }

  async function submitImportPassword(password: string) {
    if (!importRequest) return;
    beginBusy("settings");
    try {
      await applyConnectionsImport(importRequest.path, password);
      setImportRequest(null);
      setImportError(null);
    } catch (error) {
      // Keep the dialog open: retyping the password is the expected fix here.
      const message = error instanceof Error ? error.message : String(error);
      setImportError(message === WRONG_PASSWORD_ERROR ? t.connectionTransfer.wrongPassword : message);
    } finally {
      endBusy("settings");
    }
  }

  async function runPolicyCheck() {
    beginBusy("settings");
    try {
      setPolicyResult(await api.policyCheck(policyKind, policySql, 500));
    } catch (error) {
      showError(error);
    } finally {
      endBusy("settings");
    }
  }

  async function setToolEnabled(name: string, enabled: boolean) {
    beginBusy("tools");
    try {
      applySnapshot(await api.setMcpToolEnabled(name, enabled));
      pushToast(
        formatMessage(enabled ? t.toast.toolEnabled : t.toast.toolDisabled, {
          tool: toolDisplayName(t, name)
        }),
        "info"
      );
    } catch (error) {
      showError(error);
    } finally {
      endBusy("tools");
    }
  }

  function copyAgentPrompt() {
    if (requireToken) {
      const activeTokens = snapshot?.access_tokens.filter((token) => token.enabled) ?? [];
      if (activeTokens.length === 0) { setActiveView("access"); return; }
      if (activeTokens.length === 1) { void copyPromptForToken(activeTokens[0].id); return; }
      setPromptTokenDialogOpen(true); return;
    }
    const prompt = buildAgentPrompt(t, serverEndpoint, false);
    void navigator.clipboard
      .writeText(prompt)
      .then(() => pushToast(t.toast.agentCopied, "info"))
      .catch(showError);
  }

  const connections = snapshot?.config.connections ?? [];
  const enabledConnections = connections.filter((connection) => connection.enabled).length;
  const serverEndpoint = snapshot?.server_status.endpoint ?? "http://127.0.0.1:17321/mcp";
  const requireToken = snapshot?.config.server.require_token ?? true;
  const recentEvents = snapshot?.audit_events.slice(0, 8) ?? [];
  const availableUpdateVersion = updater.state.kind === "available" ? updater.state.version : null;
  const showUpdateReminder = availableUpdateVersion !== null && dismissedUpdateVersion !== availableUpdateVersion;
  const managedJreVersion = jdbcStatus?.runtime.managed_version ?? null;
  const showJreUpdateReminder = jreUpdateVersion !== null
    && dismissedJreUpdateVersion !== jreUpdateVersion
    && !snapshot?.config.settings.jdbc_java_home
    && managedJreVersion !== null
    && managedJreVersion !== jreUpdateVersion;
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
              <NavButton icon={<Wrench />} label={t.nav.tools} active={activeView === "tools"} onClick={() => setActiveView("tools")} />
              <NavButton icon={<ShieldCheck />} label={t.access.title} active={activeView === "access"} onClick={() => setActiveView("access")} />
              <NavButton icon={<Logs />} label={t.nav.audit} active={activeView === "audit"} onClick={() => setActiveView("audit")} />
              <NavButton icon={<Settings />} label={t.nav.settings} active={activeView === "settings"} onClick={() => setActiveView("settings")} />
            </nav>

            <div className="sidebar-bottom">
              {snapshot && snapshot.audit_migration.status !== "ready" ? (
                <AuditMigrationReminder t={t} state={snapshot.audit_migration} onOpen={() => setMigrationDialogOpen(true)} />
              ) : (
                <>
                  {showUpdateReminder && availableUpdateVersion && (
                    <SidebarUpdateReminder
                      t={t}
                      version={availableUpdateVersion}
                      onOpenAbout={() => {
                        setActiveView("settings");
                        setSettingsTab("about");
                      }}
                      onDismiss={() => setDismissedUpdateVersion(availableUpdateVersion)}
                    />
                  )}
                  {showJreUpdateReminder && jreUpdateVersion && (
                    <SidebarUpdateReminder
                      t={t}
                      version={jreUpdateVersion}
                      icon={<Coffee size={16} />}
                      title={t.updates.runtimeUpdateTitle}
                      compact={formatMessage(t.updates.runtimeUpdateCompact, { version: jreUpdateVersion })}
                      onOpenAbout={() => {
                        setActiveView("settings");
                        setSettingsTab("drivers");
                      }}
                      onDismiss={() => setDismissedJreUpdateVersion(jreUpdateVersion)}
                    />
                  )}
                </>
              )}
              <SidebarFooter
                t={t}
                running={Boolean(snapshot?.server_status.running)}
                startupFailed={Boolean(snapshot?.startup_error)}
                port={snapshot?.config.server.port ?? 17321}
                busy={isBusy("server")}
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
                          <button type="button" className="icon-button" onClick={() => setAuditFilters({ from: "", to: "", tool: "", connection: "", status: "", token: "" })} aria-label={t.audit.clearFilter}>
                            <X size={17} />
                          </button>
                        </IconTooltip>
                      </span>
                    )}
                  </div>
                )}
                {snapshot && activeView === "overview" && (
                  <div className="top-icon-actions">
                    <LanguageSwitcher t={t} locale={locale} disabled={isBusy("settings")} onChange={(next) => void changeLocale(next)} />
                  </div>
                )}
                <div className={clsx("top-icon-actions", activeView === "connections" && emergencyDisconnect && "emergency-active")}>
                  {snapshot && activeView === "connections" && (
                    <IconTooltip label={emergencyDisconnect ? t.connections.emergencyRestore : t.connections.emergencyDisable}>
                      <button
                        type="button"
                        className={clsx("icon-button danger", emergencyDisconnect && "emergency-toggle-active")}
                        onClick={disableAllConnections}
                        disabled={isBusy("connections") || !snapshot.server_status.running}
                        aria-pressed={emergencyDisconnect}
                        aria-label={emergencyDisconnect ? t.connections.emergencyRestore : t.connections.emergencyDisable}
                      >
                        <AlertTriangle size={17} />
                      </button>
                    </IconTooltip>
                  )}
                  {snapshot && activeView === "audit" && (
                    <IconTooltip label={t.audit.clear}>
                      <button type="button" className="icon-button danger" onClick={clearAuditEvents} disabled={isBusy("audit") || snapshot.audit_events.length === 0} aria-label={t.audit.clear}>
                        <Trash2 size={17} />
                      </button>
                    </IconTooltip>
                  )}
                  <button type="button" className={clsx("icon-button", isBusy("refresh") && "is-spinning")} onClick={() => refresh()} disabled={isBusy("refresh")} aria-label={t.common.refresh}>
                    <RefreshCw size={17} />
                  </button>
                </div>
                {snapshot && activeView === "connections" && (
                  <button type="button" className="button primary" onClick={openNewConnection} disabled={isBusy("connections") || emergencyDisconnect}>
                    <Plus size={16} />
                    {t.overview.newConnection}
                  </button>
                )}
                {snapshot && activeView === "access" && requireToken && (
                  <button type="button" className="button primary" onClick={() => setCreateTokenRequest((value) => value + 1)} disabled={isBusy("access")}><Plus size={16} />{t.access.create}</button>
                )}
              </div>
            </header>

            {!snapshot ? (
              <div className="loading-panel">{t.overview.loading}</div>
            ) : (
              <div className={clsx("view-stage", `view-${activeView}`)} key={activeView} ref={stageScrollFadeRef} onScroll={updateScrollFade}>
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
                    onCopyAgentPrompt={copyAgentPrompt}
                    onToggleServer={toggleServer}
                    onToggleEmergency={disableAllConnections}
                    busy={isBusy("server")}
                    startDisabled={!snapshot.server_status.running && !migrationReady}
                    mcpActivitySequence={mcpActivitySequence}
                    mcpActivityTone={mcpActivityTone}
                    showBackgroundTip={backgroundTipVisible}
                    backgroundTipLeaving={backgroundTipLeaving}
                    backgroundTipSuccess={backgroundTipSuccess}
                    onEnableAutoStart={enableAutoStartFromTip}
                    onDismissBackgroundTip={dismissBackgroundTip}
                  />
                )}
                {activeView === "connections" && (
                  <ConnectionsView
                    t={t}
                    connections={connections}
                    busy={isBusy("connections") || emergencyDisconnect}
                    onEdit={openExistingConnection}
                    onDelete={deleteConnection}
                    onTest={testConnection}
                    onDiagnose={diagnoseConnection}
                    onToggleEnabled={setConnectionEnabled}
                    migrationReady={migrationReady}
                    onCopyName={(name) => void copyConnectionName(name)}
                  />
                )}
                {activeView === "access" && <AccessControlView t={t} tokens={snapshot.access_tokens} connections={connections} tools={snapshot.tools} requireToken={requireToken} busy={isBusy("access")} selectedId={selectedTokenId} onSelectedIdChange={setSelectedTokenId} createRequest={createTokenRequest} onCreateRequestHandled={() => setCreateTokenRequest(0)} onEnableAuthentication={() => void enableTokenAuthentication()} onCreate={createAccessToken} onRename={renameAccessToken} onToggle={(id, enabled) => void updateAccess(() => api.setAccessTokenEnabled(id, enabled), enabled ? t.access.tokenEnabled : t.access.tokenDisabled)} onRotate={rotateAccessToken} onDelete={deleteAccessToken} onCopyPrompt={(id) => void copyPromptForToken(id)} onConnectionAllowed={(tokenId, connectionId, allowed) => void updateAccess(() => api.setTokenConnectionAllowed(tokenId, connectionId, allowed), t.access.permissionUpdated)} onToolAllowed={(tokenId, toolName, allowed) => void updateAccess(() => api.setTokenToolAllowed(tokenId, toolName, allowed), t.access.permissionUpdated)} />}
                {activeView === "tools" && <ToolsView t={t} tools={snapshot.tools} busy={isBusy("tools")} onToggle={setToolEnabled} />}
                {activeView === "audit" && <AuditView t={t} events={snapshot.audit_events} tokens={snapshot.access_tokens} tools={snapshot.tools} connections={snapshot.config.connections} filters={auditFilters} onFiltersChange={setAuditFilters} filterOpen={auditFilterOpen} onFilterOpenChange={setAuditFilterOpen} onSelect={setSelectedAudit} />}
                {activeView === "settings" && (
                  <SettingsView
                    t={t}
                    locale={locale}
                    theme={theme}
                    effectiveTheme={effectiveTheme}
                    server={snapshot.config.server}
                    settings={snapshot.config.settings}
                    autoStartStatus={snapshot.auto_start_status}
                    busy={isBusy("settings")}
                    jdbcBusy={isBusy("jdbc")}
                    jreCheckBusy={isBusy("jreCheck")}
                    tab={settingsTab}
                    auditEvents={snapshot.audit_events}
                    jdbcStatus={jdbcStatus}
                    jdbcStorageStatus={jdbcStorageStatus}
                    jdbcInstallProgress={jdbcInstallProgress}
                    jdbcRuntimeProgress={jdbcRuntimeProgress}
                    policySql={policySql}
                    policyKind={policyKind}
                    policyResult={policyResult}
                    updaterEnabled={snapshot.updater_enabled}
                    updateState={updater.state}
                    onCheckUpdate={() => void checkAllUpdates()}
                    onUpdate={() => void updater.installUpdate()}
                    onOpenProjectReleases={() => void api.openProjectReleases().catch(showError)}
                    onOpenGithubAccelerationProvider={() => void api.openGithubAccelerationProvider().catch(showError)}
                    onOpenAudit={() => setActiveView("audit")}
                    onTabChange={setSettingsTab}
                    onRefreshJdbcStatus={() => void refreshJdbcStatus()}
                    onInstallJdbcRuntime={installJdbcRuntime}
                    onRemoveJdbcRuntime={removeJdbcRuntime}
                    onCheckJdbcRuntimeUpdate={checkJdbcRuntimeUpdate}
                    onRefreshJdbcStorageStatus={() => void refreshJdbcStorageStatus()}
                    onClearJdbcCache={clearJdbcCache}
                    onStopJdbcDriverRuntime={stopJdbcDriverRuntime}
                    onOpenDataDirectory={() => void api.openDataDirectory().catch(showError)}
                    onOpenDebugLogFolder={() => void api.openDebugLogDirectory().catch(showError)}
                    onInstallJdbcDriver={installJdbcDriver}
                    onImportJdbcDriver={importJdbcDriver}
                    onDeleteJdbcDriver={(bundleId) => void deleteJdbcDriver(bundleId)}
                    onThemeChange={setTheme}
                    onPolicyKindChange={setPolicyKind}
                    onSqlChange={setPolicySql}
                    onPolicyCheck={runPolicyCheck}
                    onSaveServer={saveServer}
                    onSaveSettings={saveSettings}
                    onExportConnections={(password) => void exportConnections(password)}
                    onImportConnections={() => void importConnections()}
                    onOpenProjectHomepage={() => void api.openProjectHomepage().catch(showError)}
                    onOpenProjectSite={() => void api.openProjectSite().catch(showError)}
                    onOpenProjectChangelog={(version) => void api.openProjectChangelog(version).catch(showError)}
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
          busy={isBusy("connections")}
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
          jdbcDrivers={jdbcStatus?.drivers ?? []}
          onSubmit={saveConnection}
          onClose={() => setEditing(null)}
        />
        <AuditDetailDialog t={t} event={selectedAudit} onClose={() => setSelectedAudit(null)} />
        {importRequest ? (
          <EncryptionPasswordDialog
            t={t}
            mode="import"
            exportedAt={importRequest.exportedAt}
            error={importError}
            busy={isBusy("settings")}
            onSubmit={(password) => void submitImportPassword(password)}
            onCancel={() => {
              setImportRequest(null);
              setImportError(null);
            }}
          />
        ) : null}
        {snapshot && <PromptTokenDialog t={t} open={promptTokenDialogOpen} tokens={snapshot.access_tokens} connections={connections} tools={snapshot.tools} busy={isBusy("access")} onClose={() => setPromptTokenDialogOpen(false)} onSelect={(id) => void copyPromptForToken(id)} />}
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
