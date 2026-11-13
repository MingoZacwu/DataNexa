import type { Locale } from "../../i18n";
import type { AuditEvent, WriteOperationStatus } from "../../types";
import { writeMessages, writeReasonText } from "../../write-i18n";

export const auditWriteStatuses: WriteOperationStatus[] = [
  "pending", "executing", "committing", "committed", "rolled_back", "rejected", "expired", "cancelled", "failed", "unknown"
];

const zhCN = {
  operation: "操作",
  insert: "插入 · INSERT",
  update: "更新 · UPDATE",
  delete: "删除 · DELETE",
  target: "目标表",
  requestId: "请求编号",
  attemptId: "尝试编号",
  noRequest: "未创建写入请求",
  reasonCode: "错误代码",
  stage: "阶段",
  stageUnavailable: "未记录具体阶段",
  revision: "状态版本",
  updatedAt: "最近更新",
  nextStep: "下一步",
  outcome: "执行结果",
  outcomeUnknown: "结果未知。必须人工核查，请勿自动重试或换新幂等键提交。",
  affectedRows: "影响行数",
  notReported: "尚未报告",
  search: "搜索",
  searchPlaceholder: "目标表、错误代码、请求编号或安全原因",
  submitTool: "提交受限写入",
  statusTool: "查询写入状态",
  incomplete: "未记录完整写入状态，请使用原请求编号查询。勿据此认定成功或重新提交。",
  stageLabels: {
    submission: "提交校验", preparation: "准备预览", preparation_or_approval: "准备预览 / 等待审批",
    approval: "本机审批", execution: "事务执行", commit: "事务提交", rollback: "事务回滚", recovery: "恢复核对"
  },
  nextSteps: {
    pending: "在本机审批窗口核对目标和变更；未批准前不会执行。请等待或拒绝，不要重复提交。",
    executing: "等待事务执行完成，并使用原请求编号查询状态。不要重复提交。",
    committing: "等待提交结果，使用原请求编号查询。当前不能认定成功，请勿重试。",
    committed: "事务已确认提交。需要核对时可只读查询目标记录，不要重复执行。",
    rolled_back: "事务已确认回滚。先处理原因，再重新准备预览并逐项审批。",
    rejected: "请求未获准执行。检查原因并修正权限、配置或请求后，重新发起审批。",
    expired: "审批时限已过，本次请求不会执行。仍需操作时，重新准备并审批新请求。",
    cancelled: "本次请求已取消。检查权限或配置变更；仍需操作时重新准备并审批。",
    failed: "先处理安全错误并核查请求状态，不要自动重试或更换幂等键。",
    unknown: "人工核查数据库结果，并使用原请求编号跟踪状态。不要自动重试或更换幂等键。"
  } satisfies Record<WriteOperationStatus, string>
};

type AuditWriteText = {
  [K in keyof typeof zhCN]: K extends "nextSteps" ? Record<WriteOperationStatus, string>
    : K extends "stageLabels" ? Record<keyof typeof zhCN.stageLabels, string> : string;
};

const en: AuditWriteText = {
  operation: "Operation", insert: "Insert · INSERT", update: "Update · UPDATE", delete: "Delete · DELETE",
  target: "Target table", requestId: "Request ID", attemptId: "Attempt ID", noRequest: "No write request was created",
  reasonCode: "Error code", stage: "Stage", stageUnavailable: "Exact stage was not recorded", revision: "Revision",
  updatedAt: "Last updated", nextStep: "Next step", outcome: "Execution outcome",
  outcomeUnknown: "The outcome is unknown. Verify manually; do not retry automatically or use a new idempotency key.",
  affectedRows: "Affected rows", notReported: "Not reported yet", search: "Search",
  searchPlaceholder: "Target table, error code, request ID or safe reason",
  submitTool: "Submit restricted write", statusTool: "Get write status",
  incomplete: "A complete write status was not recorded. Query the original request ID; do not infer success or resubmit.",
  stageLabels: {
    submission: "Submission validation", preparation: "Preview preparation", preparation_or_approval: "Preparation / awaiting approval",
    approval: "Local approval", execution: "Transaction execution", commit: "Transaction commit", rollback: "Transaction rollback", recovery: "Recovery verification"
  },
  nextSteps: {
    pending: "Review the target and changes in the local approval window. Nothing executes before approval. Wait or reject; do not resubmit.",
    executing: "Wait for execution to finish and query the original request ID. Do not resubmit.",
    committing: "Wait for the commit result and query the original request ID. Success is not yet confirmed; do not retry.",
    committed: "The transaction was confirmed committed. Use a readonly query if verification is needed; do not repeat the write.",
    rolled_back: "The transaction was confirmed rolled back. Resolve the cause, then prepare a new preview and approval.",
    rejected: "Execution was not authorized. Resolve the permission, configuration or request issue before requesting a new approval.",
    expired: "Approval expired and this request will not execute. Prepare and approve a new request if still needed.",
    cancelled: "This request was cancelled. Review permission or configuration changes before preparing a new approval.",
    failed: "Resolve the safe error and verify the request status. Do not retry automatically or switch idempotency keys.",
    unknown: "Verify the database outcome manually and track the original request ID. Do not retry automatically or switch idempotency keys."
  }
};

