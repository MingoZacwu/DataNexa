import * as Dialog from "@radix-ui/react-dialog";
import * as Switch from "@radix-ui/react-switch";
import { open } from "@tauri-apps/plugin-dialog";
import clsx from "clsx";
import {
  AlertTriangle,
  CheckCircle2,
  Download,
  ExternalLink,
  FileDown,
  FileText,
  FileUp,
  FolderOpen,
  HardDrive,
  Activity,
  Github,
  Home,
  KeyRound,
  ListChecks,
  Monitor,
  PackagePlus,
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
import type { AppSnapshot, DatabaseType, ImportJdbcDriverInput, InstallJdbcDriverInput, JdbcDriverRuntimeInfo, JdbcStatus, JdbcStorageStatus, PolicyCheckResult, ServerConfig, SettingsConfig } from "../../types";
import type { UpdateState } from "../../lib/updater";
import type { EffectiveTheme, SettingsTab, ThemeMode } from "../../app/types";
import { updateScrollFade } from "../../app/utils";
import { Field, IconTooltip, SwitchField } from "../../components/ui";
import { ThemeModeControl } from "../../components/chrome";

const APP_VERSION = appConfig.version;

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
  jdbcStatus,
  jdbcStorageStatus,
  policySql,
  policyKind,
  policyResult,
  updaterEnabled,
  updateState,
  onCheckUpdate,
  onUpdate,
  onOpenProjectReleases,
  onTabChange,
  onRefreshJdbcStatus,
  onRefreshJdbcStorageStatus,
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
  jdbcStatus: JdbcStatus | null;
  jdbcStorageStatus: JdbcStorageStatus | null;
  policySql: string;
  policyKind: DatabaseType;
  policyResult: PolicyCheckResult | null;
  updaterEnabled: boolean;
  updateState: UpdateState;
  onCheckUpdate: () => void;
  onUpdate: () => void;
  onOpenProjectReleases: () => void;
  onTabChange: (tab: SettingsTab) => void;
  onRefreshJdbcStatus: () => void;
  onRefreshJdbcStorageStatus: () => void;
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
  const [auditMaxEventsDraft, setAuditMaxEventsDraft] = useState(String(settings.audit_max_events));
  const serverDraftDirty = useRef(false);
  const settingsDraftDirty = useRef(false);
  const [policyDialogOpen, setPolicyDialogOpen] = useState(false);
  const [exportDialogOpen, setExportDialogOpen] = useState(false);
  const [exportAcknowledged, setExportAcknowledged] = useState(false);
  const [bearerWarningOpen, setBearerWarningOpen] = useState(false);

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
      const saved = current.audit_max_events === settings.audit_max_events
        && current.audit_redact_sql_literals === settings.audit_redact_sql_literals
        && current.auto_check_updates === settings.auto_check_updates
        && current.auto_start_mcp === settings.auto_start_mcp
        && current.auto_lightweight_mode === settings.auto_lightweight_mode
        && current.mcp_activity_effects === settings.mcp_activity_effects
        && current.language === settings.language
        && (current.jdbc_java_home ?? null) === (settings.jdbc_java_home ?? null);
      if (saved) settingsDraftDirty.current = false;
      if (saved) setAuditMaxEventsDraft(String(settings.audit_max_events));
      return saved ? settings : current;
    });
    if (!settingsDraftDirty.current) setAuditMaxEventsDraft(String(settings.audit_max_events));
  }, [settings]);
  useEffect(() => {
    setSettingsDraft((current) => ({ ...current, language: locale }));
  }, [locale]);

  return (
    <section className="settings-page">
      <div className="settings-tabs">
        <button type="button" className={clsx(tab === "general" && "active")} onClick={() => onTabChange("general")}>
          {t.settings.general}
        </button>
        <button type="button" className={clsx(tab === "drivers" && "active")} onClick={() => onTabChange("drivers")}>
          {t.settings.driverManagement}
        </button>
        <button type="button" className={clsx(tab === "storage" && "active")} onClick={() => onTabChange("storage")}>
          {t.settings.storagePerformance}
        </button>
        <button type="button" className={clsx(tab === "about" && "active")} onClick={() => onTabChange("about")}>
          {t.settings.about}
        </button>
      </div>

      {tab === "general" ? (
        <div className="settings-stack" onScroll={updateScrollFade}>
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
              <Field label={t.settings.auditMaxEvents}>
                <input
                  type="number"
                  min={1}
                  max={5000}
                  value={auditMaxEventsDraft}
                  onChange={(event) => {
                    settingsDraftDirty.current = true;
                    setAuditMaxEventsDraft(event.target.value);
                    setSettingsDraft({ ...settingsDraft, audit_max_events: Number(event.target.value) || 0 });
                  }}
                  onBlur={(event) => {
                    const auditMaxEvents = Math.max(1, Math.min(5000, Number(event.currentTarget.value) || settings.audit_max_events));
                    setAuditMaxEventsDraft(String(auditMaxEvents));
                    setSettingsDraft((current) => ({ ...current, audit_max_events: auditMaxEvents }));
                    onSaveSettings({ ...settingsDraft, audit_max_events: auditMaxEvents });
                  }}
                />
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
      ) : tab === "drivers" ? (
        <DriverManagement
          t={t}
          status={jdbcStatus}
          settings={settings}
          busy={busy}
          onRefresh={onRefreshJdbcStatus}
          onInstall={onInstallJdbcDriver}
          onImport={onImportJdbcDriver}
          onDelete={onDeleteJdbcDriver}
          onSaveSettings={onSaveSettings}
        />
      ) : tab === "storage" ? (
        <StorageManagement status={jdbcStorageStatus} busy={busy} onRefresh={onRefreshJdbcStorageStatus} t={t} />
      ) : (
        <div className="settings-stack" onScroll={updateScrollFade}>
          <section className="panel about-panel">
            <div className="about-hero">
              <img src={appIconUrl} alt="DataNexa" />
              <div>
                <h2>DataNexa <span className="version-badge">v{APP_VERSION}</span></h2>
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
            />
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
  settings,
  busy,
  onRefresh,
  onInstall,
  onImport,
  onDelete,
  onSaveSettings
}: {
  t: I18nMessages;
  status: JdbcStatus | null;
  settings: SettingsConfig;
  busy: boolean;
  onRefresh: () => void;
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

  async function useBundledRuntime() {
    await onSaveSettings({ ...settings, jdbc_java_home: null });
    onRefresh();
  }

  return (
    <div className="settings-stack driver-management" onScroll={updateScrollFade}>
      <section className="panel driver-runtime-panel">
        <div className="driver-section-heading">
          <div>
            <h2>{t.settings.jdbcRuntime}</h2>
            <p>{t.settings.jdbcRuntimeDescription}</p>
          </div>
          <div className="runtime-heading-actions">
            <IconTooltip label={t.settings.selectJavaRuntime}>
              <button type="button" className="icon-button" onClick={() => void chooseExternalRuntime()} disabled={busy} aria-label={t.settings.selectJavaRuntime}>
                <FolderOpen size={17} />
              </button>
            </IconTooltip>
            {settings.jdbc_java_home && (
              <IconTooltip label={t.settings.useBundledRuntime}>
                <button type="button" className="icon-button" onClick={() => void useBundledRuntime()} disabled={busy} aria-label={t.settings.useBundledRuntime}>
                  <RotateCcw size={17} />
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
          <span>{runtime ? formatMessage(t.settings.runtimeSource, { source: runtimeSourceLabel(t, runtime.source) }) : t.settings.runtimeChecking}</span>
        </div>
      </section>

      <section className="panel">
        <div className="driver-section-heading">
          <div>
            <h2>{t.settings.jdbcDrivers}</h2>
            <p>{t.settings.jdbcDriversDescription}</p>
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

function StorageManagement({ t, status, busy, onRefresh }: { t: I18nMessages; status: JdbcStorageStatus | null; busy: boolean; onRefresh: () => void }) {
  const [selectedStorageView, setSelectedStorageView] = useState<"storage" | "drivers">("storage");
  const [cpuHistory, setCpuHistory] = useState<number[]>([]);
  const refreshRef = useRef(onRefresh);
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
  const cpuHistoryMax = Math.max(1, ...cpuHistory);

  return (
    <div className="settings-stack storage-management" onScroll={updateScrollFade}>
      <section className="panel driver-runtime-panel">
        <div className="driver-section-heading"><div><h2>{t.settings.storagePerformance}</h2><p>{t.settings.storagePerformanceDescription}</p></div><IconTooltip label={t.common.refresh}><button type="button" className="icon-button" onClick={onRefresh} disabled={busy}><RefreshCw size={17} /></button></IconTooltip></div>
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
          <div className="driver-section-heading"><div><h2>{t.settings.storageDetails}</h2><p>{t.settings.storageDetailsDescription}</p></div></div>
          {!status ? <div className="empty-state">{t.settings.runtimeChecking}</div> : <>
            <div className="storage-usage-bar" role="img" aria-label={t.settings.storageBreakdownLabel}>{storageBreakdown.map((item) => <span key={item.id} className={clsx("storage-usage-segment", item.tone)} style={{ width: `${item.ratio * 100}%` }} />)}</div>
            <div className="storage-breakdown">{storageBreakdown.map((item) => <div className="storage-breakdown-row" key={item.id}><div className="storage-breakdown-label"><span className={clsx("storage-breakdown-dot", item.tone)} />{item.label}</div><strong>{formatPercent(item.ratio)}</strong></div>)}</div>
            <div className="storage-detail-list">{status.items.map((item) => <div className="storage-detail-row" key={item.id}><strong>{storageItemLabel(t, item.id)}</strong><code title={item.path}>{item.path}</code><span>{formatBytes(item.bytes)}</span></div>)}</div>
          </>}
        </section>
      ) : (
        <section className="panel runtime-details-panel">
          <div className="driver-section-heading"><div><h2>{t.settings.driverRuntimeUsage}</h2><p>{t.settings.driverRuntimeUsageDescription}</p></div></div>
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

function RuntimeUsageRow({ runtime, t }: { runtime: JdbcDriverRuntimeInfo; t: I18nMessages }) {
  const healthLabel = runtime.health === "healthy" ? t.settings.healthy : runtime.health === "stopped" ? t.settings.stopped : t.settings.unhealthy;
  const healthTone = runtime.health === "healthy" ? "available" : runtime.health === "error" ? "unavailable" : "neutral";
  return <div className="runtime-driver-row"><div className="runtime-driver-identity"><div className="permission-icon"><Activity size={16} /></div><div><strong>{runtime.display_name}</strong><span>{runtime.process_count ? `${runtime.process_count} ${t.settings.processes}` : t.settings.notRunning}</span></div></div><span className={clsx("runtime-status-badge", healthTone)}>{healthLabel}</span><div className="runtime-driver-stat"><span>CPU</span><strong>{runtime.cpu_percent.toFixed(1)}%</strong></div><div className="runtime-driver-stat"><span>{t.settings.memory}</span><strong>{formatBytes(runtime.memory_bytes)}</strong></div></div>;
}

function StatusBadge({ available, label }: { available: boolean; label: string }) {
  return <span className={clsx("runtime-status-badge", available ? "available" : "unavailable")}>{label}</span>;
}

function runtimeSourceLabel(t: I18nMessages, source: string) {
  if (source === "embedded") return t.settings.runtimeSourceEmbedded;
  if (source === "external") return t.settings.runtimeSourceExternal;
  return t.settings.runtimeSourceUnavailable;
}

function storageItemLabel(t: I18nMessages, id: string) {
  if (id === "runtime") return t.settings.storageCategoryRuntime;
  if (id === "drivers") return t.settings.storageCategoryDrivers;
  if (id === "audit") return t.settings.storageCategoryAudit;
  if (id === "access") return t.settings.storageCategoryAccess;
  if (id === "config") return t.settings.storageCategoryConfig;
  return id;
}

function storageItemTone(id: string) {
  if (id === "runtime") return "runtime";
  if (id === "drivers") return "drivers";
  if (id === "audit") return "audit";
  if (id === "access") return "access";
  return "config";
}

function formatBytes(bytes: number) {
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
  onOpenProjectReleases
}: {
  t: I18nMessages;
  enabled: boolean;
  state: UpdateState;
  autoCheckUpdates: boolean;
  onAutoCheckUpdatesChange: (checked: boolean) => void;
  onCheck: () => void;
  onUpdate: () => void;
  onOpenProjectReleases: () => void;
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
      </div>
    </section>
  );
}
