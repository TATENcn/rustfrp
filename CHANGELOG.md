# Changelog

## [Unreleased]

### Fixed

- Profile names are unique (schema V18). Existing duplicates are renamed with a
  numeric suffix on migration, and conflicts return `DB_005` instead of a raw
  SQLite error. Names are trimmed on write so `"prod"` and `"prod "` cannot
  diverge.
- Duplicate proxy names inside one profile fail generation with an explicit
  message. frpc previously refused the whole file (`proxy name [x] is
  duplicated`), taking down an otherwise healthy profile.

### Fixed

- Multi-instance isolation: generated TOML files and frpc logs are keyed by
  `{profile_id}-{name}` instead of the sanitized profile name. `sanitize_filename`
  maps distinct names onto one string (`"Test Server"` and `"Test_Server"` both
  become `Test_Server`), so two profiles shared a single TOML and log file; after
  a global reload one profile could restart with the other's configuration and
  lose its tunnels silently.
- Renaming a profile removes its previous TOML instead of leaving an orphan that
  looked like another runnable profile.
- Daemon startup restarts frpc for every profile that has a generated TOML. The
  startup loop still looked for `{name}.toml` after the rename, so it found none
  and started nothing.

### Changed

- `local_proxy` read projections are defined once (`PROXY_COLUMNS`) instead of
  being repeated in every query. Adding a field used to require editing several
  aligned-by-hand column lists plus the row mapper.

### Changed

- The WASM plugin runtime is an opt-in build feature (`wasm-plugins`), off by
  default. It had no production caller, and Wasmtime was the largest dependency
  in the workspace, contradicting the Core-under-10-MiB budget. A WASM plugin
  loaded without the feature now logs a warning instead of silently doing
  nothing, and CI runs the suite both with and without it.

### Removed

- Dead code: a 146-line commented-out binding handler, four byte-identical
  `api_error` helpers (one shared definition remains), and three copies of the
  `serve` body (now one `serve_with` plus three thin wrappers).

### Fixed

- Hot reload no longer kills frpc. frpc only registers `SIGINT`/`SIGTERM`, so the
  previous `SIGHUP` was handled by the default action and terminated the process
  (exit 129) instead of reloading. Generated configs now carry an authenticated
  loopback `[webServer]` per profile and reload runs `frpc reload`, which applies
  proxy changes without dropping established connections. A rejected
  configuration keeps the previous one running and is reported per profile.
- tcpmux proxies are now valid: `multiplexer` is part of the proxy model, SQLite
  schema, TOML generator, import path, and WebUI form. frpc rejects a tcpmux
  entry without it, which previously invalidated the whole profile.
- Proxy and visitor payloads are validated on create/update, so structurally
  invalid entries are rejected instead of persisting and breaking TOML
  generation later.
- Generated configurations are checked with `frpc verify` before a process is
  spawned, surfacing structural errors as a startup error.

### Added

- `profile_runtime.admin_port` / `admin_password` and `local_proxy.multiplexer`
  (schema V16). Existing profiles get a loopback admin port on their next
  configuration generation.
- Client common-config coverage (schema V17): `clientID`, `transport.wireProtocol`,
  `auth.additionalScopes`, `start`, `log.to`/`log.level`/`log.maxDays`,
  `virtualNet.address`, `transport.tls.serverName`,
  `transport.tls.disableCustomTLSFirstByte`, and the OIDC
  `additionalEndpointParams`/`trustedCaFile`/`insecureSkipVerify`/`proxyURL`
  fields are now modelled, persisted, imported, and emitted.
- `frpc.toml` import honours frp's initialism keys (`clientID`, `localIP`,
  `proxyURL`, `tokenEndpointURL`, `routeByHTTPUser`, `tcpMux*`, `bandwidthLimit*`,
  `proxyProtocolVersion`, `httpHeaders`, `groupKey`, `maxDays`, `wireProtocol`).
  They previously did not match and were silently dropped — `localIP` fell back
  to `127.0.0.1`.
- Profile and proxy validation is enforced on create/update, including wire
  protocol, auth scopes, log level, `VirtualNet` feature-gate consistency, and
  tcpmux multiplexer rules.
- `GET /api/v1/profiles/{id}/proxy-status` reports each proxy's real state from
  frpc's admin API, so a proxy rejected by frps is visible instead of appearing
  healthy because its process is alive.
