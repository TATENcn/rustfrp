---
doc_id: 03-DEVELOPMENT
version: 1.0.0
last_modified: 2026-06-23
modification_policy: reference
---

# 开发指南

## 一、项目结构

```
rustfrp-manager/
├── Cargo.toml                    # workspace root
├── Cargo.lock
├── README.md
├── LICENSE                       # MIT
├── CHANGELOG.md
├── justfile                      # just 任务运行器（Rust 生态标准，比 Makefile 更简洁且跨平台）
├── rust-toolchain.toml           # Rust 工具链版本锁定
├── .github/
│   ├── workflows/
│   │   ├── ci.yml                # 主 CI：test + lint + build
│   │   ├── release.yml           # 发布：cross 编译 + GitHub Release
│   │   ├── security-audit.yml    # 周期性 cargo audit + cargo deny
│   │   └── nightly.yml           # 夜间深度测试（混沌/压力）
│   ├── CODEOWNERS                # 文档保护规则
│   └── dependabot.yml
├── docs/                         # 项目文档
│   ├── README.md                 # 文档索引
│   ├── 01-ARCHITECTURE.md        # 架构全景
│   ├── 02-CONSTRAINTS.md         # 开发约束
│   ├── 03-DEVELOPMENT.md         # 开发指南（本文件）
│   ├── 04-DEPENDENCIES.md        # 依赖清单与选型
│   ├── 05-CICD.md                # CI/CD 设计
│   ├── 06-DEPLOYMENT.md          # 部署指南
│   ├── 07-UI-DESIGN.md           # 界面设计
│   └── 08-SECURITY.md            # 安全设计
├── crates/
│   ├── common/                     # 共享基础设施（信号处理、日志、插件基础设施、panic 钩子）
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── signal.rs           # 跨平台信号处理（SignalHandler）
│   │       ├── logging.rs          # 日志初始化
│   │       ├── panic_hook.rs       # Panic 钩子
│   │       ├── error.rs            # SharedError
│   │       └── plugin/
│   │           ├── mod.rs
│   │           ├── manager.rs      # 插件管理器
│   │           └── ...             # 插件基础设施
│   ├── client/                     # 微内核库（纯库，零网络 I/O）
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs              # 公开 API 入口（ClientFacade trait + ClientState）
│   │       ├── core.rs             # ClientCore — 配置生成 + 进程管理 + 插件管理
│   │       ├── error.rs            # ClientError 定义 + 错误码 + i18n 键
│   │       ├── db/
│   │       │   ├── mod.rs          # 连接池（Database struct）+ 模块入口
│   │       │   ├── migrate.rs      # SQLite 增量迁移 + checksum 校验
│   │       │   ├── profile.rs      # FrpsProfile CRUD
│   │       │   ├── proxy.rs        # LocalProxy CRUD
│   │       │   ├── binding.rs      # BindingRule CRUD
│   │       │   └── visitor.rs      # LocalVisitor CRUD
│   │       ├── config/
│   │       │   ├── mod.rs
│   │       │   ├── model.rs        # 数据模型（1:1 映射 FRP TOML）
│   │       │   ├── validate.rs     # Schema 校验
│   │       │   └── generator.rs    # SQLite → TOML 生成器 + 原子写入
│   │       └── process/
│   │           ├── mod.rs
│   │           ├── guard.rs        # ProcessGuard（启动/热重载/优雅退出）
│   │           └── manager.rs      # ProcessManager（进程编排）
│   ├── rustfrp-daemon/             # NEW: HTTP API + daemon 二进制
│   │   ├── Cargo.toml              # 依赖: axum + tower + rustfrp-client
│   │   └── src/
│   │       ├── main.rs             # CLI 解析 + 启动 daemon
│   │       ├── lib.rs              # serve() 入口
│   │       └── api/
│   │           ├── mod.rs          # Router 组装 + AuthMiddleware trait + serve()
│   │           ├── state.rs        # ApiState（注入 Database + ProcessManager）
│   │           ├── response.rs     # ApiResponse<T> + 错误码映射
│   │           ├── profiles.rs     # Profile CRUD handlers
│   │           ├── proxies.rs      # Proxy CRUD handlers
│   │           ├── bindings.rs     # Binding CRUD handlers
│   │           ├── visitors.rs     # Visitor CRUD handlers
│   │           └── system.rs       # Status / Reload / Health 端点
│   ├── rustfrp-sdk/                # 插件 SDK（给插件开发者用）
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── context.rs          # PluginContext
│   │       ├── permissions.rs      # 权限枚举
│   │       └── wit/                # WIT 接口定义
│   ├── rustfrp-bin/                # FRP 二进制管理
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── download.rs         # 从 GitHub Releases 下载
│   │       ├── verify.rs           # SHA256 校验
│   │       └── extract.rs          # 解压 tar.gz
│   └── server/                     # 服务端
│       ├── control/                # 控制服务器
│       │   ├── Cargo.toml
│       │   └── src/
│       └── agent/                  # frps-agent
│           ├── Cargo.toml
│           └── src/
├── plugins/                      # 官方插件
│   ├── gui/                      # Tauri GUI
│   │   ├── src-tauri/
│   │   │   ├── Cargo.toml
│   │   │   ├── tauri.conf.json
│   │   │   ├── icons/
│   │   │   └── src/
│   │   │       ├── main.rs       # Tauri 入口
│   │   │       ├── commands.rs   # Tauri IPC commands（薄层，调用 Core）
│   │   │       └── state.rs      # 全局 AppState
│   │   ├── src/                  # Vue 前端
│   │   │   ├── App.vue
│   │   │   ├── main.ts
│   │   │   ├── router/
│   │   │   ├── stores/           # Pinia stores
│   │   │   ├── components/       # 可复用组件
│   │   │   ├── views/            # 页面视图
│   │   │   ├── locales/          # i18n 翻译文件（zh-CN/ + en/）
│   │   │   └── assets/
│   │   ├── index.html
│   │   ├── package.json
│   │   ├── vite.config.ts
│   │   ├── tailwind.config.js
│   │   └── tsconfig.json
│   ├── traffic-monitor/          # WASM 流量监控插件
│   │   ├── Cargo.toml
│   │   ├── manifest.json
│   │   └── src/
│   │       └── lib.rs
│   └── failover/                 # 未来 HA 插件
│       └── ...
├── tests/                        # 集成测试（跨 crate 测试）
│   ├── integration/
│   │   ├── db_tests.rs
│   │   ├── generator_tests.rs
│   │   └── process_tests.rs
│   └── fixtures/                 # 测试用 SQLite 数据库 + TOML 样本
├── benches/                      # 性能基准测试（跨 crate）
│   └── generator_bench.rs
└── scripts/                      # 辅助脚本
    ├── install.sh
    ├── build-all.sh
    └── debug-snapshot.sh         # bug 现场收集
```

