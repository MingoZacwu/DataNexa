# DataNexa v0.3.1

[English Release Notes](https://github.com/MingoZacwu/DataNexa/blob/v0.3.1/docs/RELEASE_NOTES.en.md)

## 版本亮点

- 审计日志新增筛选功能，可按日期范围、MCP 工具、数据库连接和执行状态快速定位记录。
- 优化桌面窗口的启动与重复打开体验。

## 新增功能

- 新增审计日志筛选面板，支持按开始日期、结束日期、工具、连接和状态组合筛选。
- 新增日期选择器和日期范围校验，避免结束日期早于开始日期。
- 筛选生效时显示筛选状态，并支持一键清除全部筛选条件。
- 审计日志分页新增页码快捷入口，切换页面后自动回到列表顶部。

## 调整与改进

- 应用主窗口现在会在首次启动时居中显示。
- 再次启动 DataNexa 时，不再创建重复实例，而是恢复并聚焦已打开的主窗口。
- 将“自动检查更新”设置移至“关于更新”区域，使更新相关选项更加集中。

## 安装与使用说明

- macOS 应用使用 Developer ID 签名，但尚未进行 Apple 公证。首次启动时，macOS 可能显示 Gatekeeper 安全提示。您可以在终端中执行以下命令来解除警告：

```shell
sudo xattr -d com.apple.quarantine /Applications/DataNexa.app
```

- Windows 安装包暂未进行 Authenticode 代码签名，Microsoft Defender SmartScreen 可能显示安全提示。
