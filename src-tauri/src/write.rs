use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sqlx::mysql::MySqlConnectOptions;
use sqlx::postgres::PgConnectOptions;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{
    ConnectOptions, Connection, Executor, MySqlConnection, PgConnection, SqliteConnection,
};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

use crate::config::{default_port, ConnectionConfig, DbKind};
use crate::db::{mysql_ssl_mode, pg_ssl_mode, resolve_password};
use crate::vault::CredentialVault;

mod mysql;
mod postgres;
mod sqlite;

/// Upper bound on the columns a writable table may expose. The effective limit
/// for one request is `MAX_REQUEST_BYTES`, not the column count.
pub const MAX_COLUMNS: usize = 256;
/// Longest single text value accepted for one column.
pub const MAX_TEXT_BYTES: usize = 16_384;
/// Largest encoded request or preview.
pub const MAX_REQUEST_BYTES: usize = 65_536;
/// Bound for the stored value read back as the approval before-image. Values
/// longer than this are displayed and compared in truncated form; the write
/// itself never carries a truncated value.
const BEFORE_IMAGE_CHARS: usize = MAX_TEXT_BYTES + 1;

const MAX_NAME_CHARS: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WriteOperation {
    Insert,
    Update,
    Delete,
}

impl WriteOperation {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Insert => "insert",
            Self::Update => "update",
            Self::Delete => "delete",
        }
    }
}

/// Wire representation of one column value. Numbers always travel as decimal
/// strings so a JSON number can never round-trip through a float.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum WriteValue {
    Null,
    Boolean(bool),
    Number(String),
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WriteRequest {
    pub connection_id: String,
    pub operation: WriteOperation,
    #[serde(default)]
    pub target_id: Option<String>,
    #[serde(default)]
    pub schema: String,
    #[serde(default)]
    pub table: String,
    #[serde(default)]
    pub values: BTreeMap<String, WriteValue>,
    #[serde(default)]
    pub primary_key: BTreeMap<String, WriteValue>,
    pub idempotency_key: String,
    #[serde(default)]
    pub reason: Option<String>,
}

// These values may contain personal data. The manager must redact audit/log
// output and protect previews in transit and at rest; never log this structure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WriteChange {
    pub column: String,
    pub before: Option<WriteValue>,
    pub after: WriteValue,
}

/// A column the request does not mention but the database still fills in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WriteNote {
    pub column: String,
    /// `uses_default` or `auto_generated`.
    pub code: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WritePreview {
    pub sql: String,
    pub changes: Vec<WriteChange>,
    pub primary_key: BTreeMap<String, WriteValue>,
    #[serde(default)]
    pub notes: Vec<WriteNote>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Clone)]
pub struct PreparedWrite {
    pub request: WriteRequest,
    pub preview: WritePreview,
    original_request: WriteRequest,
    original_preview: WritePreview,
    metadata: Metadata,
    resolved: ResolvedWrite,
    connection: ConnectionIdentity,
}

impl PreparedWrite {
    // Read by the dialect modules when they bind values for one statement.
    fn metadata(&self) -> &Metadata {
        &self.metadata
    }

    fn resolved(&self) -> &ResolvedWrite {
        &self.resolved
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WriteError {
    pub code: String,
    pub message: String,
    pub outcome_unknown: bool,
}

impl WriteError {
    fn new(code: &'static str, message: &'static str) -> Self {
        Self {
            code: code.to_string(),
            message: message.to_string(),
            outcome_unknown: false,
        }
    }

    fn unknown(message: &'static str) -> Self {
        Self {
            code: "outcome_unknown".to_string(),
            message: message.to_string(),
            outcome_unknown: true,
        }
    }
}

impl fmt::Display for WriteError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for WriteError {}

pub type Result<T> = std::result::Result<T, WriteError>;

fn database_error(error: sqlx::Error) -> WriteError {
    // The client must never see server text, but a failure that only ever says
    // "write_failed" cannot be diagnosed. Log the category and the vendor error
    // code, never the message: a server message can echo SQL fragments or
    // rejected values.
    let (category, database_code) = database_error_diagnostics(&error);
    crate::debug_log::warn(
        "write",
        format_args!(
            "write statement failed (category={category}, code={})",
            database_code.as_deref().unwrap_or("none"),
        ),
    );
    let mut failure = WriteError::new(
        "database_error",
        "The database rejected the operation; server details are withheld.",
    );
    // A vendor code is a short alphanumeric token, not server prose, and it is
    // what distinguishes "permission denied" from a protocol or syntax problem.
    if let Some(code) = database_code {
        failure.message = format!(
            "The database rejected the operation; server details are withheld (database code {code})."
        );
    }
    failure
}

/// Classifies an error and extracts the vendor code. Never returns messages.
fn database_error_diagnostics(error: &sqlx::Error) -> (&'static str, Option<String>) {
    let category = match error {
        sqlx::Error::Database(_) => "database",
        sqlx::Error::ColumnDecode { .. } | sqlx::Error::Decode(_) => "decode",
        sqlx::Error::ColumnNotFound(_) | sqlx::Error::ColumnIndexOutOfBounds { .. } => "column",
        sqlx::Error::PoolTimedOut => "pool_timeout",
        sqlx::Error::Io(_) => "io",
        sqlx::Error::Tls(_) => "tls",
        sqlx::Error::Protocol(_) => "protocol",
        sqlx::Error::Configuration(_) => "configuration",
        sqlx::Error::Encode(_) => "encode",
        sqlx::Error::RowNotFound => "row_not_found",
        _ => "other",
    };
    let code = error
        .as_database_error()
        .and_then(|error| error.code())
        .filter(|value| !value.is_empty() && value.len() <= 16)
        .filter(|value| value.bytes().all(|byte| byte.is_ascii_alphanumeric()))
        .map(|value| value.into_owned());
    (category, code)
}

fn unsupported(message: &'static str) -> WriteError {
    WriteError::new("unsupported_capability", message)
}

/// Same rejection, but with a stable reason code the UI can localize. Used by
/// the per-dialect metadata readers so the table picker can explain exactly why
/// a table cannot be written instead of only showing "not writable".
pub(crate) fn unsupported_reason(code: &str, message: &str) -> WriteError {
    WriteError {
        code: code.to_string(),
        message: message.to_string(),
        outcome_unknown: false,
    }
}

fn stale() -> WriteError {
    WriteError::new(
        "stale_preview",
        "The connection, table definition, or row changed. Prepare a new preview.",
    )
}

fn deadline(config: &ConnectionConfig) -> Duration {
    Duration::from_millis(config.query_timeout_ms.clamp(500, 30_000))
}

#[derive(Clone, PartialEq, Eq)]
struct ConnectionIdentity {
    id: String,
    kind: DbKind,
    database: String,
    host: Option<String>,
    port: Option<u16>,
    username: Option<String>,
    credential_ref: Option<String>,
    ssl_mode: Option<String>,
}

impl From<&ConnectionConfig> for ConnectionIdentity {
    fn from(config: &ConnectionConfig) -> Self {
        Self {
            id: config.id.clone(),
            kind: config.kind.clone(),
            database: config.database.clone(),
            host: config.host.clone(),
            port: config.port,
            username: config.username.clone(),
            credential_ref: config.credential_ref.clone(),
            ssl_mode: config.ssl_mode.clone(),
        }
    }
}

/// Text-shaped column types. Everything that is not a number or a boolean is
/// carried as text and validated against the shape its column declares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextFormat {
    Plain,
    Json,
    Jsonb,
    Uuid,
    Date,
    Time,
    Timestamp,
    TimestampTz,
}

impl TextFormat {
    /// Shape hint shown when a value does not match its column.
    fn hint(self) -> Option<&'static str> {
        match self {
            Self::Plain => None,
            Self::Json => Some("expects a JSON document"),
            Self::Jsonb => Some("expects a JSON document"),
            Self::Uuid => Some("expects a UUID such as 0f8fad5b-d9cb-469f-a165-70867728950e"),
            Self::Date => Some("expects YYYY-MM-DD"),
            Self::Time => Some("expects HH:MM:SS"),
            Self::Timestamp => Some("expects YYYY-MM-DD HH:MM:SS"),
            Self::TimestampTz => Some("expects YYYY-MM-DDTHH:MM:SS with Z or an offset"),
        }
    }