### 关键设计决策

1. **`client/` 是纯库，`rustfrp-daemon/` 是二进制 + HTTP API**：`client/` 不含 `main.rs`、不含网络 I/O 依赖。嵌入式目标可直接依赖 `rustfrp-client` 库。HTTP API 层在独立 `rustfrp-daemon/` crate，通过 `http-api` feature flag 控制。
2. **`db/` 子模块按表拆分**：`profile.rs` / `proxy.rs` / `binding.rs` / `visitor.rs` 各管一张表的 CRUD，`mod.rs` 做连接池和迁移，`migrate.rs` 独立管理迁移逻辑。
3. **`tests/` 从 crate 内移出**：集成测试放在工作区顶层的 `tests/` 目录，避免与单元测试混淆，且能跨 crate 测试。
4. **`rustfrp-bin` 独立 crate**：FRP 二进制下载/校验/解压独立管理。无头模式（路由器）不编译此 crate。
5. **HTTP API 版本化**：所有 API 端点使用 `/api/v1/` 前缀，为未来 breaking changes 留空间。v1 和 v2 可并存。

## 二、核心模块代码模板

### 2.1 错误类型定义

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CoreError {
    #[error("数据库错误: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("配置校验失败: {0}")]
    ConfigValidation(String),

    #[error("TOML 生成失败: {0}")]
    TomlGeneration(String),

    #[error("进程管理错误: {0}")]
    Process(String),

    #[error("插件错误: {0}")]
    Plugin(String),
}