export const auditWriteText: Record<Locale, AuditWriteText> = { "zh-CN": zhCN, en };

/**
 * Reason copy keyed by the stable `reason_code` the backend records. The stored
 * `reason` sentence is only a fallback for codes added after this build, so the
 * audit trail no longer depends on matching English prose.
 */
const reasonCopy: Record<string, { "zh-CN": string; en: string }> = {
  // Submission gates and permissions.
  write_unavailable: { "zh-CN": "写入功能暂不可用。", en: "Write operations are unavailable." },
  write_denied: { "zh-CN": "当前请求未获得写入权限。", en: "Write permission is not available for this request." },
  write_disabled: { "zh-CN": "已关闭经过鉴权的写入提交。", en: "Authenticated write submission is disabled." },
  write_draining: { "zh-CN": "写入功能正在停止。", en: "Write operations are shutting down." },
  table_not_allowed: { "zh-CN": "目标表不在允许写入的白名单中。", en: "The requested table is not enabled for writes." },
  authorization_unavailable: { "zh-CN": "无法核实写入授权。", en: "Write authorization could not be verified." },
  authorization_changed: { "zh-CN": "写入授权或配置已变更。", en: "Write authorization or configuration changed." },
  configuration_busy: { "zh-CN": "配置正在变更。", en: "The configuration is currently being changed." },
  configuration_changed: { "zh-CN": "已审批的连接配置发生变更。", en: "The approved connection configuration changed." },
  idempotency_conflict: { "zh-CN": "当前幂等键已用于另一项请求。", en: "This idempotency key belongs to a different request." },
  pending_limit: { "zh-CN": "待处理写入请求已达到上限。", en: "The pending write request limit has been reached." },
  journal_full: { "zh-CN": "写入账本已满；已有幂等记录仍被保留。", en: "The write journal is full; existing idempotency records were retained." },
  journal_error: { "zh-CN": "写入账本无法持久化，请勿改用新幂等键重试。", en: "The write journal could not be persisted. Do not retry with a new idempotency key." },
  capacity_exceeded: { "zh-CN": "连接的执行并发已达到上限。", en: "The connection execution limit has been reached." },
  audit_unavailable: { "zh-CN": "请求失败，且审计记录未能保存。", en: "The request failed and its audit record could not be saved." },

  // Approval lifecycle.
  write_not_found: { "zh-CN": "未找到可访问的写入请求。", en: "The write request was not found." },
  stale_approval: { "zh-CN": "该审批版本已不再处于待处理状态。", en: "This approval revision is no longer pending." },
  approval_not_ready: { "zh-CN": "写入预览尚未准备好，不能审批。", en: "The write preview is not ready for approval." },
  approval_expired: { "zh-CN": "本机审批请求已过期。", en: "The local approval request expired." },
  approval_unavailable: { "zh-CN": "无法打开本机审批窗口。", en: "The local approval window could not be opened." },
  write_cancelled: { "zh-CN": "写入请求已取消。", en: "The write request was cancelled." },

  // Execution and recovery.
  prepare_timeout: { "zh-CN": "准备写入预览超时。", en: "Preparing the write preview timed out." },
  execution_timeout: { "zh-CN": "写入执行超时。", en: "The write execution timed out." },
  commit_timeout: { "zh-CN": "无法确认事务提交结果。", en: "The commit outcome could not be confirmed." },
  rollback_unknown: { "zh-CN": "无法确认事务回滚结果。", en: "The rollback outcome could not be confirmed." },
  outcome_unknown: { "zh-CN": "无法确认事务结果，请勿自动重试。", en: "The transaction outcome could not be confirmed. Do not retry automatically." },
  application_restarted: { "zh-CN": "操作完成前应用已重启。", en: "The application restarted before the operation was finalized." },
  stale_preview: { "zh-CN": "准备预览后目标记录或表结构发生变更，请重新准备预览。", en: "The row or table definition changed after the preview was prepared." },
  row_count_mismatch: { "zh-CN": "操作影响的行数不等于一行。", en: "The operation did not affect exactly one row." },
  invalid_write_request: { "zh-CN": "结构化写入请求不合法，请检查字段、标识符、值类型和完整主键。", en: "The structured write request is invalid. Check fields, identifiers, value types and the complete primary key." },
  request_too_large: { "zh-CN": "结构化写入请求超过大小限制。", en: "The structured write request exceeds the size limit." },
  internal_error: { "zh-CN": "无法安全完成写入操作。", en: "The write operation could not be completed safely." },
  write_failed: { "zh-CN": "写入失败，未暴露数据库内部错误。", en: "The write operation failed. Database details were not exposed." },

  // Target resolution, shared with the restricted-writes listing.
  target_changed: { "zh-CN": "已保存的目标标识不再匹配，请重新选择表。", en: "The stored target identity no longer matches; select the table again." },
  target_mismatch: { "zh-CN": "请求的表与已保存的目标标识不一致。", en: "The supplied table does not match the saved target identifier." },
  target_ambiguous: { "zh-CN": "表名匹配到多个对象，无法确定唯一表。", en: "The table name matches more than one object." },
  invalid_target: { "zh-CN": "目标标识无效，请先重新发现可写目标。", en: "The target identifier is invalid; rediscover write targets first." },
  identifier_rules_unknown: { "zh-CN": "无法核实数据库标识符的大小写规则。", en: "The database identifier case rules could not be verified." },
  unsupported_database: { "zh-CN": "当前连接不支持经过审批的受限写入。", en: "This connection does not support approved writes." },
  connection_options: { "zh-CN": "连接设置或已保存的凭据不可用。", en: "Connection settings or stored credentials are unavailable." },
  connection_unavailable: { "zh-CN": "连接当前不可用。", en: "The connection is currently unavailable." },
  metadata_timeout: { "zh-CN": "读取表元数据超时。", en: "Reading table metadata timed out." },
  metadata_unavailable: { "zh-CN": "无法读取表的元数据。", en: "Table metadata is unavailable." }
};

