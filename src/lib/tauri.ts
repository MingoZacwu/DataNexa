import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import type {
  AccessTokenSecretResult,
  AppSnapshot,
  ConnectionDiagnostics,
  ConnectionInput,
  DatabaseType,
  ImportConnectionsResult,
  ImportJdbcDriverInput,
  InstallJdbcDriverInput,
  JdbcDriverBundle,
  JdbcStatus,
  JdbcStorageStatus,
  McpToolInfo,
  PolicyCheckResult,
  ServerConfig,
  SettingsConfig
} from "../types";
import { formatMessage, messages, type Locale } from "../i18n";
import { systemTheme } from "../app/theme";

const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
const previewText = messages["zh-CN"].api;

const mockTools: McpToolInfo[] = [
  {
    name: "datanexa_list_connections",
    description: "List enabled local readonly database connections.",
    enabled: true
  },
  {
    name: "datanexa_get_schema",
    description: "List tables and views for a connection.",
    enabled: true
  },
  {
    name: "datanexa_describe_table",
    description: "Describe columns for a safe table identifier.",
    enabled: true
  },
  {
    name: "datanexa_sample_rows",
    description: "Read a small bounded sample from a table.",
    enabled: true
  },
  {
    name: "datanexa_execute_readonly_sql",
    description: "Execute a single read-only SELECT/WITH/EXPLAIN statement after policy validation.",
    enabled: true
  },
  {
    name: "datanexa_explain_sql",
    description: "Run EXPLAIN for a read-only SQL statement.",
    enabled: true
  },
  {
    name: "datanexa_policy_check",
    description: "Validate SQL against DataNexa read-only policy without executing it.",
    enabled: true
  }
];

const mockSnapshot: AppSnapshot = {
  emergency_disconnect: false,
  updater_enabled: false,
  startup_error: null,
  auto_start_status: "disabled",
  audit_migration: { status: "ready" },
  access_tokens: [{ id: "preview-token", name: "Codex", enabled: true, created_at: new Date().toISOString(), updated_at: new Date().toISOString(), last_used_at: new Date().toISOString(), denied_connections: [], denied_tools: [] }],
  config: {
    version: 1,
    server: {
      host: "127.0.0.1",
      port: 17321,
      require_token: true,
      token: null
    },
    settings: {
      audit_max_events: 300,
      audit_redact_sql_literals: false,
      auto_check_updates: true,
      auto_start_mcp: false,
      auto_lightweight_mode: false,
      mcp_activity_effects: true,
      language: "zh-CN",
      jdbc_java_home: null
    },
    tools: mockTools.map(({ name, enabled }) => ({ name, enabled })),
    connections: [
      {
        id: "oracle_reporting_jdbc",
        name: "Oracle Reporting (JDBC)",
        type: "jdbc",
        enabled: true,
        database: "",
        host: null,
        port: null,
        username: "readonly_user",
        credential_ref: "vault://oracle_reporting_jdbc",
        ssl_mode: null,
        jdbc_bundle_id: "00000000-0000-4000-8000-000000000021",
        jdbc_url: "jdbc:oracle:thin:@//analytics-db.example.test:1521/analytics?ssl_server_dn_match=true&oracle.net.keepAlive=true",
        jdbc_driver_class: "oracle.jdbc.OracleDriver",
        max_rows: 300,
        query_timeout_ms: 8000,
        max_connections: 1,
        max_result_bytes: 1048576
      },
      {
        id: "local_mysql",
        name: "MySQL Local",
        type: "mysql",
        enabled: true,
        host: "localhost",
        port: 3306,
        database: "sales_db",
        username: "readonly_user",
        credential_ref: "vault://local_mysql",
        ssl_mode: "prefer",
        max_rows: 500,
        query_timeout_ms: 8000,
        max_connections: 1,
        max_result_bytes: 1048576
      },
      {
        id: "prod_readonly_pg",
        name: "PostgreSQL Prod",
        type: "postgres",
        enabled: false,
        host: "127.0.0.1",
        port: 5432,
        database: "analytics",
        username: "readonly_user",
        credential_ref: "vault://prod_readonly_pg",
        ssl_mode: "require",
        max_rows: 200,
        query_timeout_ms: 5000,
        max_connections: 2,
        max_result_bytes: 1048576
      }
    ]
  },
  tools: mockTools,
  server_status: {
    running: false,
    endpoint: "http://127.0.0.1:17321/mcp",
    started_at: null
  },
  audit_events: [
    {
      id: "preview-1",
      timestamp: new Date().toISOString(),
      connection_id: "local_mysql",
      connection_name: "Local MySQL",
      tool: "datanexa_execute_readonly_sql",
      status: "allowed",
      reason: null,
      elapsed_ms: 12,
      row_count: 10,
      sql: "SELECT id, name FROM accounts LIMIT 10",
      access_source: "token",
      token_id: "preview-token",
      token_name: "Codex",
      token_deleted: false,
      token_enabled: true
    }
  ]
};

