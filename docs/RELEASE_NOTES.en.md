# DataNexa v0.3.0

[中文发布说明](https://github.com/MingoZacwu/DataNexa/blob/v0.3.0/RELEASE_NOTES.md)

## Highlights

- Welcome to DataNexa! This is the first public release of DataNexa, a local read-only database MCP gateway designed to give AI agents a unified, controlled, and auditable way to access structured data.
- DataNexa keeps database access on your machine and combines connection management, read-only SQL enforcement, credential protection, MCP tool controls, and audit logging in one desktop application.
- DataNexa is available for Windows and macOS Universal.

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
- Import and export of DataNexa connection profiles for migration between installations.
- A desktop management interface with overview metrics, connection management, MCP server controls, tool permissions, audit inspection, and an interactive SQL policy console.
- Simplified Chinese and English interfaces, system/light/dark themes, and system tray controls for showing DataNexa or starting and stopping the MCP server.

## Changes and Improvements

- Simplified the update prompt so a click takes you directly to the "About Updates" page.
- Removed the standalone update confirmation dialog.
- Improved the update status copy and progress bar layout.

## Installation and Upgrade Notes

- The macOS application is Developer ID signed but is not yet notarized. macOS may display a Gatekeeper warning on first launch. You can remove the warning by running the following command in Terminal:

```shell
sudo xattr -d com.apple.quarantine /Applications/DataNexa.app
```

- The Windows installer is not Authenticode signed yet and may display a Microsoft Defender SmartScreen warning.

## Usage Notes

- Data is invaluable. Use with caution.
- The read-only policy cannot guarantee that every risky operation will be blocked. You should still actively constrain the Agent and avoid asking or allowing it to perform dangerous database operations.
- DataNexa is still under active development. Application quality and stability are not guaranteed at this stage. Production use is not recommended. If you must use it in production, evaluate the risks carefully beforehand.
