# RustFRP

[![License](https://img.shields.io/badge/license-AGPL--3.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.95%2B-orange.svg)](Cargo.toml)
[![CI](https://github.com/TATENcn/rustfrp/actions/workflows/webui-ci.yml/badge.svg)](https://github.com/TATENcn/rustfrp/actions/workflows/webui-ci.yml)

> 用一个 Web 管理台配置、运行和监控 FRP：不再手写多份 TOML，也不必逐个守护 frpc/frps 进程。

RustFRP 是一个兼容原生 FRP 的管理与运维工具，面向个人、家庭实验室和小型团队。它把服务器、代理、绑定和 Visitor 配置保存在 SQLite 中，按需生成标准 FRP TOML，并负责原生 frpc/frps 的安装、启动、热重载、故障恢复和观测。

RustFRP 不修改 FRP 协议，也不接管流量转发。已有的现代格式 `frpc.toml` 可以直接导入；底层隧道仍由原生 frpc/frps 完成。

[快速开始](#快速开始) · [核心能力](#核心能力) · [系统架构](#系统架构) · [部署说明](deploy/compose/README.md) · [开发文档](AGENTS/README.md) · [参与贡献](CONTRIBUTING.md)

## 界面预览

| 仪表盘 | 代理分配 |
| --- | --- |
| ![RustFRP 仪表盘](docs/images/webui-dashboard.png) | ![RustFRP 代理分配](docs/images/webui-proxy-assignments.png) |

日志控制台支持实例切换、stdout/stderr 筛选、搜索、行数调整、复制和下载，并保留终端颜色。

![RustFRP 日志控制台](docs/images/webui-logs.png)

## 为什么使用 RustFRP

- **集中管理配置**：在中英文 WebUI 中维护服务器、代理、绑定关系和 Visitor，SQLite 是唯一配置来源。
- **同时运行多个 frpc**：每个服务器配置对应独立进程；单个连接故障不会影响其他实例。
- **兼容原生 FRP**：生成标准 TOML，支持导入现有配置，不引入私有隧道协议。
- **自动管理 FRP 二进制**：按平台下载 frpc/frps，强制校验官方 SHA256，支持多版本安装、切换与失败回滚。
- **面向长期运行**：支持热重载、优雅退出、崩溃恢复、指数退避和结构化故障诊断。
- **可选集中运维**：可通过 Control/Agent 管理远端 frps，并接入 Prometheus、Grafana 和 Alertmanager。
- **安全地扩展**：提供 REST API、租户与权限策略，以及受权限、时间、计算量和内存限制的无 WASI 插件沙箱。

## 核心能力

| 领域 | 能力 |
| --- | --- |
| 配置管理 | Profile、Proxy、Binding、Visitor 与多环境管理；批量创建 TCP/UDP 映射；端口冲突检查 |
| FRP 协议 | TCP、UDP、HTTP、HTTPS、STCP、XTCP、SUDP、TCPMUX；Token、OIDC、`user`、`serverUser` |
| 进程守护 | 启动、停止、重启、热重载、优雅退出、崩溃恢复、重复故障归并 |
| 版本管理 | Linux、Windows、macOS 的 x86_64/arm64；官方源或自定义 HTTPS 镜像；官方校验和验证 |
| 迁移备份 | 事务化导入现代 `frpc.toml`；一致性 SQLite 备份导出 |
| 服务端管理 | Agent 主动拉取配置，ETag 缓存，`frps verify` 校验，原子切换与失败回滚 |
| 可观测性 | 流量、CPU、内存、进程和节点状态；Prometheus 指标及预配置 Grafana 仪表盘 |
| API 与扩展 | REST API、Token/策略鉴权、租户隔离、WASM 插件及参考插件 |

## 系统架构

RustFRP 的核心链路是：**SQLite 保存期望状态 → 生成并校验 TOML → 原生 frpc/frps 执行转发**。WebUI 和 REST API 负责管理，Daemon 负责本地进程生命周期；Control/Agent 与监控组件均为可选能力，故障时不应阻断既有 FRP 数据面。

![RustFRP 系统架构](docs/images/rustfrp-architecture.svg)

更完整的设计约束和架构决策见 [系统架构设计](AGENTS/01-ARCHITECTURE.md)。

## 快速开始

### Docker Compose

准备 API Token 和 Grafana 管理员密码，然后启动完整部署：

```bash
mkdir -p deploy/compose/secrets
openssl rand -hex 32 > deploy/compose/secrets/rustfrp_api_token.txt
export GRAFANA_ADMIN_PASSWORD='replace-with-a-long-password'
docker compose up -d --build
```

启动后可访问：

| 服务 | 默认地址 | 用途 |
| --- | --- | --- |
| RustFRP | <http://127.0.0.1:7900/> | 配置和管理 FRP |
| Grafana | <http://127.0.0.1:3001/> | 查看集中监控仪表盘 |
| Prometheus | <http://127.0.0.1:9090/> | 查询监控指标 |
| Alertmanager | <http://127.0.0.1:9093/> | 查看和管理告警 |

Compose 会启动 daemon、frps、control 及监控组件。数据库、运行时配置、FRP 二进制和日志保存在数据卷中。增加节点、调整端口和生产部署方式见 [Compose 部署说明](deploy/compose/README.md)。

### 从源码运行

需要 Rust 1.95+、Bun、一个可用的 frps，以及首次启动时可访问的 FRP Release 下载源。

```bash
# 终端一：运行内嵌 WebUI 和 HTTP API 的 daemon
just dev-daemon

# 终端二：按需启动前端热更新
just dev-webui
```

也可以直接指定运行参数：

```bash
cargo run -p rustfrp-daemon -- \
  --config-dir ~/.rustfrp/runtime \
  --db-path ~/.rustfrp/config.db \
  --api-listen 127.0.0.1:7900 \
  --api-token <token>
```

打开 <http://127.0.0.1:7900/>，依次完成以下操作：

1. 添加 frps 服务器配置。
2. 创建本地代理或 Visitor。
3. 建立并启用绑定关系。
4. 启动对应 frpc，使用映射地址验证连接。

## 配置迁移与备份

系统状态页可一次导入现代格式 `frpc.toml`，包括 Profile、Proxy、Binding 和 Visitor。导入在单个数据库事务中完成；失败不会留下不完整配置。同名 Profile 会安全重命名，Proxy 和 Visitor 的对外名称保持不变。

同一页面可以下载一致性的 SQLite 备份。备份包含认证 Token 等敏感配置，应按密钥文件保管，不要提交到 Git 或通过公开渠道传输。

```text
POST /api/v1/config/import
GET  /api/v1/config/export
```

## FRP 二进制与版本管理

首次启动时，RustFRP 会下载当前平台对应的 frpc，无需依赖系统 `PATH`。版本按 `versions/<version>/<platform>/` 隔离，可以共存、切换和删除。自定义 HTTPS 镜像只提供发布归档，校验和仍取自 fatedier/frp 官方发布清单。

切换版本时，RustFRP 会重启切换前正在运行的 frpc；如果新版本启动失败，则恢复原版本和原有进程。也可以通过 `RUSTFRP_FRP_VERSION` 指定 daemon 的启动版本。

## 可选的 frps Agent

Control 以只读方式从模板目录提供 `<node-id>.toml`，Agent 使用 Bearer Token 主动拉取。候选配置通过 TOML 检查及 `frps verify` 后才会原子替换；Control 不可用时继续使用最后一份有效缓存。Agent 退出不会终止 frps，重启后可以通过 PID 文件接管进程。

```bash
export RUSTFRP_AGENT_TOKEN='replace-with-a-long-random-token'

# Control
cargo run -p rustfrp-control -- \
  --templates-dir ./deploy/agent/templates \
  --targets ./targets.json

# frps 节点；不传 --frps-path 时自动下载并校验官方 frps
cargo run -p rustfrp-agent -- \
  --node-id edge-1 \
  --control-url http://control.example.com:3000
```

模板 Pull API 支持 ETag/`If-None-Match`。生产环境应在 Control 前启用 TLS 反向代理。

## 项目结构

```text
crates/
├── client/           # SQLite CRUD、TOML 生成、进程守护
├── rustfrp-bin/      # FRP 二进制下载、校验、解压与版本管理
├── rustfrp-daemon/   # HTTP API、内嵌 WebUI 和本地管理入口
├── rustfrp-sdk/      # 插件 SDK
├── common/           # 日志、信号、错误和插件运行时
└── server/
    ├── control/      # 指标拉取和 Agent 配置模板 API
    └── agent/        # frps 配置拉取、校验、缓存与进程守护
plugins/
├── webui/            # Vue 3 + Vite 管理界面
└── official/         # 官方参考 WASM 插件
```

## 开发与测试

```bash
just test-fast             # 快速单元测试
just test-all              # 全量测试（含集成测试）
just lint                  # rustfmt --check + clippy
just dev-webui             # 前端开发模式
just build-release         # 发布构建
bash scripts/e2e-local.sh  # 双 frps + 双 frpc 本地闭环测试
```

项目使用 `dev` 作为默认开发分支。请从 `dev` 创建短期分支，并向 `dev` 提交 Pull Request；只有发布 PR 可以从 `dev` 合入 `main`。完整流程见 [贡献指南](CONTRIBUTING.md)。

## 文档

- [架构与 ADR](AGENTS/01-ARCHITECTURE.md)
- [硬性约束](AGENTS/02-CONSTRAINTS.md)
- [开发规范](AGENTS/03-DEVELOPMENT.md)
- [CI/CD 与发布](AGENTS/05-CICD.md)
- [部署指南](AGENTS/06-DEPLOYMENT.md)
- [界面设计](AGENTS/07-UI-DESIGN.md)
- [安全设计](AGENTS/08-SECURITY.md)

## 许可证

[AGPL-3.0-only](LICENSE) © RustFRP contributors