// 公共 API 统一返回
pub type Result<T> = std::result::Result<T, CoreError>;
```

### 2.2 SQLite 配置模型

```rust
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrpsProfile {
    pub id: Option<i64>,
    pub name: String,
    pub server_addr: String,
    pub server_port: u16,
    pub token: String,
    pub tls_enable: bool,
    // 其余字段与 FRP 官方 TOML 规范一一对应
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalProxy {
    pub id: Option<i64>,
    pub name: String,
    pub proxy_type: ProxyType,  // tcp/udp/http/https/stcp/xtcp
    pub local_ip: String,
    pub local_port: u16,
    pub remote_port: u16,
    pub custom_domains: Option<Vec<String>>,
    pub health_check_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProxyType {
    Tcp, Udp, Http, Https, Stcp, Xtcp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindingRule {
    pub id: Option<i64>,
    pub profile_id: i64,
    pub proxy_id: i64,
    pub enabled: bool,
    pub priority: i32,
    pub group_name: Option<String>,
    pub group_key: Option<String>,
}
```

### 2.3 SQLite → TOML 生成器

```rust
use std::fs;
use std::path::Path;

/// 从 SQLite 读取配置，生成 frpc.toml
/// 必须走原子写入：tmp → rename
pub fn generate_frpc_toml(
    db: &Connection,
    output_path: &Path,
) -> Result<()> {
    // 1. 从 SQLite 读取启用的绑定规则（JOIN 三表）
    let rules = load_active_rules(db)?;

    // 2. 组装为 FRP TOML 结构
    let frp_config = build_frp_config(&rules)?;

    // 3. 序列化为 TOML 字符串
    let toml_str = toml::to_string_pretty(&frp_config)
        .map_err(|e| CoreError::TomlGeneration(e.to_string()))?;

    // 4. 原子写入：tmp → rename
    let tmp_path = output_path.with_extension("toml.tmp");
    fs::write(&tmp_path, &toml_str)
        .map_err(|e| CoreError::TomlGeneration(e.to_string()))?;
    fs::rename(&tmp_path, output_path)
        .map_err(|e| CoreError::TomlGeneration(e.to_string()))?;

    Ok(())
}

fn load_active_rules(db: &Connection) -> Result<Vec<ResolvedBinding>> {
    let mut stmt = db.prepare(
        "SELECT
            p.server_addr, p.server_port, p.token, p.tls_enable,
            x.name, x.proxy_type, x.local_ip, x.local_port,
            x.remote_port, x.custom_domains,
            r.group_name, r.group_key
         FROM binding_rule r
         JOIN frps_profile p ON r.profile_id = p.id
         JOIN local_proxy   x ON r.proxy_id   = x.id
         WHERE r.enabled = 1
         ORDER BY r.priority"
    )?;
    // ... 映射到 ResolvedBinding
    todo!()
}
```

### 2.4 进程管理器

```rust
use tokio::process::{Child, Command};
use tokio::signal;

pub struct ProcessGuard {
    child: Child,
}

impl ProcessGuard {
    /// 启动 frpc 子进程
    pub async fn start(config_path: &Path) -> Result<Self> {
        let child = Command::new("frpc")
            .arg("-c")
            .arg(config_path)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| CoreError::Process(e.to_string()))?;

        Ok(Self { child })
    }

    /// 热重载：发送 SIGHUP
    pub async fn reload(&self) -> Result<()> {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{signal, SignalKind};
            nix::sys::signal::kill(
                nix::unistd::Pid::from_raw(self.child.id().unwrap() as i32),
                nix::sys::signal::Signal::SIGHUP,
            ).map_err(|e| CoreError::Process(e.to_string()))?;
        }
        Ok(())
    }

    /// 优雅退出
    pub async fn shutdown(mut self) -> Result<()> {
        // 1. SIGTERM
        #[cfg(unix)]
        nix::sys::signal::kill(
            nix::unistd::Pid::from_raw(self.child.id().unwrap() as i32),
            nix::sys::signal::Signal::SIGTERM,
        ).ok();

        // 2. 等 3 秒
        match tokio::time::timeout(
            std::time::Duration::from_secs(3),
            self.child.wait()
        ).await {
            Ok(_) => return Ok(()),
            Err(_) => {
                // 3. SIGKILL
                self.child.kill().await.ok();
                self.child.wait().await.ok();
            }
        }
        Ok(())
    }
}
```

### 2.5 配置校验器

```rust
impl LocalProxy {
    /// 启动前校验
    pub fn validate(&self) -> Result<()> {
        // IP 格式
        if self.local_ip.parse::<std::net::IpAddr>().is_err() {
            return Err(CoreError::ConfigValidation(
                format!("无效 IP: {}", self.local_ip)
            ));
        }
        // 端口范围
        if self.local_port == 0 || self.remote_port == 0 {
            return Err(CoreError::ConfigValidation(
                "端口不能为 0".into()
            ));
        }
        // Token 非空（如果引用的 Profile 需要）
        // ...
        Ok(())
    }
}
```

### 2.7 HTTP API Handler 模板

所有 handler 遵循统一模式：`axum extractor` → `Database 调用` → `ApiResponse` 返回。

```rust
// crates/rustfrp-daemon/src/api/profiles.rs

use axum::extract::{Path, State};
use axum::Json;
use super::response::ApiResponse;
use super::state::ApiState;

/// GET /api/v1/profiles
pub async fn list(
    State(state): State<ApiState>,
) -> Result<Json<ApiResponse<Vec<FrpsProfile>>>, (StatusCode, Json<ApiResponse<()>>)> {
    let items = state.db.list_profiles().await
        .map_err(|e| (status_code(&e), Json(ApiResponse {
            success: false, data: None, count: None,
            error: Some(ApiError::from_client_error(&e)),
        })))?;

    let count = items.len();
    Ok(Json(ApiResponse::ok_list(items, count)))
}

/// GET /api/v1/profiles/{id}
pub async fn get(
    State(state): State<ApiState>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse<FrpsProfile>>, (StatusCode, Json<ApiResponse<()>>)> {
    let item = state.db.get_profile(id).await
        .map_err(|e| (status_code(&e), Json(ApiResponse {
            success: false, data: None, count: None,
            error: Some(ApiError::from_client_error(&e)),
        })))?;

    Ok(Json(ApiResponse::ok(item)))
}

/// POST /api/v1/profiles — 创建时，服务端管理 created_at/updated_at
pub async fn create(
    State(state): State<ApiState>,
    Json(mut profile): Json<FrpsProfile>,
) -> Result<(StatusCode, Json<ApiResponse<FrpsProfile>>), (StatusCode, Json<ApiResponse<()>>)> {
    let now = Utc::now().to_rfc3339();
    profile.created_at = now.clone();
    profile.updated_at = now;

    let id = state.db.insert_profile(&profile).await
        .map_err(|e| (status_code(&e), Json(ApiResponse {
            success: false, data: None, count: None,
            error: Some(ApiError::from_client_error(&e)),
        })))?;

    profile.id = Some(id);
    Ok((StatusCode::CREATED, Json(ApiResponse::ok(profile))))
}

/// PUT /api/v1/profiles/{id} — 保留原始 created_at，更新 updated_at
pub async fn update(
    State(state): State<ApiState>,
    Path(id): Path<i64>,
    Json(mut profile): Json<FrpsProfile>,
) -> Result<Json<ApiResponse<FrpsProfile>>, (StatusCode, Json<ApiResponse<()>>)> {
    profile.id = Some(id);
    profile.updated_at = Utc::now().to_rfc3339();
    if let Ok(existing) = state.db.get_profile(id).await {
        profile.created_at = existing.created_at;
    }
    // ... update + return ...
}
```

**关键约定**：
- 所有 handler 返回 `Result<Json<ApiResponse<T>>, (StatusCode, Json<ApiResponse<()>>)>` —— 200/201 走 `Ok`，4xx/5xx 走 `Err`
- `created_at` / `updated_at` 由服务端管理，客户端传入值被 `#[serde(skip_deserializing)]` 忽略
- `token` 字段在 API 响应中被 `#[serde(skip_serializing)]` 隐藏
- 数据库访问直接 `await`（`tokio::sync::Mutex` 保护），不走 `spawn_blocking`
- DELETE profile 时先 `process_manager.stop(profile_id)` 再删 DB 记录

**鉴权中间件**（MVP 不启用）：
```rust
/// 鉴权中间件 trait（api/mod.rs）
pub trait AuthMiddleware: Send + Sync + 'static {
    fn authenticate(&self, request: &Request) -> Result<(), Response>;
}

/// MVP 实现：不鉴权
pub struct NoAuth;
impl AuthMiddleware for NoAuth {
    fn authenticate(&self, _request: &Request) -> Result<(), Response> {
        Ok(())
    }
}
```

**开发命令**：
```
cargo check -p rustfrp-daemon       # 检查 daemon crate 编译
cargo run -p rustfrp-daemon         # 启动 daemon（默认 http-api feature）
cargo run -p rustfrp-daemon --no-default-features  # 纯信号模式
```

## 三、插件开发模板

### 3.1 WASM 插件

```rust
// 插件通过 WIT 接口与核心交互
// 只能调用核心暴露的 Host Functions，不可直接操作 FS/网络

use rustfrp_plugin::{PluginContext, PluginError, PluginInfo};

pub struct MyPlugin {
    ctx: PluginContext,
}

impl MyPlugin {
    pub fn new(ctx: PluginContext) -> Self {
        Self { ctx }
    }

    // 核心暴露的 Host Functions 示例：
    // - ctx.get_config() → Result<Config>
    // - ctx.get_traffic_stats() → Result<TrafficStats>
    // - ctx.subscribe_event(EventType, callback) → Result<SubscriptionId>
    // - ctx.publish_event(Event) → Result<()>
}
```

### 3.2 插件 manifest.json

```json
{
  "name": "traffic-monitor",
  "version": "1.0.0",
  "type": "wasm",
  "entry": "traffic_monitor.wasm",
  "description": "实时流量统计与历史记录",
  "permissions": ["read-traffic", "subscribe-events"],
  "dependencies": [],
  "min_core_version": "1.0.0"
}
```

### 3.3 动态库（Native）插件

适用于 GUI 渲染、硬件交互等需要原生性能的场景。动态库在进程内运行，不受 WASM 沙箱保护，需更严格的权限控制。

```rust
// Native 插件通过 C ABI 与核心交互
use std::ffi::CStr;
use std::os::raw::c_int;

/// 插件上下文（由核心层传入，包含函数指针表）
#[repr(C)]
pub struct NativePluginContext {
    pub get_config: unsafe extern "C" fn() -> *const u8,
    pub get_traffic: unsafe extern "C" fn() -> *const u8,
    pub subscribe_event: unsafe extern "C" fn(event_type: u32, cb: extern "C" fn(*const u8)),
}

#[no_mangle]
pub extern "C" fn plugin_init(ctx: *const NativePluginContext) -> c_int {
    if ctx.is_null() { return -1; }
    // 保存 ctx，初始化插件内部状态
    0
}

#[no_mangle]
pub extern "C" fn plugin_start() -> c_int { 0 }

#[no_mangle]
pub extern "C" fn plugin_stop() -> c_int { 0 }

#[no_mangle]
pub extern "C" fn plugin_unload() -> c_int { 0 }
```

**约束提醒**：
- 必须通过核心层提供的函数指针表交互，禁止直接访问核心数据结构
- 必须实现完整的 `init → start → stop → unload` 生命周期
- 编译目标需与核心层 ABI 一致
- 需通过安全审计（`cargo audit` + `cargo clippy -- -D unsafe_code`）

### 3.4 Sidecar 插件

适用于消息推送、第三方 API 对接等需完全解耦的场景。Sidecar 作为独立进程运行，可用任何语言实现。

```rust
// Sidecar 通过 stdin/stdout 以 JSON-RPC 行协议与核心通信
use std::io::{self, BufRead, Write};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};

static RUNNING: AtomicBool = AtomicBool::new(true);

fn main() {
    // 注册信号处理（SIGTERM/SIGINT 优雅退出）
    // 具体实现见下方注释

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        if !RUNNING.load(Ordering::Relaxed) { break; }

        let request: Value = match serde_json::from_str(&line.unwrap()) {
            Ok(v) => v,
            Err(e) => {
                let _ = writeln!(stdout, "{}", json!({"error": e.to_string()}));
                continue;
            }
        };

        let response = match request["method"].as_str() {
            Some("ping")  => json!({"result": "pong"}),
            Some("send_notification") => {
                // 调用 Webhook / Bark / 钉钉 等
                json!({"result": "ok"})
            }
            Some("shutdown") => {
                RUNNING.store(false, Ordering::Relaxed);
                json!({"result": "shutting down"})
            }
            _ => json!({"error": "unknown method"}),
        };

        let _ = writeln!(stdout, "{}", serde_json::to_string(&response).unwrap());
        stdout.flush().ok();
    }
}
```

**通信协议**：
- 请求（stdin）：`{"id": "uuid", "method": "method_name", "params": {...}}`（一行 JSON）
- 响应（stdout）：`{"id": "uuid", "result": ..., "error": null}`（一行 JSON）
- 核心层通过 stdin 发送请求，通过 stdout 读取响应

**约束提醒**：
- manifest.json 中 `type` 设为 `"sidecar"`，`entry` 为可执行文件路径
- 进程级隔离最安全，但通信延迟高于 WASM/Native
- 必须处理 SIGTERM 优雅退出（超时 5 秒后被核心层 SIGKILL）

---

## 四、监控对接

### 4.1 FRPS 侧（被监控节点）

FRP 原生支持 Prometheus 指标导出，无需额外开发：

```toml
# frps.toml
[webServer]
addr = "0.0.0.0"
port = 7500
# 开启后，/metrics 端点自动暴露
```

### 4.2 监控服务器侧

```rust
// 定时 Pull 各 FRPS 节点的 /metrics
// 存入 Prometheus，Grafana 展示
// 核心逻辑：HTTP GET + 超时 + 熔断

use tokio::time::{timeout, Duration};

async fn scrape_node(url: &str) -> Result<MetricsSnapshot> {
    let resp = timeout(
        Duration::from_secs(3),  // 超时
        reqwest::get(url)
    ).await??;

    let body = resp.text().await?;
    // 解析 Prometheus text format
    parse_prometheus_metrics(&body)
}
```

## 五、热重载错误处理

配置热重载失败时，frpc 通过 Stderr 输出错误信息，同时拒绝新配置、继续使用旧配置运行。核心层必须捕获 Stderr 并将错误反馈给用户。

### 5.1 实现路径

```rust
use tokio::process::Child;
use tokio::io::{BufReader, AsyncBufReadExt};
use std::time::Duration;

impl ProcessGuard {
    /// 发送 SIGHUP 并检测 frpc 是否拒绝新配置
    pub async fn reload_with_feedback(&self) -> Result<(), CoreError> {
        // 1. 发送 SIGHUP（新 TOML 已在调用前原子写入完成）
        self.send_sighup()?;

        // 2. 短暂等待 frpc 处理 SIGHUP
        tokio::time::sleep(Duration::from_millis(500)).await;

        // 3. 非阻塞读取 Stderr，检查是否有错误输出
        if let Some(ref stderr_pipe) = self.child_stderr {
            let mut reader = BufReader::new(stderr_pipe);
            let mut line = String::new();
            while let Ok(n) = reader.read_line(&mut line).await {
                if n == 0 { break; }
                if line.contains("error") || line.contains("failed") || line.contains("invalid") {
                    return Err(CoreError::ConfigValidation(
                        format!("frpc 热重载被拒绝: {}", line.trim())
                    ));
                }
                line.clear();
            }
        }

        Ok(())
    }
}
```

### 5.2 关键原则

| 原则 | 说明 |
|---|---|
| 热重载失败 ≠ 系统故障 | frpc 继续用旧配置运行，已有连接不受任何影响 |
| 错误必须反馈用户 | Stderr 内容 → GUI 弹窗或系统托盘通知，禁止静默失败 |
| 不自动回滚 SQLite | 用户可以决定：修复配置重新加载，或手动恢复到旧配置 |
| 原子写入保证 TOML 一致性 | 即使 SIGHUP 被拒绝，下次启动仍从 SQLite 重新生成 |

---

## 六、测试策略

### 6.1 分层测试

```
┌─────────────────────────────────────────────────┐
│                  E2E 测试                         │
│         实际 frpc 启动→穿透→停止                  │
│         数量：少（5-10 条核心路径）                │
│         速度：慢（每条数秒到数十秒）               │
│         运行：PR 合入前 + 每日夜间                 │
├─────────────────────────────────────────────────┤
│               集成测试                            │
│         SQLite→TOML→文件→进程全链路              │
│         数量：中（30-50 条）                      │
│         速度：中（每条 0.1-1s）                   │
│         运行：每次 push                           │
├─────────────────────────────────────────────────┤
│               单元测试                            │
│         纯函数逻辑：校验器、生成器、模型           │
│         数量：多（100+ 条）                       │
│         速度：快（< 0.01s 每条）                  │
│         运行：每次 push + 本地保存时               │
└─────────────────────────────────────────────────┘
```

### 6.2 各类型要求

| 测试类型 | 工具 | 覆盖率要求 | 说明 |
|---|---|---|---|
| 单元测试 | `cargo test --lib` | 核心层 > 80% | 每个 `pub fn` 都有对应测试 |
| 集成测试 | `cargo test --test integration` | 核心流程 100% | 跨 crate 场景：DB→生成→进程 |
| 属性测试 | `proptest` | 校验器必须过 | 自动生成边界用例测试校验器 |
| Snapshot 测试 | `insta` | TOML 生成必须过 | FRP 配置升级时可快速发现输出变化 |
| 并发测试 | `tokio::test` + `serial_test` | SQLite 多线程写入 | 10 个并发写入不应死锁 |
| 混沌测试 | `#[ignore]` 标记 | 覆盖崩溃/网络中断 | 夜间 CI 运行，不阻塞 PR |
| 性能基准 | `criterion` | 每次发布前跑 | TOML 生成耗时、内存占用 |

### 6.3 测试示例

**属性测试（校验器边界）**：

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn validate_never_panics(
        ip in "[0-9]{1,3}\\.[0-9]{1,3}\\.[0-9]{1,3}\\.[0-9]{1,3}",
        port in 0u16..=65535,
    ) {
        let proxy = LocalProxy {
            local_ip: ip,
            local_port: port,
            remote_port: port,
            ..Default::default()
        };
        // 不应该 panic，只应返回 Ok 或 Err
        let _ = proxy.validate();
    }
}
```

**Snapshot 测试（TOML 生成）**：

```rust
use insta::assert_snapshot;

#[test]
fn generate_toml_from_three_proxies() {
    let db = setup_test_db();
    seed_test_data(&db, 3);

    let toml_str = generate_frpc_toml(&db).unwrap();

    // 首次运行自动创建快照，后续运行对比
    assert_snapshot!("three_proxies", toml_str);
}
```

**并发测试（SQLite 多线程写入）**：

```rust
#[tokio::test]
async fn concurrent_profile_creates_are_safe() {
    let db = setup_test_db_shared();
    let handles: Vec<_> = (0..10).map(|i| {
        let db = db.clone();
        tokio::spawn(async move {
            create_profile(&db, FrpsProfile {
                name: format!("server-{}", i),
                server_addr: format!("10.0.0.{}", i),
                server_port: 7000 + i,
                token: "test".into(),
                ..Default::default()
            })
        })
    }).collect();

    for h in handles {
        assert!(h.await.unwrap().is_ok());
    }
    assert_eq!(list_profiles(&db).unwrap().len(), 10);
}
```

**混沌测试（进程崩溃恢复）**：

```rust
#[tokio::test]
#[ignore = "需要实际 frpc 二进制，夜间 CI 运行"]
async fn frpc_crash_recovery() {
    let guard = ProcessGuard::start(&test_config_path()).await.unwrap();
    guard.child.kill().await.unwrap();          // 模拟崩溃
    tokio::time::sleep(Duration::from_secs(5)).await;

    let status = guard.check_alive().await;
    assert!(status.is_running, "frpc 应在 5 秒内自动重启");
    assert!(status.restart_count <= 3, "重启不超 3 次");
}
```

---

## 七、SQLite Schema 迁移策略

### 7.1 迁移表结构

```sql
CREATE TABLE IF NOT EXISTS _migrations (
    version     TEXT NOT NULL PRIMARY KEY,
    description TEXT NOT NULL,
    applied_at  TEXT NOT NULL DEFAULT (datetime('now')),
    checksum    TEXT NOT NULL,       -- 迁移 SQL 的 SHA256
    duration_ms INTEGER,
    success     INTEGER NOT NULL DEFAULT 1
);
```

### 7.2 迁移流程

```rust
pub fn run_migrations(db: &Connection) -> Result<()> {
    // 1. 确保 _migrations 表存在
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS _migrations (
            version TEXT PRIMARY KEY, description TEXT NOT NULL,
            applied_at TEXT DEFAULT (datetime('now')),
            checksum TEXT NOT NULL, duration_ms INTEGER, success INTEGER DEFAULT 1
        );"
    )?;

    for (ver, description, sql) in MIGRATIONS {
        // 2. 检查是否已执行
        if let Ok(row) = db.query_row(
            "SELECT checksum FROM _migrations WHERE version = ?1", [ver],
            |r| r.get::<_, String>(0)
        ) {
            // 3. 已执行 → 校验 checksum（防止脚本被篡改）
            let expected = sha256(sql);
            if row != expected {
                return Err(CoreError::Database {
                    code: "DB_003",
                    msg: format!("迁移 v{} checksum 不匹配", ver),
                });
            }
            continue;
        }

        // 4. 未执行 → 执行迁移
        let start = Instant::now();
        db.execute_batch(sql)?;
        let duration = start.elapsed().as_millis() as i64;

        db.execute(
            "INSERT INTO _migrations (version, description, checksum, duration_ms)
             VALUES (?1, ?2, ?3, ?4)",
            params![ver, description, sha256(sql), duration],
        )?;
    }
    Ok(())
}
```

### 7.3 关键原则

| 原则 | 说明 |
|---|---|
| 只做增量迁移 | 新版本只添加新表/新字段，不修改已有结构 |
| 绝不自动回退 | 如果数据库版本高于代码，拒绝启动并提示用户升级 |
| checksum 校验 | 每次启动校验已执行迁移的 checksum，防止脚本被意外修改 |
| 迁移记录可审计 | `_migrations` 表记录了每次迁移的时间、耗时、成功/失败 |

---

## 八、调试体系

### 8.1 Panic Hook（崩溃现场收集）

```rust
// crates/rustfrp-core/src/panic_hook.rs

