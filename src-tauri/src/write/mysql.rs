use super::*;
use crate::db::{
    mysql_metadata_optional_text as metadata_optional_text, mysql_metadata_text as metadata_text,
};
use sqlx::mysql::{MySqlArguments, MySqlRow};
use sqlx::query::Query;
use sqlx::{MySql, Row};

pub(super) async fn begin(connection: &mut MySqlConnection, read_only: bool) -> Result<()> {
    // READ COMMITTED makes `SELECT ... FOR UPDATE` a pure row lock: no gap
    // locks, no table lock, and no autocommit juggling.
    sqlx::query("SET SESSION TRANSACTION ISOLATION LEVEL READ COMMITTED")
        .execute(&mut *connection)
        .await
        .map_err(database_error)?;
    // Transaction control must use the simple query protocol: MySQL rejects
    // `START TRANSACTION` inside the prepared-statement protocol (error 1295).
    raw_statement(
        connection,
        if read_only {
            "START TRANSACTION READ ONLY"
        } else {
            "START TRANSACTION"
        },
    )
    .await?;
    Ok(())
}

fn text(max_bytes: usize, format: TextFormat) -> ColumnKind {
    ColumnKind::Text { max_bytes, format }
}

/// Signed or unsigned range of a MySQL integer type of `bits` bits.
fn integer_kind(bits: u32, unsigned: bool) -> ColumnKind {
    if unsigned {
        ColumnKind::Integer {
            min: 0,
            max: (1i128 << bits) - 1,
        }
    } else {
        ColumnKind::Integer {
            min: -(1i128 << (bits - 1)),
            max: (1i128 << (bits - 1)) - 1,
        }
    }
}

fn decimal_kind(precision: Option<i64>, scale: Option<i64>) -> ColumnKind {
    ColumnKind::Decimal {
        // An unconstrained decimal is reported with nulls; keep it permissive
        // rather than rejecting a value the server would accept.
        precision: precision
            .filter(|precision| *precision > 0)
            .map_or(1000, |precision| precision as u32),
        scale: scale
            .filter(|scale| *scale >= 0)
            .map_or(1000, |scale| scale as u32),
    }
}

/// MySQL reports the storage type through `DATA_TYPE` and the full declaration
/// through `COLUMN_TYPE`; only the latter carries the unsigned flag and the
/// `tinyint(1)` boolean alias.
fn column_kind(
    data_type: &str,
    column_type: &str,
    character_length: Option<i64>,
    precision: Option<i64>,
    scale: Option<i64>,
) -> ColumnKind {
    let unsigned = column_type.contains("unsigned") || column_type.contains("zerofill");
    match data_type {
        "tinyint" if column_type.starts_with("tinyint(1)") => ColumnKind::Boolean,
        "tinyint" => integer_kind(8, unsigned),
        "smallint" => integer_kind(16, unsigned),
        "mediumint" => integer_kind(24, unsigned),
        "int" => integer_kind(32, unsigned),
        "bigint" => integer_kind(64, unsigned),
        "year" => ColumnKind::Integer {
            min: 1901,
            max: 2155,
        },
        "decimal" | "numeric" => decimal_kind(precision, scale),
        "float" | "double" | "real" => ColumnKind::Float,
        // utf8mb4 stores at most four bytes per character.
        "char" | "varchar" => text(
            character_length.map_or(MAX_TEXT_BYTES, |length| {
                (length.max(1) as usize).saturating_mul(4)
            }),
            TextFormat::Plain,
        ),
        "tinytext" | "text" | "mediumtext" | "longtext" => text(MAX_TEXT_BYTES, TextFormat::Plain),
        "date" => text(MAX_TEXT_BYTES, TextFormat::Date),
        "time" => text(MAX_TEXT_BYTES, TextFormat::Time),
        "datetime" | "timestamp" => text(MAX_TEXT_BYTES, TextFormat::Timestamp),
        "json" => text(MAX_TEXT_BYTES, TextFormat::Json),
        "enum" | "set" => text(255, TextFormat::Plain),
        _ => ColumnKind::Unsupported,
    }
}