    /// Cast used for PostgreSQL placeholders and before-image reads. Only
    /// fixed names generated here are ever interpolated.
    fn pg_cast(self) -> &'static str {
        match self {
            Self::Plain => "text",
            Self::Json => "json",
            Self::Jsonb => "jsonb",
            Self::Uuid => "uuid",
            Self::Date => "date",
            Self::Time => "time",
            Self::Timestamp => "timestamp",
            Self::TimestampTz => "timestamptz",
        }
    }
}

/// How one column may be written. `Unsupported` columns stay readable in the
/// catalogue but are never offered for writing; they never reject the table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColumnKind {
    Boolean,
    Integer { min: i128, max: i128 },
    Decimal { precision: u32, scale: u32 },
    Float,
    Text { max_bytes: usize, format: TextFormat },
    Unsupported,
}

impl ColumnKind {
    fn supported(&self) -> bool {
        !matches!(self, Self::Unsupported)
    }

    fn pg_cast(&self) -> Option<&'static str> {
        match self {
            Self::Boolean => Some("boolean"),
            Self::Integer { .. } => Some("int8"),
            Self::Decimal { .. } => Some("numeric"),
            Self::Float => Some("float8"),
            Self::Text { format, .. } => Some(format.pg_cast()),
            Self::Unsupported => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column {
    pub name: String,
    /// Type as the database reports it, for display only.
    pub declared: String,
    pub kind: ColumnKind,
    pub nullable: bool,
    pub primary_key: bool,
    /// Computed by the database; never writable.
    pub generated: bool,
    /// Generated by the database when omitted; may still be set explicitly.
    pub auto_increment: bool,
    pub has_default: bool,
    /// `ON UPDATE CURRENT_TIMESTAMP`-style automatic maintenance.
    pub on_update: bool,
}

impl Column {
    fn insertable(&self) -> bool {
        self.kind.supported() && !self.generated
    }

    fn updatable(&self) -> bool {
        self.kind.supported() && !self.generated && !self.primary_key
    }

    /// INSERT must supply this column when the database cannot fill it in.
    fn required_on_insert(&self) -> bool {
        !self.nullable && !self.has_default && !self.auto_increment && !self.generated
    }
}

/// Column-level capability reported to the desktop UI.
#[derive(Debug, Clone, Serialize)]
pub struct ColumnCapability {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub primary_key: bool,
    pub generated: bool,
    pub auto_increment: bool,
    pub has_default: bool,
    pub insertable: bool,
    pub updatable: bool,
}

impl From<&Column> for ColumnCapability {
    fn from(column: &Column) -> Self {
        Self {
            name: column.name.clone(),
            data_type: column.declared.clone(),
            nullable: column.nullable,
            primary_key: column.primary_key,
            generated: column.generated,
            auto_increment: column.auto_increment,
            has_default: column.has_default,
            insertable: column.insertable(),
            updatable: column.updatable(),
        }
    }
}

/// Table-level capability. `insert`/`update`/`delete` are the only reasons a
/// table is offered for writing; everything else is a warning shown in the
/// approval window, never a rejection.
#[derive(Debug, Clone, Serialize)]
pub struct TableCapability {
    pub columns: Vec<ColumnCapability>,
    pub insert: bool,
    pub update: bool,
    pub delete: bool,
    pub warnings: Vec<String>,
    pub reason_code: Option<String>,
    pub reason: Option<String>,
    /// The concrete object behind `reason_code` (a column and its declared type,
    /// for example), so the picker can explain a rejection without exposing a
    /// database message.
    pub reason_detail: Option<String>,
}

impl TableCapability {
    /// A table that cannot be written at all, carrying the reason to report.
    pub fn blocked(reason: &WriteError) -> Self {
        Self {
            columns: Vec::new(),
            insert: false,
            update: false,
            delete: false,
            warnings: Vec::new(),
            reason_code: Some(reason.code.clone()),
            reason: Some(reason.message.clone()),
            reason_detail: None,
        }
    }

    /// The same verdict `Metadata::capability` produces, built from the
    /// per-table column and warning sets a batch catalogue read collected.
    pub(crate) fn from_parts(columns: Vec<Column>, warnings: Vec<String>) -> Self {
        let fingerprint = column_signature(&columns);
        Metadata {
            columns,
            fingerprint,
            warnings,
        }
        .capability()
    }
}

#[derive(Clone, PartialEq, Eq)]
struct Metadata {
    columns: Vec<Column>,
    /// Signature of the column shape. Deliberately excludes table-wide schema
    /// versions so an unrelated DDL statement cannot invalidate a preview.
    fingerprint: String,
    warnings: Vec<String>,
}

impl Metadata {
    fn primary_key(&self) -> Vec<&Column> {
        self.columns
            .iter()
            .filter(|column| column.primary_key)
            .collect()
    }
    fn column(&self, name: &str) -> Option<&Column> {
        self.columns.iter().find(|column| column.name == name)
    }

    fn capability(&self) -> TableCapability {
        let insert = self.insert_supported();
        let update = self.update_supported();
        let delete = !self.primary_key().is_empty();
        let mut reason_code = None;
        let mut reason = None;
        let mut reason_detail = None;
        if !insert && !update && !delete {
            let failure = self.unsupported_reason();
            reason_detail = self.unsupported_detail(&failure.code);
            reason_code = Some(failure.code);
            reason = Some(failure.message);
        }
        TableCapability {
            columns: self.columns.iter().map(ColumnCapability::from).collect(),
            insert,
            update,
            delete,
            warnings: self.warnings.clone(),
            reason_code,
            reason,
            reason_detail,
        }
    }

    /// The concrete metadata behind a rejection, phrased as identifiers and
    /// declared types only: no row values and no database prose. The UI shows
    /// this next to the localized reason so "not writable" is explainable.
    fn unsupported_detail(&self, code: &str) -> Option<String> {
        match code {
            "primary_key_nullable" => {
                let columns = self
                    .primary_key()
                    .iter()
                    .filter(|column| column.nullable)
                    .map(|column| column.name.as_str())
                    .collect::<Vec<_>>();
                (!columns.is_empty()).then(|| columns.join(", "))
            }
            "unsupported_column_type" => self
                .columns
                .iter()
                .find(|column| column.required_on_insert() && !column.insertable())
                .map(|column| format!("{} · {}", column.name, column.declared)),
            _ => None,
        }
    }

    /// A complete, non-null primary key is the only structural requirement:
    /// it is what makes "edit exactly this row" expressible.
    fn key_is_usable(&self) -> bool {
        let primary = self.primary_key();
        !primary.is_empty() && !primary.iter().any(|column| column.nullable)
    }

    fn insert_supported(&self) -> bool {
        self.key_is_usable()
            && self
                .columns
                .iter()
                .filter(|column| column.required_on_insert())
                .all(|column| column.insertable())
    }

    fn update_supported(&self) -> bool {
        self.key_is_usable() && self.columns.iter().any(|column| column.updatable())
    }

    fn unsupported_reason(&self) -> WriteError {
        if self.primary_key().is_empty() {
            return WriteError {
                code: "no_primary_key".to_string(),
                message: "This table has no primary key, so a single row cannot be identified for writing."
                    .to_string(),
                outcome_unknown: false,
            };
        }
        if self.primary_key().iter().any(|column| column.nullable) {
            return WriteError {
                code: "primary_key_nullable".to_string(),
                message:
                    "This table's primary key allows null values, so a single row cannot be identified reliably."
                        .to_string(),
                outcome_unknown: false,
            };
        }
        let blocked = self
            .columns
            .iter()
            .find(|column| column.required_on_insert() && !column.insertable());
        if let Some(column) = blocked {
            return WriteError {
                code: "unsupported_column_type".to_string(),
                message: format!(
                    "Column `{}` is required on insert but its type ({}) cannot be written.",
                    column.name, column.declared
                ),
                outcome_unknown: false,
            };
        }
        WriteError {
            code: "no_writable_column".to_string(),
            message: "No column of this table can be written.".to_string(),
            outcome_unknown: false,
        }
    }

    fn plan(&self, request: &WriteRequest) -> Result<ResolvedWrite> {
        if !self.key_is_usable() {
            return Err(self.unsupported_reason());
        }
        let primary = self.primary_key();
        match request.operation {
            WriteOperation::Insert => self.plan_insert(request),
            WriteOperation::Update => self.plan_targeted(request, primary),
            WriteOperation::Delete => self.plan_delete(request),
        }
    }

    fn plan_insert(&self, request: &WriteRequest) -> Result<ResolvedWrite> {
        if !request.primary_key.is_empty() {
            return Err(WriteError::new(
                "invalid_primary_key",
                "INSERT identifies its row through `values`; do not send primary_key.",
            ));
        }
        if !self.insert_supported() {
            return Err(self.unsupported_reason());
        }
        let mut values = BTreeMap::new();
        for (name, value) in &request.values {
            let column = self.column(name).ok_or_else(|| {
                WriteError::new("invalid_column", "A requested column does not exist.")
            })?;
            if !column.insertable() {
                return Err(WriteError::new(
                    "invalid_column",
                    "One requested column is generated by the database and cannot be written.",
                ));
            }
            validate_value(column, value)?;
            values.insert(name.clone(), value.clone());
        }
        for column in self.columns.iter().filter(|c| c.required_on_insert()) {
            if !values.contains_key(&column.name) {
                return Err(WriteError {
                    code: "missing_required_column".to_string(),
                    message: format!(
                        "Column `{}` is required: it is NOT NULL and has no default or automatic value.",
                        column.name
                    ),
                    outcome_unknown: false,
                });
            }
        }
        let primary_key = self
            .primary_key()
            .into_iter()
            .filter_map(|column| {
                values
                    .get(&column.name)
                    .map(|value| (column.name.clone(), value.clone()))
            })
            .collect::<BTreeMap<_, _>>();
        for column in self.primary_key() {
            if let Some(value) = primary_key.get(&column.name) {
                if matches!(value, WriteValue::Null) {
                    return Err(WriteError::new(
                        "invalid_primary_key",
                        "Primary-key values must not be null.",
                    ));
                }
            }
        }
        Ok(ResolvedWrite { primary_key, values })
    }

    fn plan_targeted(
        &self,
        request: &WriteRequest,
        primary: Vec<&Column>,
    ) -> Result<ResolvedWrite> {
        let primary_key = self.validate_primary_key(request, &primary)?;
        if !self.update_supported() {
            return Err(unsupported("No column of this table can be updated."));
        }
        if request.values.is_empty() {
            return Err(WriteError::new(
                "invalid_request",
                "UPDATE requires at least one column value.",
            ));
        }
        let mut values = BTreeMap::new();
        for (name, value) in &request.values {
            let column = self.column(name).ok_or_else(|| {
                WriteError::new("invalid_column", "A requested column does not exist.")
            })?;
            if column.primary_key {
                return Err(WriteError::new(
                    "primary_key_immutable",
                    "Updating a primary-key column is not supported.",
                ));
            }
            if !column.updatable() {
                return Err(WriteError::new(
                    "invalid_column",
                    "One requested column is generated by the database and cannot be written.",
                ));
            }
            validate_value(column, value)?;
            values.insert(name.clone(), value.clone());
        }
        Ok(ResolvedWrite { primary_key, values })
    }

    fn plan_delete(&self, request: &WriteRequest) -> Result<ResolvedWrite> {
        if !request.values.is_empty() {
            return Err(WriteError::new(
                "invalid_request",
                "DELETE takes only `primary_key`; do not send values.",
            ));
        }
        let primary = self.primary_key();
        let primary_key = self.validate_primary_key(request, &primary)?;
        Ok(ResolvedWrite {
            primary_key,
            values: BTreeMap::new(),
        })
    }

    fn validate_primary_key(
        &self,
        request: &WriteRequest,
        primary: &[&Column],
    ) -> Result<BTreeMap<String, WriteValue>> {
        if request.primary_key.len() != primary.len() {
            return Err(WriteError::new(
                "invalid_primary_key",
                "The complete primary key is required and no other column may act as a predicate.",
            ));
        }
        let mut primary_key = BTreeMap::new();
        for column in primary {
            let value = request.primary_key.get(&column.name).ok_or_else(|| {
                WriteError::new(
                    "invalid_primary_key",
                    "The complete primary key is required.",
                )
            })?;
            if matches!(value, WriteValue::Null) {
                return Err(WriteError::new(
                    "invalid_primary_key",
                    "Primary-key values must not be null.",
                ));
            }
            validate_value(column, value)?;
            primary_key.insert(column.name.clone(), value.clone());
        }
        Ok(primary_key)
    }

    fn notes(&self, request: &WriteRequest, values: &BTreeMap<String, WriteValue>) -> Vec<WriteNote> {
        if request.operation != WriteOperation::Insert {
            return Vec::new();
        }
        self.columns
            .iter()
            .filter(|column| !values.contains_key(&column.name))
            .filter_map(|column| {
                let code = if column.auto_increment || column.generated {
                    "auto_generated"
                } else if column.has_default {
                    "uses_default"
                } else {
                    return None;
                };
                Some(WriteNote {
                    column: column.name.clone(),
                    code: code.to_string(),
                })
            })
            .collect()
    }
}

#[derive(Clone, PartialEq, Eq)]
struct ResolvedWrite {
    primary_key: BTreeMap<String, WriteValue>,
    values: BTreeMap<String, WriteValue>,
}

fn integer(value: &str) -> Result<i128> {
    let invalid = || {
        WriteError::new(
            "invalid_number",
            "Numbers must be exact decimal strings without exponent notation.",
        )
    };
    let (negative, digits) = match value.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, value),
    };
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(invalid());
    }
    if digits.len() > 1 && digits.starts_with('0') {
        return Err(invalid());
    }
    let parsed = value.parse::<i128>().map_err(|_| invalid())?;
    if parsed.to_string() != value {
        return Err(invalid());
    }
    let _ = negative;
    Ok(parsed)
}