pub fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Ok(log_dir) = crate::paths::log_dir() {
            let crash_file = log_dir.join(format!(
                "crash_{}.txt",
                chrono::Local::now().format("%Y%m%d_%H%M%S")
            ));
            let mut f = std::fs::File::create(&crash_file).unwrap();
            writeln!(f, "=== CRASH REPORT ===").ok();
            writeln!(f, "timestamp: {}", chrono::Local::now()).ok();
            writeln!(f, "panic: {}", info).ok();
            writeln!(f, "backtrace:\n{}", std::backtrace::Backtrace::force_capture()).ok();
            writeln!(f, "version: {}", env!("CARGO_PKG_VERSION")).ok();
            writeln!(f, "os: {}", std::env::consts::OS).ok();
        }
        eprintln!("rustfrp 内部错误。崩溃报告已保存到 ~/.rustfrp/logs/");
        default_hook(info);
    }));
}
```

### 8.2 GUI 调试面板（仅 dev 构建）

在 `#[cfg(debug_assertions)]` 下，`/debug` 路由提供实时诊断信息：

- SQLite 各表行数 + 最后修改时间
- 当前生成的 TOML 预览
- frpc 进程 PID/内存/CPU/重启次数
- 已加载插件列表
- 最近的错误日志

```rust
#[cfg(debug_assertions)]
#[tauri::command]
async fn debug_sqlite_state(state: State<'_, AppState>) -> Result<DebugDbState, String> {
    // 仅在 debug 构建中存在，发布构建中不存在此函数
}
```