/** A table's capability reasons are already localized by the restricted-writes table. */
const capabilityReasons = new Set([
  "no_primary_key", "primary_key_nullable", "unsupported_column_type", "no_writable_column",
  "target_not_found", "not_a_base_table", "unsupported_engine", "unsupported_table_shape",
  "row_level_security", "temporary_table", "rewrite_rule"
]);

const nextStepsByCode: Record<string, { "zh-CN": string; en: string }> = {
  idempotency_conflict: {
    "zh-CN": "核对原请求及幂等键。查询原请求状态，不要改键绕过已有记录。",
    en: "Review the original request and idempotency key. Query its status; do not change keys to bypass the existing record."
  },
  journal_error: {
    "zh-CN": "先恢复写入账本存储并查询原请求状态。清空审计不能修复账本；请勿换键重试。",
    en: "Restore the write journal and query the original request status. Clearing audit history does not repair the journal; do not retry with a new key."
  },
  journal_full: {
    "zh-CN": "请管理员处理账本容量；清空审计不会删除幂等记录，也不会释放账本容量。",
    en: "Ask an administrator to resolve journal capacity. Clearing audit history neither removes idempotency records nor frees journal capacity."
  },
  audit_unavailable: {
    "zh-CN": "先恢复审计存储，再查询原请求状态。不要将未记录结果视为成功或换键重试。",
    en: "Restore audit storage, then query the original request status. Missing history is not success; do not retry with a new key."
  },
  unsupported_database: {
    "zh-CN": "按原因检查连接、表结构及驱动能力；不要绕过安全限制或直接改用写入 SQL。",
    en: "Review the connection, table definition and driver capability described above. Do not bypass safeguards or switch to arbitrary write SQL."
  },
  pending_limit: {
    "zh-CN": "先处理现有待审批请求；使用已有请求编号查询状态，不要重复提交。",
    en: "Resolve existing pending approvals first. Query existing request IDs rather than resubmitting."
  },
  invalid_write_request: {
    "zh-CN": "按结构化请求约定检查字段、标识符、值类型和完整主键；修正后再提交审批。",
    en: "Check structured fields, identifiers, value types and the complete primary key before submitting for approval again."
  },
  request_too_large: {
    "zh-CN": "将请求缩减到允许大小；不要拆分为未经逐项审批的批量写入。",
    en: "Reduce the request to the allowed size. Do not replace it with bulk writes that bypass per-request approval."
  }
};

/** One short line for inline lists: the code's copy, else the backend sentence. */
export function auditReasonText(locale: Locale, event: Pick<AuditEvent, "reason" | "reason_code">) {
  const code = event.reason_code;
  if (code && capabilityReasons.has(code)) return writeReasonText(locale, { reason_code: code });
  const copy = code ? reasonCopy[code] : undefined;
  if (copy) return copy[locale];
  return event.reason ?? code ?? null;
}

export function auditWriteStatusLabel(locale: Locale, status: string) {
  return (writeMessages[locale].statusLabels as Record<string, string>)[status] ?? status;
}

export function auditWriteStageLabel(locale: Locale, stage?: string | null) {
  return stage ? (auditWriteText[locale].stageLabels as Record<string, string>)[stage] ?? stage : auditWriteText[locale].stageUnavailable;
}

export function auditWriteNextStep(locale: Locale, event: AuditEvent) {
  const text = auditWriteText[locale];
  if (event.outcome_unknown || event.write_status === "unknown") return text.nextSteps.unknown;
  if (event.write_status === "committed") return text.nextSteps.committed;
  if (event.reason_code && nextStepsByCode[event.reason_code]) return nextStepsByCode[event.reason_code][locale];
  return (text.nextSteps as Record<string, string>)[event.write_status ?? ""] ?? text.incomplete;
}
