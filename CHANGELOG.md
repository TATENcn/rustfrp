# Changelog

## [Unreleased]

### Added

- The `tls2raw` client plugin is now a first-class certificate target, next to
  `https2http` / `https2https`. It terminates TLS inside frpc and forwards raw
  TCP to the backend, so it carries the same `crtPath` / `keyPath` pair — but
  binding or applying a certificate to it used to be rejected as "not a
  certificate plugin". The proxy editor renders its certificate fields and the
  type picker offers it.
- Certificate management for `https` proxies. FRP does not terminate TLS on the
  server — `frps` only reads the SNI and routes — so the domain certificate has
  to exist on the frpc host at the `crtPath` / `keyPath` its `https2http` plugin
  declares. The daemon now stores certificates, writes those files atomically
  with `0600` permissions and `rename`, and rolls back to the previous file when
  a write fails. Certificates are imported by pasting PEM, reading a local path,
  webhook push, or an automatic pull from the Certd open API or a plain URL.
  There is deliberately no download, export, or PFX route: a certificate and its
  key never leave the host and never appear in an API response.
- Certificate bodies and keys are stored as XChaCha20-Poly1305 blobs keyed by a
  per-installation master key in `cert_store.key` (mode `0600`) beside the
  database, so a copied database file does not leak a private key.
- The certificate ingest listener is configured from the UI and enabled by
  default on loopback. That endpoint never returns certificate material, every
  request needs a token, and a token can only cover the domains it was issued
  for, so requiring an operator to switch it on first would make Certd
  integration look broken. Enabling "reachable from other hosts" rebinds it to
  `0.0.0.0` and takes effect immediately, without restarting the daemon. A port
  conflict is reported in the panel and never stops the daemon from starting.
  It exposes exactly two routes (`POST /ingest/v1/certs` and
  `GET /ingest/v1/healthz`), authenticating only through the
  `X-Cert-Ingest-Token` header — never a URL path, which callers' logs would
  record. A token only accepts certificates whose every SAN it is allowed to
  serve; a wildcard covers one label, matching TLS wildcard semantics.
  `--cert-ingest-listen` remains as a pin for container deployments that must
  fix the address.
- In the Compose stack the ingest port is published to the host loopback only,
  not to every interface. A Certd in the same stack reaches the daemon over the
  Compose network (`http://daemon:7901/ingest/v1/certs`); an external one needs
  `RUSTFRP_CERT_INGEST_BIND=0.0.0.0`.
- The proxy editor now offers a structured certificate picker for
  `https2http` / `https2https` plugins, with the raw plugin JSON still available
  for fields the form does not model.
- `GET /api/v1/status` reports `config_dir`, so the UI can suggest certificate
  paths the daemon is able to create.
- The certificates page groups its push and pull channels behind two summary
  cards — the ingest listener's live address and the pull sources' health —
  and each card opens one dialog holding the endpoint settings, tokens and
  pull sources. They no longer stack as full-width panels under the list.
- FRP version downloads have their own page under 「运维管理 → 版本下载」. It
  reads the official release list and shows each build's archive size for this
  platform, download count and publish date, with a channel picker (official /
  GHFast / GH-Proxy / custom HTTPS) and card and list layouts. Rows are dense:
  version and status badges, one line of metadata, the archive URL as a plain
  link, then the actions, so the whole list fits without scrolling.
- Several versions download concurrently and each row shows its own progress
  bar and byte counter. The daemon now keeps a task per version instead of one
  global task, so a slow download no longer blocks the next one. Requesting a
  version that is already downloading returns `FRP_INSTALL_BUSY`, and that
  version cannot be deleted until its download finishes.

### Changed

- Certificates carry a status, and it is derived rather than stored: `active`,
  `expiring_soon`, `expired`, `not_yet_valid` or `pending`. The five states are
  distinct answers to "can I serve this?", and collapsing them into a day count
  hid the case that actually hurts — a certificate that was configured but never
  arrived. `status_reason` says why in one sentence, `usable` is exactly
  `status` in `{active, expiring_soon}`, and `channel_error` carries the last
  failure from any attached channel so a `pending` certificate can explain
  itself instead of showing a blank row.
- Applying a certificate that cannot be served is refused instead of overwriting
  a working file with one every client rejects. The check lives in
  `apply_certificate`, so the API and the automatic renewal path get the same
  answer, and it fires before anything is written.
- Configuring a pull channel creates the certificate it will fill, so the list
  shows `*.example.com · pending` immediately instead of staying empty until the
  first fetch succeeds. A channel whose first fetch fails used to leave the page
  looking untouched, which reads as "my configuration was not saved".
- A certificate is now identified by its domain, not by its fingerprint, and its
  material lives in its own table. Renewal was previously modelled as "a new
  certificate appears": every reissue has a new fingerprint, so replacing the
  certificate for `example.com` appended a row instead of updating one, and the
  rows piled up until an operator deleted them by hand. There is now one
  `certificate` row per `(tenant, primary_domain)` and one
  `certificate_material` row per issued bundle, so a renewal replaces what is
  served while keeping the identity — and therefore its bindings — intact.
  "Newest" means the most recently stored bundle, not the one expiring last: a
  reissue with a shorter validity must still win, or the daemon would keep
  serving the older certificate.
