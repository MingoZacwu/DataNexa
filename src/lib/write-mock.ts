import type { ConnectionConfig, WriteOperation, WriteSchema, WriteTarget } from "../types";

// Browser-only metadata, independent from the configured target allowlist.
// Names and capabilities are demonstration fixtures, never live records.
const catalog: Record<string, WriteTarget[]> = Object.fromEntries(
  ["main", "demo_workspace", "demo_archive"].map((schema) => [schema, [
    "demo_inventory", "demo_orders", "demo_products", "demo_locations", "demo_batches",
    "demo_labels", "demo_metrics", "demo_settings", "demo_queue", "demo_events",
    "demo_summary", "DemoCaseTable", "demo_view", "demo_audit_trail"
  ].map((table): WriteTarget => {
    // Three shapes: writable, blocked for lack of a primary key, and blocked
    // for a column type the writer cannot encode.
    const blockCode = table === "demo_view"
      ? "no_primary_key"
      : table === "demo_audit_trail" ? "unsupported_column_type" : null;
    const operations: WriteOperation[] = blockCode ? [] : ["insert", "update", "delete"];
    return {
      target_id: `demo:${schema}:${table}`,
      schema,
      table,
      operations,
      reason_code: blockCode,
      reason: blockCode === "no_primary_key"
        ? "This table has no primary key, so a single row cannot be identified for writing."
        : blockCode ? "Column `payload` is required on insert but its type (blob) cannot be written." : null,
      reason_detail: blockCode === "unsupported_column_type" ? "payload · blob" : null
    };
  })])
);

export function mockWriteSchemas(connection: ConnectionConfig): WriteSchema[] {
  if (connection.type === "jdbc") return [];
  return (connection.type === "sqlite" ? ["main", "demo_archive"] : ["demo_workspace", "demo_archive"]).map((name) => ({ name }));
}

export function mockWriteTables(connection: ConnectionConfig, schema: string, search?: string): WriteTarget[] {
  if (!mockWriteSchemas(connection).some((entry) => entry.name === schema)) throw new Error("schema_not_found: Demonstration schema is not available.");
  const query = search?.trim().toLocaleLowerCase() ?? "";
  return catalog[schema]
    .filter((target) => !query || target.table.toLocaleLowerCase().includes(query))
    .map((target) => ({ ...target, operations: [...target.operations] }));
}
