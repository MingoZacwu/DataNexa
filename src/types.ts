export type DatabaseType = "sqlite" | "mysql" | "postgres";

export interface ServerConfig {
  host: string;
  port: number;
  require_token: boolean;
  token?: string | null;
}

export interface ConnectionConfig {
  id: string;
  name: string;
  type: DatabaseType;
  enabled: boolean;
  database: string;
  host?: string | null;
  port?: number | null;
  username?: string | null;
  credential_ref?: string | null;
  ssl_mode?: string | null;
  max_rows: number;
  query_timeout_ms: number;
  max_connections: number;
}

export interface SettingsConfig {
  audit_max_events: number;
  audit_redact_sql_literals: boolean;
  language: string;
}

export interface ToolConfig {
  name: string;
  enabled: boolean;
}

export interface AppConfig {
  version: number;
  server: ServerConfig;
  settings: SettingsConfig;
  tools: ToolConfig[];
  connections: ConnectionConfig[];
}

export type AuditStatus = "allowed" | "denied" | "error" | "timeout" | "truncated";

export interface AuditEvent {
  id: string;
  timestamp: string;
  connection_id?: string | null;
  connection_name?: string | null;
  tool: string;
  status: AuditStatus;
  reason?: string | null;
  elapsed_ms?: number | null;
  row_count?: number | null;
  sql?: string | null;
}

export interface ServerStatus {
  running: boolean;
  endpoint: string;
  token?: string | null;
  started_at?: string | null;
}

export interface AppSnapshot {
  config: AppConfig;
  server_status: ServerStatus;
  audit_events: AuditEvent[];
  tools: McpToolInfo[];
  updater_enabled: boolean;
}

export interface McpToolInfo {
  name: string;
  description: string;
  enabled: boolean;
}

export interface ConnectionInput {
  connection: ConnectionConfig;
  password?: string | null;
  clear_password?: boolean;
}

export interface ImportConnectionsResult {
  snapshot: AppSnapshot;
  imported_count: number;
}

export interface ConnectionDiagnostics {
  id: string;
  name: string;
  database_type: DatabaseType;
  host?: string | null;
  port?: number | null;
  database: string;
  username?: string | null;
  ssl_mode?: string | null;
  credential_ref_present: boolean;
  credential_state: "not_required" | "not_saved" | "saved_empty" | "saved" | "missing_in_vault" | "vault_error" | string;
  query_timeout_ms: number;
  max_connections: number;
  hint?: string | null;
}

export interface PolicyCheckResult {
  allowed: boolean;
  reason: string;
  rewritten_sql?: string | null;
}
