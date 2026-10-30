# DataNexa v0.8.3

[中文发布说明](https://github.com/MingoZacwu/DataNexa/blob/v0.8.3/RELEASE_NOTES.md)

## Highlights

- MCP can now start automatically with DataNexa, with a new background-running tip explaining that the service can continue after the window is closed.
- Added optional GitHub download acceleration for users with unreliable connections when checking for updates or downloading the app and Java runtime.
- Added a Help & Feedback page with direct links to the online manual and GitHub issue reporting.

## Added

### Automatic MCP Startup and Background Running

- Added a setting to start the MCP server whenever DataNexa launches. When “Launch DataNexa at login” is enabled, the MCP server starts automatically as well.
- After the MCP server starts for the first time, the Overview page shows a one-time tip explaining that DataNexa can keep running in the background after the window closes, with a shortcut to enable launch at login.

### GitHub Download Acceleration

- Added an optional GH-Proxy setting for app update checks, installer downloads, and managed Java runtime downloads.
- If an accelerated request fails, DataNexa automatically retries directly with GitHub. App installers and Java runtimes remain subject to signature or SHA-256 verification.

### Help & Feedback

- Added a Help & Feedback page in the sidebar, with links to the online manual and GitHub issue reporting.
- Localized the macOS menu bar and added an entry to open the help documentation.

## Changes and Fixes

- Fixed update settings switches affecting the state of other switches. Each switch now controls only its own setting.

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
