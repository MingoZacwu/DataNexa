import * as Dialog from "@radix-ui/react-dialog";
import clsx from "clsx";
import { AlertTriangle, Check, Download, Languages, Minus, Monitor, Moon, Play, RefreshCw, Square, Sun, Trash2, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import type { MouseEvent as ReactMouseEvent, ReactNode } from "react";
import { formatMessage, languageOptions, type I18nMessages, type Locale } from "../i18n";
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
  onDismiss,
  icon,
  title,
  compact
}: {
  t: I18nMessages;
  version: string;
  onOpenAbout: () => void;
  onDismiss: () => void;
  icon?: ReactNode;
  title?: string;
  compact?: string;
}) {
  return (
    <div className="sidebar-update-reminder">
      <button type="button" className="sidebar-update-main" onClick={onOpenAbout}>
        <span className="sidebar-update-icon">{icon ?? <Download size={16} />}</span>
        <span className="sidebar-update-copy">
          <strong>{title ?? t.updates.availableTitle}</strong>
          <span>{compact ?? formatMessage(t.updates.availableCompact, { version })}</span>
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

/**
 * Language entry point. Every label is written in its own language and the
 * tooltip is bilingual, so a user who cannot read the current interface can
 * still find the switch.
 */
export function LanguageSwitcher({ t, locale, disabled, onChange }: { t: I18nMessages; locale: Locale; disabled?: boolean; onChange: (locale: Locale) => void }) {
  const [open, setOpen] = useState(false);
  const [anchor, setAnchor] = useState<{ top: number; right: number } | null>(null);
  const buttonRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    function handlePointerDown(event: MouseEvent) {
      const target = event.target as Node;
      if (menuRef.current?.contains(target) || buttonRef.current?.contains(target)) return;
      setOpen(false);
    }
    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") setOpen(false);
    }
    // The anchor is a fixed viewport position, so close on any layout change
    // that could invalidate it.
    function handleLayoutChange() {
      setOpen(false);
    }
    document.addEventListener("mousedown", handlePointerDown);
    document.addEventListener("keydown", handleKeyDown);
    window.addEventListener("resize", handleLayoutChange);
    window.addEventListener("scroll", handleLayoutChange, true);
    return () => {
      document.removeEventListener("mousedown", handlePointerDown);
      document.removeEventListener("keydown", handleKeyDown);
      window.removeEventListener("resize", handleLayoutChange);
      window.removeEventListener("scroll", handleLayoutChange, true);
    };
  }, [open]);

  // The menu is portalled so the workspace scroll container cannot clip it.
  function openMenu() {
    const rect = buttonRef.current?.getBoundingClientRect();
    if (rect) setAnchor({ top: rect.bottom + 6, right: window.innerWidth - rect.right });
    setOpen(true);
  }

  return (
    <div className="language-switcher">
      <IconTooltip label={t.settings.languageSwitch}>
        <button
          ref={buttonRef}
          type="button"
          className={clsx("icon-button", open && "active-filter")}
          onClick={() => (open ? setOpen(false) : openMenu())}
          disabled={disabled}
          aria-haspopup="menu"
          aria-expanded={open}
          aria-label={t.settings.languageSwitch}
        >
          <Languages size={17} />
        </button>
      </IconTooltip>
      {open && anchor && createPortal(
        <div
          className="language-menu"
          ref={menuRef}
          role="menu"
          aria-label={t.settings.languageSwitch}
          style={{ top: anchor.top, right: anchor.right }}
        >
          {languageOptions.map((option) => (
            <button
              key={option.value}
              type="button"
              role="menuitemradio"
              aria-checked={locale === option.value}
              className={clsx("language-menu-item", locale === option.value && "active")}
              onClick={() => {
                setOpen(false);
                if (locale !== option.value) onChange(option.value);
              }}
            >
              <span className="language-menu-check">{locale === option.value && <Check size={14} />}</span>
              <span lang={option.value}>{option.nativeLabel}</span>
            </button>
          ))}
        </div>,
        document.body
      )}
    </div>
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


