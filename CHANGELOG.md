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

## [0.8.2] - 2026-10-17

### 中文

#### 版本亮点

- 概览页顶部新增语言切换入口，避免用户找不到语言设置入口，方便用户快速切换语言。
- JDBC 驱动的原生能力检测结果显示在驱动列表中，可以直接看到驱动是否支持只读限制与查询超时。
- 新增快速复制连接名功能，可在连接列表 → 连接详情中点击连接名复制。
- 本次更新还包含诸多问题修复与细节改进。

#### 新增功能

##### 语言切换入口

- 概览页顶部新增语言按钮，各语言以其自身名称显示，选择后立即生效。“设置 → 常规”中的语言选项保持不变。
- 中文系统使用中文，其他语言默认使用英文。默认语言不再在首次启动时写入，只有主动选择后才会保存。

##### 连接名快速复制

- 连接列表 → 连接详情中的连接名可以直接点击复制。

##### JDBC 驱动能力标识

- 驱动列表显示每个驱动的原生支持情况，包括只读限制与查询超时。
- 对使用该驱动的连接执行一次“测试连接”，或通过 MCP 发起一次查询，检测结果会自动记录。
- 驱动本身不支持时，DataNexa 仍会为查询应用只读保护与超时限制。

#### 调整与改进

- 英文与数字改用内置的 Geist 字体，中文继续使用系统字体；字号与字重也已统一，中文粗体显示更清晰。
- 统一了弹层、提示与 Toast 的毛玻璃质感，以及资源列表图标与禁用状态的显示方式，并补充了启用/停用的过渡动画。
- 改进了与 MCP 客户端的兼容性，减少客户端误读查询结果字段的情况。

#### 问题修复

##### MySQL 连接

- 修复 MySQL 时间类型字段无法读取的问题，时间值统一按 UTC 返回。
- 修复查询超时后连接长时间不可用的问题，超时的查询会及时释放连接，不再影响后续操作。
- 修复查询结果为空时不返回字段信息、部分字符串内容显示为 base64 编码的问题。

##### JDBC 查询

- 无法解析的 JDBC 语句会被拒绝执行，避免绕过只读限制。
- 修复取消查询后数据库会话未正确终止的问题。
- 修复字符串内容被错误转换、单元格内容被截断却没有任何提示的问题。
- 审计记录与调试日志中的数据库错误信息统一脱敏。

##### 界面与资源监控

- 修复一次请求进行中时，界面上大量无关功能都会变为不可用的问题，现在只有相关功能会受影响。
- 修复设置页切换标签后滚动位置错乱的问题。
- 修复资源监控中 JDBC 驱动 CPU 占用显示超过 100% 的问题，统计方式现在与系统任务管理器一致。

##### MCP 工具

- 修复查询表字段时表不存在会返回空字段列表的问题，现在会明确返回“表不存在”，不再与真正无字段的结果混淆。

### English

#### Highlights

- Added a language switcher to the top bar of the overview page, so the language setting is easier to find and switching languages is quicker.
- The driver list shows what each JDBC driver supports natively, so you can see whether read-only mode and query timeouts are available.
- Added quick copying of connection names: click the name in the connection list or the connection details to copy it.
- This update also includes a number of bug fixes and smaller improvements.

#### Added

##### Language Switcher

- A language button now sits in the top bar of the overview page. Each language is listed under its own name and switching takes effect immediately. The language option under Settings > General is unchanged.
- Chinese systems use Chinese; every other language defaults to English. A default language is no longer written on first launch — only a choice you make yourself is saved.

##### Quick Copy for Connection Names

- The connection name in the connection list or the connection details can be clicked to copy.

##### JDBC Driver Capabilities

- The driver list reports what each driver supports natively, including read-only mode and query timeouts.
- Run Test Connection on a connection that uses the driver, or issue a query over MCP, and the result is recorded automatically.
- When a driver does not support them, DataNexa still applies read-only protection and a timeout to every query.

#### Changes and Improvements

- English text and numbers now use the bundled Geist font, while Chinese keeps using the system font. Font sizes and weights have also been unified, so bold Chinese text renders more cleanly.
- Popovers, tooltips, and toasts share one frosted-glass treatment, icons and disabled states in resource lists follow a consistent style, and enabling or disabling a resource is now animated.
- Improved compatibility with MCP clients, so results are less likely to be misread.

#### Fixed

##### MySQL Connections

- MySQL time columns could not be read; they now return their values, in UTC.
- A query that timed out left its connection unusable for a long time. The connection is now released as soon as the query times out, so later operations are unaffected.
- Empty results no longer lose their field information, and some string values no longer appear as base64 text.

##### JDBC Queries

- JDBC statements that cannot be parsed are now rejected, so they cannot bypass read-only protection.
- Cancelling a query now terminates the database session correctly.
- Fixed string values being converted incorrectly, and cells being truncated without any indication.
- Database errors are redacted consistently in audit records and debug logs.

##### Interface and Resource Monitoring

- A single request in progress no longer makes a large part of the interface unavailable; only the features involved are affected.
- Switching settings tabs no longer leaves the new tab at the previous tab's scroll position.
- JDBC driver CPU usage no longer reports more than 100%; it is now measured the same way Task Manager measures it.

##### MCP Tools

- Describing the columns of a table that does not exist no longer returns an empty column list; it now reports that the table was not found, so the result cannot be confused with a table that has no columns.
