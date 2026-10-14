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

## [0.8.1] - 2026-10-02

### 中文

#### 版本亮点

- 连接导出支持密码加密：导出的连接文件默认使用加密密码保护，只有知道密码的人才能读取其中的数据库连接。
- “存储与性能”页面新增实时资源监控：除 JDBC 驱动外，还可以查看 DataNexa 自身的 CPU、内存与进程明细。

#### 新增功能

##### 加密导出连接

- 导出连接时默认使用加密密码保护：设置不少于 8 个字符的加密密码后，只有知道该密码的人才能读取文件中的数据库连接(加密密码不会被保存，丢失后文件将无法恢复)；也可以选择“不加密导出”，此时仍需确认明文风险提示，导出的文件与旧版本保持一致。
- 加密文件使用 `.dnxc` 扩展名，导入时按文件内容而不是扩展名识别类型；导入加密文件会先提示输入加密密码，并显示文件的导出时间。
- 密码错误或文件损坏不会导入任何连接，可以重新输入密码；旧版本导出的明文连接文件仍可直接导入。

##### 实时资源监控

- “存储与性能”页面新增“资源监控”视图，与原有的“空间占用”视图并列。
- 资源监控同时覆盖 DataNexa 自身(主进程及其 WebView、GPU 等子进程)与 JDBC 驱动运行时，分别展示 CPU、内存与进程数量，其中 DataNexa 的进程还会显示 PID。
- CPU 区域展示总占用与实时趋势曲线，并分别标注 DataNexa 与 JDBC 驱动的占用；内存区域展示 DataNexa、JDBC 驱动与系统其他占用的构成，并以“已用/系统总内存”展示整体内存占用(悬停可查看可用内存)。
- 可以单独停止某个 JDBC 驱动运行时，并即时看到资源占用变化。

#### 调整与改进

- macOS 构建改用 Xcode 26.3(macOS 26.2 SDK)，以适配 macOS 27 的新版窗口控件(红绿灯)设计；最低系统要求仍为 macOS 15.0。

#### 升级须知

- 加密导出的 `.dnxc` 文件无法在 0.8.0 及更早版本中导入(旧版本不识别该格式，导入会直接失败)，如需使用加密文件迁移连接，请先将两端都升级到 0.8.1 及以上版本；旧版本导出的明文连接文件不受影响。

### English

#### Highlights

- Connection exports can now be protected with an encryption password: exported connection files are encrypted by default, and only someone who knows the password can read the database connections inside.
- The "Storage & performance" settings page now includes live resource monitoring, covering DataNexa itself as well as JDBC drivers.

#### Added

##### Encrypted Connection Exports

- Exporting connections now uses an encryption password by default: with a password of at least 8 characters, only someone who knows it can read the database connections in the file (the password is never stored, and the file cannot be recovered without it). "Export without encryption" is still available — it still requires acknowledging the plaintext warning — and produces the same file as previous versions.
- Encrypted files use the `.dnxc` extension, and import detects the file type from its contents rather than the extension. Importing an encrypted file asks for the password first and shows when the file was exported.
- A wrong password or a damaged file imports nothing, so the password can simply be retyped; plaintext connection files exported by earlier versions still import directly.

##### Live Resource Monitoring

- The "Storage & performance" page now has a Resources view alongside the existing Storage view.
- Resources covers both DataNexa itself (the main process and its WebView/GPU children) and JDBC driver runtimes, listing CPU, memory, and process count, with a PID for each of DataNexa's own processes.
- The CPU area shows total usage and a live trend chart, labelled with the DataNexa and JDBC driver shares; the memory area shows the breakdown of DataNexa, JDBC drivers, and other system usage, with used memory against total system memory (hover for available memory).
- Individual JDBC driver runtimes can be stopped, with resource usage refreshing immediately.

#### Changes and Improvements

- macOS builds now use Xcode 26.3 (macOS 26.2 SDK) to follow macOS 27's redesigned window controls (the "traffic lights"). The minimum requirement is still macOS 15.0.

#### Upgrade Notes

- Encrypted `.dnxc` files cannot be imported by 0.8.0 or earlier (older builds do not recognise the format and the import fails), so upgrade both sides to 0.8.1 or later before migrating connections with an encrypted file. Plaintext connection files exported by earlier versions are unaffected.
