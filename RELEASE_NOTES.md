# DataNexa v0.5.0

[English Release Notes](https://github.com/MingoZacwu/DataNexa/blob/v0.5.0/docs/RELEASE_NOTES.en.md)

## 版本亮点

- 审计日志改用 SQLite 存储，旧版日志可在首次启动时自动迁移，并提供迁移失败后的恢复操作。
- 完善 Streamable HTTP MCP 协议兼容行为，并增加请求校验、调用速率、并发和响应大小限制。
- MCP 服务运行状态和启动失败原因会在应用界面与系统托盘中明确显示。

## 新增功能

- 为每个连接新增最大结果大小设置，范围为 64 KiB 至 8 MiB。
- 系统托盘新增 MCP 服务运行中、已停止和启动失败三种状态图标与提示。

## 调整与改进

- MCP 请求现在校验本地 Host、Origin、Content-Type、Accept 和协议版本，并限制工具调用速率及并发数。
- 查询结果同时受最大返回行数和连接级结果大小限制；达到限制时返回截断状态、截断原因和实际返回字节数。
- 设置保存过程中会保留正在编辑的内容；服务器端口或审计日志保留数量保存失败时，会恢复为有效值并刷新界面。
- MCP 手动启动失败会保留错误信息，并在侧边栏、概览和服务器页面中显示。
- 简化桌面窗口界面，并修复 macOS 窗口无法拖动的问题。

## 升级前须知

如果您从旧版本升级至 DataNexa v0.5.0，应用将在首次启动时自动将原有审计日志迁移至 SQLite。大多数情况下迁移会很快完成；日志数量较多时，界面将显示迁移进度。

为确保所有数据库操作均可审计，迁移完成前 MCP 服务和连接测试将暂不可用。如果迁移失败，您可以在应用中重试；如不需要保留旧日志，也可以将其隔离后继续使用 DataNexa。

如遇到问题，欢迎提交 [GitHub Issue](https://github.com/MingoZacwu/DataNexa/issues) 向我们反馈。

## 安装与使用说明

- macOS 版本要求 macOS 15.0 或更高版本。

- macOS 应用使用 Developer ID 签名，但尚未进行 Apple 公证。首次启动时，macOS 可能显示 Gatekeeper 安全提示。您可以在终端中执行以下命令来解除警告：

```shell
sudo xattr -d com.apple.quarantine /Applications/DataNexa.app
```

- Windows 安装包暂未进行 Authenticode 代码签名，Microsoft Defender SmartScreen 可能显示安全提示。