### 8.3 Bug 现场收集（用户可执行）

```bash
#!/bin/bash
# scripts/debug-snapshot.sh
# 收集：数据库（脱敏）、生成的 TOML、日志、系统信息

OUTPUT="rustfrp-bug-report-$(date +%Y%m%d_%H%M%S)"
mkdir -p "/tmp/$OUTPUT"

# 数据库（替换 token 为 ***）
sqlite3 ~/.rustfrp/config.db ".dump" \
  | sed "s/token = '[^']*'/token = '***REDACTED'/g" \
  > "/tmp/$OUTPUT/config_dump.sql"

# 生成的 TOML
cp ~/.rustfrp/runtime/frpc.toml "/tmp/$OUTPUT/"

# 日志
cp -r ~/.rustfrp/logs/ "/tmp/$OUTPUT/logs/"

# 系统信息
{ echo "=== OS ===" && uname -a
  echo "=== FRP version ===" && frpc --version 2>&1
  echo "=== RustFRP version ===" && rustfrp-manager --version 2>&1
  echo "=== Memory ===" && free -h 2>/dev/null || vm_stat
} > "/tmp/$OUTPUT/system_info.txt"

tar -czf "./$OUTPUT.tar.gz" -C /tmp "$OUTPUT"
echo "诊断包: ./$OUTPUT.tar.gz"
```