fn decimal(value: &str) -> Result<(bool, String, String)> {
    let invalid = || {
        WriteError::new(
            "invalid_number",
            "Decimal values must be exact strings such as 12.50, without exponent notation.",
        )
    };
    let (negative, rest) = match value.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, value),
    };
    let mut parts = rest.split('.');
    let whole = parts.next().unwrap_or_default();
    let fraction = parts.next();
    if parts.next().is_some()
        || whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || (whole.len() > 1 && whole.starts_with('0'))
        || fraction.is_some_and(|fraction| {
            fraction.is_empty() || !fraction.bytes().all(|byte| byte.is_ascii_digit())
        })
    {
        return Err(invalid());
    }
    let fraction = fraction.unwrap_or("");
    let significant = format!("{whole}{fraction}")
        .trim_start_matches('0')
        .len();
    let trimmed_fraction = fraction.trim_end_matches('0');
    if significant == 0 {
        return Ok((negative, "0".to_string(), String::new()));
    }
    Ok((
        negative,
        whole.to_string(),
        if trimmed_fraction.is_empty() {
            String::new()
        } else {
            fraction.to_string()
        },
    ))
}

fn float(value: &str) -> Result<f64> {
    let parsed = value.parse::<f64>().map_err(|_| {
        WriteError::new(
            "invalid_number",
            "Floating-point values must be plain decimal numbers, for example 12.5 or 1.2e3.",
        )
    })?;
    if !parsed.is_finite() || value.len() > 64 || value.trim() != value || value.is_empty() {
        return Err(WriteError::new(
            "invalid_number",
            "Floating-point values must be finite plain decimal numbers.",
        ));
    }
    Ok(parsed)
}