const mockJdbcStatus: JdbcStatus = {
  runtime: {
    available: true,
    source: "embedded",
    java_version: "openjdk version \"21\"",
    sidecar_available: true
  },
  drivers: [
    {
      schema_version: 1,
      bundle_id: "00000000-0000-4000-8000-000000000021",
      display_name: "Oracle JDBC",
      maven_coordinate: "com.oracle.database.jdbc:ojdbc11:23.4.0.24.05",
      repository_url: "https://repo.maven.apache.org/maven2/",
      installed_at: "2026-08-22T09:15:00.000Z",
      driver_classes: ["oracle.jdbc.OracleDriver"],
      files: [],
      total_size: 4.2 * 1024 * 1024,
      source: "maven"
    },
    {
      schema_version: 1,
      bundle_id: "00000000-0000-4000-8000-000000000022",
      display_name: "PostgreSQL JDBC",
      maven_coordinate: "org.postgresql:postgresql:42.7.7",
      repository_url: "https://maven.aliyun.com/repository/public",
      installed_at: "2026-08-24T14:30:00.000Z",
      driver_classes: ["org.postgresql.Driver"],
      files: [],
      total_size: 2.4 * 1024 * 1024,
      source: "maven"
    }
  ]
};

const mockJdbcStorageStatus: JdbcStorageStatus = {
  storage_root: "Preview",
  total_bytes: 6.6 * 1024 * 1024 + 892 * 1024 + 44 * 1024 + 3 * 1024,
  items: [
    { id: "drivers", label: "JDBC drivers", path: "Preview/jdbc-drivers", bytes: 6.6 * 1024 * 1024 },
    { id: "maven", label: "Maven repository", path: "Preview/maven-repository", bytes: 0 },
    { id: "audit", label: "Audit database", path: "Preview/audit.db", bytes: 892 * 1024 },
    { id: "access", label: "Access control", path: "Preview/access-control.db", bytes: 44 * 1024 },
    { id: "config", label: "Configuration", path: "Preview/config.toml", bytes: 3 * 1024 }
  ],
  runtimes: [
    { bundle_id: "00000000-0000-4000-8000-000000000021", display_name: "Oracle JDBC", status: "running", health: "healthy", process_count: 1, memory_bytes: 87.9 * 1024 * 1024, cpu_percent: 0.4 },
    { bundle_id: "00000000-0000-4000-8000-000000000022", display_name: "PostgreSQL JDBC", status: "stopped", health: "stopped", process_count: 0, memory_bytes: 0, cpu_percent: 0 }
  ]
};

function getMockJdbcStorageStatus(): JdbcStorageStatus {
  const seconds = Date.now() / 1000;
  const pulse = Math.sin(seconds * 0.8) * 2.8 + Math.sin(seconds * 0.23) * 1.4;
  const oracleCpu = Math.max(0.2, Math.min(6, Number((2.8 + pulse).toFixed(2))));
  return {
    ...mockJdbcStorageStatus,
    runtimes: mockJdbcStorageStatus.runtimes.map((runtime) => (
      runtime.bundle_id === "00000000-0000-4000-8000-000000000021"
        ? { ...runtime, cpu_percent: oracleCpu }
        : { ...runtime }
    ))
  };
}

async function command<T>(name: string, args?: Record<string, unknown>, fallback?: T): Promise<T> {
  if (!isTauri) {
    if (fallback === undefined) {
      throw new Error(formatMessage(previewText.desktopOnly, { name }));
    }
    await new Promise((resolve) => window.setTimeout(resolve, 120));
    return fallback;
  }

  return invoke<T>(name, args);
}

function withSettings(settings: SettingsConfig, applyAutoStart: boolean): AppSnapshot {
  return {
    ...mockSnapshot,
    auto_start_status: applyAutoStart
      ? (settings.auto_start_mcp ? "enabled" : "disabled")
      : mockSnapshot.auto_start_status,
    config: {
      ...mockSnapshot.config,
      settings
    }
  };
}

function withToolEnabled(name: string, enabled: boolean): AppSnapshot {
  const tools = mockTools.map((tool) => (tool.name === name ? { ...tool, enabled } : tool));
  return {
    ...mockSnapshot,
    tools,
    config: {
      ...mockSnapshot.config,
      tools: tools.map((tool) => ({ name: tool.name, enabled: tool.enabled }))
    }
  };
}

