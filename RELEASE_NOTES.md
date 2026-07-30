# DataNexa v0.6.0

[English Release Notes](https://github.com/MingoZacwu/DataNexa/blob/v0.6.0/docs/RELEASE_NOTES.en.md)

## 版本亮点

- 全面升级紧急断连功能，可立即取消进行中的请求、阻止新工具调用并关闭现有数据库连接池。
- 新增轻量模式，可在主窗口关闭后释放界面资源，同时保持后台服务运行。
- 改进 MySQL 架构选择与结果类型处理，提高跨架构查询以及布尔、字符串和二进制值的返回准确性。

## 新增功能

- 激活紧急断连后，概览页会明确显示当前状态。
- 概览页新增近 24 小时 MCP 调用次数；审计日志保留上限可能导致计数不完整时，会显示提示。
- 新增“能量脉冲”界面动效，MCP 工具被调用时会在概览页显示动态反馈。
- 新增自动轻量模式设置，关闭主窗口 5 分钟后自动释放界面资源；也可通过系统托盘手动进入或退出轻量模式。

## 调整与改进

- MySQL 元数据读取和表查询会使用请求中指定的架构，未指定时使用连接配置的数据库。
- 改进 MySQL 单元格解码，避免将普通数值误判为布尔值，并优先按字符串处理文本类型、按二进制处理字节数据。
- 连接测试不再写入审计日志，也不再因审计日志迁移尚未完成而被阻止。

## 安装与使用说明

- macOS 版本要求 macOS 15.0 或更高版本。

- macOS 应用使用 Developer ID 签名，但尚未进行 Apple 公证。首次启动时，macOS 可能显示 Gatekeeper 安全提示。您可以在终端中执行以下命令来解除警告：

```shell
sudo xattr -d com.apple.quarantine /Applications/DataNexa.app
```

- Windows 安装包暂未进行 Authenticode 代码签名，Microsoft Defender SmartScreen 可能显示安全提示。