pub(super) async fn metadata(
    connection: &mut MySqlConnection,
    request: &WriteRequest,
) -> Result<Metadata> {
    // The caller resolved the table against `lower_case_table_names`, so the
    // comparison itself must be binary: MySQL's identifier case rules must not
    // describe a different table than the one the request authorized.
    let tables = sqlx::query(
        "SELECT TABLE_TYPE, ENGINE FROM information_schema.TABLES \
         WHERE BINARY TABLE_SCHEMA = BINARY ? AND BINARY TABLE_NAME = BINARY ? LIMIT 2",
    )
    .bind(&request.schema)
    .bind(&request.table)
    .fetch_all(&mut *connection)
    .await
    .map_err(database_error)?;
    let [table] = tables.as_slice() else {
        return Err(unsupported_reason(
            "target_not_found",
            "The requested name is not exactly one visible MySQL table.",
        ));
    };
    if metadata_text(table, "TABLE_TYPE").map_err(database_error)? != "BASE TABLE" {
        return Err(unsupported_reason(
            "not_a_base_table",
            "Only a MySQL base table can be written; views and temporary objects are not.",
        ));
    }
    if metadata_text(table, "ENGINE").map_err(database_error)? != "InnoDB" {
        return Err(unsupported_reason(
            "unsupported_engine",
            "A single-row write needs transaction semantics, which only the InnoDB storage engine provides.",
        ));
    }
    let primary_key: i64 = sqlx::query_scalar(
        "SELECT CAST(COUNT(*) AS SIGNED) FROM information_schema.STATISTICS \
         WHERE BINARY TABLE_SCHEMA = BINARY ? AND BINARY TABLE_NAME = BINARY ? \
         AND INDEX_NAME = 'PRIMARY'",
    )
    .bind(&request.schema)
    .bind(&request.table)
    .fetch_one(&mut *connection)
    .await
    .map_err(database_error)?;
    if primary_key == 0 {
        return Err(unsupported_reason(
            "no_primary_key",
            "This table has no primary key, so a single row cannot be identified for writing.",
        ));
    }
    let raw_columns = sqlx::query(
        "SELECT COLUMN_NAME, DATA_TYPE, COLUMN_TYPE, IS_NULLABLE, COLUMN_DEFAULT, EXTRA, \
         GENERATION_EXPRESSION, COLUMN_KEY, \
         CAST(CHARACTER_MAXIMUM_LENGTH AS SIGNED) AS character_length, \
         CAST(NUMERIC_PRECISION AS SIGNED) AS numeric_precision, \
         CAST(NUMERIC_SCALE AS SIGNED) AS numeric_scale \
         FROM information_schema.COLUMNS \
         WHERE BINARY TABLE_SCHEMA = BINARY ? AND BINARY TABLE_NAME = BINARY ? \
         ORDER BY ORDINAL_POSITION LIMIT 257",
    )
    .bind(&request.schema)
    .bind(&request.table)
    .fetch_all(&mut *connection)
    .await
    .map_err(database_error)?;
    if raw_columns.is_empty() || raw_columns.len() > MAX_COLUMNS {
        return Err(unsupported_reason(
            "unsupported_table_shape",
            "This table has no columns or more columns than one write can address.",
        ));
    }
    let mut columns = Vec::with_capacity(raw_columns.len());
    for raw in &raw_columns {
        columns.push(column_of(raw)?);
    }
    let mut warnings = Vec::new();
    let triggers: i64 = sqlx::query_scalar(
        "SELECT CAST(COUNT(*) AS SIGNED) FROM information_schema.TRIGGERS \
         WHERE BINARY EVENT_OBJECT_SCHEMA = BINARY ? AND BINARY EVENT_OBJECT_TABLE = BINARY ?",
    )
    .bind(&request.schema)
    .bind(&request.table)
    .fetch_one(&mut *connection)
    .await
    .map_err(database_error)?;
    if triggers > 0 {
        warnings.push("triggers_present".to_string());
    }
    let constraints = sqlx::query(
        "SELECT CONSTRAINT_TYPE FROM information_schema.TABLE_CONSTRAINTS \
         WHERE BINARY TABLE_SCHEMA = BINARY ? AND BINARY TABLE_NAME = BINARY ? \
         GROUP BY CONSTRAINT_TYPE LIMIT 64",
    )
    .bind(&request.schema)
    .bind(&request.table)
    .fetch_all(&mut *connection)
    .await
    .map_err(database_error)?;
    let mut present = Vec::new();
    for constraint in &constraints {
        present.push(metadata_text(constraint, "CONSTRAINT_TYPE").map_err(database_error)?);
    }
    for (kind, code) in [
        ("FOREIGN KEY", "foreign_keys_present"),
        ("CHECK", "check_constraints_present"),
        ("UNIQUE", "unique_constraints_present"),
    ] {
        if present.iter().any(|present| present == kind) {
            warnings.push(code.to_string());
        }
    }
    push_text_primary_key_warning(&mut warnings, &columns);
    Ok(Metadata {
        fingerprint: column_signature(&columns),
        columns,
        warnings,
    })
}

