use super::*;
use sqlx::query::Query;
use sqlx::sqlite::SqliteArguments;
use sqlx::{Row, Sqlite};

pub(super) async fn begin(connection: &mut SqliteConnection, read_only: bool) -> Result<()> {
    if read_only {
        // query_only makes the preview transaction physically unable to write,
        // even if a later statement were generated incorrectly.
        raw_statement(connection, "PRAGMA query_only = ON").await?;
        raw_statement(connection, "BEGIN").await?;
    } else {
        // IMMEDIATE takes the write lock before the first read, so the
        // before-image and the write itself cannot observe two versions of the
        // same row. SQLite has no `FOR UPDATE`, and this is the equivalent.
        raw_statement(connection, "BEGIN IMMEDIATE").await?;
    }
    Ok(())
}

/// Numbers inside the `(...)` suffix of a declared type, e.g. `VARCHAR(255)`
/// or `DECIMAL(10,2)`. SQLite does not validate the suffix, so a malformed one
/// simply means "no declared size".
fn declared_numbers(declared: &str) -> Option<Vec<u32>> {
    let open = declared.find('(')?;
    let close = declared[open..].find(')')?;
    declared[open + 1..open + close]
        .split(',')
        .map(|part| part.trim().parse::<u32>())
        .collect::<std::result::Result<Vec<u32>, _>>()
        .ok()
}

fn text(max_bytes: usize, format: TextFormat) -> ColumnKind {
    ColumnKind::Text { max_bytes, format }
}

/// `VARCHAR(N)`/`TEXT(N)` gives the write validator a per-column byte bound.
fn text_bytes(declared: &str) -> usize {
    declared_numbers(declared)
        .and_then(|numbers| numbers.first().copied())
        .map_or(MAX_TEXT_BYTES, |length| {
            (length as usize).clamp(1, MAX_TEXT_BYTES)
        })
}

fn decimal_kind(declared: &str) -> ColumnKind {
    match declared_numbers(declared).as_deref() {
        Some([precision, scale, ..]) => ColumnKind::Decimal {
            precision: *precision,
            scale: *scale,
        },
        Some([precision]) => ColumnKind::Decimal {
            precision: *precision,
            scale: 0,
        },
        _ => ColumnKind::Decimal {
            precision: 1000,
            scale: 1000,
        },
    }
}

/// SQLite has type affinity rather than types: the declared type is the only
/// hint the database itself has about a column, so it decides how a value is
/// written and compared. Anything unrecognized is carried as text, which SQLite
/// stores losslessly; nothing here ever rejects the table.
fn column_kind(declared: &str) -> ColumnKind {
    let declared = declared.trim().to_ascii_uppercase();
    if declared.contains("INT") {
        ColumnKind::Integer {
            min: i64::MIN as i128,
            max: i64::MAX as i128,
        }
    } else if declared.contains("BOOL") {
        ColumnKind::Boolean
    } else if declared.contains("CHAR") || declared.contains("CLOB") || declared.contains("TEXT") {
        text(text_bytes(&declared), TextFormat::Plain)
    } else if declared.contains("BLOB") || declared.is_empty() {
        ColumnKind::Unsupported
    } else if declared.contains("REAL") || declared.contains("FLOA") || declared.contains("DOUB") {
        ColumnKind::Float
    } else if declared.contains("DATETIME") || declared.contains("TIMESTAMP") {
        // Checked before DATE and TIME: both are substrings of DATETIME.
        text(MAX_TEXT_BYTES, TextFormat::Timestamp)
    } else if declared.contains("DATE") {
        text(MAX_TEXT_BYTES, TextFormat::Date)
    } else if declared.contains("TIME") {
        text(MAX_TEXT_BYTES, TextFormat::Time)
    } else if declared.contains("JSON") {
        text(MAX_TEXT_BYTES, TextFormat::Json)
    } else if declared.contains("DECIMAL") || declared.contains("NUMERIC") {
        decimal_kind(&declared)
    } else if declared.contains("UUID") {
        text(MAX_TEXT_BYTES, TextFormat::Uuid)
    } else {
        text(MAX_TEXT_BYTES, TextFormat::Plain)
    }
}

