<p align="center">
  <img src="resources/readme/datanexa.png" width="144" alt="DataNexa Logo">
</p>

<h1 align="center">DataNexa</h1>

<p align="center">
  简体中文 | <a href="docs/README.en.md">English</a>
</p>

<p align="center">
  面向 AI Agent 的本地只读数据库 MCP 网关
</p>

<p align="center">
  <a href="https://github.com/MingoZacwu/DataNexa/actions/workflows/compile.yml"><img src="https://github.com/MingoZacwu/DataNexa/actions/workflows/compile.yml/badge.svg" alt="Build Status"></a>
  <a href="https://github.com/MingoZacwu/DataNexa/releases"><img src="https://img.shields.io/github/v/release/MingoZacwu/DataNexa?display_name=tag" alt="Latest Release"></a>
  <a href="LICENSE"><img src="https://img.shields.io/github/license/MingoZacwu/DataNexa" alt="MIT License"></a>
</p>

DataNexa 是一个运行在本机的数据库 MCP 服务。它为 AI Agent 提供统一、受控且可审计的数据访问入口，在执行查询前应用只读策略，并对返回行数、执行时间和连接数量进行限制。

项目目前支持 SQLite、MySQL 和 PostgreSQL，桌面端基于 Tauri、React 与 Rust 构建。

## 功能特性

- 统一管理 SQLite、MySQL 和 PostgreSQL 只读连接
- 提供表结构发现、字段描述、数据采样、只读 SQL 和查询计划等 MCP 工具
- 基于 SQL 语法树校验查询，限制为只读语句
- 支持 Bearer Token 鉴权和手动轮换
- 数据库密码保存到操作系统凭证库，不写入常规配置文件
- 支持最大返回行数、查询超时和连接池上限
- 保留本地审计记录，并可对 SQL 字面量进行脱敏
- 提供连接诊断、工具开关、紧急禁用和连接导入/导出
- 支持简体中文、英文以及浅色/深色主题

## 界面预览

<img src="resources/readme/overview.jpg" alt="DataNexa Overview UI">

## 下载与安装

预编译版本可从 [Releases](https://github.com/MingoZacwu/DataNexa/releases) 页面下载。请根据操作系统选择对应的安装包或可执行文件，并优先使用最新稳定版本。

首次运行后，按以下顺序完成配置：

1. 新建数据库连接，并使用数据库侧的只读账号。
2. 测试连接，确认网络、凭证和权限配置正确。
3. 在“MCP 服务”页面启动本地服务。
4. 复制 Agent 接入配置，并添加到支持 MCP 的客户端。

## 从源码构建

### 环境要求

- [Git](https://git-scm.com/)
- [Node.js 20](https://nodejs.org/) 或更高版本
- [pnpm 9](https://pnpm.io/)
- [Rust stable](https://www.rust-lang.org/tools/install)
- Tauri 2 所需的系统依赖

各平台的系统依赖不同，完整说明见 [Tauri Prerequisites](https://v2.tauri.app/start/prerequisites/)：

- Windows：Microsoft C++ Build Tools、WebView2，以及 Rust MSVC 工具链
- macOS：Xcode Command Line Tools
- Ubuntu/Debian：`libwebkit2gtk-4.1-dev`、`libayatana-appindicator3-dev`、`librsvg2-dev`、`patchelf`、`xdg-utils`

### 获取源码

```bash
git clone https://github.com/MingoZacwu/DataNexa.git
cd DataNexa
corepack enable
corepack prepare pnpm@9 --activate
pnpm install --frozen-lockfile
```

### 本地开发

```bash
pnpm run dev:app
```

该命令会启动 Vite 开发服务，并以 Tauri 桌面窗口运行应用。

### 编译可执行文件

```bash
pnpm run build:portable
```

编译结果位于 `src-tauri/target/release/`。Windows 下通常为 `datanexa.exe`，macOS 和 Linux 下为对应平台的 `datanexa` 可执行文件。

### 构建安装包

```bash
pnpm run build:installer
```

安装包及平台相关产物位于：

```text
src-tauri/target/release/bundle/
```

仅需检查前端类型和构建结果时，可运行：

```bash
pnpm run build
```

> DataNexa 只能为当前构建平台生成原生应用。若需要 Windows、macOS 和 Linux 版本，请分别在对应系统上构建。

## 安全说明

“只读”是降低风险的防护措施，不等同于绝对安全。接入真实数据前，建议同时落实以下措施：

- 为 DataNexa 单独创建最小权限数据库账号，并在数据库侧撤销写入和管理权限
- 对敏感表、敏感字段和生产网络设置额外的访问控制
- 保持 Bearer Token 鉴权开启，不要向不可信应用泄露 Token
- 定期检查审计记录，只启用当前任务必需的 MCP 工具
- 对数据库进行必要备份，不要将 DataNexa 作为唯一安全边界

连接导出文件包含明文数据库密码。导出后请将文件保存在访问受控的位置，完成迁移后及时删除，切勿提交到代码仓库或上传到公共存储。

如发现安全问题，请不要在公开 Issue 中披露数据库信息、访问凭证或可直接利用的细节，可通过仓库维护者提供的私有联系方式报告。

## 参与贡献

欢迎通过 [Issues](https://github.com/MingoZacwu/DataNexa/issues) 提交问题和建议，也欢迎提交 Pull Request。提交代码前，请确保前端构建、Rust 格式检查、测试和 Clippy 检查均能通过：

```bash
pnpm run build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml --locked
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings
```

提交 Issue 或日志时，请先移除连接字符串、数据库名称、账号、Token、SQL 字面量及业务数据。

## 免责与使用须知

数据无价，谨慎使用。

只读策略不能完全保证所有风险都被拦截，仍需主动约束 Agent，避免要求或允许其执行危险的数据库操作。

DataNexa 是由个人独立开发和维护的开源项目，与 MySQL、PostgreSQL、SQLite、MCP 客户端及其所属组织不存在隶属或官方合作关系。

本项目按“原样”提供，不对适用性、可靠性、安全性或数据完整性作任何明示或暗示的保证。因使用或无法使用本项目而产生的数据丢失、服务中断、安全事件或其他损失，项目作者及贡献者在适用法律允许的最大范围内不承担责任。使用者应自行评估风险，并对数据库权限、备份、网络隔离和合规要求负责。

## 许可证

本项目基于 [MIT License](LICENSE) 开源。你可以在许可证允许的范围内使用、复制、修改、合并、发布和分发本项目，但必须保留原始版权声明和许可证文本。

## 版权信息

Copyright (C) 2026 Zachary Wu

MySQL、PostgreSQL、SQLite 及其他名称和商标归其各自权利人所有。
