use super::*;
use sqlx::postgres::{PgArguments, PgRow};
use sqlx::query::Query;
use sqlx::{Postgres, Row};

pub(super) async fn begin(connection: &mut PgConnection, read_only: bool) -> Result<()> {
    // READ COMMITTED plus an explicit row lock on the single target row is the
    // whole concurrency story here: no LOCK TABLE, and no REPEATABLE READ that
    // would turn a concurrent update into a serialization failure.
    // Transaction control runs as a simple query: no bind parameters are
    // involved, and the simple protocol is what every dialect accepts here.
    raw_statement(
        connection,
        if read_only {
            "BEGIN ISOLATION LEVEL READ COMMITTED READ ONLY"
        } else {
            "BEGIN ISOLATION LEVEL READ COMMITTED READ WRITE"
        },
    )
    .await?;
    Ok(())
}

fn text(max_bytes: usize, format: TextFormat) -> ColumnKind {
    ColumnKind::Text { max_bytes, format }
}

/// Maps a built-in type by name. Only `pg_catalog` types are ever passed here,
/// so the name alone identifies the type.
fn column_kind(type_name: &str, modifier: i32) -> ColumnKind {
    match type_name {
        "bool" => ColumnKind::Boolean,
        "int2" => ColumnKind::Integer {
            min: i16::MIN as i128,
            max: i16::MAX as i128,
        },
        "int4" => ColumnKind::Integer {
            min: i32::MIN as i128,
            max: i32::MAX as i128,
        },
        "int8" => ColumnKind::Integer {
            min: i64::MIN as i128,
            max: i64::MAX as i128,
        },
        "numeric" => {
            if modifier < 0 {
                ColumnKind::Decimal {
                    precision: 1000,
                    scale: 1000,
                }
            } else {
                // PostgreSQL packs numeric(p,s) into atttypmod: the precision is
                // the high 16 bits and the scale the low 16 bits, after the
                // four-byte header.
                let packed = (modifier - 4) as u32;
                ColumnKind::Decimal {
                    precision: (packed >> 16) & 0xffff,
                    scale: packed & 0xffff,
                }
            }
        }
        "float4" | "float8" => ColumnKind::Float,
        "text" => text(MAX_TEXT_BYTES, TextFormat::Plain),
        "varchar" | "bpchar" if modifier > 4 => text((modifier - 4) as usize, TextFormat::Plain),
        "varchar" | "bpchar" => text(MAX_TEXT_BYTES, TextFormat::Plain),
        "date" => text(MAX_TEXT_BYTES, TextFormat::Date),
        "time" | "timetz" => text(MAX_TEXT_BYTES, TextFormat::Time),
        "timestamp" => text(MAX_TEXT_BYTES, TextFormat::Timestamp),
        "timestamptz" => text(MAX_TEXT_BYTES, TextFormat::TimestampTz),
        "json" => text(MAX_TEXT_BYTES, TextFormat::Json),
        "jsonb" => text(MAX_TEXT_BYTES, TextFormat::Jsonb),
        "uuid" => text(MAX_TEXT_BYTES, TextFormat::Uuid),
        _ => ColumnKind::Unsupported,
    }
}

/// Primary-key attribute numbers. `pg_index.indkey` is an `int2vector`, which
/// renders as a space-separated list.
fn key_positions(keys: &str) -> Vec<i32> {
    keys.split_whitespace()
        .filter_map(|position| position.parse::<i32>().ok())
        .collect()
}