fn text_shape_ok(format: TextFormat, value: &str) -> bool {
    let bytes = value.as_bytes();
    let digits = |slice: &[u8]| slice.iter().all(|byte| byte.is_ascii_digit());
    match format {
        TextFormat::Plain => true,
        TextFormat::Json | TextFormat::Jsonb => {
            serde_json::from_str::<serde_json::Value>(value).is_ok()
        }
        TextFormat::Uuid => {
            bytes.len() == 36
                && bytes.iter().enumerate().all(|(index, byte)| {
                    if matches!(index, 8 | 13 | 18 | 23) {
                        *byte == b'-'
                    } else {
                        byte.is_ascii_hexdigit()
                    }
                })
        }
        TextFormat::Date => {
            bytes.len() == 10
                && bytes[4] == b'-'
                && bytes[7] == b'-'
                && digits(&bytes[0..4])
                && digits(&bytes[5..7])
                && digits(&bytes[8..10])
        }
        TextFormat::Time => {
            (8..=15).contains(&bytes.len())
                && bytes[2] == b':'
                && bytes[5] == b':'
                && digits(&bytes[0..2])
                && digits(&bytes[3..5])
                && digits(&bytes[6..8])
                && (bytes.len() == 8
                    || (bytes[8] == b'.' && digits(&bytes[9..]) && bytes.len() > 9))
        }
        TextFormat::Timestamp | TextFormat::TimestampTz => {
            let separator = bytes
                .get(10)
                .is_some_and(|byte| *byte == b'T' || *byte == b' ');
            if bytes.len() < 19 || !separator {
                return false;
            }
            let date = &bytes[0..10];
            if date[4] != b'-'
                || date[7] != b'-'
                || !digits(&date[0..4])
                || !digits(&date[5..7])
                || !digits(&date[8..10])
            {
                return false;
            }
            let rest = &value[11..];
            let (clock, suffix) = match rest.find(['Z', '+', '-']) {
                Some(index) if index > 0 => (&rest[..index], Some(&rest[index..])),
                Some(_) => return false,
                None => (rest, None),
            };
            if !text_shape_ok(TextFormat::Time, clock) {
                return false;
            }
            match suffix {
                None => true,
                Some(suffix) if suffix == "Z" => format == TextFormat::TimestampTz,
                Some(suffix) => {
                    format == TextFormat::TimestampTz
                        && matches!(suffix.len(), 3 | 6)
                        && digits(suffix[1..3].as_bytes())
                        && (suffix.len() == 3
                            || (suffix.as_bytes()[3] == b':'
                                && digits(suffix[4..6].as_bytes())))
                }
            }
        }
    }
}

fn validate_value(column: &Column, value: &WriteValue) -> Result<()> {
    if matches!(value, WriteValue::Null) {
        if !column.nullable || column.primary_key {
            return Err(WriteError {
                code: "invalid_value".to_string(),
                message: format!("Column `{}` does not accept null.", column.name),
                outcome_unknown: false,
            });
        }
        return Ok(());
    }
    let valid = match (&column.kind, value) {
        (ColumnKind::Boolean, WriteValue::Boolean(_)) => true,
        (ColumnKind::Integer { min, max }, WriteValue::Number(value)) => {
            integer(value).is_ok_and(|parsed| (*min..=*max).contains(&parsed))
        }
        (ColumnKind::Decimal { precision, scale }, WriteValue::Number(value)) => {
            match decimal(value) {
                Ok((_, whole, fraction)) => {
                    let significant = format!("{whole}{fraction}")
                        .trim_start_matches('0')
                        .len()
                        .max(usize::from(whole != "0"));
                    significant <= *precision as usize
                        && fraction.len() <= *scale as usize
                }
                Err(_) => false,
            }
        }
        (ColumnKind::Float, WriteValue::Number(value)) => float(value).is_ok(),
        (ColumnKind::Text { max_bytes, format }, WriteValue::Text(value)) => {
            value.len() <= *max_bytes
                && !value.contains('\0')
                && text_shape_ok(*format, value)
        }
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(WriteError {
            code: "invalid_value".to_string(),
            message: format!(
                "The value for column `{}` is not valid for {}: {}.",
                column.name,
                column.declared,
                match column.kind {
                    ColumnKind::Text { format, .. } => format
                        .hint()
                        .unwrap_or("expects text without NUL characters"),
                    _ => "the value is null, out of range, or of the wrong type",
                }
            ),
            outcome_unknown: false,
        })
    }
}

