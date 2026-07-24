export const DEFAULT_LOCALE = "zh-CN";
export const LANGUAGE_STORAGE_KEY = "datanexa.language";

export const supportedLocales = ["zh-CN", "en"] as const;
export type Locale = (typeof supportedLocales)[number];

export const languageOptions: Array<{ value: Locale; label: string; nativeLabel: string }> = [
  { value: "zh-CN", label: "简体中文", nativeLabel: "简体中文" },
  { value: "en", label: "English", nativeLabel: "English" }
];

const zhCN = {
  common: {
    refresh: "刷新",
    minimize: "最小化",
    close: "关闭",
    closeNotice: "关闭提示",
    copy: "复制",
    cancel: "取消",
    previous: "上一页",
    next: "下一页",
    system: "系统",
    totalSuffix: "/ {total} 总计",
    justNow: "刚刚",
    rowsElapsed: "{rows} 行，耗时 {elapsed}ms"
  },
  fileDialog: {
    importConnectionsTitle: "选择要导入的数据库连接文件",
    exportConnectionsTitle: "选择数据库连接文件的保存位置",
    connectionFile: "DataNexa 数据库连接文件"
  },
  nav: {
    overview: "概览",
    connections: "数据库连接",
    server: "MCP 服务",
    tools: "工具",
    audit: "审计日志",
    settings: "设置"
  },
  sidebar: {
    serverRunning: "服务运行中 · {port}",
    serverStopped: "服务已停止",
    systemTheme: "跟随系统",
    darkMode: "深色模式",
    lightMode: "浅色模式",
    toggleTheme: "切换深浅色"
  },
  auditMigration: {
    migratingTitle: "正在迁移审计日志",
    preparing: "正在准备新的审计存储",
    progress: "已迁移 {processed} / {total} 条",
    finishing: "正在完成迁移",
    failedTitle: "审计日志迁移失败",
    failedCompact: "点击查看并恢复",
    dialogDescription: "DataNexa 需要将旧版审计日志迁移到新的 SQLite 存储。为确保数据库操作全程可审计，迁移完成前 MCP 服务将暂不可用。",
    errorReason: "错误原因",
    retry: "重试",
    clear: "清空原有日志",
    clearConfirmTitle: "隔离旧日志并继续？",
    clearConfirmDescription: "旧版 JSON 日志会被隔离且不再导入，审计数据库会被清空并重新启用。",
    confirmClear: "隔离旧日志并继续"
  },
  toast: {
    connectionSaved: "连接已保存。密码只会写入本机凭证库，不会进入 TOML 配置。",
    connectionDeleted: "连接已删除。",
    tokenRotated: "本地 MCP 访问密钥已手动轮换。",
    serverSaved: "服务设置已自动保存。",
    settingsSaved: "设置已自动保存。",
    autoStartRequiresApproval: "登录启动项已注册，请在系统设置的“登录项”中允许 DataNexa。",
    auditCleared: "审计日志已清空。",
    allConnectionsDisabled: "已紧急禁用所有数据库连接。",
    connectionEnabled: "{connection} 已启用。",
    connectionDisabled: "{connection} 已禁用。",
    toolEnabled: "{tool} 已启用。",
    toolDisabled: "{tool} 已关闭。",
    agentCopied: "Agent 接入配置已复制。",
    connectionTestPassed: "连接可用 · {elapsed} ms",
    connectionsExported: "已导出 {count} 个数据库连接。请妥善保管包含明文密码的文件。",
    connectionsImported: "已新建 {count} 个数据库连接。"
  },
  overview: {
    loading: "正在加载本地工作区...",
    metricConnections: "数据库连接",
    metricTools: "MCP 工具",
    metricServer: "MCP 服务",
    metricUptime: "运行时间",
    running: "运行中",
    stopped: "已停止",
    notStarted: "未启动",
    newConnection: "新建连接",
    viewAllConnections: "查看全部连接",
    recentLogs: "最近日志",
    viewAll: "查看全部",
    quickStart: "快速开始",
    quickConnectTitle: "连接数据库",
    quickConnectText: "配置 SQLite、MySQL 或 PostgreSQL 的只读连接。",
    quickServerTitle: "启动服务",
    quickServerText: "打开本地 MCP 端点，供 Agent 通过本机地址访问。",
    quickAgentTitle: "接入 Agent",
    quickAgentText: "复制接入 Prompt，发送给 Agent 完成 MCP 服务配置。",
    copyAgentConfig: "复制接入配置"
  },
  connections: {
    title: "数据库连接",
    empty: "还没有数据库连接，先新建一个只读连接。",
    newConnectionName: "新的只读连接",
    test: "测试连接",
    diagnose: "诊断连接",
    emergencyDisable: "紧急断连",
    toggleEnabled: "启用或禁用 {name}",
    edit: "编辑连接",
    delete: "删除连接",
    enabled: "启用",
    paused: "暂停",
    noDatabaseFile: "未选择数据库文件"
  },
  tools: {
    summary: "{enabled} / {total} 个工具已启用",
    toggle: "切换 {name}",
    groups: {
      discovery: "结构发现",
      access: "数据访问",
      analysis: "分析与策略"
    },
    names: {
      datanexa_list_connections: "列出连接",
      datanexa_get_schema: "读取 Schema",
      datanexa_describe_table: "描述表结构",
      datanexa_sample_rows: "采样行",
      datanexa_execute_readonly_sql: "执行只读 SQL",
      datanexa_explain_sql: "解释 SQL",
      datanexa_policy_check: "策略检查",
      system_auto_start_mcp: "自动启动 MCP",
      system_start_mcp: "手动启动 MCP"
    },
    intros: {
      datanexa_list_connections: "向 Agent 返回当前已启用的数据库连接清单，不包含密码和完整 DSN。",
      datanexa_get_schema: "读取指定连接的表与视图列表，用于让 Agent 了解数据库结构边界。",
      datanexa_describe_table: "读取单张表的列信息，表名和 schema 会按结构化参数处理。",
      datanexa_sample_rows: "从指定表读取受限样本行，自动使用只读策略和最大行数限制。",
      datanexa_execute_readonly_sql: "执行经过策略引擎校验的单条只读查询，这是能力最强也最需要谨慎开放的工具。",
      datanexa_explain_sql: "返回查询计划，帮助 Agent 分析 SQL，而不是直接读取业务数据。",
      datanexa_policy_check: "只做静态策略检查，不连接真实数据库，也不写入审计日志。"
    }
  },
  server: {
    endpoint: "本地 MCP 端点",
    stop: "停止服务",
    start: "启动服务",
    copyEndpoint: "复制端点",
    accessToken: "访问密钥",
    rotateToken: "手动轮换",
    generatedOnStart: "首次启动服务时生成",
    copyToken: "复制密钥",
    tokenDisabledTitle: "Bearer 访问密钥已关闭",
    tokenDisabledText: "需要鉴权时可在设置页重新打开。",
    agentAccess: "Agent 接入",
    copyToAgent: "复制给 Agent"
  },
  audit: {
    filter: "筛选",
    clearFilter: "清除筛选",
    filterTitle: "筛选审计日志",
    from: "开始日期",
    to: "结束日期",
    all: "全部",
    applyFilter: "应用筛选",
    invalidDateRange: "结束日期不能早于开始日期",
    datePlaceholder: "YYYY/MM/DD",
    selectDate: "选择日期",
    previousMonth: "上个月",
    nextMonth: "下个月",
    calendarMonth: "{year}年{month}月",
    weekdaySun: "日",
    weekdayMon: "一",
    weekdayTue: "二",
    weekdayWed: "三",
    weekdayThu: "四",
    weekdayFri: "五",
    weekdaySat: "六",
    results: "条结果",
    title: "审计日志",
    time: "时间",
    tool: "工具",
    connection: "连接",
    status: "状态",
    detail: "详情",
    empty: "暂无访问记录。",
    emptyCompact: "暂无日志。",
    clear: "清理日志",
    pageInfo: "第 {page} / {totalPages} 页，共 {total} 条",
    detailTitle: "日志详情",
    elapsedRows: "耗时 / 行数",
    elapsedRowsValue: "{elapsed}ms / {rows} 行",
    reason: "原因",
    noSql: "这条日志没有 SQL 文本。"
  },
  settings: {
    general: "通用",
    about: "关于",
    servicePolicy: "MCP 服务策略",
    listenHost: "监听地址",
    port: "端口",
    requireBearer: "要求 Bearer 密钥",
    display: "显示",
    startup: "启动与后台",
    language: "界面语言",
    theme: "主题",
    themeSystem: "跟随系统",
    themeLight: "浅色",
    themeDark: "深色",
    autoCheckUpdates: "每天自动检查更新",
    autoStartMcp: "登录时启动 DataNexa 并自动开启 MCP",
    auditLog: "审计日志",
    auditMaxEvents: "最多保留日志条数",
    auditRedactSql: "脱敏 SQL 字面量（关闭后可能记录敏感数据）",
    importExport: "导入/导出",
    importConnections: "导入连接",
    importConnectionsDescription: "从 DataNexa JSON 文件追加连接，不会修改现有连接。",
    exportConnections: "导出连接",
    exportConnectionsDescription: "将当前数据库连接及密码保存为可迁移的 JSON 文件。",
    exportWarningTitle: "导出文件包含明文密码",
    exportWarningDescription: "导出后，数据库密码将不再受 OS 凭证库保护。继续前请确认你了解以下风险。",
    exportWarningAccess: "任何能读取该文件的人都可能使用其中的数据库凭证。",
    exportWarningLocation: "仅保存到可信且访问受控的位置，不要上传到代码仓库或公共网盘。",
    exportWarningCleanup: "完成迁移后请及时删除文件，并清理备份、同步目录和回收站中的副本。",
    exportAcknowledgement: "我了解导出文件会以明文保存数据库密码",
    confirmExport: "选择位置并导出",
    policyConsole: "策略检查台",
    policyDescription: "只做静态 SQL 策略预检查，不连接真实数据库，也不写入审计日志。",
    sqlDialect: "SQL 方言",
    checkSql: "检查 SQL",
    allowed: "允许执行",
    denied: "已拒绝",
    securityPosture: "安全策略",
    securityAst: "只允许 SELECT、安全 WITH SELECT 和 EXPLAIN",
    securityVault: "凭证日常只保存到 OS 凭证库，常规配置文件不写明文密码",
    securityAudit: "审计日志不记录查询结果",
    securityReadonly: "默认只读会话，并应用行数、超时和连接数限额",
    securityWarning: "只读策略不能完全保证所有风险都被拦截，仍需约束 Agent，避免要求或允许其执行危险行为。",
    aboutText: "本地只读数据库 MCP 网关，让 Agent 通过统一、安全、可审计的入口访问你的数据源。",
    officialHomepage: "官方主页"
  },
  updates: {
    readyTitle: "应用更新",
    readyDescription: "手动检查 DataNexa 是否有可用的新版本。",
    localBuildTitle: "本地编译版本",
    localBuildDescription: "当前构建未启用自动更新，无法直接检查或安装新版本。",
    checkingTitle: "正在检查更新",
    checkingDescription: "正在连接 GitHub Release 获取最新版本信息。",
    upToDateTitle: "已是最新版本",
    upToDateDescription: "当前 DataNexa v{version} 无需更新。",
    availableTitle: "发现新版本",
    availableCompact: "v{version} 可用",
    availableDescription: "DataNexa v{version} 已发布，可立即下载更新。",
    downloadingTitle: "正在下载更新",
    downloadingDescription: "正在安全下载并校验更新包。",
    downloadingProgress: "下载进度 {progress}%",
    relaunchingTitle: "正在安装",
    relaunchingDescription: "新版本已下载，正在安装...",
    checkFailedTitle: "暂时无法检查更新",
    checkFailedDescription: "请检查网络连接后稍后重试。",
    downloadFailedTitle: "更新下载失败",
    downloadFailedDescription: "可以重试更新，或前往 GitHub Release 手动下载安装包。",
    relaunchFailedTitle: "自动重启失败",
    relaunchFailedDescription: "更新已完成，请手动关闭并重新打开 DataNexa。",
    checkNow: "检查更新",
    retry: "重试",
    updateNow: "立即更新",
    viewReleaseNotes: "查看更新日志",
    manualDownload: "手动下载",
    openReleases: "GitHub Release",
    dismissReminder: "暂时关闭更新提醒",
    confirmTitle: "更新 DataNexa",
    confirmDescription: "确认下载并安装 DataNexa v{version}。安装完成后应用将自动重启。",
    targetVersion: "目标版本"
  },
  connectionDialog: {
    addTitle: "新增连接",
    editTitle: "编辑连接",
    description: "连接元数据写入 TOML，密码等敏感信息只保存到本机凭证库。",
    basicInfo: "基础信息",
    name: "连接名称",
    stableId: "稳定 ID",
    databaseType: "数据库类型",
    enableConnection: "启用连接",
    address: "连接地址",
    databaseFile: "数据库文件",
    host: "主机",
    port: "端口",
    database: "数据库",
    username: "用户名",
    sslMode: "SSL 模式",
    sslDisable: "禁用",
    sslPrefer: "优先 / 自动",
    sslRequire: "强制",
    credentialsAndLimits: "凭证与限制",
    password: "密码",
    keepExistingPassword: "留空则保留现有密码",
    saveToVault: "保存到本机凭证库",
    clearSavedCredential: "清除已保存凭证",
    maxRows: "最大返回行数",
    queryTimeoutMs: "查询超时毫秒",
    maxConnections: "最大连接数",
    maxResultBytes: "最大结果大小（KiB）",
    currentCredential: "当前凭证：{credential}。DataNexa 不会把明文密码写入配置文件。",
    credentialNotSaved: "尚未保存",
    save: "保存连接"
  },
  status: {
    allowed: "允许",
    denied: "拒绝",
    timeout: "超时",
    truncated: "截断",
    error: "错误"
  },
  diagnostics: {
    noHint: "无额外提示。",
    summary: "诊断完成：{name} · {type} · 凭证{credential}",
    title: "连接诊断：{name} ({type})",
    address: "地址：host={host} port={port} database={database} username={username}",
    credential: "凭证：{credential}；SSL={ssl}；超时={timeout}ms；连接池={pool}",
    hint: "提示：{hint}",
    notRequired: "不需要",
    notSaved: "未保存",
    savedEmpty: "已保存空密码",
    saved: "已保存",
    missingInVault: "凭证引用存在，但本机凭证库里找不到密码",
    vaultError: "读取本机凭证库失败"
  },
  agentPrompt: {
    intro: "请把 DataNexa 配置为本地 MCP 服务，用它安全访问本机只读数据库连接。",
    configIntro: "MCP server 配置如下："
  },
  api: {
    desktopOnly: "命令 {name} 只能在 Tauri 桌面应用中使用。",
    previewTestConnection: "浏览器预览模式仅模拟连接测试。",
    previewDiagnostics: "浏览器预览模式仅显示模拟诊断。",
    previewPolicyReason: "浏览器预览模式策略检查。"
  }
};

