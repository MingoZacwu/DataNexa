# 更新日志

此文件用于维护 DataNexa 官方网站上的版本更新记录，只保留最新版本的更新记录。  
在 Release 时，DataNexa Site 会自动抓取此文件以执行更新流程。  
更早版本的历史记录由官网仓库归档保存，不需要在这里维护。

<!--
官网按版本号解析这个文件，并与官网仓库里归档的历史合并，所以：

  · 一个版本一节，用 `## [版本号] - 发布日期` 作标题（`## 0.9.0`、`## v0.9.0` 也识别）；
  · 每节里用 `### 中文` 与 `### English` 分出两个语言段，这两行本身不会显示在官网上；
  · 正文标题从 `####` 起（比语言标签深一级），列表、代码块、链接、加粗均支持；
    官网会按该版本最浅的标题自适应层级，写成 `###` 同样能正常显示；
  · 尚未发布的版本写成 `## [Unreleased]`，官网会跳过它。

这里写什么，官网就显示什么，不再做任何自动删减。
发版时若尚未补上对应小节，官网会先用 GitHub Release 正文兜底，并在 CI 中给出提醒。
-->

## [0.8.3] - 2026-10-30

### 中文

#### 版本亮点

- MCP 服务现在可在启动 DataNexa 时自动运行，并新增后台运行提示，方便了解关闭窗口后服务仍可继续运行。
- 新增可选的 GitHub 下载加速，帮助网络连接不稳定的用户检查更新、下载应用安装包和 Java 运行时。
- 新增“帮助与反馈”页面，可直接打开在线使用手册或前往 GitHub 提交问题与建议。

#### 新增功能

##### MCP 自动启动与后台运行

- 新增“启动 DataNexa 时自动开启 MCP 服务”设置。启用后，每次启动应用都会自动启动 MCP 服务；开启“登录时启动 DataNexa”后，MCP 服务也会始终随应用自动启动。
- 首次启动 MCP 服务后，概览页会提示 DataNexa 可以在关闭窗口后继续于后台运行，并提供开启开机自启动的快捷操作。提示只显示一次。

##### GitHub 下载加速

- 设置中新增可选的 GH-Proxy 加速开关，用于应用更新检查、安装包下载和托管 Java 运行时下载。
- 加速请求失败时会自动改用 GitHub 直连。下载的应用安装包和 Java 运行时仍会执行签名或 SHA-256 校验。

##### 帮助与反馈

- 侧边栏新增“帮助与反馈”页面，可打开在线使用手册或 GitHub 问题反馈入口。
- macOS 菜单栏菜单现已本地化，并增加打开帮助文档的入口。

#### 调整与问题修复

- 修复更新设置中的开关会响应到其他开关状态的问题；现在每个开关只控制自身选项。

### English

#### Highlights

- MCP can now start automatically with DataNexa, with a new background-running tip explaining that the service can continue after the window is closed.
- Added optional GitHub download acceleration for users with unreliable connections when checking for updates or downloading the app and Java runtime.
- Added a Help & Feedback page with direct links to the online manual and GitHub issue reporting.

#### Added

##### Automatic MCP Startup and Background Running

- Added a setting to start the MCP server whenever DataNexa launches. When “Launch DataNexa at login” is enabled, the MCP server starts automatically as well.
- After the MCP server starts for the first time, the Overview page shows a one-time tip explaining that DataNexa can keep running in the background after the window closes, with a shortcut to enable launch at login.

##### GitHub Download Acceleration

- Added an optional GH-Proxy setting for app update checks, installer downloads, and managed Java runtime downloads.
- If an accelerated request fails, DataNexa automatically retries directly with GitHub. App installers and Java runtimes remain subject to signature or SHA-256 verification.

##### Help & Feedback

- Added a Help & Feedback page in the sidebar, with links to the online manual and GitHub issue reporting.
- Localized the macOS menu bar and added an entry to open the help documentation.

#### Changes and Fixes

- Fixed update settings switches affecting the state of other switches. Each switch now controls only its own setting.
