# DataNexa v0.7.1

[English Release Notes](https://github.com/MingoZacwu/DataNexa/blob/v0.7.1/docs/RELEASE_NOTES.en.md)

## 版本亮点

- 调整 Bearer 鉴权设置，关闭前会明确提示安全影响，降低误操作风险。

## 新增功能

- 关闭 Bearer 鉴权时新增确认对话框，说明端点失去令牌保护及访问控制不可用等影响。

## 调整与改进

- Bearer 鉴权设置保存失败时，会恢复之前的设置状态，避免界面显示与实际配置不一致。
- 改进 Windows 对话框背景的显示效果。

## 安装与使用说明

- macOS 版本要求 macOS 15.0 或更高版本。

- macOS 应用使用 Developer ID 签名，但尚未进行 Apple 公证。首次启动时，macOS 可能显示 Gatekeeper 安全提示。您可以在终端中执行以下命令来解除警告：

```shell
sudo xattr -d com.apple.quarantine /Applications/DataNexa.app
```

- Windows 安装包暂未进行 Authenticode 代码签名，Microsoft Defender SmartScreen 可能显示安全提示。