pub(super) async fn metadata(
    connection: &mut SqliteConnection,
    request: &WriteRequest,
) -> Result<Metadata> {
    if !request.schema.eq_ignore_ascii_case("main") {
        return Err(unsupported_reason(
            "schema_not_writable",
            "SQLite writes are limited to the main schema of the current database file.",
        ));
    }
    let listed = sqlx::query(
        "SELECT type, \"wr\" FROM pragma_table_list WHERE schema = 'main' AND name = ? LIMIT 2",
    )
    .bind(&request.table)
    .fetch_all(&mut *connection)
    .await
    .map_err(database_error)?;
    let [table] = listed.as_slice() else {
        return Err(unsupported_reason(
            "target_not_found",
            "The requested name is not exactly one table in the main SQLite schema.",
        ));
    };
    if table
        .try_get::<String, _>("type")
        .map_err(database_error)?
        != "table"
    {
        return Err(unsupported_reason(
            "not_a_base_table",
            "Only an ordinary SQLite table can be written; views, virtual tables, and shadow tables are not writable.",
        ));
    }
    let without_rowid = table
        .try_get::<i64, _>("wr")
        .map_err(database_error)?
        != 0;
    let raw_columns = sqlx::query(
        "SELECT name, type, \"notnull\", dflt_value, pk, hidden \
         FROM pragma_table_xinfo(?, 'main') ORDER BY cid LIMIT 257",
    )
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
    if !columns.iter().any(|column| column.primary_key) {
        return Err(unsupported_reason(
            "no_primary_key",
            "This table has no primary key, so a single row cannot be identified for writing.",
        ));
    }
    apply_rowid_alias(&mut columns, without_rowid);
    let mut warnings = Vec::new();
    let triggers: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM main.sqlite_schema WHERE type = 'trigger' AND tbl_name = ? COLLATE NOCASE",
    )
    .bind(&request.table)
    .fetch_one(&mut *connection)
    .await
    .map_err(database_error)?;
    if triggers > 0 {
        warnings.push("triggers_present".to_string());
    }
    let foreign_keys = sqlx::query("SELECT id FROM pragma_foreign_key_list(?, 'main') LIMIT 1")
        .bind(&request.table)
        .fetch_all(&mut *connection)
        .await
        .map_err(database_error)?;
    if !foreign_keys.is_empty() {
        warnings.push("foreign_keys_present".to_string());
    }
    Ok(Metadata {
        fingerprint: column_signature(&columns),
        columns,
        warnings,
    })
}

/// One `pragma_table_xinfo` row as the write planner models it.
fn column_of(raw: &sqlx::sqlite::SqliteRow) -> Result<Column> {
    let name: String = raw.try_get("name").map_err(database_error)?;
    validate_identifier(&name)?;
    let declared: String = raw
        .try_get::<Option<String>, _>("type")
        .map_err(database_error)?
        .unwrap_or_default();
    let primary_key = raw.try_get::<i64, _>("pk").map_err(database_error)? > 0;
    // `hidden` is 0 for ordinary columns, 1 for virtual-table hidden columns,
    // and 2 or 3 for generated columns.
    let hidden = raw.try_get::<i64, _>("hidden").map_err(database_error)?;
    let kind = if hidden == 1 {
        ColumnKind::Unsupported
    } else {
        column_kind(&declared)
    };
    Ok(Column {
        name,
        declared,
        kind,
        // A primary key cannot be null in a WITHOUT ROWID table and is supplied
        // explicitly everywhere else, so it is never nullable.
        nullable: raw
            .try_get::<i64, _>("notnull")
            .map_err(database_error)?
            == 0
            && !primary_key,
        primary_key,
        generated: matches!(hidden, 2 | 3),
        auto_increment: false,
        has_default: raw
            .try_get::<Option<String>, _>("dflt_value")
            .map_err(database_error)?
            .is_some(),
        on_update: false,
    })
}

/// A single `INTEGER PRIMARY KEY` column aliases the rowid and is filled in by
/// SQLite when an insert omits it. WITHOUT ROWID tables have no rowid, so the
/// alias does not apply there.
fn apply_rowid_alias(columns: &mut [Column], without_rowid: bool) {
    if without_rowid {
        return;
    }
    let primary = columns
        .iter()
        .enumerate()
        .filter(|(_, column)| column.primary_key)
        .map(|(index, _)| index)
        .collect::<Vec<usize>>();
    if let [only] = primary.as_slice() {
        if columns
            .get(*only)
            .is_some_and(|column| column.declared.trim().eq_ignore_ascii_case("INTEGER"))
        {
            if let Some(column) = columns.get_mut(*only) {
                column.auto_increment = true;
            }
        }
    }
}

