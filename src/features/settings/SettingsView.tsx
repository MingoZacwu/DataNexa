import * as Dialog from "@radix-ui/react-dialog";
import * as Switch from "@radix-ui/react-switch";
import clsx from "clsx";
import {
  AlertTriangle,
  CheckCircle2,
  Download,
  ExternalLink,
  FileDown,
  FileText,
  FileUp,
  Github,
  Home,
  KeyRound,
  ListChecks,
  Monitor,
  RefreshCw,
  SearchCheck, ShieldAlert,
  ShieldCheck,
  ShieldOff,
  X
} from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import appConfig from "../../../app.config.json";
import appIconUrl from "../../../resources/icon.png";
import { formatMessage, languageOptions, normalizeLocale, type I18nMessages, type Locale } from "../../i18n";
import type { AppSnapshot, DatabaseType, PolicyCheckResult, ServerConfig, SettingsConfig } from "../../types";
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
  policySql,
  policyKind,
  policyResult,
  updaterEnabled,
  updateState,
  onCheckUpdate,
  onUpdate,
  onOpenProjectReleases,
  onTabChange,
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
  policySql: string;
  policyKind: DatabaseType;
  policyResult: PolicyCheckResult | null;
  updaterEnabled: boolean;
  updateState: UpdateState;
  onCheckUpdate: () => void;
  onUpdate: () => void;
  onOpenProjectReleases: () => void;
  onTabChange: (tab: SettingsTab) => void;
  onThemeChange: (theme: ThemeMode) => void;
  onPolicyKindChange: (kind: DatabaseType) => void;
  onSqlChange: (sql: string) => void;
  onPolicyCheck: () => void;
  onSaveServer: (server: ServerConfig) => Promise<boolean>;
  onSaveSettings: (settings: SettingsConfig, applyAutoStart?: boolean) => void;
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
        && current.language === settings.language;
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
