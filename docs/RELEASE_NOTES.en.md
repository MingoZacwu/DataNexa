# DataNexa v0.7.0

[中文发布说明](https://github.com/MingoZacwu/DataNexa/blob/v0.7.0/RELEASE_NOTES.md)

## Highlights

- Added fine-grained token-based access control with separate tokens and least-privilege permissions for individual agents.
- Audit logs now identify the request source and support filtering by access token, making data access easier to trace and manage.

## Added

- Added access-token creation, enable/disable, rename, rotation, and deletion, with separate database-connection and MCP-tool permissions for each token.
- Added Agent connection configuration generation for a selected token from the access-control interface. New tokens can use all currently enabled database connections and MCP tools by default.
- Audit logs now show the token name or other request source and retain historical attribution for deleted tokens.
- Access-token permissions are enforced by the backend, so unauthorized tool calls or database access are rejected.

## Changes and Improvements

- Replaced the former server-token page with the access-control interface and added an access-token filter to the audit log.

### Upgrade Notes

- Existing server access secrets are migrated to a default access token during upgrade, allowing existing authenticated setups to continue working.

## Installation Notes

- The macOS version requires macOS 15.0 or later.

- The macOS application is Developer ID signed but is not yet notarized. macOS may display a Gatekeeper warning on first launch. You can remove the warning by running the following command in Terminal:

```shell
sudo xattr -d com.apple.quarantine /Applications/DataNexa.app
```

- The Windows installer is not Authenticode signed yet and may display a Microsoft Defender SmartScreen warning.