/// One `information_schema.COLUMNS` row as the write planner models it.
fn column_of(raw: &MySqlRow) -> Result<Column> {
    let name = metadata_text(raw, "COLUMN_NAME").map_err(database_error)?;
    validate_identifier(&name)?;
    let declared = metadata_text(raw, "COLUMN_TYPE").map_err(database_error)?;
    let data_type = metadata_text(raw, "DATA_TYPE")
        .map_err(database_error)?
        .to_ascii_lowercase();
    // `EXTRA` is the only place MySQL reports automatic maintenance and
    // generated-column storage. `DEFAULT_GENERATED` there only means "this
    // column has a default", not "this column is generated".
    let extra = metadata_text(raw, "EXTRA")
        .map_err(database_error)?
        .to_ascii_lowercase();
    let generation = metadata_optional_text(raw, "GENERATION_EXPRESSION")
        .map_err(database_error)?
        .unwrap_or_default();
    let primary_key = metadata_text(raw, "COLUMN_KEY").map_err(database_error)? == "PRI";
    Ok(Column {
        name,
        declared: declared.clone(),
        kind: column_kind(
            &data_type,
            &declared.to_ascii_lowercase(),
            number(raw, "character_length")?,
            number(raw, "numeric_precision")?,
            number(raw, "numeric_scale")?,
        ),
        nullable: metadata_text(raw, "IS_NULLABLE").map_err(database_error)? == "YES"
            && !primary_key,
        primary_key,
        generated: !generation.is_empty()
            || extra.contains("stored generated")
            || extra.contains("virtual generated"),
        auto_increment: extra.contains("auto_increment"),
        has_default: metadata_optional_text(raw, "COLUMN_DEFAULT")
            .map_err(database_error)?
            .is_some()
            || extra.contains("default_generated"),
        on_update: extra.contains("on update"),
    })
}

/// Informational only: a text primary key is legal, but its collation may
/// compare two different strings as equal.
fn push_text_primary_key_warning(warnings: &mut Vec<String>, columns: &[Column]) {
    if columns
        .iter()
        .any(|column| column.primary_key && matches!(column.kind, ColumnKind::Text { .. }))
    {
        warnings.push("text_primary_key".to_string());
    }
}

