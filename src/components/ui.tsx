import * as Switch from "@radix-ui/react-switch";
import * as Tooltip from "@radix-ui/react-tooltip";
import clsx from "clsx";
import { AlertTriangle, CheckCircle2, Clipboard, Info, Plus, X } from "lucide-react";
import { createPortal } from "react-dom";
import type { ReactNode } from "react";
import type { I18nMessages } from "../i18n";
import type { AuditEvent } from "../types";
import type { ToastMessage } from "../app/types";
import { statusLabel, statusTone, toolDisplayName } from "../app/utils";

function formatCompactEventTimestamp(timestamp: string) {
  const date = new Date(timestamp);
  const now = new Date();
  const isToday = date.getFullYear() === now.getFullYear()
    && date.getMonth() === now.getMonth()
    && date.getDate() === now.getDate();

  if (isToday) return date.toLocaleTimeString();
  return date.toLocaleDateString(undefined, date.getFullYear() === now.getFullYear()
    ? { month: "2-digit", day: "2-digit" }
    : { year: "numeric", month: "2-digit", day: "2-digit" });
}

export function EventList({ t, events, onSelect }: { t: I18nMessages; events: AuditEvent[]; onSelect?: (event: AuditEvent) => void }) {
  if (events.length === 0) {
    return <div className="empty-state compact">{t.audit.emptyCompact}</div>;
  }

  return (
    <div className="event-list">
      {events.map((event) => (
        <button type="button" className="event-item" key={event.id} onClick={() => onSelect?.(event)}>
          <span className={clsx("event-dot", statusTone(event.status))} />
          <time dateTime={event.timestamp} title={new Date(event.timestamp).toLocaleString()}>{formatCompactEventTimestamp(event.timestamp)}</time>
          <span>{event.reason ?? toolDisplayName(t, event.tool)}</span>
          <StatusPill tone={statusTone(event.status)} label={statusLabel(t, event.status)} />
        </button>
      ))}
    </div>
  );
}

export function ToastViewport({ t, toasts, onDismiss }: { t: I18nMessages; toasts: ToastMessage[]; onDismiss: (id: string) => void }) {
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

export function PanelHeader({ title, action, onAction, disabled }: { title: string; action?: ReactNode; onAction?: () => void; disabled?: boolean }) {
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

export function PanelIconAction({ icon, label, className, onClick, disabled }: { icon: ReactNode; label: string; className?: string; onClick: () => void; disabled?: boolean }) {
  return (
    <IconTooltip label={label}>
      <button type="button" className={clsx("panel-icon-action", className)} onClick={onClick} disabled={disabled} aria-label={label}>
        {icon}
      </button>
    </IconTooltip>
  );
}

export function FormSection({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="form-section">
      <h3>{title}</h3>
      <div className="form-grid">{children}</div>
    </section>
  );
}

export function Field({ label, span, children }: { label: string; span?: boolean; children: ReactNode }) {
  return (
    <label className={clsx("field", span && "span-all")}>
      <span>{label}</span>
      {children}
    </label>
  );
}

export function SwitchField({
  label,
  tooltip,
  checked,
  disabled,
  onCheckedChange
}: {
  label: string;
  tooltip?: string;
  checked: boolean;
  disabled?: boolean;
  onCheckedChange: (checked: boolean) => void;
}) {
  return (
    <div className="switch-row">
      <span className="switch-label">
        {label}
        {tooltip && (
          <IconTooltip label={tooltip}>
            <button type="button" className="switch-info" aria-label={tooltip}>
              <Info size={13} />
            </button>
          </IconTooltip>
        )}
      </span>
      <Switch.Root className="switch" checked={checked} disabled={disabled} onCheckedChange={onCheckedChange} aria-label={label}>
        <Switch.Thumb className="switch-thumb" />
      </Switch.Root>
    </div>
  );
}

export function IconTooltip({ label, children }: { label: string; children: ReactNode }) {
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

export function MetricIcon({ icon }: { icon: ReactNode }) {
  return <div className="metric-icon">{icon}</div>;
}

export function MetricValue({ value, suffix }: { value: ReactNode; suffix?: string }) {
  return (
    <div className="metric-value">
      <strong>{value}</strong>
      {suffix && <small>{suffix}</small>}
    </div>
  );
}

export function QuickStep({ image, title, text, wide, actionLabel, onAction }: { image: string; title: string; text: string; wide?: boolean; actionLabel?: string; onAction?: () => void }) {
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

export function StatusPill({ tone, label }: { tone: "green" | "blue" | "amber" | "red" | "slate"; label: string }) {
  return <span className={clsx("status-pill", tone)}>{label}</span>;
}
