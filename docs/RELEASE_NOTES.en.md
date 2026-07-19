# DataNexa v0.4.0

[中文发布说明](https://github.com/MingoZacwu/DataNexa/blob/v0.4.0/RELEASE_NOTES.md)

## Highlights

- DataNexa and MCP can now start automatically at login, eliminating the need to start the service manually after each sign-in.
- Application startup and MCP error reporting are now more reliable and easier to understand when running in the background.

## Added

- Added a setting to start DataNexa and MCP at login on macOS and Windows.
- When launched as a login item, DataNexa starts MCP in the background and remains accessible from the system tray.

## Changes and Improvements

- The main window now appears only after startup initialization completes, preventing a blank window or unintended flash during launch.
- MCP startup failures, whether automatic or manual, are now surfaced in the app and system tray and recorded in the audit log.
- The Windows uninstaller now removes DataNexa's login startup entry to prevent stale startup configuration after uninstalling.

## Installation Notes

- The macOS version requires macOS 15.0 or later.

- The macOS application is Developer ID signed but is not yet notarized. macOS may display a Gatekeeper warning on first launch. You can remove the warning by running the following command in Terminal:

```shell
sudo xattr -d com.apple.quarantine /Applications/DataNexa.app
```

- The Windows installer is not Authenticode signed yet and may display a Microsoft Defender SmartScreen warning.
