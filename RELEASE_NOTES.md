# DataNexa v0.4.1

[English Release Notes](https://github.com/MingoZacwu/DataNexa/blob/v0.4.1/docs/RELEASE_NOTES.en.md)

## 版本亮点

- 修复每天自动检查更新可能不生效的问题。
- Windows 版本重装或升级后，已启用的登录启动设置会自动恢复，避免应用无法按预期随系统启动。

## 问题修复

- 修复自动更新检测可能因应用进入后台、前端重新加载或应用重启而失效的问题。
- Windows 启动时会根据已保存的偏好重新同步登录启动项，修复重装流程导致的自动启动失效问题。

## 安装与使用说明

- macOS 版本要求 macOS 15.0 或更高版本。

- macOS 应用使用 Developer ID 签名，但尚未进行 Apple 公证。首次启动时，macOS 可能显示 Gatekeeper 安全提示。您可以在终端中执行以下命令来解除警告：

```shell
sudo xattr -d com.apple.quarantine /Applications/DataNexa.app
```

- Windows 安装包暂未进行 Authenticode 代码签名，Microsoft Defender SmartScreen 可能显示安全提示。
