import * as Dialog from "@radix-ui/react-dialog";
import * as Switch from "@radix-ui/react-switch";
import { open } from "@tauri-apps/plugin-dialog";
import clsx from "clsx";
import {
  AlertTriangle,
  Bug,
  CheckCircle2,
  Database,
  Download,
  ExternalLink,
  FileDown,
  FileText,
  FileUp,
  FolderOpen,
  Gauge,
  HardDrive,
  Activity,
  Github,
  Home,
  Info,
  KeyRound,
  Monitor,
  Network,
  PackagePlus,
  Plus,
  RefreshCw,
  RotateCcw,
  SearchCheck, ShieldAlert,
  ShieldCheck,
  ShieldOff,
  Trash2,
  X
} from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { FormEvent, ReactNode } from "react";
import appConfig from "../../../app.config.json";
import appIconUrl from "../../../resources/icon.png";
import { formatMessage, languageOptions, normalizeLocale, type I18nMessages, type Locale } from "../../i18n";
import type { AppSnapshot, AuditEvent, DatabaseType, ImportJdbcDriverInput, InstallJdbcDriverInput, JdbcCacheSelection, JdbcDriverRuntimeInfo, JdbcInstallProgress, JdbcRuntimeInstallProgress, JdbcStatus, JdbcStorageStatus, PolicyCheckResult, ServerConfig, SettingsConfig } from "../../types";
import type { UpdateState } from "../../lib/updater";
import type { EffectiveTheme, SettingsTab, ThemeMode } from "../../app/types";
import { updateScrollFade, useScrollFade } from "../../app/utils";
import { Field, IconTooltip, SwitchField } from "../../components/ui";
import { ThemeModeControl } from "../../components/chrome";

const APP_VERSION = appConfig.version;
// Must match the backend snapshot list limit (audit.rs MAX_AUDIT_LIST_EVENTS).
const AUDIT_LIST_LIMIT = 5000;
const AUDIT_RETENTION_DAY_OPTIONS = [3, 7, 15, 30];
// Number of clicks on the about-page version badge that reveal the hidden
// debug options panel, mirroring the classic developer-mode gesture.
const DEBUG_OPTIONS_UNLOCK_TAPS = 5;

