# DataNexa v0.8.2

[中文发布说明](https://github.com/MingoZacwu/DataNexa/blob/v0.8.2/RELEASE_NOTES.md)

## Highlights

- Added a language switcher to the top bar of the overview page, so the language setting is easier to find and switching languages is quicker.
- The driver list shows what each JDBC driver supports natively, so you can see whether read-only mode and query timeouts are available.
- Added quick copying of connection names: click the name in the connection list or the connection details to copy it.
- This update also includes a number of bug fixes and smaller improvements.

## Added

### Language Switcher

- A language button now sits in the top bar of the overview page. Each language is listed under its own name and switching takes effect immediately. The language option under Settings > General is unchanged.
- Chinese systems use Chinese; every other language defaults to English. A default language is no longer written on first launch — only a choice you make yourself is saved.

### Quick Copy for Connection Names

- The connection name in the connection list or the connection details can be clicked to copy ([Issue #14](https://github.com/MingoZacwu/DataNexa/issues/14)).

### JDBC Driver Capabilities

- The driver list reports what each driver supports natively, including read-only mode and query timeouts.
- Run Test Connection on a connection that uses the driver, or issue a query over MCP, and the result is recorded automatically.
- When a driver does not support them, DataNexa still applies read-only protection and a timeout to every query.

## Changes and Improvements

- English text and numbers now use the bundled Geist font, while Chinese keeps using the system font. Font sizes and weights have also been unified, so bold Chinese text renders more cleanly.
- Popovers, tooltips, and toasts share one frosted-glass treatment, icons and disabled states in resource lists follow a consistent style, and enabling or disabling a resource is now animated.
- Improved compatibility with MCP clients, so results are less likely to be misread.

## Fixed

### MySQL Connections

- MySQL time columns could not be read; they now return their values, in UTC.
- A query that timed out left its connection unusable for a long time. The connection is now released as soon as the query times out, so later operations are unaffected.
- Empty results no longer lose their field information, and some string values no longer appear as base64 text.

### JDBC Queries

- JDBC statements that cannot be parsed are now rejected, so they cannot bypass read-only protection.
- Cancelling a query now terminates the database session correctly.
- Fixed string values being converted incorrectly, and cells being truncated without any indication.
- Database errors are redacted consistently in audit records and debug logs.

### Interface and Resource Monitoring

- A single request in progress no longer makes a large part of the interface unavailable; only the features involved are affected.
- Switching settings tabs no longer leaves the new tab at the previous tab's scroll position.
- JDBC driver CPU usage no longer reports more than 100%; it is now measured the same way Task Manager measures it.

## Feedback

- If you run into problems after upgrading, please report them on [GitHub Issues](https://github.com/MingoZacwu/DataNexa/issues) with steps to reproduce and details about your environment.
- To help us diagnose problems faster: on the Settings > About page, tap the version number area 5 times to reveal the hidden debug logging option. Enable it, reproduce the problem, then open the log folder from the settings to collect the log file. Log content is sanitized and never records plaintext credentials; attaching it to your issue can help us locate the problem faster.

## Installation Notes

- The macOS version requires macOS 15.0 or later.

- The macOS application is Developer ID signed but is not yet notarized. macOS may display a Gatekeeper warning on first launch. You can remove the warning by running the following command in Terminal:

```shell
sudo xattr -d com.apple.quarantine /Applications/DataNexa.app
```

- The Windows installer is not Authenticode signed yet and may display a Microsoft Defender SmartScreen warning.