- The WebUI exposes the remaining frpc fields: bandwidth limit and mode, proxy
  protocol version, health check settings, `allowUsers`, XTCP NAT traversal,
  `routeByHTTPUser`, request/response headers, annotations, metadatas, the FRP
  client plugin blob, and the XTCP visitor fields.

## [0.4.0] - 2026-10-25

### Added

- Per-server frpc version selection, with installed/default/running version visibility
  and explicit application of pending version changes.
- Background frpc installation with progress, official SHA256 verification, GitHub
  direct downloads, GHFast, gh-proxy, and custom HTTPS download channels.
- One-second Linux traffic sampling by upstream server, local proxy, and host
  interface, with time-window totals and independent scope filters.
- Repository-isolated Bun development commands with self-signed HTTPS and readable
  status, help, health, and lifecycle controls.

### Changed

- Rebuilt the WebUI using React, shadcn/ui, and Recharts; moved running processes
  into the dashboard sidebar and made version suggestions searchable and bounded.
- Daemon startup is offline by default. The management UI remains available before
  frpc is installed; startup downloads now require explicit `--auto-download`.

### Fixed

- Delete confirmations now close on successful responses without a data payload.
- Corrected form alignment and TLS option placement in server configuration.

### Security

- Updated Wasmtime 36 LTS, rustls, WebPKI, root certificates, Tokio, and ZIP
  dependencies to versions addressing the release-time RustSec advisories.

### Upgrade notes

- Back up the SQLite database before upgrading. Schema migration V15 adds per-server
  runtime version preferences; existing servers continue following the default.
- Traffic history remains bounded and in memory, resets on restart, and is not
  suitable for billing. Linux TCP collection can miss short-lived connections.
- Update the container image to receive the new startup defaults.

## [0.3.0] - 2026-08-28

### Added

- Redesigned WebUI with a unified visual system, Lucide icons, responsive layouts,
  reusable status components, and localized metric formatting.
- Modal workflows for server profiles, proxies, visitors, and proxy assignments,
  including searchable multi-selection and explicit save/reload feedback.
- ECharts-based CPU and memory history, three-second host monitoring, improved
  process state visibility, and ANSI-aware log rendering and filtering.

### Changed

- Proxy assignment actions now expose running, stopped, and pending-reload states
  consistently across the frontend and backend.
- Authentication is optional for server profiles, and profile editing no longer
  replaces the entire page.
- Dashboard uptime now updates every second while host metrics refresh every three
  seconds, keeping the interface responsive without excessive API traffic.

### Fixed

- Corrected responsive toolbar and action-button wrapping in Chinese and English.
- Fixed unusable delete confirmations, duplicate copy-button targets, missing i18n
  labels, and non-functional environment indicators.

## [0.2.0] - 2026-08-27

### Added

- Zero-trust API policy files with digest-only bearer credentials, tenant-bound
  identities, read/write/telemetry scopes, and a whoami audit endpoint; plus
  privilege-free eBPF readiness and pinned-object observability on Linux.
- Tenant-isolated Environment and Profile administration with independent
  defaults, non-enumerating ownership failures, and verified FRP user/serverUser
  generation for namespaced server-side proxy identities.
- Supply-chain policy enforcement with cargo-deny, grouped Dependabot updates,
  scheduled all-feature, ignored, and documentation test coverage, plus a
  security-patched Wasmtime 36 LTS runtime and canonical SPDX metadata.
- Deployment environments with transactional profile assignment, a global WebUI
  switcher, and automatic migration of existing profiles into Default.
- Bounded local CPU, memory, frpc-process, and environment-scoped traffic time
  series with SVG dashboard charts and Prometheus exposition.
- Full daemon + official frps + control + Prometheus + Alertmanager + provisioned
  Grafana Compose topology, including node-label-safe aggregation and offline alerts.
- Wasmtime 36 LTS plugin runtime with per-call permission checks, deterministic fuel
  exhaustion, epoch deadlines, 50 MiB guest-memory limits, lifecycle isolation,
  event/config/traffic host channels, and runnable Failover, traffic-statistics,
  and webhook-notification reference plugins.
- Native release bundles for Linux, Windows, and macOS on amd64 and arm64,
  including daemon, agent, and control binaries with SHA256 sidecars, plus a
  six-platform pull-request build gate.
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
