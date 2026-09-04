import type { UIEvent as ReactUIEvent } from "react";
import { formatMessage, type I18nMessages } from "../i18n";
import type { AuditEvent, ConnectionDiagnostics, DatabaseType, McpToolInfo } from "../types";
import type { View } from "./types";

export function updateScrollFade(event: ReactUIEvent<HTMLDivElement>) {
  const element = event.currentTarget;
  const pastStart = element.scrollTop > 1;
  const atBottom = element.scrollHeight - element.scrollTop - element.clientHeight <= 1;
  element.classList.toggle("scroll-past-start", pastStart);
  element.classList.toggle("scroll-at-end", atBottom);
}

export function viewTitle(t: I18nMessages, view: View) {
  switch (view) {
    case "overview":
      return t.nav.overview;
    case "connections":
      return t.nav.connections;
    case "access":
      return t.access.title;
    case "tools":
      return t.nav.tools;
    case "audit":
      return t.nav.audit;
    case "settings":
      return t.nav.settings;
  }
}

export function toolDisplayName(t: I18nMessages, name: string) {
  const names: Record<string, string> = t.tools.names;
  if (name === "system.auto_start_mcp") return names.system_auto_start_mcp;
  if (name === "system.start_mcp") return names.system_start_mcp;
  return names[name] ?? name;
}

export function toolIntro(t: I18nMessages, tool: McpToolInfo) {
  const intros: Record<string, string> = t.tools.intros;
  return intros[tool.name] ?? tool.description;
}

export function dbTypeLabel(type: DatabaseType) {
  if (type === "jdbc") return "JDBC";
  if (type === "postgres") return "PostgreSQL";
  if (type === "mysql") return "MySQL";
  return "SQLite";
}

export function defaultPort(type: DatabaseType) {
  if (type === "postgres") return 5432;
  if (type === "mysql") return 3306;
  return null;
}

export function statusTone(status: AuditEvent["status"]): "green" | "blue" | "amber" | "red" | "slate" {
  if (status === "allowed") return "green";
  if (status === "denied") return "red";
  if (status === "timeout") return "amber";
  if (status === "truncated") return "blue";
  if (status === "error") return "red";
  return "slate";
}

export function statusLabel(t: I18nMessages, status: AuditEvent["status"]) {
  if (status === "allowed") return t.status.allowed;
  if (status === "denied") return t.status.denied;
  if (status === "timeout") return t.status.timeout;
  if (status === "truncated") return t.status.truncated;
  return t.status.error;
}

export function formatDiagnostics(t: I18nMessages, diagnostics: ConnectionDiagnostics) {
  const summary = formatMessage(t.diagnostics.summary, {
    name: diagnostics.name,
    type: dbTypeLabel(diagnostics.database_type),
    credential: credentialStateLabel(t, diagnostics.credential_state)
  });
  return diagnostics.hint ? `${summary}\n${diagnostics.hint.trim()}` : summary;
}

export function formatConnectionTest(t: I18nMessages, message: string) {
  const elapsed = message.match(/(\d+)\s*ms/i)?.[1] ?? "-";
  return formatMessage(t.toast.connectionTestPassed, { elapsed });
}

export function compactConnectionError(error: unknown) {
  const message = error instanceof Error ? error.message : String(error);
  const lines = message.split(/\r?\n/).map((line) => line.trim()).filter(Boolean);
  return [...new Set(lines)].join("\n") || message;
}

export function credentialStateLabel(t: I18nMessages, state: string) {
  if (state === "not_required") return t.diagnostics.notRequired;
  if (state === "not_saved") return t.diagnostics.notSaved;
  if (state === "saved_empty") return t.diagnostics.savedEmpty;
  if (state === "saved") return t.diagnostics.saved;
  if (state === "missing_in_vault") return t.diagnostics.missingInVault;
  if (state === "vault_error") return t.diagnostics.vaultError;
  return state;
}

export function buildAgentPrompt(t: I18nMessages, endpoint: string, requireToken: boolean, token?: string | null) {
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

export function relativeDuration(t: I18nMessages, timestamp: string) {
  const elapsedMs = Date.now() - new Date(timestamp).getTime();
  if (elapsedMs < 0) return t.common.justNow;
  const minutes = Math.floor(elapsedMs / 60000);
  const hours = Math.floor(minutes / 60);
  if (hours > 0) return `${hours}h ${minutes % 60}m`;
  if (minutes > 0) return `${minutes}m`;
  return t.common.justNow;
}
