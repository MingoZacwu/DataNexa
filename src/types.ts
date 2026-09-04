export type DatabaseType = "sqlite" | "mysql" | "postgres" | "jdbc";

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
  jdbc_bundle_id?: string | null;
  jdbc_url?: string | null;
  jdbc_driver_class?: string | null;
  max_rows: number;
  query_timeout_ms: number;
  max_connections: number;
  max_result_bytes: number;
}

export interface SettingsConfig {
  audit_max_events: number;
  audit_redact_sql_literals: boolean;
  auto_check_updates: boolean;
  auto_start_mcp: boolean;
  auto_lightweight_mode: boolean;
  mcp_activity_effects: boolean;
  language: string;
  jdbc_java_home?: string | null;
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
  token_id?: string | null;
  access_source: "token" | "unauthenticated" | "legacy" | "system";
  token_name?: string | null;
  token_deleted: boolean;
  token_enabled: boolean;
}

export interface ServerStatus {
  running: boolean;
  endpoint: string;
  started_at?: string | null;
}

export interface AccessTokenInfo {
  id: string;
  name: string;
  enabled: boolean;
  created_at: string;
  updated_at: string;
  last_used_at?: string | null;
  denied_connections: string[];
  denied_tools: string[];
}

export interface AccessTokenSecretResult {
  token_id: string;
  secret: string;
}

export interface AppSnapshot {
  config: AppConfig;
  server_status: ServerStatus;
  emergency_disconnect: boolean;
  audit_events: AuditEvent[];
  tools: McpToolInfo[];
  updater_enabled: boolean;
  startup_error?: string | null;
  auto_start_status: "enabled" | "disabled" | "requires_approval" | "unknown";
  audit_migration: AuditMigrationState;
  access_tokens: AccessTokenInfo[];
}

export type AuditMigrationPhase = "reading_legacy_file" | "preparing_database" | "importing_events" | "committing" | "finalizing";
export type AuditMigrationState =
  | { status: "ready" }
  | { status: "migrating"; phase: AuditMigrationPhase; processed: number; total: number }
  | { status: "failed"; reason: string };

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

export interface JdbcDriverFile {
  name: string;
  size: number;
  sha256: string;
}

export interface JdbcDriverBundle {
  schema_version: number;
  bundle_id: string;
  display_name: string;
  maven_coordinate: string;
  repository_url: string;
  installed_at: string;
  driver_classes: string[];
  files: JdbcDriverFile[];
  total_size: number;
  source?: "maven" | "local" | string;
}

export interface JdbcRuntimeStatus {
  available: boolean;
  source: "embedded" | "external" | "unavailable" | string;
  java_version?: string | null;
  sidecar_available: boolean;
}

export interface JdbcStatus {
  runtime: JdbcRuntimeStatus;
  drivers: JdbcDriverBundle[];
}

export interface InstallJdbcDriverInput {
  display_name: string;
  maven_coordinate: string;
  repository_url?: string;
}

export interface ImportJdbcDriverInput {
  display_name: string;
  paths: string[];
}

export interface JdbcStorageItem {
  id: string;
  label: string;
  path: string;
  bytes: number;
}

export interface JdbcDriverRuntimeInfo {
  bundle_id: string;
  display_name: string;
  status: string;
  health: string;
  process_count: number;
  memory_bytes: number;
  cpu_percent: number;
}

export interface JdbcStorageStatus {
  storage_root: string;
  total_bytes: number;
  items: JdbcStorageItem[];
  runtimes: JdbcDriverRuntimeInfo[];
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