pub(super) async fn metadata(
    connection: &mut PgConnection,
    request: &WriteRequest,
) -> Result<Metadata> {
    let tables = sqlx::query(
        "SELECT c.oid::bigint AS oid, c.relkind::text AS relkind, \
         c.relrowsecurity AS row_security, c.relpersistence::text AS persistence \
         FROM pg_catalog.pg_class c \
         JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
         WHERE n.nspname = $1 AND c.relname = $2 LIMIT 2",
    )
    .bind(&request.schema)
    .bind(&request.table)
    .fetch_all(&mut *connection)
    .await
    .map_err(database_error)?;
    let [table] = tables.as_slice() else {
        return Err(unsupported_reason(
            "target_not_found",
            "The requested name is not exactly one relation in this schema.",
        ));
    };
    let oid: i64 = table.try_get("oid").map_err(database_error)?;
    let relkind: String = table.try_get("relkind").map_err(database_error)?;
    if relkind != "r" {
        return Err(unsupported_reason(
            "not_a_base_table",
            "Only an ordinary PostgreSQL table can be written; views, materialized views, foreign tables, sequences, and partitioned parent tables cannot be targeted by a single row.",
        ));
    }
    if table
        .try_get::<bool, _>("row_security")
        .map_err(database_error)?
    {
        return Err(unsupported_reason(
            "row_level_security",
            "Row-level security is enabled, so the rows a statement affects cannot be predicted.",
        ));
    }
    if table
        .try_get::<String, _>("persistence")
        .map_err(database_error)?
        == "t"
    {
        return Err(unsupported_reason(
            "temporary_table",
            "Temporary tables belong to one session and cannot be written durably.",
        ));
    }
    // A non-_RETURN rewrite rule can silently redirect the write elsewhere.
    let rules = sqlx::query(
        "SELECT rulename::text AS name FROM pg_catalog.pg_rewrite \
         WHERE ev_class = $1::oid AND rulename <> '_RETURN' LIMIT 2",
    )
    .bind(oid)
    .fetch_all(&mut *connection)
    .await
    .map_err(database_error)?;
    if !rules.is_empty() {
        return Err(unsupported_reason(
            "rewrite_rule",
            "This table has a rewrite rule, which can silently redirect a write.",
        ));
    }
    let indexes = sqlx::query(
        "SELECT i.indkey::text AS keys FROM pg_catalog.pg_index \
         WHERE i.indrelid = $1::oid AND i.indisprimary LIMIT 2",
    )
    .bind(oid)
    .fetch_all(&mut *connection)
    .await
    .map_err(database_error)?;
    let [index] = indexes.as_slice() else {
        return Err(unsupported_reason(
            "no_primary_key",
            "This table has no primary key, so a single row cannot be identified for writing.",
        ));
    };
    let primary = key_positions(&index.try_get::<String, _>("keys").map_err(database_error)?);
    if primary.is_empty() {
        return Err(unsupported_reason(
            "no_primary_key",
            "This table has no primary key, so a single row cannot be identified for writing.",
        ));
    }
    let raw_columns = sqlx::query(
        "SELECT a.attname::text AS name, a.attnum::int AS position, \
         t.typname::text AS type_name, pg_catalog.format_type(a.atttypid, a.atttypmod) AS declared, \
         a.atttypmod AS modifier, a.attnotnull AS not_null, a.atthasdef AS has_default, \
         a.attidentity::text AS identity, a.attgenerated::text AS generated \
         FROM pg_catalog.pg_attribute a \
         JOIN pg_catalog.pg_type t ON t.oid = a.atttypid \
         JOIN pg_catalog.pg_namespace tn ON tn.oid = t.typnamespace \
         WHERE a.attrelid = $1::oid AND a.attnum > 0 AND NOT a.attisdropped \
         AND tn.nspname = 'pg_catalog' \
         ORDER BY a.attnum LIMIT 257",
    )
    .bind(oid)
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
        let position: i32 = raw.try_get("position").map_err(database_error)?;
        columns.push(column_of(raw, primary.contains(&position))?);
    }
    let mut warnings = Vec::new();
    let triggers: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)::bigint FROM pg_catalog.pg_trigger \
         WHERE tgrelid = $1::oid AND NOT tgisinternal",
    )
    .bind(oid)
    .fetch_one(&mut *connection)
    .await
    .map_err(database_error)?;
    if triggers > 0 {
        warnings.push("triggers_present".to_string());
    }
    let constraints = sqlx::query(
        "SELECT contype::text AS kind, COUNT(*)::bigint AS total FROM pg_catalog.pg_constraint \
         WHERE conrelid = $1::oid GROUP BY contype LIMIT 64",
    )
    .bind(oid)
    .fetch_all(&mut *connection)
    .await
    .map_err(database_error)?;
    let mut present = Vec::new();
    for constraint in &constraints {
        let kind: String = constraint.try_get("kind").map_err(database_error)?;
        let total: i64 = constraint.try_get("total").map_err(database_error)?;
        if total > 0 {
            present.push(kind);
        }
    }
    push_constraint_warnings(&mut warnings, &present);
    Ok(Metadata {
        fingerprint: column_signature(&columns),
        columns,
        warnings,
    })
}