---

## 九、justfile 命令参考

```justfile
# === 开发 ===
dev:
    RUST_LOG=rustfrp=debug cargo run -- gui

dev-core:
    RUST_LOG=rustfrp_core=trace cargo run -- daemon

# === 测试 ===
test-fast:
    cargo test --lib

test-all:
    cargo test --workspace

test-slow:
    cargo test --workspace -- --ignored

test-core:
    cargo test -p rustfrp-core

test-proptest:
    PROPTEST_CASES=10000 cargo test --lib -- proptest

# === 静态检查 ===
lint:
    cargo fmt --all -- --check
    cargo clippy --all-targets --all-features -- -D warnings

# === 日志 ===
logs-errors:
    cat ~/.rustfrp/logs/*.log | grep ERROR | tail -50

logs-frpc:
    tail -f ~/.rustfrp/logs/frpc.log

# === 调试 ===
debug-gui:
    RUST_BACKTRACE=full RUST_LOG=rustfrp=trace cargo run -- gui

debug-tokio:
    TOKIO_CONSOLE_ENABLED=1 cargo run --features tokio-console -- gui

debug-leak:
    cargo run -- gui &
    sleep 5
    heaptrack -p $(pgrep rustfrp-manager)

debug-db:
    sqlite3 ~/.rustfrp/config.db ".dump"

debug-toml:
    cat ~/.rustfrp/runtime/frpc.toml

# === 构建 ===
build-release:
    cargo build --release

build-all:
    ./scripts/build-all.sh
```