/// Identifiers come from database metadata or from a request that has already
/// been resolved against the authorized table list. They are quoted, so the
/// only hard requirement is that they are printable and bounded.
fn validate_identifier(identifier: &str) -> Result<()> {
    if identifier.is_empty()
        || identifier.chars().count() > MAX_NAME_CHARS
        || identifier.chars().any(|character| character.is_control())
    {
        return Err(WriteError::new(
            "invalid_identifier",
            "A schema, table, or column name is empty, too long, or contains control characters.",
        ));
    }
    Ok(())
}

fn validate_request(config: &ConnectionConfig, request: &WriteRequest) -> Result<()> {
    if !config.enabled || config.id != request.connection_id {
        return Err(WriteError::new(
            "connection_unavailable",
            "The connection is disabled or does not match the write request.",
        ));
    }
    if matches!(config.kind, DbKind::Jdbc) {
        return Err(unsupported("JDBC writes are not supported."));
    }
    validate_identifier(&request.schema)?;
    validate_identifier(&request.table)?;
    if request.values.len() > MAX_COLUMNS || request.primary_key.len() > MAX_COLUMNS {
        return Err(WriteError::new(
            "request_limit",
            "A write may address at most 256 columns.",
        ));
    }
    if request.values.is_empty() && request.operation == WriteOperation::Update {
        return Err(WriteError::new(
            "invalid_request",
            "UPDATE requires at least one column value.",
        ));
    }
    if request.connection_id.len() > 128
        || request.idempotency_key.is_empty()
        || request.idempotency_key.len() > 128
        || !request
            .idempotency_key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_.:".contains(&byte))
        || request
            .reason
            .as_ref()
            .is_some_and(|reason| reason.len() > 1024 || reason.contains('\0'))
    {
        return Err(WriteError::new(
            "request_limit",
            "Connection, idempotency-key, or reason limits were exceeded.",
        ));
    }
    for (name, value) in request.values.iter().chain(request.primary_key.iter()) {
        validate_identifier(name)?;
        if let WriteValue::Text(text) = value {
            if text.len() > MAX_TEXT_BYTES || text.contains('\0') {
                return Err(WriteError::new(
                    "request_limit",
                    "Text values must not contain NUL and must not exceed 16384 bytes.",
                ));
            }
        }
    }
    let size = serde_json::to_vec(request)
        .map_err(|_| WriteError::new("invalid_request", "The write request cannot be encoded."))?
        .len();
    if size > MAX_REQUEST_BYTES {
        return Err(WriteError::new(
            "request_limit",
            "The encoded write request must not exceed 65536 bytes.",
        ));
    }
    Ok(())
}

fn quote(kind: &DbKind, name: &str) -> String {
    match kind {
        DbKind::Mysql => format!("`{}`", name.replace('`', "``")),
        _ => format!("\"{}\"", name.replace('"', "\"\"")),
    }
}

fn target(kind: &DbKind, request: &WriteRequest) -> String {
    format!(
        "{}.{}",
        quote(kind, &request.schema),
        quote(kind, &request.table)
    )
}

fn placeholder(kind: &DbKind, position: usize, column: Option<&Column>) -> String {
    match kind {
        DbKind::Postgres => {
            let cast = column
                .and_then(|column| column.kind.pg_cast())
                .unwrap_or("text");
            format!("${position}::{cast}")
        }
        _ => "?".to_string(),
    }
}

fn predicate(
    kind: &DbKind,
    primary_key: &BTreeMap<String, WriteValue>,
    offset: usize,
    metadata: &Metadata,
) -> String {
    primary_key
        .keys()
        .enumerate()
        .map(|(index, name)| {
            format!(
                "{} = {}",
                quote(kind, name),
                placeholder(kind, offset + index + 1, metadata.column(name))
            )
        })
        .collect::<Vec<_>>()
        .join(" AND ")
}

fn default_values_clause(kind: &DbKind) -> &'static str {
    match kind {
        DbKind::Mysql => "() VALUES ()",
        _ => "DEFAULT VALUES",
    }
}

