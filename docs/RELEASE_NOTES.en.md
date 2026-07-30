# DataNexa v0.6.0

[中文发布说明](https://github.com/MingoZacwu/DataNexa/blob/v0.6.0/RELEASE_NOTES.md)

## Highlights

- Upgraded emergency disconnect to immediately cancel in-flight requests, block new tool calls, and close active database pools.
- Added a lightweight mode that releases window UI resources while keeping background services running.
- Improved MySQL schema selection and result-type handling for more accurate cross-schema queries and boolean, string, and binary values.

## Added

- The overview now clearly indicates when emergency disconnect is active.
- The overview now shows MCP call volume for the last 24 hours and indicates when the audit-retention limit may make the count incomplete.
- Added an “Energy Pulse” interface effect that provides dynamic feedback on the overview when an MCP tool is called.
- Added an automatic lightweight-mode setting that releases window UI resources five minutes after the main window is closed. Lightweight mode can also be entered or exited manually from the system tray.

## Changes and Improvements

- MySQL metadata lookups and table queries now use the requested schema, falling back to the database configured for the connection when no schema is provided.
- Improved MySQL cell decoding so ordinary numeric values are not mistaken for booleans, text types are handled as strings first, and byte data is returned as binary values.
- Connection tests no longer write audit-log entries and are no longer blocked while audit-log migration is pending.

## Installation Notes

- The macOS version requires macOS 15.0 or later.

- The macOS application is Developer ID signed but is not yet notarized. macOS may display a Gatekeeper warning on first launch. You can remove the warning by running the following command in Terminal:

```shell
sudo xattr -d com.apple.quarantine /Applications/DataNexa.app
```

- The Windows installer is not Authenticode signed yet and may display a Microsoft Defender SmartScreen warning.