---

## 十、Rust ↔ TypeScript 类型共享

使用 `ts-rs` 防止 IPC 类型漂移：

```rust
// crates/rustfrp-core/src/config/model.rs
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]  // 生成 TypeScript 类型到 bindings/
pub struct FrpsProfile {
    pub id: Option<i64>,
    pub name: String,
    pub server_addr: String,
    pub server_port: u16,
    pub token: String,        // #[ts(skip)] 可跳过此字段不导出
    pub tls_enable: bool,
}

// 构建时自动生成：
// plugins/gui/src/bindings/FrpsProfile.ts
```

### 导入使用

```typescript
// 前端 import 自动生成的类型
import type { FrpsProfile } from '@/bindings/FrpsProfile';

// Tauri invoke 使用类型
const profile: FrpsProfile = await invoke('create_profile', { profile: formData });
```

### 注意事项
- `#[ts(export)]` 标记的 struct 会在构建时自动生成 `.ts` 文件
- 敏感字段（Token）可选 `#[ts(skip)]` 不导出到前端类型
- CI 中运行 `cargo test` 会检测类型文件是否有更新，确保不漂移

---

## 十一、场景最佳实践

### 11.1 新增核心功能

1. 对照 `02-CONSTRAINTS.md` 速查表，逐条检查 ARCH-001 ~ ARCH-008 和 CODE-003 ~ CODE-004
2. 确认新功能不能以插件形式实现 → 否则必须放插件层
3. 检查是否引入新依赖 → 核心层依赖应尽量少，且不引入网络 I/O 类依赖
4. 实现后验证：`cargo test` + 内存占用检查