/// Batch capability for a whole schema. Three set-based queries replace the
/// per-table reads `metadata` performs, which is what makes a large schema
/// usable: the cost is a constant number of round trips, not six per table.
pub(super) async fn capabilities(
    connection: &mut MySqlConnection,
    schema: &str,
    tables: &[String],
) -> Result<Vec<TableCapability>> {
    let total = tables.len();
    let catalog_sql = format!(
        "SELECT TABLE_NAME, TABLE_TYPE, ENGINE FROM information_schema.TABLES \
         WHERE BINARY TABLE_SCHEMA = BINARY ? AND BINARY TABLE_NAME IN ({})",
        placeholders(total)
    );
    let mut catalog = sqlx::query(&catalog_sql).bind(schema);
    for table in tables {
        catalog = catalog.bind(table);
    }
    let catalog = catalog.fetch_all(&mut *connection).await.map_err(database_error)?;
    let mut table_type = std::collections::HashMap::new();
    let mut engine = std::collections::HashMap::new();
    for row in &catalog {
        let name = metadata_text(row, "TABLE_NAME").map_err(database_error)?;
        table_type.insert(
            name.clone(),
            metadata_text(row, "TABLE_TYPE").map_err(database_error)?,
        );
        engine.insert(
            name,
            metadata_optional_text(row, "ENGINE").map_err(database_error)?,
        );
    }

    let key_sql = format!(
        "SELECT DISTINCT TABLE_NAME FROM information_schema.STATISTICS \
         WHERE BINARY TABLE_SCHEMA = BINARY ? AND BINARY TABLE_NAME IN ({}) \
         AND INDEX_NAME = 'PRIMARY'",
        placeholders(total)
    );
    let mut key_query = sqlx::query(&key_sql).bind(schema);
    for table in tables {
        key_query = key_query.bind(table);
    }
    let keyed = key_query.fetch_all(&mut *connection).await.map_err(database_error)?;
    let mut has_primary_key = std::collections::HashSet::new();
    for row in &keyed {
        has_primary_key.insert(metadata_text(row, "TABLE_NAME").map_err(database_error)?);
    }

    let column_sql = format!(
        "SELECT TABLE_NAME, COLUMN_NAME, DATA_TYPE, COLUMN_TYPE, IS_NULLABLE, COLUMN_DEFAULT, \
         EXTRA, GENERATION_EXPRESSION, COLUMN_KEY, \
         CAST(CHARACTER_MAXIMUM_LENGTH AS SIGNED) AS character_length, \
         CAST(NUMERIC_PRECISION AS SIGNED) AS numeric_precision, \
         CAST(NUMERIC_SCALE AS SIGNED) AS numeric_scale \
         FROM information_schema.COLUMNS \
         WHERE BINARY TABLE_SCHEMA = BINARY ? AND BINARY TABLE_NAME IN ({}) \
         ORDER BY TABLE_NAME, ORDINAL_POSITION",
        placeholders(total)
    );
    let mut columns_query = sqlx::query(&column_sql).bind(schema);
    for table in tables {
        columns_query = columns_query.bind(table);
    }
    let raw_columns = columns_query.fetch_all(&mut *connection).await.map_err(database_error)?;
    let mut columns: std::collections::HashMap<String, Vec<Column>> = std::collections::HashMap::new();
    // Track which tables exceeded the column bound: a truncated column list
    // would otherwise look like a valid narrow table.
    let mut over_wide: std::collections::HashSet<String> = std::collections::HashSet::new();
    for raw in &raw_columns {
        let name = metadata_text(raw, "TABLE_NAME").map_err(database_error)?;
        let entry = columns.entry(name.clone()).or_default();
        if entry.len() < MAX_COLUMNS {
            entry.push(column_of(raw)?);
        } else {
            over_wide.insert(name);
        }
    }

    let trigger_sql = format!(
        "SELECT DISTINCT EVENT_OBJECT_TABLE FROM information_schema.TRIGGERS \
         WHERE BINARY EVENT_OBJECT_SCHEMA = BINARY ? AND BINARY EVENT_OBJECT_TABLE IN ({})",
        placeholders(total)
    );
    let mut trigger_query = sqlx::query(&trigger_sql).bind(schema);
    for table in tables {
        trigger_query = trigger_query.bind(table);
    }
    let triggered = trigger_query.fetch_all(&mut *connection).await.map_err(database_error)?;
    let mut has_trigger = std::collections::HashSet::new();
    for row in &triggered {
        has_trigger.insert(metadata_text(row, "EVENT_OBJECT_TABLE").map_err(database_error)?);
    }

    let constraint_sql = format!(
        "SELECT TABLE_NAME, CONSTRAINT_TYPE FROM information_schema.TABLE_CONSTRAINTS \
         WHERE BINARY TABLE_SCHEMA = BINARY ? AND BINARY TABLE_NAME IN ({}) \
         GROUP BY TABLE_NAME, CONSTRAINT_TYPE",
        placeholders(total)
    );
    let mut constraint_query = sqlx::query(&constraint_sql).bind(schema);
    for table in tables {
        constraint_query = constraint_query.bind(table);
    }
    let constraints = constraint_query.fetch_all(&mut *connection).await.map_err(database_error)?;
    let mut constraint_types: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    for row in &constraints {
        let name = metadata_text(row, "TABLE_NAME").map_err(database_error)?;
        constraint_types.entry(name).or_default().push(
            metadata_text(row, "CONSTRAINT_TYPE").map_err(database_error)?,
        );
    }

    Ok(tables
        .iter()
        .map(|table| {
            // Anything that is not a base InnoDB table is rejected exactly as
            // the single-table path rejects it, with its own reason code.
            let kind = table_type.get(table).map(String::as_str).unwrap_or("");
            if kind.is_empty() {
                return TableCapability::blocked(&unsupported_reason(
                    "target_not_found",
                    "This table is no longer visible.",
                ));
            }
            if kind != "BASE TABLE" {
                return TableCapability::blocked(&unsupported_reason(
                    "not_a_base_table",
                    "Only a MySQL base table can be written; views and temporary objects are not.",
                ));
            }
            if engine.get(table).and_then(|value| value.as_deref()) != Some("InnoDB") {
                return TableCapability::blocked(&unsupported_reason(
                    "unsupported_engine",
                    "A single-row write needs transaction semantics, which only the InnoDB storage engine provides.",
                ));
            }
            if !has_primary_key.contains(table) {
                return TableCapability::blocked(&unsupported_reason(
                    "no_primary_key",
                    "This table has no primary key, so a single row cannot be identified for writing.",
                ));
            }
            let mut warnings = Vec::new();
            if has_trigger.contains(table) {
                warnings.push("triggers_present".to_string());
            }
            if let Some(present) = constraint_types.get(table) {
                for (kind, code) in [
                    ("FOREIGN KEY", "foreign_keys_present"),
                    ("CHECK", "check_constraints_present"),
                    ("UNIQUE", "unique_constraints_present"),
                ] {
                    if present.iter().any(|present| present == kind) {
                        warnings.push(code.to_string());
                    }
                }
            }
            let table_columns = columns.remove(table).unwrap_or_default();
            if over_wide.contains(table) || table_columns.is_empty() {
                return TableCapability::blocked(&unsupported_reason(
                    "unsupported_table_shape",
                    "This table has no columns or more columns than one write can address.",
                ));
            }
            push_text_primary_key_warning(&mut warnings, &table_columns);
            TableCapability::from_parts(table_columns, warnings)
        })
        .collect())
}

