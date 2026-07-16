# DataNexa v0.3.1

[中文发布说明](https://github.com/MingoZacwu/DataNexa/blob/v0.3.1/RELEASE_NOTES.md)

## Highlights

- Audit logs can now be filtered by date range, MCP tool, database connection, and execution status.
- Improved desktop window startup and relaunch behavior.

## Added

- Added an audit log filter panel with combined filters for start date, end date, tool, connection, and status.
- Added a date picker and date-range validation to prevent the end date from preceding the start date.
- Added an active filter indicator and a one-click action to clear all filters.
- Added page number shortcuts to audit log pagination and automatic scrolling to the top when changing pages.

## Changes and Improvements

- The main application window is now centered on first launch.
- Relaunching DataNexa now restores and focuses the existing main window instead of creating a duplicate instance.
- Moved the automatic update check setting into the About Updates section to keep update-related controls together.

## Installation Notes

- The macOS application is Developer ID signed but is not yet notarized. macOS may display a Gatekeeper warning on first launch. You can remove the warning by running the following command in Terminal:

```shell
sudo xattr -d com.apple.quarantine /Applications/DataNexa.app
```

- The Windows installer is not Authenticode signed yet and may display a Microsoft Defender SmartScreen warning.