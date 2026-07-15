import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import type {
  AppSnapshot,
  ConnectionDiagnostics,
  ConnectionInput,
  DatabaseType,
  ImportConnectionsResult,
  McpToolInfo,
  PolicyCheckResult,
  ServerConfig,
  SettingsConfig
} from "../types";
import { formatMessage, messages, type Locale } from "../i18n";

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
  updater_enabled: false,
  config: {
    version: 1,
    server: {
      host: "127.0.0.1",
      port: 17321,
      require_token: true,
      token: "local-preview-token"
    },
    settings: {
      audit_max_events: 300,
      audit_redact_sql_literals: false,
      auto_check_updates: true,
      language: "zh-CN"
    },
    tools: mockTools.map(({ name, enabled }) => ({ name, enabled })),
    connections: [
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
        max_connections: 1
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
        max_connections: 2
      }
    ]
  },
  tools: mockTools,
  server_status: {
    running: false,
    endpoint: "http://127.0.0.1:17321/mcp",
    token: "local-preview-token",
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
      sql: "SELECT id, name FROM accounts LIMIT 10"
    }
  ]
};

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

function withSettings(settings: SettingsConfig): AppSnapshot {
  return {
    ...mockSnapshot,
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

function withAllConnectionsDisabled(): AppSnapshot {
  return {
    ...mockSnapshot,
    config: {
      ...mockSnapshot.config,
      connections: mockSnapshot.config.connections.map((connection) => ({ ...connection, enabled: false }))
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
  saveServerConfig: (server: ServerConfig) =>
    command<AppSnapshot>("save_server_config", { server }, mockSnapshot),
  saveSettingsConfig: (settings: SettingsConfig) =>
    command<AppSnapshot>("save_settings_config", { settings }, withSettings(settings)),
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
    command<AppSnapshot>("disable_all_connections", undefined, withAllConnectionsDisabled()),
  clearAuditEvents: () =>
    command<AppSnapshot>("clear_audit_events", undefined, withAuditCleared()),
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
  stopServer: () => command<AppSnapshot>("stop_mcp_server", undefined, mockSnapshot),
  rotateToken: () => command<AppSnapshot>("rotate_server_token", undefined, {
    ...mockSnapshot,
    server_status: { ...mockSnapshot.server_status, token: "rotated-preview-token" }
  }),
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
  }
};
