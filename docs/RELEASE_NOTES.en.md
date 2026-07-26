# DataNexa v0.5.0

[中文发布说明](https://github.com/MingoZacwu/DataNexa/blob/v0.5.0/RELEASE_NOTES.md)

## Highlights

- Audit logs now use SQLite. Legacy logs are migrated on first launch, with recovery options available if migration fails.
- Streamable HTTP MCP compatibility has been improved with request validation, rate limiting, concurrency controls, and response-size limits.
- MCP status and startup failures are clearly surfaced in the app and system tray.

## Added

- A per-connection maximum result size setting, from 64 KiB to 8 MiB.
- Distinct system-tray icons and tooltips for a running, stopped, or failed MCP server.

## Changes and Improvements

- MCP requests now validate the local Host, Origin, Content-Type, Accept, and protocol-version headers, and tool calls are subject to rate and concurrency limits.
- Query results are limited by both maximum rows and the connection-level result-size limit; truncated responses include the truncation reason and returned byte count.
- In-progress settings edits are preserved while saves complete. Failed server-port or audit-retention saves restore valid values and refresh the interface.
- Manual MCP startup failures are retained and shown in the sidebar, overview, and server views.
- Optimized the desktop window interface by removing the title bars for a cleaner, more consistent look.

## Upgrade Notes

When upgrading from an earlier version to DataNexa v0.5.0, the application automatically migrates existing audit logs to SQLite on first launch. The migration usually completes quickly; if there are many log entries, progress is shown in the interface.

To ensure that all database operations remain auditable, the MCP server and connection tests are temporarily unavailable until migration completes. If migration fails, you can retry it in the application. If you do not need to retain the legacy logs, you can quarantine them and continue using DataNexa.

If you encounter any problems, please report them through a [GitHub Issue](https://github.com/MingoZacwu/DataNexa/issues).

## Installation Notes

- The macOS version requires macOS 15.0 or later.

- The macOS application is Developer ID signed but is not yet notarized. macOS may display a Gatekeeper warning on first launch. You can remove the warning by running the following command in Terminal:

```shell
sudo xattr -d com.apple.quarantine /Applications/DataNexa.app
```

- The Windows installer is not Authenticode signed yet and may display a Microsoft Defender SmartScreen warning.
