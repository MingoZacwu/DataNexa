# DataNexa v0.2.1-rc

[中文发布说明](https://github.com/MingoZacwu/DataNexa/blob/v0.2.1-rc/RELEASE_NOTES.md)

## Highlights

- This is the first release candidate of DataNexa, a local read-only database MCP gateway designed to give AI agents a unified, controlled, and auditable way to access structured data.
- DataNexa keeps database access on your machine and combines connection management, read-only SQL enforcement, credential protection, MCP tool controls, and audit logging in one desktop application.
- This release includes a Windows NSIS installer and a Universal macOS application for both Apple Silicon and Intel Macs.

## Added

- SQLite, MySQL, and PostgreSQL connection management with connection testing, diagnostics, per-connection enable/disable controls, and an emergency disconnect action.
- Secure credential storage through the operating system credential vault. Database passwords are kept out of the regular application configuration.
- A local MCP server with optional Bearer token authentication, token rotation, configurable loopback host and port, and quick-copy agent connection information.
- Seven independently configurable MCP tools:
  - List enabled database connections without exposing passwords or complete DSNs.
  - Discover tables and views.
  - Inspect table column metadata.
  - Read bounded sample rows.
  - Execute a single read-only SQL query.
  - Run `EXPLAIN` for read-only SQL.
  - Validate SQL against the DataNexa policy without executing it.
- SQL policy enforcement based on parsed SQL syntax, including single-statement validation, blocking of destructive or side-effecting operations, and automatic result-size limits.
- Query safeguards for maximum rows, execution timeout, and connection pool size, configurable per database connection.
- Persistent audit logs covering allowed, denied, failed, timed-out, and truncated operations, with execution time, row count, connection, tool, and failure-reason details.
- Optional SQL literal redaction for audit records. Query result data is not written to the audit log.
- Import and export of DataNexa connection profiles for migration between installations, with explicit warnings when exported files contain plaintext credentials.
- A desktop management interface with overview metrics, connection management, MCP server controls, tool permissions, audit inspection, and an interactive SQL policy console.
- Simplified Chinese and English interfaces, system/light/dark themes, and system tray controls for showing DataNexa or starting and stopping the MCP server.
- Tauri updater runtime integration and signed updater artifact generation for future in-app update delivery.

## Changed

- Established the initial DataNexa application configuration, local data storage, security policy, and MCP tool contract for the first public release line.
- Standardized Windows and macOS release packaging through the automated GitHub Release workflow.

## Fixed

- This is the first public release candidate, so there are no changes relative to an earlier public version.

## Installation and Upgrade Notes

- This is a release candidate intended for validating installation, database compatibility, MCP integration, packaging, and the automated release pipeline before the stable release.
- The updater backend and signed update artifacts are included, but the frontend update prompt and installation flow are not exposed in this release candidate yet.
- The macOS application is Developer ID signed but is not yet notarized. macOS may display a Gatekeeper warning on first launch.
- The Windows installer is not Authenticode signed yet and may display a Microsoft Defender SmartScreen warning.
- Connection export files can contain database passwords in plaintext. Store them only in a trusted location and delete them immediately after migration.