/// One `pg_attribute` row as the write planner models it.
fn column_of(raw: &PgRow, primary_key: bool) -> Result<Column> {
    let name: String = raw.try_get("name").map_err(database_error)?;
    validate_identifier(&name)?;
    let type_name: String = raw.try_get("type_name").map_err(database_error)?;
    let modifier: i32 = raw.try_get("modifier").map_err(database_error)?;
    let identity: String = raw.try_get("identity").map_err(database_error)?;
    let generated: String = raw.try_get("generated").map_err(database_error)?;
    Ok(Column {
        name,
        declared: raw.try_get("declared").map_err(database_error)?,
        kind: column_kind(&type_name, modifier),
        nullable: !raw.try_get::<bool, _>("not_null").map_err(database_error)? && !primary_key,
        primary_key,
        // attgenerated is 's' for a stored and 'v' for a virtual column.
        generated: !generated.is_empty(),
        // attidentity is 'a' for GENERATED ALWAYS and 'd' for BY DEFAULT.
        auto_increment: matches!(identity.as_str(), "a" | "d"),
        has_default: raw.try_get::<bool, _>("has_default").map_err(database_error)?,
        on_update: false,
    })
}

fn push_constraint_warnings(warnings: &mut Vec<String>, present: &[String]) {
    for (kind, code) in [
        ("f", "foreign_keys_present"),
        ("c", "check_constraints_present"),
        ("u", "unique_constraints_present"),
    ] {
        if present.iter().any(|present| present.as_str() == kind) {
            warnings.push(code.to_string());
        }
    }
}