export function SettingsView({
  t,
  locale,
  theme,
  effectiveTheme,
  server,
  settings,
  autoStartStatus,
  busy,
  tab,
  auditEvents,
  jdbcStatus,
  jdbcStorageStatus,
  jdbcInstallProgress,
  jdbcRuntimeProgress,
  policySql,
  policyKind,
  policyResult,
  updaterEnabled,
  updateState,
  onCheckUpdate,
  onUpdate,
  onOpenProjectReleases,
  onOpenAudit,
  onTabChange,
  onRefreshJdbcStatus,
  onInstallJdbcRuntime,
  onRemoveJdbcRuntime,
  onCheckJdbcRuntimeUpdate,
  onRefreshJdbcStorageStatus,
  onClearJdbcCache,
  onOpenDataDirectory,
  onOpenDebugLogFolder,
  onInstallJdbcDriver,
  onImportJdbcDriver,
  onDeleteJdbcDriver,
  onThemeChange,
  onPolicyKindChange,
  onSqlChange,
  onPolicyCheck,
  onSaveServer,
  onSaveSettings,
  onExportConnections,
  onImportConnections,
  onOpenProjectHomepage,
  onOpenProjectSite
}: {
  t: I18nMessages;
  locale: Locale;
  theme: ThemeMode;
  effectiveTheme: EffectiveTheme;
  server: ServerConfig;
  settings: SettingsConfig;
  autoStartStatus: AppSnapshot["auto_start_status"];
  busy: boolean;
  tab: SettingsTab;
  auditEvents: AuditEvent[];
  jdbcStatus: JdbcStatus | null;
  jdbcStorageStatus: JdbcStorageStatus | null;
  jdbcInstallProgress: JdbcInstallProgress | null;
  jdbcRuntimeProgress: JdbcRuntimeInstallProgress | null;
  policySql: string;
  policyKind: DatabaseType;
  policyResult: PolicyCheckResult | null;
  updaterEnabled: boolean;
  updateState: UpdateState;
  onCheckUpdate: () => void;
  onUpdate: () => void;
  onOpenProjectReleases: () => void;
  onOpenAudit: () => void;
  onTabChange: (tab: SettingsTab) => void;
  onRefreshJdbcStatus: () => void;
  onInstallJdbcRuntime: () => Promise<boolean>;
  onRemoveJdbcRuntime: () => Promise<boolean>;
  onCheckJdbcRuntimeUpdate: () => Promise<string | null>;
  onRefreshJdbcStorageStatus: () => void;
  onClearJdbcCache: (selection: JdbcCacheSelection) => Promise<boolean>;
  onOpenDataDirectory: () => void;
  onOpenDebugLogFolder: () => void;
  onInstallJdbcDriver: (input: InstallJdbcDriverInput) => Promise<boolean>;
  onImportJdbcDriver: (input: ImportJdbcDriverInput) => Promise<boolean>;
  onDeleteJdbcDriver: (bundleId: string) => void;
  onThemeChange: (theme: ThemeMode) => void;
  onPolicyKindChange: (kind: DatabaseType) => void;
  onSqlChange: (sql: string) => void;
  onPolicyCheck: () => void;
  onSaveServer: (server: ServerConfig) => Promise<boolean>;
  onSaveSettings: (settings: SettingsConfig, applyAutoStart?: boolean) => Promise<void>;
  onExportConnections: () => void;
  onImportConnections: () => void;
  onOpenProjectHomepage: () => void;
  onOpenProjectSite: () => void;
}) {
  const [serverDraft, setServerDraft] = useState(server);
  const [settingsDraft, setSettingsDraft] = useState(settings);
  const [serverPortDraft, setServerPortDraft] = useState(String(server.port));
  const serverDraftDirty = useRef(false);
  const settingsDraftDirty = useRef(false);
  const [policyDialogOpen, setPolicyDialogOpen] = useState(false);
  const [exportDialogOpen, setExportDialogOpen] = useState(false);
  const [exportAcknowledged, setExportAcknowledged] = useState(false);
  const [bearerWarningOpen, setBearerWarningOpen] = useState(false);
  const [versionTaps, setVersionTaps] = useState(0);
  const [debugOptionsVisible, setDebugOptionsVisible] = useState(false);
  const [debugLogConfirmOpen, setDebugLogConfirmOpen] = useState(false);
  const scrollFadeRef = useScrollFade();

  useEffect(() => {
    setServerDraft((current) => {
      if (!serverDraftDirty.current) return server;
      const saved = current.host === server.host
        && current.port === server.port
        && current.require_token === server.require_token;
      if (saved) serverDraftDirty.current = false;
      if (saved) setServerPortDraft(String(server.port));
      return saved ? server : current;
    });
    if (!serverDraftDirty.current) setServerPortDraft(String(server.port));
  }, [server]);
  useEffect(() => {
    setSettingsDraft((current) => {
      if (!settingsDraftDirty.current) return settings;
      const saved = current.audit_retention_days === settings.audit_retention_days
        && current.audit_redact_sql_literals === settings.audit_redact_sql_literals
        && current.auto_check_updates === settings.auto_check_updates
        && current.auto_start_mcp === settings.auto_start_mcp
        && current.auto_lightweight_mode === settings.auto_lightweight_mode
        && current.mcp_activity_effects === settings.mcp_activity_effects
        && current.language === settings.language
        && (current.jdbc_java_home ?? null) === (settings.jdbc_java_home ?? null)
        && current.auto_circuit_breaker === settings.auto_circuit_breaker
        && current.auto_circuit_breaker_window_minutes === settings.auto_circuit_breaker_window_minutes
        && current.auto_circuit_breaker_threshold === settings.auto_circuit_breaker_threshold
        && current.debug_logging_enabled === settings.debug_logging_enabled;
      if (saved) settingsDraftDirty.current = false;
      return saved ? settings : current;
    });
  }, [settings]);
  useEffect(() => {
    setSettingsDraft((current) => ({ ...current, language: locale }));
  }, [locale]);
  useEffect(() => {
    // Collapse the unlocked debug panel again once the user leaves the About
    // tab while debug logging is off; while logging stays enabled the panel
    // remains visible for quick access to the log folder.
    if (tab !== "about" && !settingsDraft.debug_logging_enabled) {
      setVersionTaps(0);
      setDebugOptionsVisible(false);
    }
  }, [tab, settingsDraft.debug_logging_enabled]);
  // Lightweight mode destroys the whole frontend, which resets the unlock
  // state. When debug logging is already running, restore the panel without
  // the hidden version-badge gesture so users keep control over it.
  const debugPanelAutoShown = useRef(false);
  useEffect(() => {
    if (debugPanelAutoShown.current) return;
    debugPanelAutoShown.current = true;
    if (settings.debug_logging_enabled) {
      setDebugOptionsVisible(true);
    }
    // Runs once per frontend mount on purpose.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [settings.debug_logging_enabled]);

  const blockedCutoff = Date.now() - 24 * 60 * 60 * 1000;
  const blockedLast24Hours = auditEvents.filter((event) => (
    event.status === "denied" && new Date(event.timestamp).getTime() >= blockedCutoff
  )).length;
  const oldestAuditEvent = auditEvents[auditEvents.length - 1];
  const blockedPossiblyTruncated = auditEvents.length >= AUDIT_LIST_LIMIT
    && Boolean(oldestAuditEvent && new Date(oldestAuditEvent.timestamp).getTime() >= blockedCutoff);
  const bearerAuthEnabled = server.require_token;

  return (
    <section className="settings-page">
      <div className="settings-tabs">
        <button type="button" className={clsx(tab === "general" && "active")} onClick={() => onTabChange("general")}>
          {t.settings.general}
        </button>
        <button type="button" className={clsx(tab === "security" && "active")} onClick={() => onTabChange("security")}>
          {t.settings.security}
        </button>
        <button type="button" className={clsx("jdbc-support-tab", tab === "drivers" && "active")} onClick={() => onTabChange("drivers")}>
          <span>{t.settings.driverManagement}</span>
          <span className="settings-tab-badge">{t.settings.preview}</span>
        </button>
        <button type="button" className={clsx(tab === "storage" && "active")} onClick={() => onTabChange("storage")}>
          {t.settings.storagePerformance}
        </button>
        <button type="button" className={clsx(tab === "about" && "active")} onClick={() => onTabChange("about")}>
          {t.settings.about}
        </button>
      </div>

      {tab === "general" ? (
        <div ref={scrollFadeRef} className="settings-stack" onScroll={updateScrollFade}>
          <section className="panel">
            <h2>{t.settings.servicePolicy}</h2>
            <div className="form-grid settings-grid">
              <Field label={t.settings.listenHost}>
                <input
                  value={serverDraft.host}
                  onChange={(event) => {
                    serverDraftDirty.current = true;
                    setServerDraft({ ...serverDraft, host: event.target.value });
                  }}
                  onBlur={async (event) => {
                    const saved = await onSaveServer({ ...serverDraft, host: event.currentTarget.value });
                    if (!saved) {
                      serverDraftDirty.current = false;
                      setServerDraft(server);
                      setServerPortDraft(String(server.port));
                    }
                  }}
                />
              </Field>
              <Field label={t.settings.port}>
                <input
                  type="number"
                  value={serverPortDraft}
                  onChange={(event) => {
                    serverDraftDirty.current = true;
                    setServerPortDraft(event.target.value);
                    setServerDraft({ ...serverDraft, port: Number(event.target.value) || 0 });
                  }}
                  onBlur={async (event) => {
                    const port = Math.max(1, Math.min(65535, Number(event.currentTarget.value) || server.port));
                    setServerPortDraft(String(port));
                    setServerDraft((current) => ({ ...current, port }));
                    const saved = await onSaveServer({ ...serverDraft, port });
                    if (!saved) {
                      serverDraftDirty.current = false;
                      setServerDraft(server);
                      setServerPortDraft(String(server.port));
                    }
                  }}
                />
              </Field>
              <div className="field">
                <span>{t.settings.accessControl}</span>
                <SwitchField label={t.settings.requireBearer} checked={serverDraft.require_token} disabled={busy} onCheckedChange={(checked) => {
                  if (!checked) {
                    setBearerWarningOpen(true);
                    return;
                  }
                  const next = { ...serverDraft, require_token: checked };
                  serverDraftDirty.current = true;
                  setServerDraft(next);
                  void onSaveServer(next);
                }} />
              </div>
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
                    settingsDraftDirty.current = true;
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
              <div className="field">
                <span>{t.settings.interfaceEffects}</span>
                <SwitchField label={t.settings.mcpActivityEffects} checked={settingsDraft.mcp_activity_effects} disabled={busy} onCheckedChange={(checked) => {
                  const next = { ...settingsDraft, mcp_activity_effects: checked };
                  settingsDraftDirty.current = true;
                  setSettingsDraft(next);
                  onSaveSettings(next);
                }} />
              </div>
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
            <h2>{t.settings.startup}</h2>
            <div className="form-grid settings-grid">
              <div className="field">
                <span>{t.settings.autoStart}</span>
                <SwitchField label={t.settings.autoStartMcp} checked={autoStartStatus === "enabled"} disabled={busy} onCheckedChange={(checked) => {
                  const next = { ...settingsDraft, auto_start_mcp: checked };
                  settingsDraftDirty.current = true;
                  setSettingsDraft(next);
                  onSaveSettings(next, true);
                }} />
              </div>
              <div className="field">
                <span>{t.settings.lightweightMode}</span>
                <SwitchField label={t.settings.autoLightweightMode} tooltip={t.settings.autoLightweightModeHint} checked={settingsDraft.auto_lightweight_mode} disabled={busy} onCheckedChange={(checked) => {
                  const next = { ...settingsDraft, auto_lightweight_mode: checked };
                  settingsDraftDirty.current = true;
                  setSettingsDraft(next);
                  onSaveSettings(next);
                }} />
              </div>
            </div>
          </section>

          <section className="panel">
            <h2>{t.settings.auditLog}</h2>
            <div className="form-grid settings-grid">
              <Field label={t.settings.auditRetentionDays}>
                <select
                  value={String(settingsDraft.audit_retention_days)}
                  disabled={busy}
                  onChange={(event) => {
                    const next = { ...settingsDraft, audit_retention_days: Number(event.target.value) };
                    settingsDraftDirty.current = true;
                    setSettingsDraft(next);
                    onSaveSettings(next);
                  }}
                >
                  {AUDIT_RETENTION_DAY_OPTIONS.map((days) => (
                    <option key={days} value={days}>{formatMessage(t.settings.auditRetentionDaysOption, { days })}</option>
                  ))}
                </select>
              </Field>
              <div className="field">
                <span>{t.settings.auditPrivacy}</span>
                <SwitchField label={t.settings.auditRedactSql} checked={settingsDraft.audit_redact_sql_literals} disabled={busy} onCheckedChange={(checked) => {
                  const next = { ...settingsDraft, audit_redact_sql_literals: checked };
                  settingsDraftDirty.current = true;
                  setSettingsDraft(next);
                  onSaveSettings(next);
                }} />
              </div>
            </div>
          </section>

          <Dialog.Root
            open={exportDialogOpen}
            onOpenChange={(open) => {
              setExportDialogOpen(open);
              if (!open) setExportAcknowledged(false);
            }}
          >
            <section className="panel transfer-panel">
              <h2>{t.settings.importExport}</h2>
              <div className="transfer-actions">
                <button type="button" className="transfer-action" disabled={busy} onClick={onImportConnections}>
                  <span className="transfer-action-copy">
                    <strong>{t.settings.importConnections}</strong>
                    <span>{t.settings.importConnectionsDescription}</span>
                  </span>
                  <span className="transfer-action-icon" aria-hidden="true"><FileUp size={18} /></span>
                </button>
                <Dialog.Trigger asChild>
                  <button type="button" className="transfer-action" disabled={busy}>
                    <span className="transfer-action-copy">
                    <strong>{t.settings.exportConnections}</strong>
                      <span>{t.settings.exportConnectionsDescription}</span>
                    </span>
                    <span className="transfer-action-icon" aria-hidden="true"><FileDown size={18} /></span>
                  </button>
                </Dialog.Trigger>
              </div>
            </section>
            <Dialog.Portal>
              <Dialog.Overlay className="dialog-overlay" />
              <Dialog.Content className="policy-dialog transfer-dialog">
                <div className="dialog-titlebar">
                  <div>
                    <Dialog.Title>{t.settings.exportWarningTitle}</Dialog.Title>
                    <Dialog.Description>{t.settings.exportWarningDescription}</Dialog.Description>
                  </div>
                  <Dialog.Close asChild>
                    <button type="button" className="icon-button" aria-label={t.common.close}>
                      <X size={18} />
                    </button>
                  </Dialog.Close>
                </div>
                <div className="transfer-warning">
                  <div className="transfer-warning-icon"><AlertTriangle size={22} /></div>
                  <ul>
                    <li>{t.settings.exportWarningAccess}</li>
                    <li>{t.settings.exportWarningLocation}</li>
                    <li>{t.settings.exportWarningCleanup}</li>
                  </ul>
                </div>
                <label className="transfer-acknowledgement">
                  <input
                    type="checkbox"
                    checked={exportAcknowledged}
                    onChange={(event) => setExportAcknowledged(event.target.checked)}
                  />
                  <span>{t.settings.exportAcknowledgement}</span>
                </label>
                <footer className="transfer-dialog-actions">
                  <Dialog.Close asChild>
                    <button type="button" className="button ghost">{t.common.cancel}</button>
                  </Dialog.Close>
                  <button
                    type="button"
                    className="button danger-solid"
                    disabled={!exportAcknowledged || busy}
                    onClick={() => {
                      setExportDialogOpen(false);
                      setExportAcknowledged(false);
                      onExportConnections();
                    }}
                  >
                    <FileDown size={16} />
                    {t.settings.confirmExport}
                  </button>
                </footer>
              </Dialog.Content>
            </Dialog.Portal>
          </Dialog.Root>

          <Dialog.Root
            open={bearerWarningOpen}
            onOpenChange={(open) => {
              if (!busy) setBearerWarningOpen(open);
            }}
          >
            <Dialog.Portal>
              <Dialog.Overlay className="dialog-overlay" />
              <Dialog.Content className="policy-dialog transfer-dialog">
                <div className="dialog-titlebar">
                  <div>
                    <Dialog.Title>{t.settings.bearerWarningTitle}</Dialog.Title>
                    <Dialog.Description>{t.settings.bearerWarningDescription}</Dialog.Description>
                  </div>
                  <Dialog.Close asChild>
                    <button type="button" className="icon-button" disabled={busy} aria-label={t.common.close}>
                      <X size={18} />
                    </button>
                  </Dialog.Close>
                </div>
                <div className="transfer-warning">
                  <div className="transfer-warning-icon"><ShieldAlert size={22} /></div>
                  <ul>
                    <li>{t.settings.bearerWarningSecurity}</li>
                    <li>{t.settings.bearerWarningAccessControl}</li>
                  </ul>
                </div>
                <footer className="transfer-dialog-actions">
                  <Dialog.Close asChild>
                    <button type="button" className="button ghost" disabled={busy}>{t.common.cancel}</button>
                  </Dialog.Close>
                  <button
                    type="button"
                    className="button danger-solid"
                    disabled={busy}
                    onClick={async () => {
                      const next = { ...serverDraft, require_token: false };
                      serverDraftDirty.current = true;
                      setServerDraft(next);
                      const saved = await onSaveServer(next);
                      if (saved) {
                        setBearerWarningOpen(false);
                      } else {
                        serverDraftDirty.current = false;
                        setServerDraft(server);
                      }
                    }}
                  >
                    <ShieldOff size={16} />
                    {t.settings.confirmDisableBearer}
                  </button>
                </footer>
              </Dialog.Content>
            </Dialog.Portal>
          </Dialog.Root>
        </div>
      ) : tab === "security" ? (
        <div ref={scrollFadeRef} className="settings-stack" onScroll={updateScrollFade}>
          <Dialog.Root open={policyDialogOpen} onOpenChange={setPolicyDialogOpen}>
            <section className={clsx("security-hero", blockedLast24Hours > 0 && "alerted")}>
              <div className="security-hero-core">
                <span className="security-hero-emblem">{blockedLast24Hours > 0 ? <ShieldAlert size={22} /> : <ShieldCheck size={22} />}</span>
                <div>
                  <span>{t.settings.securityBlockedLabel}</span>
                  <strong>
                    {blockedLast24Hours}
                    <small> {t.settings.securityBlockedUnit}</small>
                    {blockedPossiblyTruncated && (
                      <IconTooltip label={t.settings.securityBlockedTruncatedHint}>
                        <span className="call-count-overflow" role="img" aria-label={t.settings.securityBlockedTruncatedHint} tabIndex={0}>
                          <Plus size={10} strokeWidth={2.5} />
                        </span>
                      </IconTooltip>
                    )}
                  </strong>
                </div>
              </div>
              <div className="security-hero-actions">
                <Dialog.Trigger asChild>
                  <button type="button" className="button primary" disabled={busy}>
                    <SearchCheck size={16} />
                    {t.settings.policyConsole}
                  </button>
                </Dialog.Trigger>
                <button type="button" className="button ghost" onClick={onOpenAudit}>
                  <FileText size={16} />
                  {t.settings.securityViewAudit}
                </button>
              </div>
            </section>

            <section className="panel">
              <div className="panel-header">
                <h2 className={clsx("defense-state", !bearerAuthEnabled && "warning")}>
                  <span className="defense-state-dot" />
                  {bearerAuthEnabled ? t.settings.securityDefenseActive : t.settings.securityDefensePartial}
                </h2>
              </div>
              <ul className="defense-list">
                <li>
                  <span className="defense-icon"><Network size={16} /></span>
                  <div>
                    <strong>{t.settings.securityLayerNetwork}</strong>
                    <p>{t.settings.securityLayerNetworkDesc}</p>
                  </div>
                  <span className="defense-index">L1</span>
                </li>
                <li className={clsx(!bearerAuthEnabled && "defense-item-warning")}>
                  <span className="defense-icon"><KeyRound size={16} /></span>
                  <div>
                    <strong>
                      {t.settings.securityLayerAuth}
                      {!bearerAuthEnabled && <span className="defense-item-badge">{t.settings.securityLayerAuthOffBadge}</span>}
                    </strong>
                    <p>{bearerAuthEnabled ? t.settings.securityLayerAuthDesc : t.settings.securityLayerAuthOffDesc}</p>
                  </div>
                  <span className="defense-index">L2</span>
                </li>
                <li>
                  <span className="defense-icon"><ShieldCheck size={16} /></span>
                  <div>
                    <strong>{t.settings.securityLayerSyntax}</strong>
                    <p>{t.settings.securityLayerSyntaxDesc}</p>
                  </div>
                  <span className="defense-index">L3</span>
                </li>
                <li>
                  <span className="defense-icon"><Database size={16} /></span>
                  <div>
                    <strong>{t.settings.securityLayerDatabase}</strong>
                    <p>{t.settings.securityLayerDatabaseDesc}</p>
                  </div>
                  <span className="defense-index">L4</span>
                </li>
                <li>
                  <span className="defense-icon"><Gauge size={16} /></span>
                  <div>
                    <strong>{t.settings.securityLayerResource}</strong>
                    <p>{t.settings.securityLayerResourceDesc}</p>
                  </div>
                  <span className="defense-index">L5</span>
                </li>
                <li>
                  <span className="defense-icon"><FileText size={16} /></span>
                  <div>
                    <strong>{t.settings.securityLayerAudit}</strong>
                    <p>{t.settings.securityLayerAuditDesc}</p>
                  </div>
                  <span className="defense-index">L6</span>
                </li>
              </ul>
              <ul className="security-list defense-notice">
                <li className="security-warning"><AlertTriangle size={16} /> {t.settings.securityWarning}</li>
              </ul>
            </section>

            <section className="panel">
              <h2>{t.settings.circuitBreaker}</h2>
              <div className="form-grid settings-grid">
                <div className="field span-all">
                  <span>{t.settings.circuitBreaker}</span>
                  <SwitchField
                    label={t.settings.circuitBreakerToggle}
                    tooltip={t.settings.circuitBreakerToggleHint}
                    checked={settingsDraft.auto_circuit_breaker}
                    disabled={busy || !bearerAuthEnabled}
                    onCheckedChange={(checked) => {
                      const next = { ...settingsDraft, auto_circuit_breaker: checked };
                      settingsDraftDirty.current = true;
                      setSettingsDraft(next);
                      onSaveSettings(next);
                    }}
                  />
                </div>
                <Field label={t.settings.circuitBreakerWindow}>
                  <select
                    value={String(settingsDraft.auto_circuit_breaker_window_minutes)}
                    disabled={busy || !bearerAuthEnabled || !settingsDraft.auto_circuit_breaker}
                    onChange={(event) => {
                      const next = { ...settingsDraft, auto_circuit_breaker_window_minutes: Number(event.target.value) };
                      settingsDraftDirty.current = true;
                      setSettingsDraft(next);
                      onSaveSettings(next);
                    }}
                  >
                    {[1, 5, 10, 30, 60].map((minutes) => (
                      <option key={minutes} value={minutes}>{formatMessage(t.settings.circuitBreakerWindowOption, { minutes })}</option>
                    ))}
                  </select>
                </Field>
                <Field label={t.settings.circuitBreakerThreshold}>
                  <select
                    value={String(settingsDraft.auto_circuit_breaker_threshold)}
                    disabled={busy || !bearerAuthEnabled || !settingsDraft.auto_circuit_breaker}
                    onChange={(event) => {
                      const next = { ...settingsDraft, auto_circuit_breaker_threshold: Number(event.target.value) };
                      settingsDraftDirty.current = true;
                      setSettingsDraft(next);
                      onSaveSettings(next);
                    }}
                  >
                    {[3, 5, 10, 20, 50].map((count) => (
                      <option key={count} value={count}>{formatMessage(t.settings.circuitBreakerThresholdOption, { count })}</option>
                    ))}
                  </select>
                </Field>
              </div>
              <ul className="security-list circuit-note">
                <li className="security-info">
                  <Info size={16} />
                  {bearerAuthEnabled ? t.settings.circuitBreakerNote : t.settings.circuitBreakerRequiresBearer}
                </li>
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
      ) : tab === "drivers" ? (
        <DriverManagement
          t={t}
          status={jdbcStatus}
          installProgress={jdbcInstallProgress}
          runtimeProgress={jdbcRuntimeProgress}
          settings={settings}
          busy={busy}
          onRefresh={onRefreshJdbcStatus}
          onInstallRuntime={onInstallJdbcRuntime}
          onRemoveRuntime={onRemoveJdbcRuntime}
          onCheckRuntimeUpdate={onCheckJdbcRuntimeUpdate}
          onInstall={onInstallJdbcDriver}
          onImport={onImportJdbcDriver}
          onDelete={onDeleteJdbcDriver}
          onSaveSettings={onSaveSettings}
        />
      ) : tab === "storage" ? (
        <StorageManagement status={jdbcStorageStatus} busy={busy} onRefresh={onRefreshJdbcStorageStatus} onClearJdbcCache={onClearJdbcCache} onOpenDataDirectory={onOpenDataDirectory} debugLoggingEnabled={settingsDraft.debug_logging_enabled} t={t} />
      ) : (
        <div ref={scrollFadeRef} className="settings-stack" onScroll={updateScrollFade}>
          <section className="panel about-panel">
            <div className="about-hero">
              <img src={appIconUrl} alt="DataNexa" />
              <div>
                <h2>DataNexa <span
                  className="version-badge"
                  onClick={() => {
                    const next = versionTaps + 1;
                    if (next >= DEBUG_OPTIONS_UNLOCK_TAPS) {
                      setVersionTaps(0);
                      setDebugOptionsVisible(true);
                    } else {
                      setVersionTaps(next);
                    }
                  }}
                >v{APP_VERSION}</span></h2>
                <p>{t.settings.aboutText}</p>
              </div>
            </div>
            <AboutUpdateSection
              t={t}
              enabled={updaterEnabled}
              state={updateState}
              autoCheckUpdates={settingsDraft.auto_check_updates}
              onAutoCheckUpdatesChange={(checked) => {
                const next = { ...settingsDraft, auto_check_updates: checked };
                settingsDraftDirty.current = true;
                setSettingsDraft(next);
                onSaveSettings(next);
              }}
              onCheck={onCheckUpdate}
              onUpdate={onUpdate}
              onOpenProjectReleases={onOpenProjectReleases}
            >
              {debugOptionsVisible && (
                <>
                  <div className="about-update-switch-row">
                    <span className="switch-label">
                      {t.settings.debugLogging}
                      <IconTooltip label={t.settings.debugLoggingTooltip}>
                        <button type="button" className="switch-info" aria-label={t.settings.debugLoggingTooltip}>
                          <Info size={13} />
                        </button>
                      </IconTooltip>
                    </span>
                    <span className="debug-log-controls">
                      {settingsDraft.debug_logging_enabled && (
                        <IconTooltip label={t.settings.openDebugLogFolder}>
                          <button
                            type="button"
                            className="debug-log-folder-btn"
                            onClick={onOpenDebugLogFolder}
                            disabled={busy}
                            aria-label={t.settings.openDebugLogFolder}
                          >
                            <FolderOpen size={15} />
                          </button>
                        </IconTooltip>
                      )}
                      <Switch.Root
                        className="switch"
                        checked={settingsDraft.debug_logging_enabled}
                        disabled={busy}
                        aria-label={t.settings.debugLogging}
                        onCheckedChange={(checked) => {
                          if (checked) {
                            setDebugLogConfirmOpen(true);
                            return;
                          }
                          const next = { ...settingsDraft, debug_logging_enabled: false };
                          settingsDraftDirty.current = true;
                          setSettingsDraft(next);
                          onSaveSettings(next);
                        }}
                      >
                        <Switch.Thumb className="switch-thumb" />
                      </Switch.Root>
                    </span>
                  </div>
                  <Dialog.Root open={debugLogConfirmOpen} onOpenChange={setDebugLogConfirmOpen}>
                    <Dialog.Portal>
                      <Dialog.Overlay className="dialog-overlay" />
                      <Dialog.Content className="policy-dialog transfer-dialog">
                        <div className="dialog-titlebar">
                          <div>
                            <Dialog.Title>{t.settings.debugLogConfirmTitle}</Dialog.Title>
                            <Dialog.Description>{t.settings.debugLogConfirmDescription}</Dialog.Description>
                          </div>
                          <Dialog.Close asChild>
                            <button type="button" className="icon-button" aria-label={t.common.close}>
                              <X size={18} />
                            </button>
                          </Dialog.Close>
                        </div>
                        <div className="transfer-warning">
                          <div className="transfer-warning-icon"><AlertTriangle size={22} /></div>
                          <ul>
                            <li>{t.settings.debugLogConfirmTroubleshootOnly}</li>
                            <li>{t.settings.debugLogConfirmPerformance}</li>
                            <li>{t.settings.debugLogConfirmSanitized}</li>
                          </ul>
                        </div>
                        <footer className="transfer-dialog-actions">
                          <Dialog.Close asChild>
                            <button type="button" className="button ghost">{t.common.cancel}</button>
                          </Dialog.Close>
                          <button
                            type="button"
                            className="button danger-solid"
                            onClick={() => {
                              setDebugLogConfirmOpen(false);
                              const next = { ...settingsDraft, debug_logging_enabled: true };
                              settingsDraftDirty.current = true;
                              setSettingsDraft(next);
                              onSaveSettings(next);
                            }}
                          >
                            <Bug size={16} />
                            {t.settings.debugLogConfirmEnable}
                          </button>
                        </footer>
                      </Dialog.Content>
                    </Dialog.Portal>
                  </Dialog.Root>
                </>
              )}
            </AboutUpdateSection>
            <footer className="about-footer">
              <div className="about-footer-links">
                <a
                  className="github-link"
                  href="https://mingozacwu.github.io/datanexa-site/"
                  onClick={(event) => {
                    event.preventDefault();
                    onOpenProjectSite();
                  }}
                >
                  <Home size={16} />
                  {t.settings.officialHomepage}
                </a>
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
              </div>
              <p>(C) 2026 Zachary Wu All Rights Reserved.</p>
            </footer>
          </section>
        </div>
      )}
    </section>
  );
}

function DriverManagement({
  t,
  status,
  installProgress,
  runtimeProgress,
  settings,
  busy,
  onRefresh,
  onInstallRuntime,
  onRemoveRuntime,
  onCheckRuntimeUpdate,
  onInstall,
  onImport,
  onDelete,
  onSaveSettings
}: {
  t: I18nMessages;
  status: JdbcStatus | null;
  installProgress: JdbcInstallProgress | null;
  runtimeProgress: JdbcRuntimeInstallProgress | null;
  settings: SettingsConfig;
  busy: boolean;
  onRefresh: () => void;
  onInstallRuntime: () => Promise<boolean>;
  onRemoveRuntime: () => Promise<boolean>;
  onCheckRuntimeUpdate: () => Promise<string | null>;
  onInstall: (input: InstallJdbcDriverInput) => Promise<boolean>;
  onImport: (input: ImportJdbcDriverInput) => Promise<boolean>;
  onDelete: (bundleId: string) => void;
  onSaveSettings: (settings: SettingsConfig, applyAutoStart?: boolean) => Promise<void>;
}) {
  const [dialogOpen, setDialogOpen] = useState(false);
  const [detailsOpen, setDetailsOpen] = useState(false);
  const [displayName, setDisplayName] = useState("");
  const [coordinate, setCoordinate] = useState("");
  const [repositoryUrl, setRepositoryUrl] = useState("https://repo.maven.apache.org/maven2/");
  const [customRepository, setCustomRepository] = useState("");
  const [localDialogOpen, setLocalDialogOpen] = useState(false);
  const [localDisplayName, setLocalDisplayName] = useState("");
  const [localPaths, setLocalPaths] = useState<string[]>([]);
  const [runtimeUpdate, setRuntimeUpdate] = useState<string | null>(null);
  const [runtimeRemoveDialogOpen, setRuntimeRemoveDialogOpen] = useState(false);
  const scrollFadeRef = useScrollFade();

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const installed = await onInstall({
      display_name: displayName,
      maven_coordinate: coordinate,
      repository_url: repositoryUrl === "custom" ? customRepository : repositoryUrl
    });
    if (installed) {
      setDialogOpen(false);
      setDisplayName("");
      setCoordinate("");
    }
  }

  async function chooseLocalJars() {
    const selected = await open({
      title: t.settings.selectJdbcJars,
      multiple: true,
      filters: [{ name: "JAR", extensions: ["jar"] }]
    });
    const paths = Array.isArray(selected) ? selected : typeof selected === "string" ? [selected] : [];
    if (paths.length) {
      setLocalPaths(paths);
      if (!localDisplayName) setLocalDisplayName(paths[0].split(/[\\/]/).pop()?.replace(/\.jar$/i, "") ?? "JDBC driver");
    }
  }

  async function submitLocal(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const imported = await onImport({ display_name: localDisplayName, paths: localPaths });
    if (imported) {
      setLocalDialogOpen(false);
      setLocalDisplayName("");
      setLocalPaths([]);
    }
  }

  const runtime = status?.runtime;

  async function chooseExternalRuntime() {
    const selected = await open({
      title: t.settings.selectJavaRuntime,
      directory: true,
      multiple: false
    });
    if (typeof selected !== "string") return;
    await onSaveSettings({ ...settings, jdbc_java_home: selected });
    onRefresh();
  }

  async function useManagedRuntime() {
    await onSaveSettings({ ...settings, jdbc_java_home: null });
    onRefresh();
  }

  async function checkManagedRuntimeUpdate() {
    setRuntimeUpdate(await onCheckRuntimeUpdate());
  }

  async function installManagedRuntime() {
    if (await onInstallRuntime()) setRuntimeUpdate(null);
  }

  async function removeManagedRuntime() {
    if (await onRemoveRuntime()) {
      setRuntimeUpdate(null);
      setRuntimeRemoveDialogOpen(false);
    }
  }

  return (
    <div ref={scrollFadeRef} className="settings-stack driver-management" onScroll={updateScrollFade}>
      <section className="panel driver-runtime-panel">
        <div className="driver-section-heading">
          <div>
            <h2>{t.settings.jdbcRuntime}</h2>
          </div>
          <div className="runtime-heading-actions">
            <IconTooltip label={t.settings.selectJavaRuntime}>
              <button type="button" className="icon-button" onClick={() => void chooseExternalRuntime()} disabled={busy} aria-label={t.settings.selectJavaRuntime}>
                <FolderOpen size={17} />
              </button>
            </IconTooltip>
            {!settings.jdbc_java_home && runtime?.source === "managed" && (
              <IconTooltip label={t.settings.checkJdbcRuntimeUpdate}>
                <button type="button" className="icon-button" onClick={() => void checkManagedRuntimeUpdate()} disabled={busy} aria-label={t.settings.checkJdbcRuntimeUpdate}>
                  <Download size={17} />
                </button>
              </IconTooltip>
            )}
            {settings.jdbc_java_home && (
              <IconTooltip label={t.settings.useManagedRuntime}>
                <button type="button" className="icon-button" onClick={() => void useManagedRuntime()} disabled={busy} aria-label={t.settings.useManagedRuntime}>
                  <RotateCcw size={17} />
                </button>
              </IconTooltip>
            )}
            {runtime?.managed_version && (
              <IconTooltip label={t.settings.removeJdbcRuntime}>
                <button type="button" className="icon-button danger" onClick={() => setRuntimeRemoveDialogOpen(true)} disabled={busy} aria-label={t.settings.removeJdbcRuntime}>
                  <Trash2 size={17} />
                </button>
              </IconTooltip>
            )}
            <IconTooltip label={t.common.refresh}>
              <button type="button" className="icon-button" onClick={onRefresh} disabled={busy} aria-label={t.common.refresh}>
                <RefreshCw size={17} />
              </button>
            </IconTooltip>
          </div>
        </div>
        <div className="runtime-status-row">
          <StatusBadge available={Boolean(runtime?.available)} label={runtime?.available ? t.settings.runtimeAvailable : t.settings.runtimeUnavailable} />
          <code>{runtime?.java_version ?? t.settings.runtimeNotDetected}</code>
          <div className="runtime-status-meta">
            {runtime ? (
              runtime.source !== "unavailable" && (
                <span>{formatMessage(t.settings.runtimeSource, { source: runtimeSourceLabel(t, runtime.source) })}</span>
              )
            ) : (
              <span>{t.settings.runtimeChecking}</span>
            )}
            {runtime && !settings.jdbc_java_home && !runtime.available && (
              <button type="button" className="button primary runtime-status-action" onClick={() => void installManagedRuntime()} disabled={busy}>
                <Download size={16} />
                {t.settings.installJdbcRuntime}
              </button>
            )}
            {!settings.jdbc_java_home && runtime?.available && runtimeUpdate && (
              <button type="button" className="button primary runtime-status-action" onClick={() => void installManagedRuntime()} disabled={busy}>
                <Download size={16} />
                {formatMessage(t.settings.updateJdbcRuntime, { version: runtimeUpdate })}
              </button>
            )}
          </div>
        </div>
        {runtimeProgress && <JdbcRuntimeProgressView t={t} progress={runtimeProgress} />}
        <Dialog.Root open={runtimeRemoveDialogOpen} onOpenChange={(open) => !busy && setRuntimeRemoveDialogOpen(open)}>
          <Dialog.Portal>
            <Dialog.Overlay className="dialog-overlay" />
            <Dialog.Content className="policy-dialog jdbc-remove-dialog">
              <div className="dialog-titlebar">
                <div>
                  <Dialog.Title>{t.settings.removeJdbcRuntimeConfirmTitle}</Dialog.Title>
                  <Dialog.Description>{t.settings.removeJdbcRuntimeConfirm}</Dialog.Description>
                </div>
                <Dialog.Close asChild>
                  <button type="button" className="icon-button" disabled={busy} aria-label={t.common.close}><X size={18} /></button>
                </Dialog.Close>
              </div>
              <footer>
                <Dialog.Close asChild><button type="button" className="button ghost" disabled={busy}>{t.common.cancel}</button></Dialog.Close>
                <button type="button" className="button stop" disabled={busy} onClick={() => void removeManagedRuntime()}>
                  <Trash2 size={16} />
                  {t.settings.removeJdbcRuntime}
                </button>
              </footer>
            </Dialog.Content>
          </Dialog.Portal>
        </Dialog.Root>
      </section>

      <section className="panel">
        <div className="driver-section-heading">
          <div>
            <h2>{t.settings.jdbcDrivers}</h2>
          </div>
          <div className="driver-heading-actions">
            <button type="button" className="button ghost" onClick={() => setLocalDialogOpen(true)} disabled={busy || !runtime?.available}>
              <FileUp size={16} />
              {t.settings.importJdbcDriver}
            </button>
            <button type="button" className="button primary" onClick={() => setDialogOpen(true)} disabled={busy || !runtime?.available}>
            <PackagePlus size={16} />
            {t.settings.installDriver}
            </button>
          </div>
        </div>

        <div className="driver-list">
          {!status ? (
            <div className="empty-state">{t.settings.runtimeChecking}</div>
          ) : status.drivers.length === 0 ? (
            <div className="empty-state">{t.settings.noJdbcDrivers}</div>
          ) : status.drivers.map((driver) => (
            <div className="driver-row" key={driver.bundle_id}>
              <div className="driver-row-main">
                <strong>{driver.display_name}</strong>
                <code>{driver.source === "local" ? t.settings.localDriver : driver.maven_coordinate}</code>
                <span>
                  {driver.driver_classes[0] ?? t.settings.driverClassNotDetected}
                  {" · "}{formatBytes(driver.total_size)}
                </span>
              </div>
              <IconTooltip label={t.settings.deleteDriver}>
                <button type="button" className="icon-button danger" onClick={() => onDelete(driver.bundle_id)} disabled={busy}>
                  <Trash2 size={16} />
                </button>
              </IconTooltip>
            </div>
          ))}
        </div>
      </section>

      <section className="jdbc-risk-panel">
        <div className="jdbc-risk-summary">
          <div className="jdbc-risk-summary-icon" aria-hidden="true">
            <ShieldAlert size={18} />
          </div>
          <p>{t.settings.jdbcRiskSummary}</p>
          <button type="button" className="button ghost jdbc-risk-details-button" onClick={() => setDetailsOpen(true)}>
            {t.settings.jdbcRiskDetails}
          </button>
        </div>
      </section>

      <Dialog.Root open={detailsOpen} onOpenChange={setDetailsOpen}>
        <Dialog.Portal>
          <Dialog.Overlay className="dialog-overlay" />
          <Dialog.Content className="policy-dialog jdbc-risk-dialog">
            <div className="dialog-titlebar">
              <div>
                <Dialog.Title>{t.settings.jdbcRiskDetailsTitle}</Dialog.Title>
              </div>
              <Dialog.Close asChild>
                <button type="button" className="icon-button" aria-label={t.common.close}><X size={18} /></button>
              </Dialog.Close>
            </div>
            <div className="jdbc-risk-detail-body">
              <p>{t.settings.jdbcRiskIntro}</p>
              <ul>
                <li>{t.settings.jdbcRiskCompatibility}</li>
                <li>{t.settings.jdbcRiskDialect}</li>
                <li>{t.settings.jdbcRiskResources}</li>
                <li>{t.settings.jdbcRiskDriver}</li>
              </ul>
              <p className="jdbc-risk-detail-recommendation">{t.settings.jdbcRiskRecommendation}</p>
            </div>
            <footer className="jdbc-risk-dialog-footer">
              <Dialog.Close asChild>
                <button type="button" className="button primary">{t.common.close}</button>
              </Dialog.Close>
            </footer>
          </Dialog.Content>
        </Dialog.Portal>
      </Dialog.Root>

      <Dialog.Root open={dialogOpen} onOpenChange={(open) => !busy && setDialogOpen(open)}>
        <Dialog.Portal>
          <Dialog.Overlay className="dialog-overlay" />
          <Dialog.Content className="policy-dialog jdbc-install-dialog">
            <div className="dialog-titlebar">
              <div>
                <Dialog.Title>{t.settings.installDriver}</Dialog.Title>
                <Dialog.Description>{t.settings.installDriverDescription}</Dialog.Description>
              </div>
              <Dialog.Close asChild>
                <button type="button" className="icon-button" disabled={busy} aria-label={t.common.close}><X size={18} /></button>
              </Dialog.Close>
            </div>
            <form className="jdbc-install-form" onSubmit={submit}>
              {installProgress?.operation === "install" && <JdbcInstallProgressView t={t} progress={installProgress} />}
              <Field label={t.settings.driverDisplayName}>
                <input value={displayName} onChange={(event) => setDisplayName(event.target.value)} maxLength={80} required />
              </Field>
              <Field label={t.settings.mavenCoordinate}>
                <input value={coordinate} onChange={(event) => setCoordinate(event.target.value)} placeholder="groupId:artifactId:version" autoComplete="off" required />
              </Field>
              <Field label={t.settings.mavenRepository}>
                <select value={repositoryUrl} onChange={(event) => setRepositoryUrl(event.target.value)}>
                  <option value="https://repo.maven.apache.org/maven2/">Maven Central</option>
                  <option value="https://maven.aliyun.com/repository/public">Aliyun</option>
                  <option value="https://repo.huaweicloud.com/repository/maven">Huawei Cloud</option>
                  <option value="https://mirrors.cloud.tencent.com/nexus/repository/maven-public">Tencent Cloud</option>
                  <option value="custom">{t.settings.customRepository}</option>
                </select>
              </Field>
              {repositoryUrl === "custom" && <Field label={t.settings.customRepository}><input value={customRepository} onChange={(event) => setCustomRepository(event.target.value)} placeholder="https://repo.example.com/maven-public/" /></Field>}
              <div className="driver-security-note">
                <ShieldAlert size={17} />
                <span>{t.settings.jdbcDriverSecurityNotice}</span>
              </div>
              <footer>
                <Dialog.Close asChild><button type="button" className="button ghost" disabled={busy}>{t.common.cancel}</button></Dialog.Close>
                <button type="submit" className="button primary" disabled={busy || !displayName.trim() || !coordinate.trim() || (repositoryUrl === "custom" && !customRepository.trim())}>{t.settings.installDriver}</button>
              </footer>
            </form>
          </Dialog.Content>
        </Dialog.Portal>
      </Dialog.Root>

      <Dialog.Root open={localDialogOpen} onOpenChange={(open) => !busy && setLocalDialogOpen(open)}>
        <Dialog.Portal>
          <Dialog.Overlay className="dialog-overlay" />
          <Dialog.Content className="policy-dialog jdbc-install-dialog">
            <div className="dialog-titlebar">
              <div><Dialog.Title>{t.settings.importJdbcDriver}</Dialog.Title><Dialog.Description>{t.settings.importJdbcDriverDescription}</Dialog.Description></div>
              <Dialog.Close asChild><button type="button" className="icon-button" disabled={busy} aria-label={t.common.close}><X size={18} /></button></Dialog.Close>
            </div>
            <form className="jdbc-install-form" onSubmit={submitLocal}>
              {installProgress?.operation === "import" && <JdbcInstallProgressView t={t} progress={installProgress} />}
              <Field label={t.settings.driverDisplayName}><input value={localDisplayName} onChange={(event) => setLocalDisplayName(event.target.value)} maxLength={80} required /></Field>
              <div className="local-driver-picker"><button type="button" className="button ghost" onClick={() => void chooseLocalJars()} disabled={busy}><FolderOpen size={16} />{t.settings.selectJdbcJars}</button><span>{localPaths.length ? `${localPaths.length} ${t.settings.jdbcFilesSelected}` : t.settings.noJdbcFilesSelected}</span></div>
              <Field label={t.settings.jdbcDriverPathPlaceholder}><input value={localPaths.join("; ")} onChange={(event) => setLocalPaths(event.target.value.split(";").map((path) => path.trim()).filter(Boolean))} placeholder="C:\\drivers\\postgresql.jar;C:\\drivers\\lib" /></Field>
              <footer><Dialog.Close asChild><button type="button" className="button ghost" disabled={busy}>{t.common.cancel}</button></Dialog.Close><button type="submit" className="button primary" disabled={busy || !localDisplayName.trim() || localPaths.length === 0}>{t.settings.importJdbcDriver}</button></footer>
            </form>
          </Dialog.Content>
        </Dialog.Portal>
      </Dialog.Root>
    </div>
  );
}

function JdbcInstallProgressView({ t, progress }: { t: I18nMessages; progress: JdbcInstallProgress }) {
  const percent = progress.progress;
  const label = {
    preparing: t.settings.jdbcInstallPreparing,
    downloading: t.settings.jdbcInstallDownloading,
    copying: t.settings.jdbcInstallCopying,
    verifying: t.settings.jdbcInstallVerifying,
    inspecting: t.settings.jdbcInstallInspecting,
    finalizing: t.settings.jdbcInstallFinalizing
  }[progress.phase];
  return (
    <div className="jdbc-install-progress" aria-live="polite">
      <div className="jdbc-install-progress-heading">
        <span>{label}</span>
        <span>{percent === null ? t.settings.jdbcInstallInProgress : `${percent}%`}</span>
      </div>
      <div className={clsx("update-progress", percent === null && "indeterminate")} role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={percent ?? undefined}>
        <span style={{ width: percent === null ? "22%" : `${percent}%` }} />
      </div>
    </div>
  );
}

function JdbcRuntimeProgressView({ t, progress }: { t: I18nMessages; progress: JdbcRuntimeInstallProgress }) {
  const percent = progress.progress;
  const label = {
    preparing: t.settings.jdbcRuntimePreparing,
    downloading: t.settings.jdbcRuntimeDownloading,
    verifying: t.settings.jdbcRuntimeVerifying,
    extracting: t.settings.jdbcRuntimeExtracting,
    finalizing: t.settings.jdbcRuntimeFinalizing
  }[progress.phase];
  const downloaded = formatBytes(progress.downloaded_bytes);
  const total = progress.total_bytes ? formatBytes(progress.total_bytes) : null;
  return (
    <div className="jdbc-install-progress jdbc-runtime-progress" aria-live="polite">
      <div className="jdbc-install-progress-heading">
        <span>{label}</span>
        <span>{total ? `${downloaded} / ${total}` : (percent === null ? t.settings.jdbcInstallInProgress : `${percent}%`)}</span>
      </div>
      <div className={clsx("update-progress", percent === null && "indeterminate")} role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={percent ?? undefined}>
        <span style={{ width: percent === null ? "22%" : `${percent}%` }} />
      </div>
    </div>
  );
}

function StorageManagement({ t, status, busy, onRefresh, onClearJdbcCache, onOpenDataDirectory, debugLoggingEnabled }: { t: I18nMessages; status: JdbcStorageStatus | null; busy: boolean; onRefresh: () => void; onClearJdbcCache: (selection: JdbcCacheSelection) => Promise<boolean>; onOpenDataDirectory: () => void; debugLoggingEnabled: boolean }) {
  const [selectedStorageView, setSelectedStorageView] = useState<"storage" | "drivers">("storage");
  const [mavenCacheDialogOpen, setMavenCacheDialogOpen] = useState(false);
  const [cacheSelection, setCacheSelection] = useState<JdbcCacheSelection>({ maven: true, old_runtimes: true, debug_logs: false });
  const [cpuHistory, setCpuHistory] = useState<number[]>([]);
  const refreshRef = useRef(onRefresh);
  const scrollFadeRef = useScrollFade();
  useEffect(() => {
    refreshRef.current = onRefresh;
  }, [onRefresh]);
  useEffect(() => {
    if (!status) refreshRef.current();
    const timer = window.setInterval(() => refreshRef.current(), 5000);
    return () => window.clearInterval(timer);
  }, []);
  const totalCpuPercent = status?.runtimes.reduce((total, runtime) => total + Math.max(0, runtime.cpu_percent), 0) ?? 0;
  useEffect(() => {
    if (!status) return;
    const nextCpu = status.runtimes.reduce((total, runtime) => total + Math.max(0, runtime.cpu_percent), 0);
    setCpuHistory((history) => history.length ? [...history.slice(-23), nextCpu] : Array.from({ length: 12 }, () => nextCpu));
  }, [status]);
  const storageBreakdown = status?.items.map((item) => ({
    ...item,
    label: storageItemLabel(t, item.id),
    tone: storageItemTone(item.id),
    ratio: status.total_bytes > 0 ? item.bytes / status.total_bytes : 0
  })) ?? [];
  const storageBreakdownById = new Map(storageBreakdown.map((item) => [item.id, item]));
  const debugLogBytes = status?.items.find((item) => item.id === "logs")?.bytes ?? 0;
  const cpuHistoryMax = Math.max(1, ...cpuHistory);

  return (
    <div ref={scrollFadeRef} className="settings-stack storage-management" onScroll={updateScrollFade}>
      <section className="panel driver-runtime-panel">
        <div className="driver-section-heading"><div><h2>{t.settings.storagePerformance}</h2></div><IconTooltip label={t.common.refresh}><button type="button" className="icon-button" onClick={onRefresh} disabled={busy}><RefreshCw size={17} /></button></IconTooltip></div>
        <div className="overview-grid storage-overview-grid">
          <button
            type="button"
            className={clsx("metric-card", "blue", "storage-selector-card", selectedStorageView === "storage" && "selected")}
            onClick={() => setSelectedStorageView("storage")}
            aria-pressed={selectedStorageView === "storage"}
          >
            <div className="metric-icon"><HardDrive size={17} /></div>
            <div><span>{t.settings.totalStorage}</span><strong>{status ? formatBytes(status.total_bytes) : t.settings.runtimeChecking}</strong></div>
          </button>
          <button
            type="button"
            className={clsx("metric-card", "green", "storage-selector-card", selectedStorageView === "drivers" && "selected")}
            onClick={() => setSelectedStorageView("drivers")}
            aria-pressed={selectedStorageView === "drivers"}
          >
            <div className="metric-icon"><Activity size={17} /></div>
            <div><span>{t.settings.runningDrivers}</span><strong>{status ? status.runtimes.filter((runtime) => runtime.status === "running").length : "-"}</strong></div>
          </button>
        </div>
      </section>
      {selectedStorageView === "storage" ? (
        <section className="panel storage-details-panel">
          <div className="driver-section-heading">
            <div><h2>{t.settings.storageDetails}</h2></div>
            <div className="runtime-heading-actions">
                <IconTooltip label={t.settings.openDataDirectory}>
                <button type="button" className="icon-button" onClick={onOpenDataDirectory} disabled={busy} aria-label={t.settings.openDataDirectory}>
                  <FolderOpen size={17} />
                </button>
              </IconTooltip>
                <IconTooltip label={t.settings.clearJdbcCache}>
                <button type="button" className="icon-button danger" onClick={() => { setCacheSelection({ maven: Boolean(status?.maven_cache_bytes), old_runtimes: Boolean(status?.managed_runtime_old_bytes), debug_logs: !debugLoggingEnabled && Boolean(status?.items.some((item) => item.id === "logs" && item.bytes > 0)) }); setMavenCacheDialogOpen(true); }} disabled={busy || !status} aria-label={t.settings.clearJdbcCache}>
                  <Trash2 size={17} />
                </button>
              </IconTooltip>
            </div>
          </div>
          {!status ? <div className="empty-state">{t.settings.runtimeChecking}</div> : <>
            <div className="storage-usage-bar" role="img" aria-label={t.settings.storageBreakdownLabel}>{storageBreakdown.map((item) => <span key={item.id} className={clsx("storage-usage-segment", item.tone)} style={{ width: `${item.ratio * 100}%` }} />)}</div>
            <div className="storage-detail-list">{status.items.map((item) => {
              const breakdown = storageBreakdownById.get(item.id);
              return <div className="storage-detail-row" key={item.id}>
                <div className="storage-detail-label">
                  <span className={clsx("storage-breakdown-dot", breakdown?.tone ?? storageItemTone(item.id))} />
                  <strong>{storageItemLabel(t, item.id)}</strong>
                  {breakdown && <span className="storage-percent-badge">{formatPercent(breakdown.ratio)}</span>}
                </div>
                <code title={item.path}>{item.path}</code>
                <span>{formatBytes(item.bytes)}</span>
              </div>;
            })}</div>
            <Dialog.Root open={mavenCacheDialogOpen} onOpenChange={(open) => !busy && setMavenCacheDialogOpen(open)}>
              <Dialog.Portal>
                <Dialog.Overlay className="dialog-overlay" />
                <Dialog.Content className="policy-dialog jdbc-cache-dialog">
                  <div className="dialog-titlebar">
                    <div>
                      <Dialog.Title>{t.settings.clearJdbcCacheConfirmTitle}</Dialog.Title>
                      <Dialog.Description>{t.settings.clearJdbcCacheConfirmDescription}</Dialog.Description>
                    </div>
                    <Dialog.Close asChild>
                      <button type="button" className="icon-button" disabled={busy} aria-label={t.common.close}><X size={18} /></button>
                    </Dialog.Close>
                  </div>
                  <div className="jdbc-cache-options">
                    <CacheOption label={t.settings.clearMavenCacheOption} size={status.maven_cache_bytes} checked={cacheSelection.maven} disabled={status.maven_cache_bytes === 0} onChange={(checked) => setCacheSelection((value) => ({ ...value, maven: checked }))} />
                    <CacheOption label={t.settings.clearOldRuntimeOption} size={status.managed_runtime_old_bytes} checked={cacheSelection.old_runtimes} disabled={status.managed_runtime_old_bytes === 0} onChange={(checked) => setCacheSelection((value) => ({ ...value, old_runtimes: checked }))} />
                    <CacheOption label={t.settings.clearDebugLogsOption} size={debugLogBytes} checked={cacheSelection.debug_logs} disabled={debugLogBytes === 0 || debugLoggingEnabled} onChange={(checked) => setCacheSelection((value) => ({ ...value, debug_logs: checked }))} />
                    <div className="jdbc-cache-total"><span>{t.settings.selectedSpace}</span><strong>{formatBytes((cacheSelection.maven ? status.maven_cache_bytes : 0) + (cacheSelection.old_runtimes ? status.managed_runtime_old_bytes : 0) + (cacheSelection.debug_logs ? debugLogBytes : 0))}</strong></div>
                  </div>
                  <footer>
                    <Dialog.Close asChild><button type="button" className="button ghost" disabled={busy}>{t.common.cancel}</button></Dialog.Close>
                    <button type="button" className="button stop" disabled={busy || (!cacheSelection.maven && !cacheSelection.old_runtimes && !cacheSelection.debug_logs)} onClick={() => void onClearJdbcCache(cacheSelection).then((cleared) => cleared && setMavenCacheDialogOpen(false))}>
                      <Trash2 size={16} />
                      {t.settings.confirmClearJdbcCache}
                    </button>
                  </footer>
                </Dialog.Content>
              </Dialog.Portal>
            </Dialog.Root>
          </>}
        </section>
      ) : (
        <section className="panel runtime-details-panel">
          <div className="driver-section-heading"><div><h2>{t.settings.driverRuntimeUsage}</h2></div></div>
          {!status ? <div className="empty-state">{t.settings.runtimeChecking}</div> : <>
            <div className="runtime-cpu-overview">
              <div className="runtime-cpu-heading"><div><span>{t.settings.totalCpuUsage}</span><small>{t.settings.cpuUsageLive}</small></div><strong>{totalCpuPercent.toFixed(1)}%</strong></div>
              <div className="runtime-cpu-meter" role="img" aria-label={t.settings.totalCpuUsage}><span style={{ width: `${Math.min(100, totalCpuPercent)}%` }} /></div>
              <div className="runtime-cpu-history" role="img" aria-label={t.settings.cpuUsageHistory}>{cpuHistory.map((value, index) => <span key={`${index}-${value}`} style={{ height: `${Math.max(8, Math.min(100, value / cpuHistoryMax * 100))}%` }} />)}</div>
            </div>
            <div className="runtime-driver-list">{status.runtimes.length ? status.runtimes.map((runtime) => <RuntimeUsageRow key={runtime.bundle_id} runtime={runtime} t={t} />) : <div className="empty-state">{t.settings.noJdbcRuntimes}</div>}</div>
          </>}
        </section>
      )}
    </div>
  );
}

function CacheOption({ label, size, checked, disabled, onChange }: { label: string; size: number; checked: boolean; disabled: boolean; onChange: (checked: boolean) => void }) {
  return (
    <label className={clsx("jdbc-cache-option", disabled && "disabled")}>
      <input type="checkbox" checked={checked && !disabled} disabled={disabled} onChange={(event) => onChange(event.target.checked)} />
      <span>{label}</span>
      <strong>{formatBytes(size)}</strong>
    </label>
  );
}

function RuntimeUsageRow({ runtime, t }: { runtime: JdbcDriverRuntimeInfo; t: I18nMessages }) {
  const healthLabel = runtime.health === "healthy" ? t.settings.healthy : runtime.health === "stopped" ? t.settings.stopped : t.settings.unhealthy;
  const healthTone = runtime.health === "healthy" ? "available" : runtime.health === "error" ? "unavailable" : "neutral";
  return <div className="runtime-driver-row"><div className="runtime-driver-identity"><div className="permission-icon"><Activity size={16} /></div><div><strong>{runtime.display_name}</strong><span>{runtime.process_count ? `${runtime.process_count} ${t.settings.processes}` : t.settings.notRunning}</span></div></div><span className={clsx("runtime-status-badge", healthTone)}>{healthLabel}</span><div className="runtime-driver-stat"><span>CPU</span><strong>{runtime.cpu_percent.toFixed(1)}%</strong></div><div className="runtime-driver-stat"><span>{t.settings.memory}</span><strong>{formatBytes(runtime.memory_bytes)}</strong></div></div>;
}

function StatusBadge({ available, label }: { available: boolean; label: string }) {
  return <span className={clsx("runtime-status-badge", available ? "available" : "unavailable")}>{label}</span>;
}

function runtimeSourceLabel(t: I18nMessages, source: string) {
  if (source === "managed") return t.settings.runtimeSourceManaged;
  if (source === "embedded") return t.settings.runtimeSourceEmbedded;
  if (source === "external") return t.settings.runtimeSourceExternal;
  return t.settings.runtimeSourceUnavailable;
}

function storageItemLabel(t: I18nMessages, id: string) {
  if (id === "runtime") return t.settings.storageCategoryRuntime;
  if (id === "drivers") return t.settings.storageCategoryDrivers;
  if (id === "maven") return t.settings.storageCategoryMaven;
  if (id === "audit") return t.settings.storageCategoryAudit;
  if (id === "access") return t.settings.storageCategoryAccess;
  if (id === "config") return t.settings.storageCategoryConfig;
  if (id === "logs") return t.settings.storageCategoryLogs;
  if (id === "other") return t.settings.storageCategoryOther;
  return id;
}

function storageItemTone(id: string) {
  if (id === "runtime") return "runtime";
  if (id === "drivers") return "drivers";
  if (id === "maven") return "maven";
  if (id === "audit") return "audit";
  if (id === "access") return "access";
  if (id === "logs") return "logs";
  if (id === "other") return "other";
  return "config";
}

function formatBytes(bytes: number) {
  if (bytes === 0) return "0 KiB";
  if (bytes < 1024 * 1024) return `${Math.max(1, Math.round(bytes / 1024))} KiB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MiB`;
}

function formatPercent(ratio: number) {
  const percent = Math.max(0, ratio * 100);
  return percent > 0 && percent < 0.1 ? "<0.1%" : `${percent.toFixed(1)}%`;
}
export function AboutUpdateSection({
  t,
  enabled,
  state,
  autoCheckUpdates,
  onAutoCheckUpdatesChange,
  onCheck,
  onUpdate,
  onOpenProjectReleases,
  children
}: {
  t: I18nMessages;
  enabled: boolean;
  state: UpdateState;
  autoCheckUpdates: boolean;
  onAutoCheckUpdatesChange: (checked: boolean) => void;
  onCheck: () => void;
  onUpdate: () => void;
  onOpenProjectReleases: () => void;
  children?: ReactNode;
}) {
  let icon: ReactNode = <RefreshCw size={19} />;
  let title = t.updates.readyTitle;
  let description = t.updates.readyDescription;

  if (!enabled || state.kind === "disabled") {
    icon = <Monitor size={19} />;
    title = t.updates.localBuildTitle;
    description = t.updates.localBuildDescription;
  } else if (state.kind === "checking") {
    icon = <RefreshCw size={19} />;
    title = t.updates.checkingTitle;
    description = t.updates.checkingDescription;
  } else if (state.kind === "up-to-date") {
    icon = <CheckCircle2 size={19} />;
    title = t.updates.upToDateTitle;
    description = formatMessage(t.updates.upToDateDescription, { version: APP_VERSION });
  } else if (state.kind === "available") {
    icon = <Download size={19} />;
    title = t.updates.availableTitle;
    description = formatMessage(t.updates.availableDescription, { version: state.version });
  } else if (state.kind === "downloading") {
    icon = <Download size={19} />;
    title = t.updates.downloadingTitle;
    description = state.total
      ? formatMessage(t.updates.downloadingProgress, {
          progress: Math.min(100, Math.round((state.downloaded / state.total) * 100))
        })
      : t.updates.downloadingDescription;
  } else if (state.kind === "relaunching") {
    icon = <RefreshCw size={19} />;
    title = t.updates.relaunchingTitle;
    description = t.updates.relaunchingDescription;
  } else if (state.kind === "error") {
    icon = <AlertTriangle size={19} />;
    if (state.phase === "download") {
      title = t.updates.downloadFailedTitle;
      description = t.updates.downloadFailedDescription;
    } else if (state.phase === "relaunch") {
      title = t.updates.relaunchFailedTitle;
      description = t.updates.relaunchFailedDescription;
    } else {
      title = t.updates.checkFailedTitle;
      description = t.updates.checkFailedDescription;
    }
  }

  const progress = state.kind === "downloading" && state.total
    ? Math.min(100, Math.round((state.downloaded / state.total) * 100))
    : null;

  return (
    <section className={clsx("about-update-section", "state-" + state.kind)}>
      <div className={clsx("about-update-icon", (state.kind === "checking" || state.kind === "relaunching") && "is-spinning")}>
        {icon}
      </div>
      <div className="about-update-content">
        <strong>{title}</strong>
        <p>{description}</p>
      </div>
      <div className="about-update-actions">
        {state.kind === "downloading" && (
          <div
            className={clsx("update-progress", progress === null && "indeterminate")}
            role="progressbar"
            aria-label={t.updates.downloadingTitle}
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={progress ?? undefined}
          >
            <span style={{ width: progress === null ? "22%" : String(progress) + "%" }} />
          </div>
        )}
        {(!enabled || state.kind === "disabled") && (
          <button type="button" className="button soft" onClick={onOpenProjectReleases}>
            <ExternalLink size={16} />
            {t.updates.openReleases}
          </button>
        )}
        {enabled && (state.kind === "idle" || state.kind === "up-to-date" || (state.kind === "error" && state.phase === "check")) && (
          <button type="button" className="button soft" onClick={onCheck}>
            <RefreshCw size={16} />
            {state.kind === "error" ? t.updates.retry : t.updates.checkNow}
          </button>
        )}
        {state.kind === "available" && (
          <>
            <button type="button" className="button soft" onClick={onOpenProjectReleases}>
              <ExternalLink size={16} />
              {t.updates.viewReleaseNotes}
            </button>
            <button type="button" className="button primary" onClick={onUpdate}>
              <Download size={16} />
              {t.updates.updateNow}
            </button>
          </>
        )}
        {state.kind === "error" && state.phase === "download" && (
          <>
            <button type="button" className="button soft" onClick={onOpenProjectReleases}>
              <ExternalLink size={16} />
              {t.updates.manualDownload}
            </button>
            <button type="button" className="button primary" onClick={onUpdate}>
              <Download size={16} />
              {t.updates.retry}
            </button>
          </>
        )}
        {state.kind === "error" && state.phase === "relaunch" && (
          <button type="button" className="button soft" onClick={onOpenProjectReleases}>
            <ExternalLink size={16} />
            {t.updates.openReleases}
          </button>
        )}
      </div>
      <div className="about-update-preferences">
        <label className="about-update-switch-row">
          <span>{t.settings.autoCheckUpdates}</span>
          <Switch.Root className="switch" checked={enabled && autoCheckUpdates} disabled={!enabled} onCheckedChange={onAutoCheckUpdatesChange}>
            <Switch.Thumb className="switch-thumb" />
          </Switch.Root>
        </label>
        {children}
      </div>
    </section>
  );
}