fn write_sql(
    kind: &DbKind,
    request: &WriteRequest,
    resolved: &ResolvedWrite,
    metadata: &Metadata,
) -> String {
    match request.operation {
        WriteOperation::Insert if resolved.values.is_empty() => format!(
            "INSERT INTO {} {}",
            target(kind, request),
            default_values_clause(kind)
        ),
        WriteOperation::Insert => format!(
            "INSERT INTO {} ({}) VALUES ({})",
            target(kind, request),
            resolved
                .values
                .keys()
                .map(|name| quote(kind, name))
                .collect::<Vec<_>>()
                .join(", "),
            resolved
                .values
                .keys()
                .enumerate()
                .map(|(index, name)| placeholder(kind, index + 1, metadata.column(name)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        WriteOperation::Update => format!(
            "UPDATE {} SET {} WHERE {}",
            target(kind, request),
            resolved
                .values
                .keys()
                .enumerate()
                .map(|(index, name)| {
                    format!(
                        "{} = {}",
                        quote(kind, name),
                        placeholder(kind, index + 1, metadata.column(name))
                    )
                })
                .collect::<Vec<_>>()
                .join(", "),
            predicate(kind, &resolved.primary_key, resolved.values.len(), metadata)
        ),
        WriteOperation::Delete => format!(
            "DELETE FROM {} WHERE {}",
            target(kind, request),
            predicate(kind, &resolved.primary_key, 0, metadata)
        ),
    }
}

fn value_expr(kind: &DbKind, identifier: &str) -> String {
    match kind {
        DbKind::Mysql => format!("LEFT(CAST({identifier} AS CHAR), {BEFORE_IMAGE_CHARS})"),
        DbKind::Sqlite => format!("substr(CAST({identifier} AS TEXT), 1, {BEFORE_IMAGE_CHARS})"),
        _ => format!("LEFT(({identifier})::text, {BEFORE_IMAGE_CHARS})"),
    }
}

fn select_sql(
    kind: &DbKind,
    columns: &[&str],
    request: &WriteRequest,
    primary_key: &BTreeMap<String, WriteValue>,
    metadata: &Metadata,
    locked: bool,
) -> String {
    format!(
        "SELECT {} FROM {} WHERE {} LIMIT 2{}",
        columns
            .iter()
            .map(|name| value_expr(kind, &quote(kind, name)))
            .collect::<Vec<_>>()
            .join(", "),
        target(kind, request),
        predicate(kind, primary_key, 0, metadata),
        if locked && !matches!(kind, DbKind::Sqlite) {
            " FOR UPDATE"
        } else {
            ""
        }
    )
}

/// Row read back from the database; every value is the canonical text form the
/// database itself produced, so before-image comparison is stable across reads.
type ReadRow = BTreeMap<String, WriteValue>;

// Raw, exclusively owned connections are never inserted into a pool. Dropping
// this enum closes the socket/SQLite worker without claiming rollback success.
enum PhysicalConnection {
    Sqlite(SqliteConnection),
    Postgres(PgConnection),
    Mysql(MySqlConnection),
}

impl PhysicalConnection {
    /// Runs one transaction-control statement. These go through the simple
    /// query protocol: MySQL rejects `START TRANSACTION`, `BEGIN`, `COMMIT` and
    /// `ROLLBACK` inside the prepared-statement protocol (error 1295), and none
    /// of them can carry a bound parameter.
    async fn statement(&mut self, statement: &str) -> Result<()> {
        match self {
            Self::Sqlite(connection) => raw_statement(connection, statement).await,
            Self::Postgres(connection) => raw_statement(connection, statement).await,
            Self::Mysql(connection) => raw_statement(connection, statement).await,
        }
    }

    async fn begin(&mut self, read_only: bool) -> Result<()> {
        match self {
            Self::Sqlite(connection) => sqlite::begin(connection, read_only).await,
            Self::Postgres(connection) => postgres::begin(connection, read_only).await,
            Self::Mysql(connection) => mysql::begin(connection, read_only).await,
        }
    }

    async fn metadata(&mut self, request: &WriteRequest) -> Result<Metadata> {
        match self {
            Self::Sqlite(connection) => sqlite::metadata(connection, request).await,
            Self::Postgres(connection) => postgres::metadata(connection, request).await,
            Self::Mysql(connection) => mysql::metadata(connection, request).await,
        }
    }

    async fn rows(
        &mut self,
        request: &WriteRequest,
        primary_key: &BTreeMap<String, WriteValue>,
        metadata: &Metadata,
        columns: &[&str],
        locked: bool,
    ) -> Result<Vec<ReadRow>> {
        match self {
            Self::Sqlite(connection) => {
                sqlite::rows(connection, request, primary_key, metadata, columns, locked).await
            }
            Self::Postgres(connection) => {
                postgres::rows(connection, request, primary_key, metadata, columns, locked).await
            }
            Self::Mysql(connection) => {
                mysql::rows(connection, request, primary_key, metadata, columns, locked).await
            }
        }
    }

    async fn dml(&mut self, prepared: &PreparedWrite) -> Result<u64> {
        match self {
            Self::Sqlite(connection) => sqlite::dml(connection, prepared).await,
            Self::Postgres(connection) => postgres::dml(connection, prepared).await,
            Self::Mysql(connection) => mysql::dml(connection, prepared).await,
        }
    }
}

/// Runs one parameterless statement such as `START TRANSACTION`, `COMMIT` or
/// `ROLLBACK` through the simple query protocol.
///
/// MySQL rejects transaction control inside the prepared-statement protocol
/// (error 1295, "This command is not supported in the prepared statement
/// protocol yet"), which made every write fail before it started. `execute` is
/// called through the `Executor` trait rather than `RawSql::execute` because
/// only the trait method's `Send` boxed future survives the higher-ranked
/// lifetime inference a spawned task needs.
async fn raw_statement<C>(connection: &mut C, statement: &str) -> Result<()>
where
    C: Connection,
    for<'c> &'c mut C: Executor<'c, Database = C::Database>,
{
    Executor::execute(&mut *connection, sqlx::raw_sql(statement))
        .await
        .map_err(database_error)?;
    Ok(())
}

async fn connect(
    config: &ConnectionConfig,
    vault: &CredentialVault,
    read_only: bool,
) -> Result<PhysicalConnection> {
    let options_error = |_| {
        WriteError::new(
            "connection_options",
            "Connection settings or stored credentials are unavailable.",
        )
    };
    match config.kind {
        DbKind::Sqlite => {
            // A write transaction needs a real file and normal locking. URI
            // modes can bypass file locks or create private memory databases.
            if config.database.contains('?') || config.database.contains(":memory:") {
                return Err(unsupported(
                    "SQLite writes require a file-backed connection without URI query options.",
                ));
            }
            let options = SqliteConnectOptions::from_str(&config.database)
                .map_err(database_error)?
                .read_only(read_only)
                .create_if_missing(false)
                .busy_timeout(deadline(config))
                .statement_cache_capacity(0)
                .pragma("trusted_schema", "OFF")
                .pragma("foreign_keys", "ON")
                .disable_statement_logging();
            if options
                .get_filename()
                .to_str()
                .is_none_or(|name| name.starts_with("file:") || name.is_empty())
            {
                return Err(unsupported(
                    "SQLite writes require a regular file path and the built-in filesystem driver.",
                ));
            }
            Ok(PhysicalConnection::Sqlite(
                SqliteConnection::connect_with(&options)
                    .await
                    .map_err(database_error)?,
            ))
        }
        DbKind::Postgres => {
            let password = resolve_password(config, vault, None).map_err(options_error)?;
            let host = config
                .host
                .as_deref()
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    WriteError::new("connection_options", "A database host is required.")
                })?;
            let options = PgConnectOptions::new()
                .host(host)
                .port(
                    config
                        .port
                        .or_else(|| default_port(&config.kind))
                        .unwrap_or(5432),
                )
                .database(&config.database)
                .username(config.username.as_deref().unwrap_or(""))
                .password(&password)
                .ssl_mode(pg_ssl_mode(config).map_err(options_error)?)
                .application_name("DataNexa")
                .statement_cache_capacity(0)
                .disable_statement_logging();
            let mut connection = PgConnection::connect_with(&options)
                .await
                .map_err(database_error)?;
            for setting in ["statement_timeout", "lock_timeout"] {
                sqlx::query(&format!("SET {setting} = {}", deadline(config).as_millis()))
                    .execute(&mut connection)
                    .await
                    .map_err(database_error)?;
            }
            Ok(PhysicalConnection::Postgres(connection))
        }
        DbKind::Mysql => {
            let password = resolve_password(config, vault, None).map_err(options_error)?;
            let host = config
                .host
                .as_deref()
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    WriteError::new("connection_options", "A database host is required.")
                })?;
            let options = MySqlConnectOptions::new()
                .host(host)
                .port(
                    config
                        .port
                        .or_else(|| default_port(&config.kind))
                        .unwrap_or(3306),
                )
                .database(&config.database)
                .username(config.username.as_deref().unwrap_or(""))
                .password(&password)
                .ssl_mode(mysql_ssl_mode(config).map_err(options_error)?)
                .charset("utf8mb4")
                .statement_cache_capacity(0)
                .disable_statement_logging();
            let mut connection = MySqlConnection::connect_with(&options)
                .await
                .map_err(database_error)?;
            for statement in [
                "SET SESSION sql_mode = 'STRICT_ALL_TABLES,NO_ENGINE_SUBSTITUTION'".to_string(),
                format!(
                    "SET SESSION lock_wait_timeout = {}",
                    deadline(config).as_secs().max(1)
                ),
                format!(
                    "SET SESSION innodb_lock_wait_timeout = {}",
                    deadline(config).as_secs().max(1)
                ),
            ] {
                sqlx::query(&statement)
                    .execute(&mut connection)
                    .await
                    .map_err(database_error)?;
            }
            Ok(PhysicalConnection::Mysql(connection))
        }
        DbKind::Jdbc => Err(unsupported("JDBC writes are not supported.")),
    }
}

