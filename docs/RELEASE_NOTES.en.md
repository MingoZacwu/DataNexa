# DataNexa v0.7.1

[中文发布说明](https://github.com/MingoZacwu/DataNexa/blob/v0.7.1/RELEASE_NOTES.md)

## Highlights

- Bearer authentication settings now explain the security impact before they are disabled, reducing the risk of accidental exposure.

## Added

- Added a confirmation dialog when turning off Bearer authentication, explaining that the endpoint will lose token protection and access controls will no longer be available.

## Changes and Improvements

- If saving a Bearer authentication change fails, the previous setting is restored so the interface stays consistent with the actual configuration.
- Improved dialog backdrop rendering on Windows.

## Installation Notes

- The macOS version requires macOS 15.0 or later.

- The macOS application is Developer ID signed but is not yet notarized. macOS may display a Gatekeeper warning on first launch. You can remove the warning by running the following command in Terminal:

```shell
sudo xattr -d com.apple.quarantine /Applications/DataNexa.app
```

- The Windows installer is not Authenticode signed yet and may display a Microsoft Defender SmartScreen warning.
