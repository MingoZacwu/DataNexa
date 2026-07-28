import * as Dialog from "@radix-ui/react-dialog";
import clsx from "clsx";
import { AlertTriangle, Download, Minus, Monitor, Moon, Play, RefreshCw, Square, Sun, Trash2, X } from "lucide-react";
import type { MouseEvent as ReactMouseEvent, ReactNode } from "react";
import { formatMessage, type I18nMessages } from "../i18n";
import { api } from "../lib/tauri";
import type { AuditMigrationState } from "../types";
import type { EffectiveTheme, ThemeMode } from "../app/types";
import { IconTooltip } from "./ui";

export function WindowDragRegion() {
  function handleDragStart(event: ReactMouseEvent<HTMLDivElement>) {
    if (event.button !== 0) return;
    if (event.detail > 1) {
      event.preventDefault();
      return;
    }
    void api.startWindowDrag().catch(() => undefined);
  }

  return (
    <div className="window-drag-region" onMouseDown={handleDragStart} aria-hidden="true" />
  );
}

export function WindowControls({ t }: { t: I18nMessages }) {
  return (
    <div className="window-controls">
      <button type="button" className="window-control minimize" onClick={() => void api.minimizeWindow().catch(() => undefined)} aria-label={t.common.minimize}>
        <Minus size={13} />
      </button>
      <button type="button" className="window-control close" onClick={() => void api.hideWindow().catch(() => undefined)} aria-label={t.common.close}>
        <X size={13} />
      </button>
    </div>
  );
}

export function AuditMigrationReminder({ t, state, onOpen }: { t: I18nMessages; state: Exclude<AuditMigrationState, { status: "ready" }>; onOpen: () => void }) {
  if (state.status === "failed") {
    return (
      <button type="button" className="sidebar-migration-reminder failed" onClick={onOpen}>
        <span className="sidebar-migration-icon"><AlertTriangle size={16} /></span>
        <span className="sidebar-migration-copy"><strong>{t.auditMigration.failedTitle}</strong><span>{t.auditMigration.failedCompact}</span></span>
      </button>
    );
  }
  const finishing = state.phase === "committing" || state.phase === "finalizing";
  const percent = state.total > 0 ? Math.min(100, Math.round((state.processed / state.total) * 100)) : 0;
  return (
    <div className="sidebar-migration-reminder migrating">
      <span className="sidebar-migration-icon"><RefreshCw className="is-spinning" size={16} /></span>
      <span className="sidebar-migration-copy"><strong>{t.auditMigration.migratingTitle}</strong><span>{finishing ? t.auditMigration.finishing : state.total > 0 ? formatMessage(t.auditMigration.progress, { processed: state.processed, total: state.total }) : t.auditMigration.preparing}</span></span>
      <span className="migration-progress" role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={percent}><span style={{ width: `${percent}%` }} /></span>
    </div>
  );
}

export function AuditMigrationDialog({ t, state, open, busy, confirmClear, onOpenChange, onRetry, onRequestClear, onCancelClear, onConfirmClear }: { t: I18nMessages; state: Extract<AuditMigrationState, { status: "failed" }>; open: boolean; busy: boolean; confirmClear: boolean; onOpenChange: (open: boolean) => void; onRetry: () => void; onRequestClear: () => void; onCancelClear: () => void; onConfirmClear: () => void }) {
  return (
    <Dialog.Root open={open} onOpenChange={(next) => { if (!busy) { if (!next) onCancelClear(); onOpenChange(next); } }}>
      <Dialog.Portal>
        <Dialog.Overlay className="dialog-overlay" />
        <Dialog.Content className={clsx("policy-dialog migration-dialog", confirmClear && "confirming")}>
          <div className="dialog-titlebar">
            <div className="migration-dialog-copy">
              <Dialog.Title>{confirmClear ? t.auditMigration.clearConfirmTitle : t.auditMigration.failedTitle}</Dialog.Title>
              <Dialog.Description>{confirmClear ? t.auditMigration.clearConfirmDescription : t.auditMigration.dialogDescription}</Dialog.Description>
            </div>
            <Dialog.Close asChild><button type="button" className="icon-button" disabled={busy} aria-label={t.common.close}><X size={17} /></button></Dialog.Close>
          </div>
          {!confirmClear && (
            <div className="migration-error" role="alert">
              <span className="migration-error-icon"><AlertTriangle size={16} /></span>
              <div><strong>{t.auditMigration.errorReason}</strong><p>{state.reason}</p></div>
            </div>
          )}
          <div className="migration-dialog-actions">
            {confirmClear ? (
              <>
                <button type="button" className="button ghost" disabled={busy} onClick={onCancelClear}>{t.common.cancel}</button>
                <button type="button" className="button danger" disabled={busy} onClick={onConfirmClear}><Trash2 size={15} />{t.auditMigration.confirmClear}</button>
              </>
            ) : (
              <>
                <button type="button" className="button" disabled={busy} onClick={onRequestClear}>{t.auditMigration.clear}</button>
                <button type="button" className="button primary" autoFocus disabled={busy} onClick={onRetry}><RefreshCw size={15} className={clsx(busy && "is-spinning")} />{t.auditMigration.retry}</button>
              </>
            )}
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

export function SidebarUpdateReminder({
  t,
  version,
  onOpenAbout,
  onDismiss
}: {
  t: I18nMessages;
  version: string;
  onOpenAbout: () => void;
  onDismiss: () => void;
}) {
  return (
    <div className="sidebar-update-reminder">
      <button type="button" className="sidebar-update-main" onClick={onOpenAbout}>
        <span className="sidebar-update-icon"><Download size={16} /></span>
        <span className="sidebar-update-copy">
          <strong>{t.updates.availableTitle}</strong>
          <span>{formatMessage(t.updates.availableCompact, { version })}</span>
        </span>
      </button>
      <IconTooltip label={t.updates.dismissReminder}>
        <button type="button" className="sidebar-update-dismiss" onClick={onDismiss} aria-label={t.updates.dismissReminder}>
          <X size={14} />
        </button>
      </IconTooltip>
    </div>
  );
}

export function SidebarFooter({
  t,
  running,
  startupFailed,
  port,
  busy,
  disabled,
  onToggle
}: {
  t: I18nMessages;
  running: boolean;
  startupFailed: boolean;
  port: number;
  busy: boolean;
  disabled: boolean;
  onToggle: () => void;
}) {
  const toggleLabel = running ? t.server.stop : t.server.start;
  return (
    <div className="sidebar-footer">
      <div className={clsx("sidebar-status-line", startupFailed ? "error" : running && "running")}>
        <span className="status-orb" />
        <span>{startupFailed ? t.sidebar.serverFailed : running ? formatMessage(t.sidebar.serverRunning, { port }) : t.sidebar.serverStopped}</span>
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

export function NavButton({ icon, label, active, onClick }: { icon: ReactNode; label: string; active: boolean; onClick: () => void }) {
  return (
    <button type="button" className={clsx("nav-button", active && "active")} onClick={onClick}>
      {icon}
      <span>{label}</span>
    </button>
  );
}

export function ThemeModeControl({
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