/// Read-only metadata session over one physical connection. Capability is
/// answered from metadata alone: no transaction, no table lock, and no write
/// privilege is needed, and several tables share one connection.
pub struct MetadataSession {
    connection: PhysicalConnection,
    limit: Duration,
}

impl MetadataSession {
    pub async fn open(config: &ConnectionConfig, vault: &CredentialVault) -> Result<Self> {
        let limit = deadline(config);
        let connection = timeout(limit, connect(config, vault, true))
            .await
            .map_err(|_| {
                WriteError::new(
                    "metadata_timeout",
                    "Opening the metadata connection timed out.",
                )
            })??;
        Ok(Self { connection, limit })
    }

    pub async fn capability(&mut self, schema: &str, table: &str) -> Result<TableCapability> {
        validate_identifier(schema)?;
        validate_identifier(table)?;
        let request = WriteRequest {
            connection_id: String::new(),
            operation: WriteOperation::Update,
            target_id: None,
            schema: schema.to_string(),
            table: table.to_string(),
            values: BTreeMap::new(),
            primary_key: BTreeMap::new(),
            idempotency_key: "capability-inspection".to_string(),
            reason: None,
        };
        let limit = self.limit;
        timeout(limit, self.connection.metadata(&request))
            .await
            .map_err(|_| {
                WriteError::new(
                    "metadata_timeout",
                    "Inspecting table capabilities timed out.",
                )
            })
            .and_then(|result| result)
            .map(|metadata| metadata.capability())
    }

    /// Capabilities for many tables of one schema. Each dialect reads the
    /// catalogue with a handful of set-based queries instead of a full
    /// per-table inspection, so listing a large schema is bounded by a few
    /// round trips rather than a few thousand. Order matches `tables`.
    pub async fn capabilities(
        &mut self,
        schema: &str,
        tables: &[String],
    ) -> Result<Vec<TableCapability>> {
        validate_identifier(schema)?;
        for table in tables {
            validate_identifier(table)?;
        }
        if tables.is_empty() {
            return Ok(Vec::new());
        }
        let limit = self.limit;
        timeout(limit, async {
            match &mut self.connection {
                PhysicalConnection::Sqlite(connection) => {
                    sqlite::capabilities(connection, schema, tables).await
                }
                PhysicalConnection::Postgres(connection) => {
                    postgres::capabilities(connection, schema, tables).await
                }
                PhysicalConnection::Mysql(connection) => {
                    mysql::capabilities(connection, schema, tables).await
                }
            }
        })
        .await
        .map_err(|_| {
            WriteError::new(
                "metadata_timeout",
                "Inspecting table capabilities timed out.",
            )
        })?
    }
}

/// `?,?,?` for a set-based query. Every dialect here accepts positional
/// placeholders, so one helper covers MySQL and SQLite.
fn placeholders(count: usize) -> String {
    std::iter::repeat("?").take(count).collect::<Vec<_>>().join(",")
}

/// `$start,$start+1,...`, for PostgreSQL numbered parameters.
fn numbered_placeholders(start: usize, count: usize) -> String {
    (start..start + count)
        .map(|index| format!("${index}"))
        .collect::<Vec<_>>()
        .join(",")
}

/// Capability of one table, derived from metadata only.
pub async fn inspect_table(
    config: &ConnectionConfig,
    vault: &CredentialVault,
    schema: &str,
    table: &str,
) -> Result<TableCapability> {
    let mut session = MetadataSession::open(config, vault).await?;
    session.capability(schema, table).await
}

pub async fn prepare(
    config: &ConnectionConfig,
    vault: &CredentialVault,
    request: WriteRequest,
) -> Result<PreparedWrite> {
    validate_request(config, &request)?;
    // The approval window must explain which columns the database will fill in.
    let operation = request.operation;
    let limit = deadline(config);
    let mut connection = timeout(limit, connect(config, vault, true))
        .await
        .map_err(|_| WriteError::new("timeout", "The read-only preview connection timed out."))??;
    let result = timeout(limit, async {
        connection.begin(true).await?;
        let metadata = connection.metadata(&request).await?;
        let resolved = metadata.plan(&request)?;
        let preview = match operation {
            WriteOperation::Insert => {
                let columns = request.values.keys().map(String::as_str).collect::<Vec<_>>();
                let existing = if resolved.primary_key.is_empty() {
                    Vec::new()
                } else {
                    connection
                        .rows(
                            &request,
                            &resolved.primary_key,
                            &metadata,
                            &columns,
                            false,
                        )
                        .await?
                };
                if !existing.is_empty() {
                    return Err(WriteError::new(
                        "row_exists",
                        "The supplied primary key already exists.",
                    ));
                }
                WritePreview {
                    sql: write_sql(&config.kind, &request, &resolved, &metadata),
                    changes: request
                        .values
                        .iter()
                        .map(|(column, after)| WriteChange {
                            column: column.clone(),
                            before: None,
                            after: after.clone(),
                        })
                        .collect(),
                    primary_key: resolved.primary_key.clone(),
                    notes: metadata.notes(&request, &resolved.values),
                    warnings: metadata.warnings.clone(),
                }
            }
            WriteOperation::Update => {
                let columns = request.values.keys().map(String::as_str).collect::<Vec<_>>();
                let rows = connection
                    .rows(&request, &resolved.primary_key, &metadata, &columns, false)
                    .await?;
                let [row] = rows.as_slice() else {
                    return Err(WriteError::new(
                        "row_count",
                        "The complete primary key must identify exactly one existing row.",
                    ));
                };
                WritePreview {
                    sql: write_sql(&config.kind, &request, &resolved, &metadata),
                    changes: request
                        .values
                        .iter()
                        .map(|(column, after)| WriteChange {
                            column: column.clone(),
                            before: row.get(column).cloned(),
                            after: after.clone(),
                        })
                        .collect(),
                    primary_key: resolved.primary_key.clone(),
                    notes: Vec::new(),
                    warnings: metadata.warnings.clone(),
                }
            }
            WriteOperation::Delete => {
                let keys = resolved.primary_key.keys().map(String::as_str).collect::<Vec<_>>();
                let rows = connection
                    .rows(&request, &resolved.primary_key, &metadata, &keys, false)
                    .await?;
                if rows.len() != 1 {
                    return Err(WriteError::new(
                        "row_count",
                        "The complete primary key must identify exactly one existing row.",
                    ));
                }
                WritePreview {
                    sql: write_sql(&config.kind, &request, &resolved, &metadata),
                    changes: resolved
                        .primary_key
                        .iter()
                        .map(|(column, value)| WriteChange {
                            column: column.clone(),
                            before: Some(value.clone()),
                            after: WriteValue::Null,
                        })
                        .collect(),
                    primary_key: resolved.primary_key.clone(),
                    notes: Vec::new(),
                    warnings: metadata.warnings.clone(),
                }
            }
        };
        if serde_json::to_vec(&preview)
            .map_err(|_| {
                WriteError::new("invalid_preview", "The write preview cannot be encoded.")
            })?
            .len()
            > MAX_REQUEST_BYTES
        {
            return Err(WriteError::new(
                "preview_limit",
                "The encoded write preview must not exceed 65536 bytes.",
            ));
        }
        Ok(PreparedWrite {
            original_request: request.clone(),
            original_preview: preview.clone(),
            request,
            preview,
            metadata,
            resolved,
            connection: ConnectionIdentity::from(config),
        })
    })
    .await
    .map_err(|_| WriteError::new("timeout", "The read-only preview timed out."))
    .and_then(|result| result);
    // A preview is always read-only, so a failed cleanup cannot have committed
    // anything; only a successful preview needs a clean rollback.
    let cleanup = timeout(limit, connection.statement("ROLLBACK")).await;
    drop(connection);
    if result.is_ok() && !matches!(cleanup, Ok(Ok(()))) {
        return Err(WriteError::new(
            "preview_cleanup",
            "The read-only preview transaction could not be closed cleanly.",
        ));
    }
    result
}

