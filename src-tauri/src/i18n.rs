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

pub fn backend_text(language: &str) -> BackendText {
    BackendText {
        locale: Locale::from_language(language),
    }
}

impl BackendText {
    pub fn local_host_only(self) -> &'static str {
        match self.locale {
            Locale::ZhCn => "DataNexa v1 仅允许绑定 127.0.0.1 或 localhost。",
            Locale::En => "DataNexa v1 can only bind to 127.0.0.1 or localhost.",
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

    pub fn diagnostics_for_client(
        self,
        database_type: &str,
        host: &str,
        port: &str,
        database: &str,
        username: &str,
        credential: &str,
        ssl_mode: &str,
        timeout_ms: u64,
        pool_size: u32,
        hint: &str,
    ) -> String {
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

    pub fn tray_quit(self) -> &'static str {
        match self.locale {
            Locale::ZhCn => "退出",
            Locale::En => "Quit",
        }
    }
}