**常见陷阱**：
- 把 UI 相关代码放入核心层（违反 ARCH-001）
- 在核心层直接操作 TOML 文件而非通过 SQLite（违反 ARCH-003）
- 新增 SQLite 字段不对比 FRP 官方 TOML 规范（违反 ARCH-004）
- 新增 CoreError 变体不添加错误码（违反 CODE-003）
- 日志中泄露 Token（违反 CODE-004）

### 11.2 新增插件

1. 选择正确的插件类型：
   - 纯业务逻辑（数据计算/算法/校验）→ WASM
   - 需要原生性能（GUI/硬件交互）→ Native 动态库
   - 需要完全解耦或非 Rust 实现 → Sidecar
2. 编写 manifest.json，声明**最小权限集**（不申请不需要的权限）
3. 实现完整生命周期（`init → start → stop → unload`）
4. 所有公共方法返回 `Result`，禁止 `panic!`
5. 编写隔离测试：模拟插件 panic，断言核心仍正常运行

**常见陷阱**：
- 权限声明过度（申请不需要的权限）
- WASM 插件中尝试访问文件系统（只能通过 Host Functions）
- 插件 panic 未用 `catch_unwind` 包裹

### 11.3 性能敏感开发

1. **先测量，后优化**——不要凭感觉
2. 内存分析：`valgrind --tool=massif` 或 `heaptrack`
3. CPU 分析：`perf record` + `flamegraph`
4. 关注指标：RSS 内存、空闲时 CPU、TOML 生成耗时
5. 优化后必须运行回归测试，确认不引入性能退化

---

## 十二、FRP 版本管理策略

### 8.1 版本要求

- **最低支持**：FRP v0.52.0（配置文件格式从 INI 切换为 TOML 后的首个稳定版）
- **推荐使用**：最新稳定版

### 8.2 二进制获取与校验

```rust
pub struct FrpBinary {
    pub version: String,
    pub path: std::path::PathBuf,
    pub checksum: String,
}

impl FrpBinary {
    /// 检查本地缓存，不存在则下载
    pub fn ensure(target_version: &str) -> Result<Self> {
        let cache_dir = dirs::cache_dir()
            .unwrap()
            .join("rustfrp")
            .join("binaries");

        let binary_path = cache_dir.join(format!("frpc_{}", target_version));

        if binary_path.exists() {
            // 校验 SHA256
            return Ok(Self { /* ... */ });
        }

        // 从 GitHub Releases 下载：
        // https://github.com/fatedier/frp/releases/download/
        //   v{version}/frp_{version}_{arch}.tar.gz
        todo!("下载 + 校验 SHA256 + 解压到 cache_dir")
    }

    /// 探测已安装 frpc 的版本号
    pub fn detect_version(path: &std::path::Path) -> Result<String> {
        // 执行 frpc --version，解析输出版本号
        todo!()
    }
}
```

### 8.3 版本兼容矩阵

| 本项目版本 | 支持 FRP 版本范围 | 说明 |
|---|---|---|
| v0.1.x | v0.52.0 ~ latest | MVP 阶段，跟随最新 FRP 规范 |
| 未来版本 | 以 CHANGELOG 为准 | 若 FRP 引入 breaking TOML 变更，做适配 |

**策略**：
- 首次启动自动检测/下载 frpc 二进制到 `~/.rustfrp/binaries/`
- 用户可在设置中指定自定义 frpc 路径（多版本共存场景）
- TOML 生成逻辑跟随最新 FRP 规范，向后兼容至 v0.52.0
- 每次启动时检查 frpc 版本是否在支持范围内，不支持则提示升级/降级

---

## 十三、开发前检查清单

- [ ] 新增功能是否属于核心层？→ 若不是，应做插件
- [ ] 是否引入了新依赖？→ 评估必要性
- [ ] SQLite 表字段是否 1:1 对应 FRP 规范？→ 不发明新字段
- [ ] 是否涉及配置的双向同步？→ 禁止，SQLite 是唯一真理
- [ ] TOML 写入是否走原子路径？→ tmp + rename
- [ ] 子进程退出是否走 SIGTERM → 等 → SIGKILL？→ 防僵尸进程
- [ ] 敏感信息（Token/密码）是否在日志中脱敏？→ 使用 tracing 的 `#[instrument(skip(token))]`
- [ ] 热重载失败是否将 Stderr 反馈给用户？→ 禁止静默失败
- [ ] 插件是否声明了最小权限集？→ 不申请不需要的权限
- [ ] frpc 版本是否在支持范围内？→ 启动时检查并提示
- [ ] 新增 CoreError 变体是否定义了错误码（`code()`）和 i18n 键（`user_message_key()`）？
- [ ] 日志中是否对敏感字段使用了 `#[instrument(skip(...))]`？
- [ ] 是否在 `#[cfg(debug_assertions)]` 下添加了调试命令？
