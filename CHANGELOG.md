# Changelog

## [Unreleased]

### Added

- FRP multi-version registry with version/platform-isolated installs, official
  release discovery, HTTPS mirror selection anchored to official checksums, and
  transactional WebUI switching/deletion with process rollback.
- Functional `rustfrp-agent` with authenticated Pull configuration, ETag support,
  TOML plus native `frps verify` validation, atomic last-known-good caching, PID
  adoption, crash restart, and an frps lifecycle independent from agent crashes.
- WebUI controls to copy server-side mapped addresses and create up to 100 TCP
  or UDP port mappings from lists and ranges with conflict validation.
- Mandatory SHA256 verification against the official FRP release manifest,
  version-aware binary integrity markers, and Windows ZIP archive support.
- One-shot modern `frpc.toml` migration into SQLite with atomic rollback,
  conflict-safe profile naming, Proxy/Visitor support, and WebUI controls.
- Transactionally consistent SQLite backup download from the daemon and WebUI.
- Rootless daemon container, Compose deployment, health checks, and multi-architecture
  GHCR publishing.

### Changed

- frpc crash recovery now uses bounded exponential backoff, replenishes its retry
  budget after a stable run, groups repeated failures, and exposes structured
  authentication/network/configuration/port/TLS diagnostics in the status API.
- API configuration payloads now apply model defaults to omitted fields, keeping
  older clients compatible when new fields are introduced.

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-08-26

### Added

- RustFRP daemon：HTTP API + 内嵌 WebUI（Vue 3 + naive-ui），仪表盘 / 服务器配置 / 代理规则 / 绑定关系 / 访客 / 日志 / 系统状态，中英文双语
- frpc 二进制自托管：首次启动自动从 GitHub Releases 下载、校验、解压到 `~/.rustfrp/frp/`，进程守护用绝对路径拉起，不依赖系统 PATH（`rustfrp-bin` crate）
- 多 frpc 实例并发托管：一个 Profile 对应一个 frpc 子进程，崩溃自动重启（≤3 次）、SIGHUP 热重载、优雅退出
- SQLite 配置管理：增量迁移（v1–v10）+ schema checksum 校验，配置 CRUD 与版本历史
- FRP TOML 1:1 生成：tcp / udp / http / https / stcp / xtcp / sudp / tcpmux 代理与访客规则
- WebUI profile 表单支持 token / OIDC 鉴权配置；更新 profile 时留空 token 保留原值
- 本地端到端穿透测试脚本 `scripts/e2e-local.sh`（双 frps + 双 frpc 闭环）

### Fixed

- `FrpVersion::from_tag` 下载 URL 缺少 `v` 前缀导致 404
- WebUI 日志视图路由引用缺失文件导致 daemon 编译失败
- 依赖声明遗漏（client / daemon 的 workspace 依赖恢复完整）

### Changed

- 全量 rustfmt 与 clippy 修复，达到 CI 质量门（`fmt --check` / `clippy -D warnings`）
- workspace 79 个测试通过
