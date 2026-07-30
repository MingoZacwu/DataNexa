import * as Dialog from "@radix-ui/react-dialog";
import clsx from "clsx";
import { CalendarDays, ChevronLeft, ChevronRight, Clipboard, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { formatMessage, type I18nMessages } from "../../i18n";
import type { AuditEvent, ConnectionConfig, McpToolInfo } from "../../types";
import type { AuditFilters } from "../../app/types";
import { statusLabel, statusTone, toolDisplayName } from "../../app/utils";
import { StatusPill } from "../../components/ui";

const AUDIT_PAGE_SIZE = 50;

function parseAuditDate(value: string, endOfDay = false) {
  const normalized = value.trim().replace(/[年月]/g, "-").replace(/日$/, "").replace(/\//g, "-");
  if (!/^\d{4}-\d{1,2}-\d{1,2}$/.test(normalized)) return null;
  const date = new Date(`${normalized}${endOfDay ? "T23:59:59.999" : "T00:00:00"}`);
  return Number.isNaN(date.getTime()) ? null : date;
}

export function AuditDateField({ t, value, minDate, maxDate, onChange }: { t: I18nMessages; value: string; minDate?: Date | null; maxDate?: Date | null; onChange: (value: string) => void }) {
  const initial = parseAuditDate(value) ?? new Date();
  const [open, setOpen] = useState(false);
  const [month, setMonth] = useState(new Date(initial.getFullYear(), initial.getMonth(), 1));
  const fieldRef = useRef<HTMLDivElement>(null);
  const calendarRef = useRef<HTMLDivElement>(null);
  const daysInMonth = new Date(month.getFullYear(), month.getMonth() + 1, 0).getDate();
  const startOffset = new Date(month.getFullYear(), month.getMonth(), 1).getDay();
  const cells = Array.from({ length: Math.ceil((startOffset + daysInMonth) / 7) * 7 }, (_, index) => {
    const day = index - startOffset + 1;
    return day > 0 && day <= daysInMonth ? day : null;
  });
  const selectedKey = initial ? `${initial.getFullYear()}-${initial.getMonth()}-${initial.getDate()}` : "";
  const weekdays = [t.audit.weekdaySun, t.audit.weekdayMon, t.audit.weekdayTue, t.audit.weekdayWed, t.audit.weekdayThu, t.audit.weekdayFri, t.audit.weekdaySat];
  const selectDay = (day: number) => {
    const selected = new Date(month.getFullYear(), month.getMonth(), day);
    if ((minDate && selected < minDate) || (maxDate && selected > maxDate)) return;
    const formatted = `${selected.getFullYear()}/${String(selected.getMonth() + 1).padStart(2, "0")}/${String(day).padStart(2, "0")}`;
    onChange(formatted);
    setOpen(false);
  };

  useEffect(() => {
    if (!open) return;
    const closeWhenOutside = (target: EventTarget | null) => {
      if (!(target instanceof Node) || fieldRef.current?.contains(target) || calendarRef.current?.contains(target)) return;
      setOpen(false);
    };
    const onPointerDown = (event: PointerEvent) => closeWhenOutside(event.target);
    const onKeyDown = (event: KeyboardEvent) => event.key === "Escape" && setOpen(false);
    document.addEventListener("pointerdown", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [open]);

  return (
    <div className="audit-date-field" ref={fieldRef}>
      <input className="audit-date-input" type="text" inputMode="numeric" placeholder={t.audit.datePlaceholder} value={value} onFocus={() => setOpen(true)} onChange={(event) => onChange(event.target.value)} />
      <button type="button" className="audit-date-trigger" aria-label={t.audit.selectDate} onClick={() => setOpen((current) => !current)}><CalendarDays size={16} /></button>
      {open && (
        <div className="audit-calendar" role="dialog" aria-label={t.audit.selectDate} ref={calendarRef} onPointerDown={(event) => event.stopPropagation()}>
          <div className="audit-calendar-header"><button type="button" aria-label={t.audit.previousMonth} onClick={() => setMonth(new Date(month.getFullYear(), month.getMonth() - 1, 1))}><ChevronLeft size={15} /></button><strong>{formatMessage(t.audit.calendarMonth, { year: month.getFullYear(), month: month.getMonth() + 1 })}</strong><button type="button" aria-label={t.audit.nextMonth} onClick={() => setMonth(new Date(month.getFullYear(), month.getMonth() + 1, 1))}><ChevronRight size={15} /></button></div>
          <div className="audit-calendar-weekdays">{weekdays.map((day) => <span key={day}>{day}</span>)}</div>
          <div className="audit-calendar-days">{cells.map((day, index) => {
            if (!day) return <span key={`empty-${index}`} />;
            const date = new Date(month.getFullYear(), month.getMonth(), day);
            const disabled = Boolean((minDate && date < minDate) || (maxDate && date > maxDate));
            return <button type="button" key={`${day}-${index}`} disabled={disabled} className={selectedKey === `${month.getFullYear()}-${month.getMonth()}-${day}` ? "selected" : ""} onClick={() => selectDay(day)}>{day}</button>;
          })}</div>
        </div>
      )}
    </div>
  );
}

export function AuditView({ t, events, tools, connections, filters, onFiltersChange, filterOpen, onFilterOpenChange, onSelect }: { t: I18nMessages; events: AuditEvent[]; tools: McpToolInfo[]; connections: ConnectionConfig[]; filters: AuditFilters; onFiltersChange: (filters: AuditFilters) => void; filterOpen: boolean; onFilterOpenChange: (open: boolean) => void; onSelect: (event: AuditEvent) => void }) {
  const [page, setPage] = useState(1);
  const [draftFilters, setDraftFilters] = useState(filters);
  const auditBodyRef = useRef<HTMLDivElement>(null);
  const draftFrom = draftFilters.from ? parseAuditDate(draftFilters.from) : null;
  const draftTo = draftFilters.to ? parseAuditDate(draftFilters.to) : null;
  const invalidDateRange = Boolean(draftFrom && draftTo && draftTo < draftFrom);
  const filteredEvents = events.filter((event) => {
    const date = new Date(event.timestamp);
    const from = filters.from ? parseAuditDate(filters.from) : null;
    const to = filters.to ? parseAuditDate(filters.to, true) : null;
    return (!from || date >= from) && (!to || date <= to) && (!filters.tool || event.tool === filters.tool) && (!filters.connection || (event.connection_id ?? "") === filters.connection) && (!filters.status || event.status === filters.status);
  });
  const totalPages = Math.max(1, Math.ceil(filteredEvents.length / AUDIT_PAGE_SIZE));
  const currentPage = Math.min(page, totalPages);
  const pageStart = (currentPage - 1) * AUDIT_PAGE_SIZE;
  const pageEvents = filteredEvents.slice(pageStart, pageStart + AUDIT_PAGE_SIZE);
  const pageNumbers = Array.from({ length: Math.min(3, totalPages) }, (_, index) => {
    if (totalPages <= 3) return index + 1;
    if (currentPage <= 2) return index + 1;
    if (currentPage >= totalPages - 1) return totalPages - 2 + index;
    return currentPage - 1 + index;
  });

  useEffect(() => {
    setPage((current) => Math.min(current, totalPages));
  }, [totalPages]);

  useEffect(() => {
    auditBodyRef.current?.scrollTo({ top: 0 });
  }, [currentPage]);

  useEffect(() => {
    if (filterOpen) setDraftFilters(filters);
  }, [filterOpen, filters]);

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
        <div className="audit-table-body" ref={auditBodyRef}>
          {filteredEvents.length === 0 ? (
            <div className="empty-state">{t.audit.empty}</div>
          ) : (
            pageEvents.map((event) => (
              <button type="button" className="audit-row audit-button" key={event.id} onClick={() => onSelect(event)}>
                <span>{new Date(event.timestamp).toLocaleString()}</span>
                <span>{toolDisplayName(t, event.tool)}</span>
                <span>{event.connection_name ?? event.connection_id ?? "—"}</span>
                <span>
                  <StatusPill tone={statusTone(event.status)} label={statusLabel(t, event.status)} />
                </span>
                <span>{event.reason ?? formatMessage(t.common.rowsElapsed, { rows: event.row_count ?? 0, elapsed: event.elapsed_ms ?? 0 })}</span>
              </button>
            ))
          )}
        </div>
        {filteredEvents.length > 0 && (
          <div className="pagination-footer">
            <span>{formatMessage(t.audit.pageInfo, { page: currentPage, totalPages, total: filteredEvents.length })}</span>
            <div className="pagination-actions">
              <button type="button" className="button ghost" disabled={currentPage <= 1} onClick={() => setPage((value) => Math.max(1, value - 1))}>
                {t.common.previous}
              </button>
              <div className="page-number-actions">
                {pageNumbers.map((pageNumber) => <button type="button" key={pageNumber} className={clsx("page-number", pageNumber === currentPage && "active")} onClick={() => setPage(pageNumber)}>{pageNumber}</button>)}
              </div>
              <button type="button" className="button ghost" disabled={currentPage >= totalPages} onClick={() => setPage((value) => Math.min(totalPages, value + 1))}>
                {t.common.next}
              </button>
            </div>
          </div>
        )}
      </div>
      <Dialog.Root open={filterOpen} onOpenChange={onFilterOpenChange}>
        <Dialog.Portal>
          <Dialog.Overlay className="dialog-overlay" />
          <Dialog.Content className="policy-dialog audit-filter-dialog">
            <div className="dialog-titlebar">
              <Dialog.Title>{t.audit.filterTitle}</Dialog.Title>
              <Dialog.Close asChild>
                <button type="button" className="icon-button" aria-label={t.common.close}><X size={18} /></button>
              </Dialog.Close>
            </div>
            <div className="audit-filter-grid">
              <label><span>{t.audit.from}</span><AuditDateField t={t} value={draftFilters.from} maxDate={draftTo} onChange={(from) => setDraftFilters({ ...draftFilters, from })} /></label>
              <label><span>{t.audit.to}</span><AuditDateField t={t} value={draftFilters.to} minDate={draftFrom} onChange={(to) => setDraftFilters({ ...draftFilters, to })} /></label>
              <label><span>{t.audit.tool}</span><select value={draftFilters.tool} onChange={(event) => setDraftFilters({ ...draftFilters, tool: event.target.value })}><option value="">{t.audit.all}</option>{tools.map((tool) => <option key={tool.name} value={tool.name}>{toolDisplayName(t, tool.name)}</option>)}</select></label>
              <label><span>{t.audit.connection}</span><select value={draftFilters.connection} onChange={(event) => setDraftFilters({ ...draftFilters, connection: event.target.value })}><option value="">{t.audit.all}</option>{connections.map((connection) => <option key={connection.id} value={connection.id}>{connection.name}</option>)}</select></label>
              <label><span>{t.audit.status}</span><select value={draftFilters.status} onChange={(event) => setDraftFilters({ ...draftFilters, status: event.target.value })}><option value="">{t.audit.all}</option>{(["allowed", "denied", "error", "timeout", "truncated"] as AuditEvent["status"][]).map((status) => <option key={status} value={status}>{statusLabel(t, status)}</option>)}</select></label>
            </div>
            {invalidDateRange && <p className="audit-filter-error">{t.audit.invalidDateRange}</p>}
            <div className="dialog-actions"><button type="button" className="button ghost" onClick={() => onFilterOpenChange(false)}>{t.common.cancel}</button><button type="button" className="button primary" disabled={invalidDateRange} onClick={() => { onFiltersChange(draftFilters); setPage(1); onFilterOpenChange(false); }}>{t.audit.applyFilter}</button></div>
          </Dialog.Content>
        </Dialog.Portal>
      </Dialog.Root>
    </section>
  );
}

export function AuditDetailDialog({ t, event, onClose }: { t: I18nMessages; event: AuditEvent | null; onClose: () => void }) {
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
                  <dd>{toolDisplayName(t, event.tool)}</dd>
                </div>
                <div>
                  <dt>{t.audit.connection}</dt>
                  <dd>{event.connection_name ?? event.connection_id ?? "—"}</dd>
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


