# DataNexa v0.4.1

[中文发布说明](https://github.com/MingoZacwu/DataNexa/blob/v0.4.1/RELEASE_NOTES.md)

## Highlights

- Fixed an issue that could prevent daily automatic update checks from working.
- On Windows, enabled login startup is restored after reinstalling or upgrading, so the app continues to start as configured.

## Fixed

- Fixed automatic update checks potentially failing after the app moved to the background, the frontend reloaded, or the app restarted.
- On Windows startup, the login startup entry is synchronized with the saved preference, fixing auto-start failures caused by reinstall flows.

## Installation Notes

- The macOS version requires macOS 15.0 or later.

- The macOS application is Developer ID signed but is not yet notarized. macOS may display a Gatekeeper warning on first launch. You can remove the warning by running the following command in Terminal:

```shell
sudo xattr -d com.apple.quarantine /Applications/DataNexa.app
```

- The Windows installer is not Authenticode signed yet and may display a Microsoft Defender SmartScreen warning.
