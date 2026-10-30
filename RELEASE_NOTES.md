# DataNexa v0.8.3

[English Release Notes](https://github.com/MingoZacwu/DataNexa/blob/v0.8.3/docs/RELEASE_NOTES.en.md)

## 版本亮点

- MCP 服务现在可在启动 DataNexa 时自动运行，并新增后台运行提示，方便了解关闭窗口后服务仍可继续运行。
- 新增可选的 GitHub 下载加速，帮助网络连接不稳定的用户检查更新、下载应用安装包和 Java 运行时。
- 新增“帮助与反馈”页面，可直接打开在线使用手册或前往 GitHub 提交问题与建议。

## 新增功能

### MCP 自动启动与后台运行

- 新增“启动 DataNexa 时自动开启 MCP 服务”设置。启用后，每次启动应用都会自动启动 MCP 服务；开启“登录时启动 DataNexa”后，MCP 服务也会始终随应用自动启动。
- 首次启动 MCP 服务后，概览页会提示 DataNexa 可以在关闭窗口后继续于后台运行，并提供开启开机自启动的快捷操作。提示只显示一次。

### GitHub 下载加速

- 设置中新增可选的 GH-Proxy 加速开关，用于应用更新检查、安装包下载和托管 Java 运行时下载。
- 加速请求失败时会自动改用 GitHub 直连。下载的应用安装包和 Java 运行时仍会执行签名或 SHA-256 校验。

### 帮助与反馈

- 侧边栏新增“帮助与反馈”页面，可打开在线使用手册或 GitHub 问题反馈入口。
- macOS 菜单栏菜单现已本地化，并增加打开帮助文档的入口。

## 调整与问题修复

- 修复更新设置中的开关会响应到其他开关状态的问题；现在每个开关只控制自身选项。

## 提交反馈

- 如果升级后遇到问题，欢迎在 [GitHub Issues](https://github.com/MingoZacwu/DataNexa/issues) 提交反馈，并尽量附上问题的复现步骤与环境信息。
- 如需协助定位问题，可以在“设置 → 关于”页面连续点按版本号区域 5 次，打开隐藏的调试日志选项；开启后复现问题，再在设置中打开日志文件夹获取日志文件。日志内容已经过脱敏处理，不会记录明文凭据，随 Issue 一并提交可以帮助我们更快定位问题。

## 安装与使用说明

- macOS 版本要求 macOS 15.0 或更高版本。

- macOS 应用使用 Developer ID 签名，但尚未进行 Apple 公证。首次启动时，macOS 可能显示 Gatekeeper 安全提示。您可以在终端中执行以下命令来解除警告：

```shell
sudo xattr -d com.apple.quarantine /Applications/DataNexa.app
```

- Windows 安装包暂未进行 Authenticode 代码签名，Microsoft Defender SmartScreen 可能显示安全提示。