type MessageShape<T> = {
  [Key in keyof T]: T[Key] extends string ? string : MessageShape<T[Key]>;
};

export type I18nMessages = MessageShape<typeof zhCN>;

const en: I18nMessages = {
  common: {
    refresh: "Refresh",
    minimize: "Minimize",
    close: "Close",
    closeNotice: "Dismiss notification",
    copy: "Copy",
    cancel: "Cancel",
    previous: "Previous",
    next: "Next",
    system: "System",
    totalSuffix: "/ {total} total",
    justNow: "Just now",
    rowsElapsed: "{rows} rows, {elapsed}ms"
  },
  fileDialog: {
    importConnectionsTitle: "Select database connections to import",
    exportConnectionsTitle: "Choose where to save database connections",
    connectionFile: "DataNexa database connection file"
  },
  nav: {
    overview: "Overview",
    connections: "Connections",
    server: "MCP Server",
    tools: "Tools",
    audit: "Audit Log",
    settings: "Settings"
  },
  sidebar: {
    serverRunning: "Server running · {port}",
    serverStopped: "Server stopped",
    systemTheme: "Use system theme",
    darkMode: "Dark mode",
    lightMode: "Light mode",
    toggleTheme: "Toggle theme"
  },
  auditMigration: {
    migratingTitle: "Migrating audit log",
    preparing: "Preparing the new audit storage",
    progress: "Migrated {processed} / {total} entries",
    finishing: "Finishing migration",
    failedTitle: "Audit log migration failed",
    failedCompact: "Click to review and recover",
    dialogDescription: "DataNexa needs to migrate the legacy audit log to new SQLite storage. To keep database operations auditable, the MCP server will remain unavailable until migration completes.",
    errorReason: "Error reason",
    retry: "Retry",
    clear: "Clear old logs",
    clearConfirmTitle: "Quarantine old logs and continue?",
    clearConfirmDescription: "The legacy JSON log will be quarantined and skipped, then the audit database will be cleared and re-enabled.",
    confirmClear: "Quarantine logs and continue"
  },
  toast: {
    connectionSaved: "Connection saved. Passwords are stored only in the local credential vault, not in TOML config.",
    connectionDeleted: "Connection deleted.",
    tokenRotated: "Local MCP access token rotated manually.",
    serverSaved: "Server settings saved automatically.",
    settingsSaved: "Settings saved automatically.",
    autoStartRequiresApproval: "The login item is registered. Allow DataNexa in System Settings > Login Items.",
    auditCleared: "Audit log cleared.",
    allConnectionsDisabled: "All database connections disabled immediately.",
    connectionEnabled: "{connection} enabled.",
    connectionDisabled: "{connection} disabled.",
    toolEnabled: "{tool} enabled.",
    toolDisabled: "{tool} disabled.",
    agentCopied: "Agent connection config copied.",
    connectionTestPassed: "Connection ready · {elapsed} ms",
    connectionsExported: "Exported {count} database connections. Protect the file containing plaintext passwords.",
    connectionsImported: "Created {count} database connections."
  },
  overview: {
    loading: "Loading local workspace...",
    metricConnections: "Database connections",
    metricTools: "MCP tools",
    metricServer: "MCP server",
    metricUptime: "Uptime",
    running: "Running",
    stopped: "Stopped",
    notStarted: "Not started",
    newConnection: "New connection",
    viewAllConnections: "View all connections",
    recentLogs: "Recent logs",
    viewAll: "View all",
    quickStart: "Quick start",
    quickConnectTitle: "Connect database",
    quickConnectText: "Configure read-only SQLite, MySQL, or PostgreSQL connections.",
    quickServerTitle: "Start server",
    quickServerText: "Open the local MCP endpoint for agents on this machine.",
    quickAgentTitle: "Connect agent",
    quickAgentText: "Copy the access prompt and send it to your agent to complete MCP setup.",
    copyAgentConfig: "Copy access config"
  },
  connections: {
    title: "Database connections",
    empty: "No database connections yet. Create a read-only connection first.",
    newConnectionName: "New read-only connection",
    test: "Test connection",
    diagnose: "Diagnose connection",
    emergencyDisable: "Emergency disconnect",
    toggleEnabled: "Enable or disable {name}",
    edit: "Edit connection",
    delete: "Delete connection",
    enabled: "Enabled",
    paused: "Paused",
    noDatabaseFile: "No database file selected"
  },
  tools: {
    summary: "{enabled} / {total} tools enabled",
    toggle: "Toggle {name}",
    groups: {
      discovery: "Discovery",
      access: "Data access",
      analysis: "Analysis & policy"
    },
    names: {
      datanexa_list_connections: "List connections",
      datanexa_get_schema: "Read schema",
      datanexa_describe_table: "Describe table",
      datanexa_sample_rows: "Sample rows",
      datanexa_execute_readonly_sql: "Run read-only SQL",
      datanexa_explain_sql: "Explain SQL",
      datanexa_policy_check: "Policy check",
      system_auto_start_mcp: "Auto-start MCP",
      system_start_mcp: "Start MCP"
    },
    intros: {
      datanexa_list_connections: "Returns enabled database connections to the agent without passwords or full DSNs.",
      datanexa_get_schema: "Reads tables and views for a connection so the agent understands schema boundaries.",
      datanexa_describe_table: "Reads column metadata for one table, with table and schema handled as structured arguments.",
      datanexa_sample_rows: "Reads a bounded sample from a table, using read-only policy and row limits automatically.",
      datanexa_execute_readonly_sql: "Runs one read-only query after policy validation. This is the strongest tool and should be enabled carefully.",
      datanexa_explain_sql: "Returns a query plan so the agent can analyze SQL without reading business data directly.",
      datanexa_policy_check: "Performs static policy validation only. It does not connect to databases or write audit logs."
    }
  },
  server: {
    endpoint: "Local MCP endpoint",
    stop: "Stop server",
    start: "Start server",
    copyEndpoint: "Copy endpoint",
    accessToken: "Access token",
    rotateToken: "Rotate manually",
    generatedOnStart: "Generated on first server start",
    copyToken: "Copy token",
    tokenDisabledTitle: "Bearer access token is off",
    tokenDisabledText: "Turn it back on from Settings when authentication is required.",
    agentAccess: "Agent access",
    copyToAgent: "Copy for agent"
  },
  audit: {
    title: "Audit log",
    time: "Time",
    tool: "Tool",
    connection: "Connection",
    status: "Status",
    detail: "Details",
    empty: "No access records yet.",
    emptyCompact: "No logs yet.",
    clear: "Clear logs",
    pageInfo: "Page {page} / {totalPages}, {total} total",
    detailTitle: "Log details",
    elapsedRows: "Elapsed / rows",
    elapsedRowsValue: "{elapsed}ms / {rows} rows",
    reason: "Reason",
    noSql: "This log entry has no SQL text.",
    filter: "Filter",
    clearFilter: "Clear filter",
    filterTitle: "Filter audit log",
    from: "From date",
    to: "To date",
    all: "All",
    applyFilter: "Apply filter",
    invalidDateRange: "End date cannot be earlier than start date",
    datePlaceholder: "YYYY/MM/DD",
    selectDate: "Select date",
    previousMonth: "Previous month",
    nextMonth: "Next month",
    calendarMonth: "{year}/{month}",
    weekdaySun: "Sun",
    weekdayMon: "Mon",
    weekdayTue: "Tue",
    weekdayWed: "Wed",
    weekdayThu: "Thu",
    weekdayFri: "Fri",
    weekdaySat: "Sat",
    results: "results"
  },
  settings: {
    general: "General",
    about: "About",
    servicePolicy: "MCP server policy",
    listenHost: "Listen host",
    port: "Port",
    requireBearer: "Require Bearer token",
    display: "Display",
    startup: "Startup & background",
    language: "Interface language",
    theme: "Theme",
    themeSystem: "System",
    themeLight: "Light",
    themeDark: "Dark",
    autoCheckUpdates: "Check automatically every day",
    autoStartMcp: "Start DataNexa and MCP when you log in",
    auditLog: "Audit log",
    auditMaxEvents: "Maximum retained log entries",
    auditRedactSql: "Redact SQL literals (off may record sensitive data)",
    importExport: "Import / export",
    importConnections: "Import connections",
    importConnectionsDescription: "Append connections from a DataNexa JSON file without changing existing connections.",
    exportConnections: "Export connections",
    exportConnectionsDescription: "Save the current database connections and passwords as a portable JSON file.",
    exportWarningTitle: "The export file contains plaintext passwords",
    exportWarningDescription: "After export, database passwords are no longer protected by the OS credential vault. Confirm that you understand these risks.",
    exportWarningAccess: "Anyone who can read the file may be able to use its database credentials.",
    exportWarningLocation: "Save it only to a trusted, access-controlled location. Never upload it to a source repository or public drive.",
    exportWarningCleanup: "Delete the file after migration, including copies in backups, synced folders, and the recycle bin.",
    exportAcknowledgement: "I understand that database passwords are stored in plaintext in the export file",
    confirmExport: "Choose location and export",
    policyConsole: "Policy console",
    policyDescription: "Runs static SQL policy validation only. It does not connect to real databases or write audit logs.",
    sqlDialect: "SQL dialect",
    checkSql: "Check SQL",
    allowed: "Allowed",
    denied: "Denied",
    securityPosture: "Security policy",
    securityAst: "Only SELECT, safe WITH SELECT, and EXPLAIN are allowed",
    securityVault: "Credentials normally stay in the OS vault and plaintext passwords are not written to regular config",
    securityAudit: "Audit logs do not store query results",
    securityReadonly: "Read-only sessions apply row, timeout, and connection limits by default",
    securityWarning: "Read-only policy cannot guarantee every risk is blocked. Constrain agents and avoid asking or allowing them to perform dangerous actions.",
    aboutText: "A local read-only database MCP gateway that gives agents one unified, safe, and auditable way to access your data sources.",
    officialHomepage: "Official homepage"
  },
  updates: {
    readyTitle: "Application updates",
    readyDescription: "Check manually for a newer version of DataNexa.",
    localBuildTitle: "Local build",
    localBuildDescription: "Automatic updates are disabled in this build, so it cannot check for or install releases.",
    checkingTitle: "Checking for updates",
    checkingDescription: "Connecting to GitHub Releases for the latest version.",
    upToDateTitle: "You're up to date",
    upToDateDescription: "DataNexa v{version} is the latest available version.",
    availableTitle: "Update available",
    availableCompact: "v{version} available",
    availableDescription: "DataNexa v{version} is ready to download.",
    downloadingTitle: "Downloading update",
    downloadingDescription: "Downloading and verifying the update package securely.",
    downloadingProgress: "Download progress {progress}%",
    relaunchingTitle: "Installing update",
    relaunchingDescription: "The new version has been downloaded and is being installed...",
    checkFailedTitle: "Unable to check for updates",
    checkFailedDescription: "Check your network connection and try again later.",
    downloadFailedTitle: "Update download failed",
    downloadFailedDescription: "Retry the update or download the installer manually from GitHub Releases.",
    relaunchFailedTitle: "Automatic restart failed",
    relaunchFailedDescription: "The update is installed. Close and reopen DataNexa manually.",
    checkNow: "Check for updates",
    retry: "Retry",
    updateNow: "Update now",
    viewReleaseNotes: "View release notes",
    manualDownload: "Download manually",
    openReleases: "GitHub Releases",
    dismissReminder: "Dismiss update reminder",
    confirmTitle: "Update DataNexa",
    confirmDescription: "Download and install DataNexa v{version}. The app will restart when installation is complete.",
    targetVersion: "Target version"
  },
  connectionDialog: {
    addTitle: "Add connection",
    editTitle: "Edit connection",
    description: "Connection metadata is written to TOML. Sensitive values such as passwords stay in the local credential vault.",
    basicInfo: "Basic information",
    name: "Connection name",
    stableId: "Stable ID",
    databaseType: "Database type",
    enableConnection: "Enable connection",
    address: "Connection address",
    databaseFile: "Database file",
    host: "Host",
    port: "Port",
    database: "Database",
    username: "Username",
    sslMode: "SSL mode",
    sslDisable: "Disable",
    sslPrefer: "Prefer / automatic",
    sslRequire: "Require",
    credentialsAndLimits: "Credentials and limits",
    password: "Password",
    keepExistingPassword: "Leave blank to keep the saved password",
    saveToVault: "Save to local credential vault",
    clearSavedCredential: "Clear saved credential",
    maxRows: "Maximum returned rows",
    queryTimeoutMs: "Query timeout in milliseconds",
    maxConnections: "Maximum connections",
    maxResultBytes: "Maximum result size (KiB)",
    currentCredential: "Current credential: {credential}. DataNexa never writes plaintext passwords to config files.",
    credentialNotSaved: "Not saved yet",
    save: "Save connection"
  },
  status: {
    allowed: "Allowed",
    denied: "Denied",
    timeout: "Timeout",
    truncated: "Truncated",
    error: "Error"
  },
  diagnostics: {
    noHint: "No extra hint.",
    summary: "Diagnostics complete: {name} · {type} · credential {credential}",
    title: "Connection diagnostics: {name} ({type})",
    address: "Address: host={host} port={port} database={database} username={username}",
    credential: "Credential: {credential}; SSL={ssl}; timeout={timeout}ms; pool={pool}",
    hint: "Hint: {hint}",
    notRequired: "Not required",
    notSaved: "Not saved",
    savedEmpty: "Saved empty password",
    saved: "Saved",
    missingInVault: "Credential reference exists, but the password was not found in the local vault",
    vaultError: "Failed to read the local credential vault"
  },
  agentPrompt: {
    intro: "Configure DataNexa as a local MCP server so it can safely access local read-only database connections.",
    configIntro: "MCP server config:"
  },
  api: {
    desktopOnly: "Command {name} is only available in the Tauri desktop app.",
    previewTestConnection: "Browser preview mode only simulates connection tests.",
    previewDiagnostics: "Browser preview mode only shows mock diagnostics.",
    previewPolicyReason: "Browser preview mode policy check."
  }
};