/// Batch capability for a whole schema. The single-table reader issues six
/// queries per table; this one issues eight for the entire list, which is what
/// makes a schema with hundreds of tables open in bounded time.
pub(super) async fn capabilities(
    connection: &mut PgConnection,
    schema: &str,
    tables: &[String],
) -> Result<Vec<TableCapability>> {
    let total = tables.len();
    let catalog_sql = format!(
        "SELECT c.oid::bigint AS oid, c.relname::text AS name, c.relkind::text AS relkind, \
         c.relrowsecurity AS row_security, c.relpersistence::text AS persistence \
         FROM pg_catalog.pg_class c \
         JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
         WHERE n.nspname = $1 AND c.relname IN ({})",
        numbered_placeholders(2, total)
    );
    let mut catalog_query = sqlx::query(&catalog_sql).bind(schema);
    for table in tables {
        catalog_query = catalog_query.bind(table);
    }
    let catalog = catalog_query.fetch_all(&mut *connection).await.map_err(database_error)?;
    let mut oids: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    let mut catalog_reason: std::collections::HashMap<String, (String, String)> =
        std::collections::HashMap::new();
    for row in &catalog {
        let name: String = row.try_get("name").map_err(database_error)?;
        let oid: i64 = row.try_get("oid").map_err(database_error)?;
        let relkind: String = row.try_get("relkind").map_err(database_error)?;
        if relkind != "r" {
            catalog_reason.insert(
                name,
                (
                    "not_a_base_table".to_string(),
                    "Only an ordinary PostgreSQL table can be written; views, materialized views, foreign tables, sequences, and partitioned parent tables cannot be targeted by a single row.".to_string(),
                ),
            );
            continue;
        }
        if row
            .try_get::<bool, _>("row_security")
            .map_err(database_error)?
        {
            catalog_reason.insert(
                name,
                (
                    "row_level_security".to_string(),
                    "Row-level security is enabled, so the rows a statement affects cannot be predicted.".to_string(),
                ),
            );
            continue;
        }
        if row
            .try_get::<String, _>("persistence")
            .map_err(database_error)?
            == "t"
        {
            catalog_reason.insert(
                name,
                (
                    "temporary_table".to_string(),
                    "Temporary tables belong to one session and cannot be written durably.".to_string(),
                ),
            );
            continue;
        }
        oids.insert(name, oid);
    }
    if oids.is_empty() {
        return Ok(tables
            .iter()
            .map(|table| {
                let (code, message) = catalog_reason.get(table).map_or(
                    (
                        "target_not_found".to_string(),
                        "This table is no longer visible.".to_string(),
                    ),
                    |(code, message)| (code.clone(), message.clone()),
                );
                TableCapability::blocked(&unsupported_reason(&code, &message))
            })
            .collect());
    }

    let oid_list = oids.values().copied().collect::<Vec<_>>();
    let oid_marks = numbered_placeholders(1, oid_list.len());

    let rules_sql = format!(
        "SELECT oid, rulename::text AS name FROM (
           SELECT ev_class AS oid, rulename FROM pg_catalog.pg_rewrite
           WHERE ev_class IN ({oid_marks}) AND rulename <> '_RETURN'
         ) rules"
    );
    let mut rules_query = sqlx::query(&rules_sql);
    for oid in &oid_list {
        rules_query = rules_query.bind(oid);
    }
    let rules = rules_query.fetch_all(&mut *connection).await.map_err(database_error)?;
    let mut rewrite_rule: std::collections::HashSet<i64> = std::collections::HashSet::new();
    for row in &rules {
        rewrite_rule.insert(row.try_get("oid").map_err(database_error)?);
    }

    let key_sql = format!(
        "SELECT indrelid::bigint AS oid, indkey::text AS keys FROM pg_catalog.pg_index \
         WHERE indrelid IN ({}) AND indisprimary",
        numbered_placeholders(1, oid_list.len())
    );
    let mut key_query = sqlx::query(&key_sql);
    for oid in &oid_list {
        key_query = key_query.bind(oid);
    }
    let indexes = key_query.fetch_all(&mut *connection).await.map_err(database_error)?;
    let mut primary: std::collections::HashMap<i64, Vec<i32>> = std::collections::HashMap::new();
    for row in &indexes {
        let oid: i64 = row.try_get("oid").map_err(database_error)?;
        let keys: String = row.try_get("keys").map_err(database_error)?;
        primary.insert(oid, key_positions(&keys));
    }

    let column_sql = format!(
        "SELECT a.attrelid::bigint AS oid, a.attname::text AS name, a.attnum::int AS position, \
         t.typname::text AS type_name, pg_catalog.format_type(a.atttypid, a.atttypmod) AS declared, \
         a.atttypmod AS modifier, a.attnotnull AS not_null, a.atthasdef AS has_default, \
         a.attidentity::text AS identity, a.attgenerated::text AS generated \
         FROM pg_catalog.pg_attribute a \
         JOIN pg_catalog.pg_type t ON t.oid = a.atttypid \
         JOIN pg_catalog.pg_namespace tn ON tn.oid = t.typnamespace \
         WHERE a.attrelid IN ({}) AND a.attnum > 0 AND NOT a.attisdropped \
         AND tn.nspname = 'pg_catalog' \
         ORDER BY a.attrelid, a.attnum",
        numbered_placeholders(1, oid_list.len())
    );
    let mut columns_query = sqlx::query(&column_sql);
    for oid in &oid_list {
        columns_query = columns_query.bind(oid);
    }
    let raw_columns = columns_query.fetch_all(&mut *connection).await.map_err(database_error)?;
    let mut columns: std::collections::HashMap<i64, Vec<Column>> = std::collections::HashMap::new();
    // Track which relations exceeded the column bound: a truncated column list
    // would otherwise look like a valid narrow table.
    let mut over_wide: std::collections::HashSet<i64> = std::collections::HashSet::new();
    for raw in &raw_columns {
        let oid: i64 = raw.try_get("oid").map_err(database_error)?;
        let position: i32 = raw.try_get("position").map_err(database_error)?;
        let keys = primary.get(&oid).map(Vec::as_slice).unwrap_or_default();
        let entry = columns.entry(oid).or_default();
        if entry.len() < MAX_COLUMNS {
            entry.push(column_of(raw, keys.contains(&position))?);
        } else {
            over_wide.insert(oid);
        }
    }

    let trigger_sql = format!(
        "SELECT DISTINCT tgrelid::bigint AS oid FROM pg_catalog.pg_trigger \
         WHERE tgrelid IN ({}) AND NOT tgisinternal",
        numbered_placeholders(1, oid_list.len())
    );
    let mut trigger_query = sqlx::query(&trigger_sql);
    for oid in &oid_list {
        trigger_query = trigger_query.bind(oid);
    }
    let triggered = trigger_query.fetch_all(&mut *connection).await.map_err(database_error)?;
    let mut has_trigger: std::collections::HashSet<i64> = std::collections::HashSet::new();
    for row in &triggered {
        has_trigger.insert(row.try_get("oid").map_err(database_error)?);
    }

    let constraint_sql = format!(
        "SELECT conrelid::bigint AS oid, contype::text AS kind, COUNT(*)::bigint AS total \
         FROM pg_catalog.pg_constraint WHERE conrelid IN ({}) \
         GROUP BY conrelid, contype",
        numbered_placeholders(1, oid_list.len())
    );
    let mut constraint_query = sqlx::query(&constraint_sql);
    for oid in &oid_list {
        constraint_query = constraint_query.bind(oid);
    }
    let constraints = constraint_query.fetch_all(&mut *connection).await.map_err(database_error)?;
    let mut present: std::collections::HashMap<i64, Vec<String>> = std::collections::HashMap::new();
    for row in &constraints {
        let oid: i64 = row.try_get("oid").map_err(database_error)?;
        let total: i64 = row.try_get("total").map_err(database_error)?;
        if total > 0 {
            present
                .entry(oid)
                .or_default()
                .push(row.try_get("kind").map_err(database_error)?);
        }
    }

    Ok(tables
        .iter()
        .map(|table| {
            if let Some((code, message)) = catalog_reason.get(table) {
                return TableCapability::blocked(&unsupported_reason(code, message));
            }
            let Some(oid) = oids.get(table).copied() else {
                return TableCapability::blocked(&unsupported_reason(
                    "target_not_found",
                    "This table is no longer visible.",
                ));
            };
            if rewrite_rule.contains(&oid) {
                return TableCapability::blocked(&unsupported_reason(
                    "rewrite_rule",
                    "This table has a rewrite rule, which can silently redirect a write.",
                ));
            }
            if primary.get(&oid).is_none_or(Vec::is_empty) {
                return TableCapability::blocked(&unsupported_reason(
                    "no_primary_key",
                    "This table has no primary key, so a single row cannot be identified for writing.",
                ));
            }
            let table_columns = columns.remove(&oid).unwrap_or_default();
            if over_wide.contains(&oid) || table_columns.is_empty() {
                return TableCapability::blocked(&unsupported_reason(
                    "unsupported_table_shape",
                    "This table has no columns or more columns than one write can address.",
                ));
            }
            let mut warnings = Vec::new();
            if has_trigger.contains(&oid) {
                warnings.push("triggers_present".to_string());
            }
            if let Some(kinds) = present.get(&oid) {
                push_constraint_warnings(&mut warnings, kinds);
            }
            TableCapability::from_parts(table_columns, warnings)
        })
        .collect())
}