- How a certificate gets updated is now a property *of that certificate*, not a
  pair of global panels. Push tokens and pull sources became rows of
  `certificate_source`; a channel can be attached to a certificate, moved to a
  different one, detached, or marked preferred, so switching a certificate
  between push and pull no longer means deleting and re-creating credentials.
  A certificate may exist before it has any material, which is what lets an
  operator configure the source first and let the first push or fetch fill it
  in. The two summary cards on the Certificates page are gone; the endpoint
  settings moved to System Status → Certificate channels, and each certificate
  manages its own channels.
- Pull channels run on their own: the daemon checks once a day, imports what it
  fetched, applies it to every bound proxy, and restarts the frpc process that
  serves it. Before this, nothing called the pull path except a manual API call,
  and an applied certificate needed a manual process restart because `frpc
  reload` keeps the TLS certificate it loaded at startup.
- Both directions now validate the domain, and record which one failed. A push
  may only submit a certificate whose every SAN the token allows, and a token
  attached to a certificate may only replace that certificate's domain
  (`CERT_008` otherwise). A pull may only install a certificate that covers the
  channel's target domain; containment is required, not equality, because a CA
  may reissue with extra SANs. Creating a pull channel rejects a target the
  attached certificate does not cover, so the mistake surfaces before the first
  fetch.

### Fixed

- Every Certd pull failed with `no message (code 20002)` on a host whose clock
  was wrong, and the message hid why. Certd reports failures in `message`, a
  field the response envelope never declared, so every rejection was reported as
  "no message" with only a numeric code. The envelope now reads `message` (with
  `msg` as an alias for older builds) and appends the likely cause for the codes
  that have one.
- The resource chart's "last hour" could show far less than an hour. The buffer
  was bounded by count only (3 600 samples at 1 Hz), so an eight-hour clock
  correction left it holding nine hours of samples and the one-hour filter
  retained just the two minutes after the jump. Samples are now evicted by age
  as well as by count, so the window the UI offers is the window the data
  covers.
- A pull now recovers from a skewed clock instead of failing forever. Certd's
  rejection is a timestamp check, and its `Date` header says what "now" is, so a
  request that comes back `20002` is retried once, signed with the server's
  clock. That is the only way a daemon with no privileges can work on a machine
  whose time is wrong — observed here as an eight-hour RTC offset, where the pull
  now succeeds on the retry and logs both values.
- The certificate push URL shown in the UI was built from the listener's bind
  address, so an operator reaching the console over a domain or a LAN address
  was told `http://127.0.0.1:7901/ingest/v1/certs` — an address only the host
  itself can dial. The push route is now also mounted on the control API, and the
  daemon resolves the advertised URL from the request (`Host`, then
  `X-Forwarded-Host`/`X-Forwarded-Proto`) so it always names the address the
  browser actually used; `RUSTFRP_CERT_INGEST_PUBLIC_URL` pins it for a proxy
  that rewrites paths or remaps ports. The response carries `url_source` so the
  UI can say whether the value was configured, derived from the request, or
  fell back to the bind address, and the token snippet in the same dialog now
  uses the same value instead of recomputing it from `window.location`.
- A certificate pull source that stores no credentials (a plain URL, an
  anonymous fetch) failed on every run with `DB_003: Invalid column type Null`
  instead of fetching. The nullable `enc_secret` column was read as
  `Option<Vec<u8>>`, where rusqlite rejects NULL on the inner column; the row is
  now read as `Option<Option<Vec<u8>>>`, so "no secret" is distinct from "no
  such source" — and a source that disappears mid-run now reports
  `RecordNotFound` rather than an empty credential.

## [0.5.4] - 2026-11-15

### Fixed

- The desktop app no longer dies when the tray icon cannot be created. The error
  was returned from Tauri's `setup` hook, which turns it into a panic that kills
  the process before any window or data directory exists — observed on Windows as
  `Failed to setup app: ... tray icon error: Access is denied`. A tray is a
  convenience, so a failure now logs a warning and the window works as usual.

## [0.5.3] - 2026-11-15

### Fixed

- The desktop app no longer flashes a console window. It is a GUI process, and
  on Windows a console child of a windowless parent either shows its own window
  or fails with `STATUS_DLL_INIT_FAILED`; every `frpc` invocation (`verify`,
  `reload`, the supervised instance, and each restart) hit this. All four spawn
  sites now set `CREATE_NO_WINDOW` through one shared helper. Measured from a
  real GUI-subsystem parent: without the flag the child exits `-1073741502`
  (`0xC0000142`), with it the child reports `GetConsoleWindow() == 0` and
  succeeds.
- Startup restores only the profiles the user left running, instead of every
  profile that has a generated TOML. A profile whose frps was unreachable
  retried on every launch (one spawn plus three backoffs), which on the desktop
  shell meant a window appearing and vanishing in a loop. The intent is already
  persisted in `profile_runtime.desired_running` by the start/stop switch and
  the API.