/// Batch capability for the whole SQLite schema. `pragma_table_xinfo` is a
/// table-valued function, so one `pragma_table_list` scan plus one xinfo scan
/// per table replaces the six per-table queries of the single-table path.
/// SQLite tables are small in number, and the xinfo scan is local, so this is
/// already the fast path.
pub(super) async fn capabilities(
    connection: &mut SqliteConnection,
    schema: &str,
    tables: &[String],
) -> Result<Vec<TableCapability>> {
    if !schema.eq_ignore_ascii_case("main") {
        return Err(unsupported_reason(
            "schema_not_writable",
            "SQLite writes are limited to the main schema of the current database file.",
        ));
    }
    let listed = sqlx::query(
        "SELECT name, type, \"wr\" FROM pragma_table_list WHERE schema = 'main'",
    )
    .fetch_all(&mut *connection)
    .await
    .map_err(database_error)?;
    let mut kinds: std::collections::HashMap<String, (String, i64)> = std::collections::HashMap::new();
    for row in &listed {
        let name: String = row.try_get("name").map_err(database_error)?;
        kinds.insert(
            name,
            (
                row.try_get("type").map_err(database_error)?,
                row.try_get("wr").map_err(database_error)?,
            ),
        );
    }
    let triggers = sqlx::query(
        "SELECT DISTINCT tbl_name FROM main.sqlite_schema WHERE type = 'trigger'",
    )
    .fetch_all(&mut *connection)
    .await
    .map_err(database_error)?;
    let mut has_trigger: std::collections::HashSet<String> = std::collections::HashSet::new();
    for row in &triggers {
        has_trigger.insert(
            row.try_get::<String, _>("tbl_name")
                .map_err(database_error)?,
        );
    }

    let mut result = Vec::with_capacity(tables.len());
    for table in tables {
        let Some((kind, without_rowid)) = kinds.get(table) else {
            result.push(TableCapability::blocked(&unsupported_reason(
                "target_not_found",
                "This table is no longer visible.",
            )));
            continue;
        };
        if kind != "table" {
            result.push(TableCapability::blocked(&unsupported_reason(
                "not_a_base_table",
                "Only an ordinary SQLite table can be written; views, virtual tables, and shadow tables are not writable.",
            )));
            continue;
        }
        let raw_columns = sqlx::query(
            "SELECT name, type, \"notnull\", dflt_value, pk, hidden \
             FROM pragma_table_xinfo(?, 'main') ORDER BY cid LIMIT 257",
        )
        .bind(table)
        .fetch_all(&mut *connection)
        .await
        .map_err(database_error)?;
        if raw_columns.is_empty() || raw_columns.len() > MAX_COLUMNS {
            result.push(TableCapability::blocked(&unsupported_reason(
                "unsupported_table_shape",
                "This table has no columns or more columns than one write can address.",
            )));
            continue;
        }
        let mut columns = Vec::with_capacity(raw_columns.len());
        for raw in &raw_columns {
            columns.push(column_of(raw)?);
        }
        if !columns.iter().any(|column| column.primary_key) {
            result.push(TableCapability::blocked(&unsupported_reason(
                "no_primary_key",
                "This table has no primary key, so a single row cannot be identified for writing.",
            )));
            continue;
        }
        apply_rowid_alias(&mut columns, *without_rowid != 0);
        let mut warnings = Vec::new();
        if has_trigger.contains(table) {
            warnings.push("triggers_present".to_string());
        }
        let foreign_keys = sqlx::query("SELECT id FROM pragma_foreign_key_list(?, 'main') LIMIT 1")
            .bind(table)
            .fetch_all(&mut *connection)
            .await
            .map_err(database_error)?;
        if !foreign_keys.is_empty() {
            warnings.push("foreign_keys_present".to_string());
        }
        result.push(TableCapability::from_parts(columns, warnings));
    }
    Ok(result)
}

/// Integers travel as decimal strings; SQLite binds them as 64-bit values.
fn signed(text: &str) -> Result<i64> {
    i64::try_from(integer(text)?).map_err(|_| {
        WriteError::new(
            "invalid_number",
            "The integer value does not fit 64 bits.",
        )
    })
}

fn bind<'q>(
    mut query: Query<'q, Sqlite, SqliteArguments<'q>>,
    metadata: &Metadata,
    values: impl Iterator<Item = (&'q String, &'q WriteValue)>,
) -> Result<Query<'q, Sqlite, SqliteArguments<'q>>> {
    for (name, value) in values {
        let column = metadata.column(name).ok_or_else(stale)?;
        query = match (value, &column.kind) {
            (WriteValue::Null, _) => query.bind(Option::<String>::None),
            (WriteValue::Boolean(value), _) => query.bind(*value),
            (WriteValue::Number(text), ColumnKind::Integer { .. }) => query.bind(signed(text)?),
            (WriteValue::Number(text), ColumnKind::Float) => query.bind(float(text)?),
            // Decimal keeps its exact textual form; SQLite applies the column's
            // NUMERIC affinity when it stores the bound text.
            (WriteValue::Number(text), _) => query.bind(text.as_str()),
            (WriteValue::Text(text), _) => query.bind(text.as_str()),
        };
    }
    Ok(query)
}

pub(super) async fn rows(
    connection: &mut SqliteConnection,
    request: &WriteRequest,
    primary_key: &BTreeMap<String, WriteValue>,
    metadata: &Metadata,
    columns: &[&str],
    locked: bool,
) -> Result<Vec<ReadRow>> {
    let sql = select_sql(
        &DbKind::Sqlite,
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
    rows.iter()
        .map(|row| {
            columns
                .iter()
                .enumerate()
                .map(|(index, name)| {
                    // The SQL casts every column to text, so the value never
                    // needs to be decoded as its declared type.
                    let value = row
                        .try_get::<Option<String>, _>(index)
                        .map_err(database_error)?
                        .map_or(WriteValue::Null, WriteValue::Text);
                    Ok(((*name).to_string(), value))
                })
                .collect()
        })
        .collect()
}

pub(super) async fn dml(
    connection: &mut SqliteConnection,
    prepared: &PreparedWrite,
) -> Result<u64> {
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