export const messages: Record<Locale, I18nMessages> = {
  "zh-CN": zhCN,
  en
};

export function normalizeLocale(locale: string | null | undefined): Locale {
  if (!locale) return DEFAULT_LOCALE;
  if (locale === "zh-CN" || locale === "zh_CN" || locale.toLowerCase() === "zh-cn") return "zh-CN";
  if (locale === "en" || locale.toLowerCase().startsWith("en-")) return "en";
  return DEFAULT_LOCALE;
}

export function detectLocale(): Locale {
  if (typeof window !== "undefined") {
    const stored = window.localStorage.getItem(LANGUAGE_STORAGE_KEY);
    if (stored) return normalizeLocale(stored);
  }

  if (typeof navigator !== "undefined") {
    const candidates = [navigator.language, ...(navigator.languages ?? [])];
    const matched = candidates.find((candidate) => candidate.toLowerCase().startsWith("en") || candidate.toLowerCase().startsWith("zh"));
    if (matched) return normalizeLocale(matched);
  }

  return DEFAULT_LOCALE;
}

export function persistLocale(locale: Locale) {
  if (typeof window !== "undefined") {
    window.localStorage.setItem(LANGUAGE_STORAGE_KEY, locale);
  }
}

export function formatMessage(template: string, values: Record<string, string | number> = {}) {
  return template.replace(/\{(\w+)\}/g, (match, key) => String(values[key] ?? match));
}