pub async fn execute(
    config: &ConnectionConfig,
    vault: &CredentialVault,
    prepared: &PreparedWrite,
    cancellation: CancellationToken,
) -> Result<PendingWrite> {
    validate_request(config, &prepared.request)?;
    if prepared.original_request != prepared.request
        || prepared.original_preview != prepared.preview
        || prepared.connection != ConnectionIdentity::from(config)
    {
        return Err(stale());
    }
    let limit = deadline(config);
    let mut connection = tokio::select! {
        biased;
        _ = cancellation.cancelled() => return Err(WriteError::new("cancelled", "The write was cancelled before opening a transaction.")),
        result = timeout(limit, connect(config, vault, false)) => result
            .map_err(|_| WriteError::new("timeout", "The write connection timed out before opening a transaction."))??,
    };
    let result = tokio::select! {
        biased;
        _ = cancellation.cancelled() => Err(WriteError::new("cancelled", "The write was cancelled and rollback was confirmed.")),
        result = timeout(limit, async {
            connection.begin(false).await?;
            let metadata = connection.metadata(&prepared.request).await?;
            if metadata.fingerprint != prepared.metadata.fingerprint
                || metadata.plan(&prepared.request)?.primary_key != prepared.preview.primary_key
            {
                return Err(stale());
            }
            match prepared.request.operation {
                WriteOperation::Insert => {
                    // A duplicate key is rejected by the database itself; the
                    // single-row INSERT below can only add one row.
                }
                WriteOperation::Update => {
                    let columns = prepared.request.values.keys().map(String::as_str).collect::<Vec<_>>();
                    let rows = connection.rows(&prepared.request, &prepared.preview.primary_key, &metadata, &columns, true).await?;
                    let [row] = rows.as_slice() else { return Err(stale()) };
                    if prepared.preview.changes.iter().any(|change| {
                        row.get(&change.column) != change.before.as_ref()
                    }) {
                        return Err(stale());
                    }
                }
                WriteOperation::Delete => {
                    let keys = prepared.preview.primary_key.keys().map(String::as_str).collect::<Vec<_>>();
                    let rows = connection.rows(&prepared.request, &prepared.preview.primary_key, &metadata, &keys, true).await?;
                    if rows.len() != 1 {
                        return Err(stale());
                    }
                }
            }
            let affected = connection.dml(prepared).await?;
            // A full primary key can only ever match one row. MySQL reports
            // zero affected rows for an UPDATE that stores identical values,
            // which is a success, not a failure.
            let accepted = match prepared.request.operation {
                WriteOperation::Update => affected <= 1,
                _ => affected == 1,
            };
            if !accepted {
                return Err(WriteError::new("row_count", "The write did not affect exactly one row."));
            }
            Ok(())
        }) => result.map_err(|_| WriteError::new("timeout", "The write timed out and rollback was confirmed.")).and_then(|result| result),
    };
    match result {
        Ok(()) if !cancellation.is_cancelled() => Ok(PendingWrite { connection, limit }),
        other => {
            let error = other.err().unwrap_or_else(|| {
                WriteError::new(
                    "cancelled",
                    "The write was cancelled and rollback was confirmed.",
                )
            });
            let rollback = timeout(limit, connection.statement("ROLLBACK")).await;
            drop(connection);
            if matches!(rollback, Ok(Ok(()))) {
                Err(error)
            } else {
                Err(WriteError::unknown(
                    "Rollback could not be confirmed. Do not retry automatically.",
                ))
            }
        }
    }
}

#[must_use = "A pending write must be explicitly committed or rolled back."]
pub struct PendingWrite {
    connection: PhysicalConnection,
    limit: Duration,
}

impl PendingWrite {
    /// The manager must durably record `committing` and recheck authorization
    /// before this call. Dropping this future never proves failure or rollback.
    pub async fn commit(mut self) -> Result<u64> {
        let result = timeout(self.limit, self.connection.statement("COMMIT")).await;
        // COMMIT was acknowledged: subsequent close failures cannot turn that
        // known committed result into a false failure.
        drop(self.connection);
        if matches!(result, Ok(Ok(()))) {
            Ok(1)
        } else {
            Err(WriteError::unknown(
                "Commit could not be confirmed. Do not retry; reconcile the transaction outcome.",
            ))
        }
    }

    pub async fn rollback(mut self) -> Result<()> {
        let result = timeout(self.limit, self.connection.statement("ROLLBACK")).await;
        drop(self.connection);
        if matches!(result, Ok(Ok(()))) {
            Ok(())
        } else {
            Err(WriteError::unknown(
                "Rollback could not be confirmed. Do not retry automatically.",
            ))
        }
    }
}

fn fingerprint(parts: &[String]) -> String {
    use sha2::{Digest, Sha256};
    let mut digest = Sha256::new();
    for part in parts {
        digest.update((part.len() as u64).to_be_bytes());
        digest.update(part.as_bytes());
    }
    format!("{:x}", digest.finalize())
}

fn column_signature(columns: &[Column]) -> String {
    fingerprint(
        &columns
            .iter()
            .map(|column| {
                format!(
                    "{}|{}|{}|{}|{}|{}|{}|{}",
                    column.name,
                    column.declared,
                    column.nullable,
                    column.primary_key,
                    column.generated,
                    column.auto_increment,
                    column.has_default,
                    column.on_update
                )
            })
            .collect::<Vec<_>>(),
    )
}
