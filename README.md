# RustFRP

[![License](https://img.shields.io/badge/license-AGPL--3.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.88%2B-orange.svg)](rust-toolchain.toml)
[![CI](https://github.com/TATENcn/rustfrp/actions/workflows/webui-ci.yml/badge.svg)](https://github.com/TATENcn/rustfrp/actions/workflows/webui-ci.yml)

> 一个轻量的 FRP 配置管理中心与进程守护：把「SQLite 配置 → frpc.toml 生成 → 多 frpc 实例托管」做成开箱即用的一体化工具。

RustFRP 是一个 **frpc 智能包装器**（微内核 + 插件化单体），提供 Web 管理界面、多实例并发托管、frpc 二进制自托管下载，以及面向服务端的原生 frps 兼容。它不修改 frp 本身，只负责把配置、生成、守护、观测做得更顺手。

## 特性

- **多 frpc 实例并发** — 一个 Profile 对应一个 frpc 进程，多个服务器配置同时穿透
- **SQLite 为唯一真理** — 配置存在本地 SQLite，运行时 TOML 由它生成，绝不反向写回
- **1:1 对齐 FRP TOML 规范** — 不发明 FRP 不认识的字段，协议 100% 兼容
- **frpc 二进制自托管** — 首次启动自动从 GitHub 下载、SHA256 校验、解压到 `~/.rustfrp/frp/`，无需手动安装、不依赖系统 PATH
- **Web 管理界面** — Vue 3 + naive-ui，仪表盘、服务器配置、代理规则、绑定关系、访客、日志、系统状态，中英文双语
- **进程守护** — 崩溃自动重启（≤3 次）、SIGHUP 热重载、优雅退出（SIGTERM → SIGKILL）
- **HTTP API** — 完整 REST 管理面，可无界面调用或二次开发

## 架构

```
┌─────────────────────────────────────────────────────────────┐
│                   RustFRP Daemon (frpc wrapper)             │
│                                                             │
│  ┌──────────┐    ┌──────────┐    ┌───────────────────┐     │
│  │  SQLite  │───▶│  TOML 生成│───▶│  frpc 子进程守护   │     │
│  │ (truth)  │    │ (产物)   │    │  (每 Profile 一个) │     │
│  └──────────┘    └──────────┘    └───────────────────┘     │
│       ▲                                                     │
│       │  HTTP API + WebUI (0.0.0.0:7900)                    │
└─────────────────────────────────────────────────────────────┘
                          │
                    ┌─────▼─────┐
                    │  frps     │  原生 frps，零侵入
                    │ (server)  │  支持公网/内网
                    └───────────┘
```

核心公式：

> **SQLite 是真理来源 → TOML 是运行时产物 → 原生 FRP 是穿透引擎**

## 快速开始

### 前置要求

- Rust 1.88+（当前锁定依赖所需；`rust-toolchain.toml` 使用 stable）
- 网络可达 GitHub Releases（首次启动自动下载 frpc）
- 一个 frps 服务端（原生 frp，本项目不打包服务端）

### 启动

```bash
# 开发模式（前端热更新，需另开终端跑 daemon）
just dev-webui

# 直接运行 daemon（内嵌 WebUI + HTTP API）
just dev-daemon
# 等价于：
cargo run -p rustfrp-daemon

# 常用启动参数
cargo run -p rustfrp-daemon -- --config-dir ~/.rustfrp/runtime \
    --db-path ~/.rustfrp/config.db \
    --api-listen 127.0.0.1:7900          # 默认仅本机；对外开 0.0.0.0:7900
    --api-token <token>                   # 建议生产环境开启鉴权
```

打开 <http://127.0.0.1:7900/> 即可使用 Web 管理界面。

### Docker Compose

```bash
# Strongly recommended when port 7900 is reachable by other hosts.
export RUSTFRP_API_TOKEN='replace-with-a-long-random-token'
docker compose up -d
```

The image embeds the WebUI, listens on port `7900`, stores its SQLite database,
generated configuration, downloaded FRP binaries, and logs in the `rustfrp-data`
volume, and exposes a container health check at `/api/v1/health`.

### 使用流程

1. **服务器配置** — 填你的 frps 地址、端口、token（支持 token / OIDC 鉴权）
2. **代理规则** — 定义要穿透的本地服务（tcp / udp / http / https / stcp / xtcp 等）
3. **绑定关系** — 把服务器配置和代理规则关联，启用并启动
4. 访问穿透出的地址验证

### 配置迁移与备份

系统状态页可以将现代格式的 `frpc.toml` 一次性导入 SQLite，包含 Profile、
Proxy、Binding 和 Visitor。导入使用单个数据库事务，同名 Profile 会自动添加
数字后缀；Proxy/Visitor 名称保持不变，以免改变 FRP 对外名称。导入的绑定默认
处于待启动状态。

同一页面可以下载一致性的 `.sqlite` 备份。备份包含认证 token 等敏感配置，
应按密钥文件保管，不要提交到 Git 或通过公开渠道传输。

对应 API：

```text
POST /api/v1/config/import
GET  /api/v1/config/export
```

frpc 二进制会在首次启动时自动就绪，无需手动安装。

## 二进制自托管

| 项 | 说明 |
|---|---|
| 下载来源 | fatedier/frp GitHub Releases |
| 落地目录 | `~/.rustfrp/frp/frpc` |
| 版本控制 | 环境变量 `RUSTFRP_FRP_VERSION` 覆盖默认版本 |
| 平台探测 | 自动匹配 `linux_amd64` / `linux_arm64` / `darwin_*` / `windows_*` |

## 项目结构

```
crates/
├── client/           # 核心库：SQLite CRUD、TOML 生成、进程守护
├── rustfrp-bin/      # FRP 二进制下载/校验/解压（自托管）
├── rustfrp-daemon/   # HTTP API + 内嵌 WebUI
├── rustfrp-sdk/      # 插件 SDK（占位）
├── common/           # 共享：信号、日志、插件管理器、错误
└── server/
    ├── control/      # 监控服务器（Pull 模式指标采集）
    └── agent/        # 服务端 frps 管理（开发中）
plugins/
└── webui/            # Vue 3 + Vite + naive-ui 管理界面
```

## 文档

项目设计文档在 [AGENTS/](AGENTS/) 目录：

- [01-ARCHITECTURE](AGENTS/01-ARCHITECTURE.md) — 系统架构与 ADR
- [02-CONSTRAINTS](AGENTS/02-CONSTRAINTS.md) — 硬性约束
- [03-DEVELOPMENT](AGENTS/03-DEVELOPMENT.md) — 开发规范
- [05-CICD](AGENTS/05-CICD.md) — CI/CD 与发布
- [06-DEPLOYMENT](AGENTS/06-DEPLOYMENT.md) — 部署指南
- [07-UI-DESIGN](AGENTS/07-UI-DESIGN.md) — UI 设计
- [08-SECURITY](AGENTS/08-SECURITY.md) — 安全

## 开发

贡献代码请从默认分支 `dev` 创建短期分支，并向 `dev` 提交 PR。只有发布 PR
可以从 `dev` 合入 `main`，随后在 `main` 提交上创建版本 tag。完整规则见
[CONTRIBUTING.md](CONTRIBUTING.md)。

```bash
just test-fast        # 单元测试
just test-all         # 全量测试（含集成）
just lint             # fmt --check + clippy
just dev-webui        # 前端开发模式
just build-release    # 发布构建
```

## 测试

本地端到端穿透测试（双 frps + 双 frpc 闭环）：

```bash
bash scripts/e2e-local.sh
```

## 许可证

[AGPL-3.0](LICENSE) © RustFRP contributors
