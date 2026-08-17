# DataNexa v0.7.0

[English Release Notes](https://github.com/MingoZacwu/DataNexa/blob/v0.7.0/docs/RELEASE_NOTES.en.md)

## 版本亮点

- 新增基于访问令牌的细粒度访问控制，可为不同 Agent 创建独立令牌并分别配置权限。
- 审计日志现在记录请求来源并支持按访问令牌筛选，便于追踪和管理数据访问。

## 新增功能

- 支持创建、启用或停用、重命名、轮换和删除访问令牌，并可为每个令牌分别限制数据库连接和 MCP 工具。
- 可从访问控制界面为指定令牌生成 Agent 接入配置；新令牌默认可使用当前已启用的数据库连接和 MCP 工具。
- 审计日志展示令牌名称或其他请求来源，并保留已删除令牌的历史归属信息。
- 访问令牌权限会在后端执行，未授权的工具调用或数据库访问会被拒绝。

## 调整与改进

- 原服务器令牌页面改为访问控制界面，审计日志筛选器新增访问令牌选项。

### 升级说明

- 原有访问密钥会在升级时迁移为默认访问令牌，已有鉴权配置可以继续使用。

## 安装与使用说明

- macOS 版本要求 macOS 15.0 或更高版本。

- macOS 应用使用 Developer ID 签名，但尚未进行 Apple 公证。首次启动时，macOS 可能显示 Gatekeeper 安全提示。您可以在终端中执行以下命令来解除警告：

```shell
sudo xattr -d com.apple.quarantine /Applications/DataNexa.app
```

- Windows 安装包暂未进行 Authenticode 代码签名，Microsoft Defender SmartScreen 可能显示安全提示。
