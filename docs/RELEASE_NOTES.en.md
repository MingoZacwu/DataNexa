# DataNexa v0.8.1

[中文发布说明](https://github.com/MingoZacwu/DataNexa/blob/v0.8.1/RELEASE_NOTES.md)

## Highlights

- Connection exports can now be protected with an encryption password: exported connection files are encrypted by default, and only someone who knows the password can read the database connections inside.
- The "Storage & performance" settings page now includes live resource monitoring, covering DataNexa itself as well as JDBC drivers.

## Added

### Encrypted Connection Exports

- Exporting connections now uses an encryption password by default: with a password of at least 8 characters, only someone who knows it can read the database connections in the file (the password is never stored, and the file cannot be recovered without it). "Export without encryption" is still available — it still requires acknowledging the plaintext warning — and produces the same file as previous versions.
- Encrypted files use the `.dnxc` extension, and import detects the file type from its contents rather than the extension. Importing an encrypted file asks for the password first and shows when the file was exported.
- A wrong password or a damaged file imports nothing, so the password can simply be retyped; plaintext connection files exported by earlier versions still import directly.

### Live Resource Monitoring

- The "Storage & performance" page now has a Resources view alongside the existing Storage view.
- Resources covers both DataNexa itself (the main process and its WebView/GPU children) and JDBC driver runtimes, listing CPU, memory, and process count, with a PID for each of DataNexa's own processes.
- The CPU area shows total usage and a live trend chart, labelled with the DataNexa and JDBC driver shares; the memory area shows the breakdown of DataNexa, JDBC drivers, and other system usage, with used memory against total system memory (hover for available memory).
- Individual JDBC driver runtimes can be stopped, with resource usage refreshing immediately.

## Changes and Improvements

- macOS builds now use Xcode 26.3 (macOS 26.2 SDK) to follow macOS 27's redesigned window controls (the "traffic lights"). The minimum requirement is still macOS 15.0.

## Upgrade Notes and Feedback

- Encrypted `.dnxc` files cannot be imported by 0.8.0 or earlier (older builds do not recognise the format and the import fails), so upgrade both sides to 0.8.1 or later before migrating connections with an encrypted file. Plaintext connection files exported by earlier versions are unaffected.
- If you run into problems after upgrading, please report them on [GitHub Issues](https://github.com/MingoZacwu/DataNexa/issues) with steps to reproduce and details about your environment.
- To help us diagnose problems faster: on the Settings > About page, tap the version number area 5 times to reveal the hidden debug logging option. Enable it, reproduce the problem, then open the log folder from the settings to collect the log file. Log content is sanitized and never records plaintext credentials; attaching it to your issue can help us locate the problem faster.

## Installation Notes

- The macOS version requires macOS 15.0 or later.

- The macOS application is Developer ID signed but is not yet notarized. macOS may display a Gatekeeper warning on first launch. You can remove the warning by running the following command in Terminal:

```shell
sudo xattr -d com.apple.quarantine /Applications/DataNexa.app
```

- The Windows installer is not Authenticode signed yet and may display a Microsoft Defender SmartScreen warning.