function withConnectionEnabled(id: string, enabled: boolean): AppSnapshot {
  return {
    ...mockSnapshot,
    config: {
      ...mockSnapshot.config,
      connections: mockSnapshot.config.connections.map((connection) =>
        connection.id === id ? { ...connection, enabled } : connection
      )
    }
  };
}

let mockEmergencyDisconnect = false;

function withEmergencyDisconnectToggled(): AppSnapshot {
  mockEmergencyDisconnect = !mockEmergencyDisconnect;
  return {
    ...mockSnapshot,
    emergency_disconnect: mockEmergencyDisconnect,
    config: {
      ...mockSnapshot.config,
      connections: mockSnapshot.config.connections.map((connection) => (
        mockEmergencyDisconnect ? { ...connection, enabled: false } : connection
      ))
    }
  };
}

function withAuditCleared(): AppSnapshot {
  return {
    ...mockSnapshot,
    audit_events: []
  };
}

function connectionTransferFileName() {
  return `datanexa-connections-${new Date().toISOString().slice(0, 10)}.json`;
}

export const api = {
  snapshot: () => command<AppSnapshot>("get_app_snapshot", undefined, mockSnapshot),
  jdbcStatus: () => command<JdbcStatus>("get_jdbc_status", undefined, mockJdbcStatus),
  jdbcStorageStatus: () => command<JdbcStorageStatus>("get_jdbc_storage_status", undefined, getMockJdbcStorageStatus()),
  clearMavenCache: () => command<boolean>("clear_maven_cache", undefined, true),
  installJdbcDriver: (input: InstallJdbcDriverInput) =>
    command<JdbcDriverBundle>("install_jdbc_driver", { input }, {
      schema_version: 1,
      bundle_id: crypto.randomUUID(),
      display_name: input.display_name,
      maven_coordinate: input.maven_coordinate,
      repository_url: "https://repo.maven.apache.org/maven2/",
      installed_at: new Date().toISOString(),
      driver_classes: [],
      files: [],
      total_size: 0,
      source: "maven"
    }),
  importJdbcDriver: (input: ImportJdbcDriverInput) =>
    command<JdbcDriverBundle>("import_jdbc_driver", { input }, {
      schema_version: 1,
      bundle_id: crypto.randomUUID(),
      display_name: input.display_name,
      maven_coordinate: "",
      repository_url: "",
      installed_at: new Date().toISOString(),
      driver_classes: [],
      files: [],
      total_size: 0,
      source: "local"
    }),
  deleteJdbcDriver: (bundleId: string) =>
    command<JdbcStatus>("delete_jdbc_driver", { bundleId }, mockJdbcStatus),
  createAccessToken: (name: string) => command<AccessTokenSecretResult>("create_access_token", { name }, { token_id: "preview-new-token", secret: "preview-new-secret" }),
  renameAccessToken: (id: string, name: string) => command<AppSnapshot>("rename_access_token", { id, name }, mockSnapshot),
  setAccessTokenEnabled: (id: string, enabled: boolean) => command<AppSnapshot>("set_access_token_enabled", { id, enabled }, mockSnapshot),
  rotateAccessToken: (id: string) => command<AccessTokenSecretResult>("rotate_access_token", { id }, { token_id: id, secret: "preview-rotated-secret" }),
  deleteAccessToken: (id: string) => command<AppSnapshot>("delete_access_token", { id }, mockSnapshot),
  getAccessTokenSecret: (id: string) => command<AccessTokenSecretResult>("get_access_token_secret", { id }, { token_id: id, secret: "preview-token-secret" }),
  setTokenConnectionAllowed: (tokenId: string, connectionId: string, allowed: boolean) => command<AppSnapshot>("set_token_connection_allowed", { tokenId, connectionId, allowed }, mockSnapshot),
  setTokenToolAllowed: (tokenId: string, toolName: string, allowed: boolean) => command<AppSnapshot>("set_token_tool_allowed", { tokenId, toolName, allowed }, mockSnapshot),
  saveServerConfig: (server: ServerConfig) =>
    command<AppSnapshot>("save_server_config", { server }, mockSnapshot),
  saveSettingsConfig: (settings: SettingsConfig, applyAutoStart = false) =>
    command<AppSnapshot>("save_settings_config", { settings, applyAutoStart }, withSettings(settings, applyAutoStart)),
  exportConnections: async (locale: Locale) => {
    if (!isTauri) {
      throw new Error(formatMessage(previewText.desktopOnly, { name: "export_connections" }));
    }
    const dialogText = messages[locale].fileDialog;
    const path = await save({
      title: dialogText.exportConnectionsTitle,
      defaultPath: connectionTransferFileName(),
      filters: [{ name: dialogText.connectionFile, extensions: ["json"] }]
    });
    if (!path) return null;
    return command<number>("export_connections", { path });
  },
  importConnections: async (locale: Locale) => {
    if (!isTauri) {
      throw new Error(formatMessage(previewText.desktopOnly, { name: "import_connections" }));
    }
    const dialogText = messages[locale].fileDialog;
    const path = await open({
      title: dialogText.importConnectionsTitle,
      multiple: false,
      directory: false,
      filters: [{ name: dialogText.connectionFile, extensions: ["json"] }]
    });
    if (!path) return null;
    return command<ImportConnectionsResult>("import_connections", { path });
  },
  setMcpToolEnabled: (name: string, enabled: boolean) =>
    command<AppSnapshot>("set_mcp_tool_enabled", { name, enabled }, withToolEnabled(name, enabled)),
  upsertConnection: (input: ConnectionInput) =>
    command<AppSnapshot>("upsert_connection", { input }, mockSnapshot),
  deleteConnection: (id: string) => command<AppSnapshot>("delete_connection", { id }, mockSnapshot),
  setConnectionEnabled: (id: string, enabled: boolean) =>
    command<AppSnapshot>("set_connection_enabled", { id, enabled }, withConnectionEnabled(id, enabled)),
  disableAllConnections: () =>
    command<AppSnapshot>("disable_all_connections", undefined, withEmergencyDisconnectToggled()),
  clearAuditEvents: () =>
    command<AppSnapshot>("clear_audit_events", undefined, withAuditCleared()),
  retryAuditMigration: () => command<AppSnapshot>("retry_audit_migration", undefined, mockSnapshot),
  clearLegacyAuditLog: () => command<AppSnapshot>("clear_legacy_audit_log", undefined, mockSnapshot),
  testConnection: (id: string) => command<string>("test_connection", { id }, previewText.previewTestConnection),
  testConnectionInput: (input: ConnectionInput) =>
    command<string>("test_connection_input", { input }, previewText.previewTestConnection),
  diagnoseConnection: (id: string) =>
    command<ConnectionDiagnostics>("diagnose_connection", { id }, {
      id,
      name: "Preview connection",
      database_type: "postgres",
      host: "localhost",
      port: 5432,
      database: "app",
      username: "readonly_user",
      ssl_mode: "prefer",
      credential_ref_present: true,
      credential_state: "saved",
      query_timeout_ms: 8000,
      max_connections: 1,
      hint: previewText.previewDiagnostics
    }),
  startServer: () => command<AppSnapshot>("start_mcp_server", undefined, {
    ...mockSnapshot,
    server_status: { ...mockSnapshot.server_status, running: true, started_at: new Date().toISOString() }
  }),
  stopServer: () => {
    mockEmergencyDisconnect = false;
    return command<AppSnapshot>("stop_mcp_server", undefined, mockSnapshot);
  },
  policyCheck: (kind: DatabaseType, sql: string, maxRows = 500) =>
    command<PolicyCheckResult>(
      "policy_check",
      { kind, sql, maxRows },
      {
        allowed: /^\s*(select|with|explain)\b/i.test(sql) && !/;\s*\S|drop|truncate|delete|update|insert|alter|create/i.test(sql),
        reason: previewText.previewPolicyReason,
        rewritten_sql: /^\s*(select|with)\b/i.test(sql) ? `${sql.replace(/;+\s*$/, "")} LIMIT ${maxRows}` : null
      }
    ),
  minimizeWindow: () => command<void>("minimize_main_window", undefined, undefined),
  hideWindow: () => command<void>("hide_main_window", undefined, undefined),
  startWindowDrag: () => command<void>("start_window_drag", undefined, undefined),
  setWindowMaterialTheme: (dark: boolean | null) =>
    command<[boolean, boolean]>("set_window_material_theme", { dark }, [dark ?? systemTheme() === "dark", false]),
  openProjectHomepage: () => {
    if (!isTauri) {
      window.open("https://github.com/MingoZacwu/DataNexa", "_blank", "noopener,noreferrer");
      return Promise.resolve();
    }
    return command<void>("open_project_homepage", undefined, undefined);
  },
  openProjectReleases: () => {
    if (!isTauri) {
      window.open("https://github.com/MingoZacwu/DataNexa/releases", "_blank", "noopener,noreferrer");
      return Promise.resolve();
    }
    return command<void>("open_project_releases", undefined, undefined);
  },
  openProjectSite: () => {
    if (!isTauri) {
      window.open("https://mingozacwu.github.io/datanexa-site/", "_blank", "noopener,noreferrer");
      return Promise.resolve();
    }
    return command<void>("open_project_site", undefined, undefined);
  }
};