fn number(row: &MySqlRow, name: &str) -> Result<Option<i64>> {
    row.try_get::<Option<i64>, _>(name)
        .map_err(database_error)
}

/// Integers travel as decimal strings. A `BIGINT UNSIGNED` column holds values
/// above the signed range, so those are bound unsigned.
fn signed(text: &str) -> Result<i64> {
    i64::try_from(integer(text)?).map_err(|_| out_of_range())
}

fn out_of_range() -> WriteError {
    WriteError::new(
        "invalid_number",
        "The integer value does not fit the column's 64-bit storage.",
    )
}

fn bind<'q>(
    mut query: Query<'q, MySql, MySqlArguments>,
    metadata: &Metadata,
    values: impl Iterator<Item = (&'q String, &'q WriteValue)>,
) -> Result<Query<'q, MySql, MySqlArguments>> {
    for (name, value) in values {
        let column = metadata.column(name).ok_or_else(stale)?;
        query = match (value, &column.kind) {
            (WriteValue::Null, _) => query.bind(Option::<String>::None),
            (WriteValue::Boolean(value), _) => query.bind(*value),
            (WriteValue::Number(text), ColumnKind::Integer { max, .. })
                if *max > i64::MAX as i128 =>
            {
                let parsed = integer(text)?;
                if parsed > i64::MAX as i128 {
                    query.bind(u64::try_from(parsed).map_err(|_| out_of_range())?)
                } else {
                    query.bind(signed(text)?)
                }
            }
            (WriteValue::Number(text), ColumnKind::Integer { .. }) => query.bind(signed(text)?),
            (WriteValue::Number(text), ColumnKind::Float) => query.bind(float(text)?),
            // Decimals keep their exact textual form; MySQL converts the string
            // to the column's DECIMAL type.
            (WriteValue::Number(text), _) => query.bind(text.as_str()),
            (WriteValue::Text(text), _) => query.bind(text.as_str()),
        };
    }
    Ok(query)
}

pub(super) async fn rows(
    connection: &mut MySqlConnection,
    request: &WriteRequest,
    primary_key: &BTreeMap<String, WriteValue>,
    metadata: &Metadata,
    columns: &[&str],
    locked: bool,
) -> Result<Vec<ReadRow>> {
    let sql = select_sql(
        &DbKind::Mysql,
        columns,
        request,
        primary_key,
        metadata,
        locked,
    );
    let rows = bind(sqlx::query(&sql), metadata, primary_key.iter())?
        .fetch_all(&mut *connection)
        .await
        .map_err(database_error)?;
    rows.iter().map(|row| decode(row, columns)).collect()
}

fn decode(row: &MySqlRow, columns: &[&str]) -> Result<ReadRow> {
    columns
        .iter()
        .enumerate()
        .map(|(index, name)| {
            // The SQL casts every column to text. The metadata helper also
            // accepts text whose collation is flagged binary, which SQLx would
            // otherwise refuse to decode as `String`.
            let value = metadata_optional_text(row, index)
                .map_err(database_error)?
                .map_or(WriteValue::Null, WriteValue::Text);
            Ok(((*name).to_string(), value))
        })
        .collect()
}

pub(super) async fn dml(connection: &mut MySqlConnection, prepared: &PreparedWrite) -> Result<u64> {
    let metadata = prepared.metadata();
    let mut query = bind(
        sqlx::query(&prepared.preview.sql),
        metadata,
        prepared.request.values.iter(),
    )?;
    if matches!(
        prepared.request.operation,
        WriteOperation::Update | WriteOperation::Delete
    ) {
        query = bind(query, metadata, prepared.resolved().primary_key.iter())?;
    }
    Ok(query
        .execute(&mut *connection)
        .await
        .map_err(database_error)?
        .rows_affected())
}
