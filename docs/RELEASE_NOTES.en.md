# DataNexa v0.8.0

[中文发布说明](https://github.com/MingoZacwu/DataNexa/blob/v0.8.0/RELEASE_NOTES.md)

## Highlights

- Added JDBC support (technical preview): install any JDBC driver via its Maven coordinate to connect to more enterprise and regional databases.
- Added a "Security" settings page with blocked-request statistics and an access token auto circuit breaker.
- Introduced the DataNexa managed Java runtime so JDBC works out of the box; an existing external Java runtime can be used instead.
- **Note**: audit log retention changed from a maximum event count to a retention window in days. See [Changes and Improvements](#changes-and-improvements) below.

## Added

### JDBC Support (Technical Preview)

- JDBC operations run in an isolated Java sidecar process; SQLite, MySQL, and PostgreSQL connections keep using their native backends and are unaffected.
- New "Driver Management" settings page:
  - Install any JDBC driver and its transitive dependencies from a Maven coordinate, with no database brand allowlist; a Maven mirror can be configured.
  - Downloaded drivers are verified by size and SHA-256 and stored as immutable, versioned Driver Bundles that can coexist, be updated, and be removed.
  - Local JAR import is supported, along with Maven download cache inspection and clearing.
- New "JDBC" connection type: pick an installed driver and enter a JDBC URL to create a connection, with connection testing, diagnostics, and connection import/export.
- JDBC queries go through the existing read-only policy, access control, auditing, timeouts, and result limits; statements in unknown dialects or that cannot be confirmed safe are rejected.
- JDBC credentials and URLs are sanitized in error messages and diagnostic output.

### DataNexa Managed Java Runtime

- Download the DataNexa managed Java runtime with one click from the settings page; integrity is verified automatically after download, so Java does not need to be installed separately.
- An existing external Java runtime on your machine can be selected in the settings instead.
- New managed runtime versions are detected automatically, and update reminders appear as a sidebar card.

### Security Settings Page

- New "Security" settings page showing blocked-request statistics for the last 24 hours, a defense-in-depth overview, and warnings based on the Bearer authentication state.
- Access token auto circuit breaker: with Bearer authentication enabled, a token is automatically disabled once its denied requests reach the configured threshold within the counting window. Tripped and manually re-enabled tokens are recorded in the audit log, and a re-enabled token starts with a fresh counting window.

### Storage and Performance

- New "Storage and Performance" settings page showing local data usage and actual storage paths, broken down by category such as the Java runtime, JDBC drivers, Maven repository, and audit logs.
- Cache items that are safe to delete (such as the Maven download cache and legacy runtimes) can be cleared in one click to free up space; installed drivers, audit logs, and configuration are not affected.
- The DataNexa data directory can be opened directly in the system file manager.

## Changes and Improvements

- **Audit log retention change**: audit log cleanup switched from a maximum event count to a retention window in days, with options of 3/7/15/30 days (7 days by default). After upgrading to this version, audit records older than the retention window are cleaned up automatically. **If you need to keep audit records for longer, back up your logs before upgrading and choose a longer retention window in the settings.**
- Connection import is more robust: invalid or unavailable entries in the import file are skipped, and the import result is reported clearly without affecting the remaining entries.
- Application and Java runtime update checks are cached to avoid duplicate requests.

## Fixed

- Improved compatibility with Claude Desktop and legacy MCP clients: stricter tool input schemas, support for MCP protocol version 2025-03-26 and `structuredContent` responses, and tolerance for legacy clients that omit some protocol headers.
- Fixed scroll edge fades and scroll bars staying visible when content fits without scrolling; they now appear only when the content is scrollable.

## Upgrade Notes and Feedback

- In this version update, there are many changes. We could not fully test every feature and usage combination, and JDBC support could not be verified against every database and driver version. If you run into problems after upgrading, please report them on [GitHub Issues](https://github.com/MingoZacwu/DataNexa/issues) with steps to reproduce and details about your environment (operating system, database type and version, and driver name and version).
- To help us diagnose problems faster: on the Settings > About page, tap the version number area 5 times to reveal the hidden debug logging option. Enable it, reproduce the problem, then open the log folder from the settings to collect the log file. Log content is sanitized and never records plaintext credentials; attaching it to your issue can help us locate the problem faster.

## Installation Notes

- The macOS version requires macOS 15.0 or later.

- The macOS application is Developer ID signed but is not yet notarized. macOS may display a Gatekeeper warning on first launch. You can remove the warning by running the following command in Terminal:

```shell
sudo xattr -d com.apple.quarantine /Applications/DataNexa.app
```

- The Windows installer is not Authenticode signed yet and may display a Microsoft Defender SmartScreen warning.