## [0.5.2] - 2026-11-15

### Fixed

- The desktop application ships its own icon. It previously carried the Vue
  logo: the green triangle came from the favicon `plugins/webui` was initialised
  with, and this is not a Vue project. The mark is now three stacked rounded
  squares, authored as `plugins/gui/app-icon.svg` and rendered into every
  platform icon and a real multi-size `favicon.ico` (16 through 256 px).
- Launching the Windows build no longer opens an extra console window.
  `main.rs` was missing `#![cfg_attr(not(debug_assertions), windows_subsystem =
  "windows")]`, so the release binary was linked as a console program.

## [0.5.1] - 2026-11-15

### Fixed

- The sidebar no longer claims a hardcoded version. It read `v0.4.0` from a
  literal in `shell.tsx`, so 0.5.0 shipped an installer whose own footer
  disagreed with `GET /api/v1/health`. The footer now follows the version the
  running daemon reports.

## [0.5.0] - 2026-11-15

### Added

- A desktop application (`plugins/gui`, Tauri v2) that runs the same daemon
  in-process and opens a native window on its loopback API, so a browser is no
  longer required to manage tunnels. The window hides to a tray icon instead of
  stopping the tunnels, and Quit runs the normal `SIGTERM` → 3s → `SIGKILL`
  teardown per frpc instance. Release tags now publish `.deb`, `.AppImage`,
  `.msi`/`.exe`, and `.dmg` bundles alongside the existing archives; pull
  requests build the Linux bundles through `.github/workflows/desktop.yml`.
- `instance.lock` in the data directory, held exclusively for the lifetime of a
  daemon or desktop process. Two processes sharing one SQLite file would each
  start their own frpc for the same profile, so the second one now refuses to
  start with a message naming the lock file instead of silently doubling the
  tunnels.
- `GET /api/v1/*` can now be served on a caller-supplied listener
  (`bind_api_listener` + `serve_with_listener`), which is what lets the desktop
  shell bind `127.0.0.1:0` and learn the assigned port before the webview
  navigates to it.
- `SignalHandler::request_shutdown()` and `ClientCore::signal_handler()`: an
  embedded host asks for a graceful stop without sending itself a signal, which
  would kill the webview before teardown completed.

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

### Changed

- The desktop window authenticates with a 256-bit token generated at launch and
  injected into `localStorage` before the bundle runs. It never appears in the
  URL, and the webview has no IPC commands, no capability permissions, and
  `withGlobalTauri` disabled.
- The WebUI hides the sign-out control and stops clearing the token on `401`
  when it detects the desktop shell, where no token can be typed in.

- API scope classification treats proxies, bindings, visitors, and logs as
  tenant-owned resources; they previously required platform scopes despite being
  per-tenant data.
- The WebUI's unused environment create/update/delete client bindings are
  removed. The switcher and per-profile reassignment remain; environment
  lifecycle stays an API/operator action.

- RustFRP runs one fixed frp version. Per-profile version pinning and its
  "restart required" state are removed (schema V19 drops `frpc_version`): a
  single daemon cannot validate generated TOML against several binary versions
  at once, so a pin could only produce configurations that older frpc builds
  reject. Switch the active version through the FRP version registry instead;
  `PUT /api/v1/profiles/{id}/frpc-version` is gone.

- `local_proxy` read projections are defined once (`PROXY_COLUMNS`) instead of
  being repeated in every query. Adding a field used to require editing several
  aligned-by-hand column lists plus the row mapper.

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

- Tenant isolation is enforced for proxies, bindings, visitors, and logs. Those
  handlers previously ignored the caller's tenant: a token issued for one tenant
  could list another tenant's proxies (including their internal `local_ip`
  topology), read any profile's logs, and delete foreign proxies by id.
  Ownership now lives on `frps_profile.tenant_id` and `local_proxy.tenant_id`
  (schema V20); visitors and bindings inherit their profile's tenant, and every
  read filters on it. Foreign ids report "not found" so ids cannot be probed.
- Assigning a profile to an environment also assigns it to that environment's
  tenant, keeping the two paths consistent.
- Environment switching now scopes every dashboard number. `GET /api/v1/status`
  accepts `environment_id` and filters `total_profiles`, `total_bindings`,
  `total_visitors`, `active_frpc_instances`, and `processes` to it;
  `total_proxies` stays tenant-wide because proxies are reusable across
  profiles. The logs page profile picker is limited to the selected environment
  too. Selecting an environment whose profile list is empty now shows zero
  instances instead of every profile in the tenant.

- Profile names are unique (schema V18). Existing duplicates are renamed with a
  numeric suffix on migration, and conflicts return `DB_005` instead of a raw
  SQLite error. Names are trimmed on write so `"prod"` and `"prod "` cannot
  diverge.
- Duplicate proxy names inside one profile fail generation with an explicit
  message. frpc previously refused the whole file (`proxy name [x] is
  duplicated`), taking down an otherwise healthy profile.

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