/// Integers travel as decimal strings; PostgreSQL binds them as 64-bit values.
fn signed(text: &str) -> Result<i64> {
    i64::try_from(integer(text)?).map_err(|_| {
        WriteError::new(
            "invalid_number",
            "The integer value does not fit 64 bits.",
        )
    })
}

fn bind<'q>(
    mut query: Query<'q, Postgres, PgArguments>,
    metadata: &Metadata,
    values: impl Iterator<Item = (&'q String, &'q WriteValue)>,
) -> Result<Query<'q, Postgres, PgArguments>> {
    for (name, value) in values {
        let column = metadata.column(name).ok_or_else(stale)?;
        query = match (value, &column.kind) {
            (WriteValue::Null, _) => query.bind(Option::<String>::None),
            (WriteValue::Boolean(value), _) => query.bind(*value),
            (WriteValue::Number(text), ColumnKind::Integer { .. }) => query.bind(signed(text)?),
            (WriteValue::Number(text), ColumnKind::Float) => query.bind(float(text)?),
            // Decimals keep their exact textual form; the placeholder already
            // carries the `::numeric` cast.
            (WriteValue::Number(text), _) => query.bind(text.as_str()),
            (WriteValue::Text(text), _) => query.bind(text.as_str()),
        };
    }
    Ok(query)
}

pub(super) async fn rows(
    connection: &mut PgConnection,
    request: &WriteRequest,
    primary_key: &BTreeMap<String, WriteValue>,
    metadata: &Metadata,
    columns: &[&str],
    locked: bool,
) -> Result<Vec<ReadRow>> {
    let sql = select_sql(
        &DbKind::Postgres,
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

fn decode(row: &PgRow, columns: &[&str]) -> Result<ReadRow> {
    columns
        .iter()
        .enumerate()
        .map(|(index, name)| {
            // The SQL casts every column to text, so the value is never decoded
            // as its declared type.
            let value = row
                .try_get::<Option<String>, _>(index)
                .map_err(database_error)?
                .map_or(WriteValue::Null, WriteValue::Text);
            Ok(((*name).to_string(), value))
        })
        .collect()
}

pub(super) async fn dml(connection: &mut PgConnection, prepared: &PreparedWrite) -> Result<u64> {
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
