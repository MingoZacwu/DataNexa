#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Locale {
    ZhCn,
    En,
}

impl Locale {
    pub fn from_language(language: &str) -> Self {
        let normalized = language.trim().to_ascii_lowercase();
        if normalized == "en" || normalized.starts_with("en-") {
            return Self::En;
        }
        Self::ZhCn
    }
}

#[derive(Debug, Clone, Copy)]
pub struct BackendText {
    locale: Locale,
}

pub struct ConnectionDiagnosticText<'a> {
    pub database_type: &'a str,
    pub host: &'a str,
    pub port: &'a str,
    pub database: &'a str,
    pub username: &'a str,
    pub credential: &'a str,
    pub ssl_mode: &'a str,
    pub timeout_ms: u64,
    pub pool_size: u32,
    pub hint: &'a str,
}

pub fn backend_text(language: &str) -> BackendText {
    BackendText {
        locale: Locale::from_language(language),
    }
}

impl BackendText {
    pub fn tray_mcp_status_running(self) -> &'static str {
        match self.locale {
            Locale::ZhCn => "MCP 服务运行中",
            Locale::En => "MCP server running",
        }
    }

    pub fn tray_mcp_status_stopped(self) -> &'static str {
        match self.locale {
            Locale::ZhCn => "MCP 服务已停止",
            Locale::En => "MCP server stopped",
        }
    }

    pub fn tray_mcp_status_error(self) -> &'static str {
        match self.locale {
            Locale::ZhCn => "MCP 服务启动失败",
            Locale::En => "MCP server failed to start",
        }
    }

    pub fn tray_mcp_startup_error(self) -> &'static str {
        match self.locale {
            Locale::ZhCn => "MCP 自动启动失败（打开应用查看详情）",
            Locale::En => "MCP auto-start failed (open app for details)",
        }
    }
    pub fn local_host_only(self) -> &'static str {
        match self.locale {
            Locale::ZhCn => "监听地址仅支持 127.0.0.1 或 localhost。",
            Locale::En => "The listen address must be 127.0.0.1 or localhost.",
        }
    }

    pub fn unknown_mcp_tool(self, name: &str) -> String {
        match self.locale {
            Locale::ZhCn => format!("未知的 MCP 工具：{name}"),
            Locale::En => format!("Unknown MCP tool: {name}"),
        }
    }

    pub fn connection_test_ok(self, elapsed_ms: u128) -> String {
        match self.locale {
            Locale::ZhCn => format!("连接测试通过，耗时 {elapsed_ms}ms。"),
            Locale::En => format!("Connection test passed in {elapsed_ms}ms."),
        }
    }

    pub fn connection_id_invalid(self) -> &'static str {
        match self.locale {
            Locale::ZhCn => "连接 ID 必须以字母或下划线开头，且只能包含字母、数字、下划线或短横线",
            Locale::En => "Connection ID must start with a letter or underscore and contain only letters, numbers, underscores, or hyphens",
        }
    }

    pub fn connection_name_required(self) -> &'static str {
        match self.locale {
            Locale::ZhCn => "连接名称不能为空",
            Locale::En => "Connection name is required",
        }
    }

    pub fn database_required(self) -> &'static str {
        match self.locale {
            Locale::ZhCn => "数据库不能为空",
            Locale::En => "Database is required",
        }
    }

    pub fn host_required(self) -> &'static str {
        match self.locale {
            Locale::ZhCn => "MySQL 和 PostgreSQL 必须填写主机地址",
            Locale::En => "MySQL and PostgreSQL connections require a host",
        }
    }

    pub fn no_extra_hint(self) -> &'static str {
        match self.locale {
            Locale::ZhCn => "无额外提示。",
            Locale::En => "No extra hint.",
        }
    }

    pub fn diagnostics_for_client(self, diagnostics: ConnectionDiagnosticText<'_>) -> String {
        let ConnectionDiagnosticText {
            database_type,
            host,
            port,
            database,
            username,
            credential,
            ssl_mode,
            timeout_ms,
            pool_size,
            hint,
        } = diagnostics;
        match self.locale {
            Locale::ZhCn => format!(
                "连接诊断：type={database_type} host={host} port={port} database={database} username={username} credential={credential} ssl={ssl_mode} timeout={timeout_ms}ms pool={pool_size}。提示：{hint}"
            ),
            Locale::En => format!(
                "Connection diagnostics: type={database_type} host={host} port={port} database={database} username={username} credential={credential} ssl={ssl_mode} timeout={timeout_ms}ms pool={pool_size}. Hint: {hint}"
            ),
        }
    }

    pub fn missing_password_hint(self) -> &'static str {
        match self.locale {
            Locale::ZhCn => "当前连接没有可用密码。请在编辑连接里重新输入密码并保存。",
            Locale::En => "This connection has no usable password. Re-enter the password in the connection editor and save it.",
        }
    }

    pub fn mysql_127_hint(self) -> &'static str {
        match self.locale {
            Locale::ZhCn => "如果本地 MySQL 返回 Access denied，可尝试把 host 改为 localhost；MySQL 授权可能区分 user@localhost 与 user@127.0.0.1。",
            Locale::En => "If local MySQL returns Access denied, try changing host to localhost; MySQL grants may distinguish user@localhost from user@127.0.0.1.",
        }
    }

    pub fn mysql_localhost_hint(self) -> &'static str {
        match self.locale {
            Locale::ZhCn => {
                "如果 localhost 连接失败，可再尝试 127.0.0.1，并确认 MySQL 开启了 TCP 监听。"
            }
            Locale::En => {
                "If localhost fails, try 127.0.0.1 and confirm MySQL is listening on TCP."
            }
        }
    }

    pub fn ssl_required_hint(self) -> &'static str {
        match self.locale {
            Locale::ZhCn => "当前强制使用 SSL；本地数据库如果没有启用 SSL，请在编辑连接里改为“禁用”。",
            Locale::En => "SSL is currently required; if a local database does not have SSL enabled, set SSL mode to Disable in the connection editor.",
        }
    }

    pub fn mysql_auth_failed(self, raw: &str) -> String {
        match self.locale {
            Locale::ZhCn => format!(
                "MySQL 认证失败。请在连接编辑窗口重新输入密码并保存；如果密码正确，请检查该用户是否允许当前来源地址连接。本地 MySQL 尤其要分别尝试 host=localhost 与 host=127.0.0.1，因为授权可能区分它们。原始错误：{raw}"
            ),
            Locale::En => format!(
                "MySQL authentication failed. Re-enter and save the password in the connection editor; if the password is correct, check whether the user may connect from this source host. For local MySQL, try both host=localhost and host=127.0.0.1 because grants may distinguish them. Original error: {raw}"
            ),
        }
    }

    pub fn mysql_database_missing(self, raw: &str) -> String {
        match self.locale {
            Locale::ZhCn => format!("MySQL 数据库不存在，或当前用户无权访问该数据库。原始错误：{raw}"),
            Locale::En => format!("The MySQL database does not exist, or the current user cannot access it. Original error: {raw}"),
        }
    }

    pub fn postgres_auth_failed(self, raw: &str) -> String {
        match self.locale {
            Locale::ZhCn => format!(
                "PostgreSQL 认证失败。请重新输入密码并保存；如果密码正确，请检查 pg_hba.conf、用户来源地址和认证方式。原始错误：{raw}"
            ),
            Locale::En => format!(
                "PostgreSQL authentication failed. Re-enter and save the password; if the password is correct, check pg_hba.conf, the user source address, and authentication method. Original error: {raw}"
            ),
        }
    }

    pub fn postgres_database_missing(self, raw: &str) -> String {
        match self.locale {
            Locale::ZhCn => format!("PostgreSQL 数据库不存在，或当前用户无权访问该数据库。原始错误：{raw}"),
            Locale::En => format!("The PostgreSQL database does not exist, or the current user cannot access it. Original error: {raw}"),
        }
    }

    pub fn postgres_permission_denied(self, raw: &str) -> String {
        match self.locale {
            Locale::ZhCn => format!("PostgreSQL 权限不足。请确认该用户具备连接数据库和读取元数据的权限。原始错误：{raw}"),
            Locale::En => format!("PostgreSQL permission denied. Confirm the user can connect to the database and read metadata. Original error: {raw}"),
        }
    }

    pub fn network_failed(self, db: &str, raw: &str) -> String {
        match self.locale {
            Locale::ZhCn => format!(
                "{db} 网络连接失败。请检查 host、端口、防火墙、数据库服务是否在监听 TCP，以及本地服务是否需要使用 localhost 而不是 127.0.0.1。原始错误：{raw}"
            ),
            Locale::En => format!(
                "{db} network connection failed. Check host, port, firewall, whether the database service is listening on TCP, and whether a local service needs localhost instead of 127.0.0.1. Original error: {raw}"
            ),
        }
    }

    pub fn tls_failed(self, db: &str, raw: &str) -> String {
        match self.locale {
            Locale::ZhCn => format!(
                "{db} TLS/SSL 握手失败。若是本地数据库，通常可在连接编辑里把 SSL 模式改为“禁用”；若是云数据库，请确认 SSL 模式和证书要求。原始错误：{raw}"
            ),
            Locale::En => format!(
                "{db} TLS/SSL handshake failed. For a local database, SSL mode can usually be set to Disable in the connection editor; for a cloud database, confirm SSL mode and certificate requirements. Original error: {raw}"
            ),
        }
    }

    pub fn connection_timeout(self, db: &str) -> String {
        match self.locale {
            Locale::ZhCn => format!("{db} 连接超时。请检查 host、端口、网络可达性，并适当增大查询超时。"),
            Locale::En => format!("{db} connection timed out. Check host, port, network reachability, and increase the query timeout if needed."),
        }
    }

    pub fn policy_sql_empty(self) -> &'static str {
        match self.locale {
            Locale::ZhCn => "SQL 不能为空。",
            Locale::En => "SQL cannot be empty.",
        }
    }

    pub fn policy_destructive_blocked(self) -> &'static str {
        match self.locale {
            Locale::ZhCn => "只读策略已阻止 DDL、DML、破坏性语句和可能产生副作用的 SQL。",
            Locale::En => "The read-only policy blocked DDL, DML, destructive statements, or SQL that may have side effects.",
        }
    }

    pub fn policy_side_effect_blocked(self) -> &'static str {
        match self.locale {
            Locale::ZhCn => "已阻止可能产生副作用的函数或文件导入/导出语法。",
            Locale::En => {
                "Functions or file import/export syntax that may have side effects were blocked."
            }
        }
    }

    pub fn policy_parser_rejected(self, error: &str) -> String {
        match self.locale {
            Locale::ZhCn => format!("SQL 解析器拒绝该语句：{error}"),
            Locale::En => format!("The SQL parser rejected this statement: {error}"),
        }
    }

    pub fn policy_single_statement_only(self) -> &'static str {
        match self.locale {
            Locale::ZhCn => "每次工具调用只允许一条 SQL 语句。",
            Locale::En => "Only one SQL statement is allowed per tool call.",
        }
    }

    pub fn policy_select_only(self) -> &'static str {
        match self.locale {
            Locale::ZhCn => "只允许 SELECT、安全的 WITH SELECT 和 EXPLAIN 语句。",
            Locale::En => "Only SELECT, safe WITH SELECT, and EXPLAIN statements are allowed.",
        }
    }

    pub fn policy_allowed(self) -> &'static str {
        match self.locale {
            Locale::ZhCn => "只读 SQL 策略已允许该语句。",
            Locale::En => "The read-only SQL policy allowed this statement.",
        }
    }

    pub fn tray_show(self) -> &'static str {
        match self.locale {
            Locale::ZhCn => "显示 DataNexa",
            Locale::En => "Show DataNexa",
        }
    }

    pub fn tray_mcp_server(self) -> &'static str {
        match self.locale {
            Locale::ZhCn => "MCP 服务",
            Locale::En => "MCP Server",
        }
    }

    pub fn jdbc_error(self, raw: &str) -> String {
        let raw = raw.trim();
        if raw.is_empty() {
            return match self.locale {
                Locale::ZhCn => "JDBC 操作失败。".to_string(),
                Locale::En => "JDBC operation failed.".to_string(),
            };
        }
        if matches!(self.locale, Locale::En) {
            return raw.to_string();
        }

        let exact = match raw {
            "Maven coordinate must use groupId:artifactId:version" => "Maven 坐标必须使用 groupId:artifactId:version 格式。",
            "Driver display name must contain between 1 and 80 characters" => "驱动显示名称长度必须在 1 到 80 个字符之间。",
            "Maven completed without producing JDBC driver JAR files" => "Maven 完成后没有生成 JDBC 驱动 JAR 文件。",
            "Maven repository path must not be a symbolic link" => "Maven 仓库路径不能是符号链接。",
            "Maven repository path is not a directory" => "Maven 仓库路径不是目录。",
            "The installed bundle does not expose any JDBC Driver implementations" => "已安装的 Bundle 未提供任何 JDBC Driver 实现。",
            "Select at least one JDBC driver JAR or directory" => "请至少选择一个 JDBC 驱动 JAR 文件或目录。",
            "Local JDBC driver files exceed the 1 GiB limit" => "本地 JDBC 驱动文件超过 1 GiB 限制。",
            "The imported bundle does not expose any JDBC Driver implementations" => "导入的 Bundle 未提供任何 JDBC Driver 实现。",
            "JDBC manager is unavailable in this test context" => "当前测试环境无法使用 JDBC 管理器。",
            "Invalid JDBC driver bundle ID" => "JDBC 驱动 Bundle ID 无效。",
            "This JDBC driver is referenced by a saved connection and cannot be deleted" => "该 JDBC 驱动仍被已保存的连接引用，无法删除。",
            "JDBC driver bundle was not found" => "未找到 JDBC 驱动 Bundle。",
            "JDBC driver bundle is incomplete or damaged" => "JDBC 驱动 Bundle 不完整或已损坏。",
            "Connection is not a JDBC connection" => "该连接不是 JDBC 连接。",
            "connection not found" => "未找到数据库连接。",
            "connection is disabled" => "数据库连接已禁用。",
            "JDBC driver is required" => "必须选择 JDBC 驱动 Bundle。",
            "JDBC URL is required" => "必须填写 JDBC URL。",
            "a valid JDBC URL is required" => "必须填写有效的 JDBC URL。",
            "JDBC connection did not return database metadata" => "JDBC 连接未返回数据库元数据。",
            "JDBC driver JAR has an invalid file name" => "JDBC 驱动 JAR 文件名无效。",
            "JDBC driver bundle path is not valid UTF-8" => "JDBC 驱动 Bundle 路径不是有效的 UTF-8 文本。",
            "JDBC sidecar stdin was unavailable" => "JDBC sidecar 的标准输入不可用。",
            "JDBC sidecar stdout was unavailable" => "JDBC sidecar 的标准输出不可用。",
            "JDBC operation timed out" => "JDBC 操作超时。",
            "JDBC sidecar response exceeded the size limit" => "JDBC sidecar 响应超过大小限制。",
            "JDBC sidecar returned an invalid response" => "JDBC sidecar 返回了无效响应。",
            "JDBC sidecar protocol validation failed" => "JDBC sidecar 协议校验失败。",
            "JDBC operation failed" => "JDBC 操作失败。",
            "The selected external Java Runtime is unavailable" => "选定的外部 Java 运行时不可用。",
            "JDBC sidecar is unavailable. Build jdbc-sidecar/pom.xml or prepare the bundled runtime." => "JDBC sidecar 不可用。请构建 jdbc-sidecar/pom.xml，或准备内置 Java 运行时。",
            "DataNexa Java Runtime is not installed. Download it or select an external Java Runtime." => "DataNexa Java 运行时尚未安装。请下载或选择外部 Java 运行时。",
            "DataNexa JRE public key is not configured" => "DataNexa JRE 公钥尚未配置。",
            "Downloaded DataNexa JRE failed SHA-256 verification" => "下载的 DataNexa JRE 未通过 SHA-256 校验。",
            "DataNexa JRE manifest signature verification failed" => "DataNexa JRE 清单签名校验失败。",
            "Unsupported DataNexa JRE manifest" => "不支持的 DataNexa JRE 清单。",
            "DataNexa JRE archive size is invalid" => "DataNexa JRE 压缩包大小无效。",
            "DataNexa JRE archive SHA-256 is invalid" => "DataNexa JRE 压缩包 SHA-256 值无效。",
            "DataNexa JRE archive URL is not trusted" => "DataNexa JRE 压缩包下载地址不受信任。",
            "DataNexa JRE archive is too large" => "DataNexa JRE 压缩包过大。",
            "DataNexa JRE archive contains an unsafe path" => "DataNexa JRE 压缩包包含不安全路径。",
            "DataNexa JRE archive contains an unsupported entry" => "DataNexa JRE 压缩包包含不支持的文件项。",
            "DataNexa JRE archive format is unsupported" => "不支持的 DataNexa JRE 压缩包格式。",
            "Downloaded DataNexa JRE is incomplete" => "下载的 DataNexa JRE 不完整。",
            "DataNexa JRE archive exceeded its declared size" => "DataNexa JRE 压缩包超过清单声明的大小。",
            "DataNexa JRE archive size does not match its manifest" => "DataNexa JRE 压缩包大小与清单不一致。",
            "Managed Java Runtime metadata is invalid" => "DataNexa Java 运行时元数据无效。",
            "Managed Java Runtime is incomplete" => "DataNexa Java 运行时不完整。",
            "Cannot remove DataNexa Java Runtime while JDBC sessions are running" => "JDBC 会话运行期间不能删除 DataNexa Java 运行时。",
            "Unsupported JDBC driver bundle manifest" => "不支持的 JDBC 驱动 Bundle 清单。",
            "Maven repository must be an HTTPS URL" => "Maven 仓库必须使用 HTTPS URL。",
            "No .jar files were found in the selected paths" => "所选路径中没有找到 .jar 文件。",
            "Local JDBC driver import contains too many JAR files" => "本地 JDBC 驱动导入包含过多 JAR 文件。",
            "JDBC driver bundle contains no driver files" => "JDBC 驱动 Bundle 不包含驱动文件。",
            "JDBC driver bundle contains an invalid file name" => "JDBC 驱动 Bundle 包含无效文件名。",
            "JDBC driver bundle manifest contains duplicate files" => "JDBC 驱动 Bundle 清单包含重复文件。",
            "JDBC driver bundle does not contain a jars directory" => "JDBC 驱动 Bundle 不包含 jars 目录。",
            "JDBC driver bundle contains an unexpected directory entry" => "JDBC 驱动 Bundle 包含意外的目录项。",
            "JDBC driver bundle contains an unlisted file" => "JDBC 驱动 Bundle 包含清单未列出的文件。",
            "JDBC driver bundle is missing a manifest file" => "JDBC 驱动 Bundle 缺少清单文件。",
            "JDBC driver bundle total size does not match its manifest" => "JDBC 驱动 Bundle 总大小与清单不一致。",
            "invalid table identifier" => "表标识符无效。",
            "policy accepted SQL without a rewritten statement" => "策略已允许 SQL，但未生成重写后的语句。",
            _ => "",
        };
        if !exact.is_empty() {
            return exact.to_string();
        }

        for (prefix, label) in [
            (
                "JDBC driver JAR has an invalid file name",
                "JDBC 驱动 JAR 文件名无效",
            ),
            ("JDBC driver path does not exist: ", "JDBC 驱动路径不存在："),
            (
                "JDBC driver bundle is missing file ",
                "JDBC 驱动 Bundle 缺少文件：",
            ),
            (
                "JDBC driver bundle contains duplicate file names: ",
                "JDBC 驱动文件名重复：",
            ),
            (
                "JDBC runtime could not be started: ",
                "JDBC Java 运行时启动失败：",
            ),
            (
                "JDBC sidecar exited unexpectedly: ",
                "JDBC sidecar 意外退出：",
            ),
            ("JDBC sidecar stdin failed: ", "JDBC sidecar 标准输入失败："),
            (
                "JDBC sidecar stdin flush failed: ",
                "JDBC sidecar 刷新标准输入失败：",
            ),
            (
                "JDBC sidecar stdout failed: ",
                "JDBC sidecar 标准输出失败：",
            ),
            (
                "Invalid DataNexa JRE public key: ",
                "DataNexa JRE 公钥无效：",
            ),
            (
                "Invalid DataNexa JRE manifest signature: ",
                "DataNexa JRE 清单签名无效：",
            ),
            (
                "DataNexa JRE manifest signature verification failed: ",
                "DataNexa JRE 清单签名校验失败：",
            ),
            (
                "DataNexa JRE is not available for ",
                "当前平台没有可用的 DataNexa JRE：",
            ),
        ] {
            if let Some(detail) = raw.strip_prefix(prefix) {
                return format!("{label}{detail}");
            }
        }

        if let Some(detail) = raw.strip_prefix("JDBC driver bundle file ") {
            if let Some(name) = detail.strip_suffix(" has an unexpected size") {
                return format!("JDBC 驱动 Bundle 文件 {name} 大小不符合预期。");
            }
            if let Some(name) = detail.strip_suffix(" failed SHA-256 verification") {
                return format!("JDBC 驱动 Bundle 文件 {name} 未通过 SHA-256 校验。");
            }
        }

        if let Some((code, detail)) = raw
            .strip_prefix("JDBC ")
            .and_then(|value| value.split_once(": "))
        {
            let label = match code {
                "invalid_url" => "JDBC URL 无效",
                "url_not_accepted" => "已安装的 JDBC 驱动不接受此 JDBC URL",
                "driver_not_found" => "未发现 JDBC 驱动。请为此连接填写 Driver Class",
                "driver_load_failed" => "JDBC 驱动加载失败",
                "authentication_failed" => "JDBC 认证失败",
                "connection_lost" => "JDBC 连接已断开",
                "jdbc_error" => "JDBC 数据库操作失败",
                "invalid_bundle" => "JDBC 驱动 Bundle 无效",
                "invalid_table" => "表名无效",
                "invalid_sql" => "SQL 无效",
                "protocol_mismatch" => "JDBC sidecar 协议版本不兼容",
                "unsupported_action" => "JDBC sidecar 不支持此操作",
                "maven_resolution_failed" => "Maven 依赖解析失败",
                _ => "JDBC 操作失败",
            };
            let detail = match detail {
                "A JDBC URL beginning with jdbc: is required" => "JDBC URL 必须以 jdbc: 开头。",
                "The installed JDBC driver does not accept this JDBC URL" => "已安装的 JDBC 驱动不接受此 JDBC URL。",
                "No JDBC driver was discovered. Configure Driver Class explicitly for this connection." => "未发现 JDBC 驱动，请为此连接填写 Driver Class。",
                "Configured Driver Class does not implement java.sql.Driver" => "配置的 Driver Class 未实现 java.sql.Driver。",
                "JDBC driver bundle path is required" => "必须提供 JDBC 驱动 Bundle 路径。",
                "JDBC driver bundle does not contain a jars directory" => "JDBC 驱动 Bundle 不包含 jars 目录。",
                "JDBC driver bundle contains no JAR files" => "JDBC 驱动 Bundle 不包含 JAR 文件。",
                "A table name is required" => "必须填写表名。",
                "A SQL statement is required" => "必须填写 SQL 语句。",
                "JDBC operation failed" => "JDBC 操作失败。",
                _ => detail,
            };
            return if detail.is_empty() {
                format!("{label}。")
            } else {
                format!("{label}：{detail}")
            };
        }
        format!("JDBC 操作失败：{raw}")
    }

    pub fn jdbc_sample_rows_unsupported(self) -> &'static str {
        match self.locale {
            Locale::ZhCn => "通用 JDBC 配置不支持采样行，因为 DataNexa 无法保证采样 SQL 在不同数据库间可移植。",
            Locale::En => "Sample rows are unsupported for the generic JDBC profile because DataNexa cannot guarantee portable sampling SQL.",
        }
    }

    pub fn jdbc_explain_unsupported(self) -> &'static str {
        match self.locale {
            Locale::ZhCn => "通用 JDBC 配置不支持 Explain SQL，因为 Explain 语法和计划获取方式依赖具体数据库。",
            Locale::En => "Explain SQL is unsupported for the generic JDBC profile because Explain syntax and plan retrieval are database-specific.",
        }
    }

    pub fn jdbc_driver_required(self) -> &'static str {
        match self.locale {
            Locale::ZhCn => "必须选择 JDBC 驱动 Bundle。",
            Locale::En => "A JDBC driver bundle is required.",
        }
    }

    pub fn jdbc_url_required(self) -> &'static str {
        match self.locale {
            Locale::ZhCn => "JDBC URL 必须以 jdbc: 开头。",
            Locale::En => "A JDBC URL beginning with jdbc: is required.",
        }
    }

    pub fn tray_lightweight_mode(self) -> &'static str {
        match self.locale {
            Locale::ZhCn => "轻量模式",
            Locale::En => "Lightweight Mode",
        }
    }

    pub fn tray_quit(self) -> &'static str {
        match self.locale {
            Locale::ZhCn => "退出",
            Locale::En => "Quit",
        }
    }
}
