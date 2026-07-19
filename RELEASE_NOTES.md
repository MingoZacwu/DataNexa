# DataNexa v0.4.0

[English Release Notes](https://github.com/MingoZacwu/DataNexa/blob/v0.4.0/docs/RELEASE_NOTES.en.md)

## 版本亮点

- 新增登录时自动启动 DataNexa 和 MCP 的选项，无需每次登录后手动开启服务。
- 改进应用启动流程和 MCP 启动错误反馈，后台运行状态更加清晰可靠。

## 新增功能

- 在 macOS 和 Windows 上新增“登录时启动 DataNexa 并自动开启 MCP”设置。
- 通过登录启动项打开时，DataNexa 会在后台启动 MCP，并继续通过系统托盘提供控制入口。

## 调整与改进

- 应用主窗口会在启动初始化完成后再显示，避免启动过程中出现空白窗口或意外闪现。
- MCP 自动或手动启动失败时，会在应用和系统托盘中显示反馈，并将失败记录到审计日志。
- Windows 卸载程序现在会清理 DataNexa 的登录启动项，避免卸载后残留启动配置。

## 安装与使用说明

- macOS 版本要求 macOS 15.0 或更高版本。

- macOS 应用使用 Developer ID 签名，但尚未进行 Apple 公证。首次启动时，macOS 可能显示 Gatekeeper 安全提示。您可以在终端中执行以下命令来解除警告：

```shell
sudo xattr -d com.apple.quarantine /Applications/DataNexa.app
```

- Windows 安装包暂未进行 Authenticode 代码签名，Microsoft Defender SmartScreen 可能显示安全提示。
