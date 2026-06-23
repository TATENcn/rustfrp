---
doc_id: 04-DEPENDENCIES
version: 1.0.0
last_modified: 2026-06-23
modification_policy: reference
summary: 完整第三方依赖清单、选型理由、版本锁定策略、API 版本演进
---

# 依赖清单与技术选型

## 一、Workspace 级依赖

所有 workspace member 共享的版本声明，定义在根 `Cargo.toml` 的 `[workspace.dependencies]` 中。

```toml
[workspace.dependencies]
# === 异步运行时 ===
tokio = { version = "1", features = ["full"] }

# === 序列化 ===
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"

# === 数据库 ===
rusqlite = { version = "0.31", features = ["bundled", "vtab"] }

# === 错误处理 ===
thiserror = "1"
anyhow = "1"                # 仅应用层（monitor、gui），核心层禁止使用

# === 日志 ===
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }

# === 校验（手写，不引入 validator/garde） ===
# MVP 阶段校验规则仅 3 项（IP/端口/Token），手写即可
# 未来若规则膨胀，评估引入 garde（零依赖 derive 宏）

# === 跨平台 ===
cfg-if = "1"

# === ID 生成 ===
uuid = { version = "1", features = ["v4"] }

# === 时间 ===
chrono = { version = "0.4", features = ["serde"] }

# === 内存分配 ===
# 编译时可选，通过 feature flag 切换
mimalloc = { version = "0.1", optional = true }
tikv-jemallocator = { version = "0.5", optional = true }

# === CLI（监控服务器与 daemon 模式使用） ===
clap = { version = "4", features = ["derive"] }

# === HTTP 客户端（仅 monitor 和 frp 管理 crate 使用） ===
reqwest = { version = "0.12", features = ["rustls-tls"], default-features = false }
```

### 内存分配器可配置

```
# 默认：ARM 设备（路由器）用 mimalloc，x86_64 用 jemalloc
[features]
default = []
jemalloc = ["tikv-jemallocator"]
mimalloc-dep = ["mimalloc"]

# 用户编译时选择：
# cargo build --features jemalloc    → x86_64 服务器
# cargo build --features mimalloc-dep → ARM 嵌入式
# cargo build                         → 系统默认分配器
```

---

## 二、核心层 `rustfrp-core` 依赖

**定位**：微内核。只含 SQLite CRUD + TOML 生成 + 进程管理 + 插件管理器。**不含网络 I/O、不含 UI、不含文件下载。**

```toml
# crates/rustfrp-core/Cargo.toml
[dependencies]
tokio.workspace = true
serde.workspace = true
serde_json.workspace = true
toml.workspace = true
rusqlite.workspace = true
thiserror.workspace = true
tracing.workspace = true
uuid.workspace = true
chrono.workspace = true

# 核心层专用（零网络依赖）
dirs = "5"                    # 获取系统标准目录
sha2 = "0.10"                 # SHA256（迁移 checksum 校验）
```

**核心层绝不引入**：`reqwest`、`flate2`、`tar`、`anyhow`。FRP 二进制下载/解压由独立的 `rustfrp-frp` crate 负责。

---

## 三、FRP 二进制管理层 `rustfrp-frp` 依赖

**定位**：FRP 二进制文件的下载、校验、解压、版本检测。从核心层拆分出来，无头模式（路由器）不编译此 crate，实现真正零网络依赖。

```toml
# crates/rustfrp-frp/Cargo.toml
[dependencies]
tokio.workspace = true
serde.workspace = true
thiserror.workspace = true
tracing.workspace = true

dirs = "5"
sha2 = "0.10"
flate2 = "1"                  # 解压 .tar.gz
tar = "0.4"
reqwest = { version = "0.12", features = ["rustls-tls"], default-features = false }
```

---

## 四、插件 SDK `rustfrp-sdk` 依赖

**定位**：提供给插件开发者使用的 SDK。包含 PluginContext、权限枚举、WIT 接口定义。

```toml
# crates/rustfrp-sdk/Cargo.toml
[dependencies]
wasmtime = "24"               # WASM 运行时
libloading = "0.8"            # 动态库加载
serde.workspace = true
serde_json.workspace = true
tracing.workspace = true
thiserror.workspace = true
```

---

## 五、监控服务器 `rustfrp-monitor` 依赖

**定位**：Pull 模式指标采集 + Web 大盘。

```toml
# crates/rustfrp-monitor/Cargo.toml
[dependencies]
tokio.workspace = true
serde.workspace = true
serde_json.workspace = true
reqwest.workspace = true
clap.workspace = true
tracing.workspace = true
thiserror.workspace = true

axum = "0.7"                  # Web 框架
tower = "0.4"
tower-http = { version = "0.5", features = ["cors", "trace"] }
prometheus = "0.13"           # Prometheus 客户端库
```

---

## 六、GUI 插件 `plugins/gui` 依赖

### 6.1 Rust 侧（Tauri）

```toml
# plugins/gui/src-tauri/Cargo.toml
[dependencies]
tauri = { version = "2", features = ["tray-icon"] }
tauri-plugin-shell = "2"
tauri-plugin-notification = "2"
tauri-plugin-autostart = "2"
tauri-plugin-store = "2"      # 本地 KV 存储（窗口位置等 UI 状态）

rustfrp-core = { path = "../../../crates/rustfrp-core" }
rustfrp-frp = { path = "../../../crates/rustfrp-frp" }

ts-rs = "8"                   # Rust → TypeScript 类型导出
```

### 6.2 前端（npm）

```json
{
  "dependencies": {
    "vue": "^3.5",
    "pinia": "^2.2",
    "vue-router": "^4.4",
    "vue-i18n": "^9.14",
    "echarts": "^5.5",
    "@tauri-apps/api": "^2",
    "@tauri-apps/plugin-shell": "^2",
    "@tauri-apps/plugin-notification": "^2",
    "@tauri-apps/plugin-autostart": "^2",
    "tailwindcss": "^3.4",
    "@headlessui/vue": "^1.7",
    "lucide-vue-next": "^0.400"
  },
  "devDependencies": {
    "typescript": "^5.5",
    "vite": "^5.4",
    "@vitejs/plugin-vue": "^5.1",
    "prettier": "^3.3",
    "eslint": "^9"
  }
}
```

---

## 七、开发/测试/构建依赖

```toml
[workspace.dev-dependencies]
tempfile = "3"
test-log = "0.2"
rstest = "0.22"               # 参数化测试
mockall = "0.13"              # trait mock
criterion = "0.5"             # 性能基准测试
proptest = "1.5"              # 属性测试
serial_test = "3.1"           # 串行化测试（SQLite 测试需要）
pretty_assertions = "1"       # assert_eq! 的 diff 视图
tokio-test = "0.4"            # tokio 测试工具
insta = "1"                   # TOML 输出快照测试
loom = "0.7"                  # 并发 bug 检测（可选，编译慢）
```

---

## 八、选型理由

| 依赖 | 为什么选它 | 为什么没选替代品 |
|---|---|---|
| `mimalloc` | ARM 架构下表现优于 jemalloc（页表开销 4KB vs 4MB）；嵌入式设备更友好 | jemalloc 在 x86_64 服务器场景更强。通过 feature flag 让用户可选 |
| `rusqlite` + `bundled` | 编译时嵌入 SQLite，不依赖系统 libsqlite3，真正零配置 | `sqlx` 需要运行时数据库连接串，破坏"单文件"哲学 |
| `wasmtime` | 字节码联盟出品，Rust 生态最成熟的 WASM 运行时；安全沙箱完善 | `wasmer` 商业背景重，`wasmedge` 对非 Linux 平台支持弱 |
| `reqwest` + `rustls` | 纯 Rust TLS 实现，跨平台编译零痛苦 | `openssl` 在交叉编译时需要目标平台的 OpenSSL 头文件，极痛苦 |
| `axum` | tokio 生态最活跃的 Web 框架，零开销抽象 | `actix-web` 有自己独立的运行时，与 tokio 生态不完全兼容 |
| `ts-rs` | Rust struct 自动生成 TypeScript 类型定义，防止 IPC 类型漂移 | 手动维护两份类型定义极易不一致 |
| `insta` | TOML 输出快照测试，FRP 配置升级时可以快速发现生成内容变化 | 手写断言每次都要更新大量字符串 |
| `proptest` | 自动生成边界用例测试校验器，人工难以穷举所有非法输入 | 手写测试用例覆盖不全 |
| `just` | Rust 生态主流任务运行器，语法简洁，跨平台 | Makefile 语法晦涩且 Unix 独占；`cargo-make` 额外依赖太重 |

---

## 九、版本锁定策略

| 策略项 | 决定 |
|---|---|
| 依赖版本声明 | 使用 caret（`^`），即 `"1"` 表示 `>=1.0.0, <2.0.0` |
| 锁定文件 | `Cargo.lock` 和 `package-lock.json` 都提交进仓库 |
| 次要/补丁升级 | CI 通过则 Dependabot 自动合并 |
| 主版本升级 | 人工评估变更日志，需在 PR 中附升级理由 |
| 安全漏洞 | `cargo audit` 每日 CI 运行。`critical` 级别 24 小时内修复，`high` 级别 72 小时 |
| `cargo deny` | 每次 push 运行，检查：重复依赖、许可证冲突、被禁用的 crate |
| 降级规则 | 若升级导致 CI 失败且 2 小时内无法修复，回退到上一个通过版本 |
| Rust MSRV | 始终跟随最新 stable。不承诺支持旧版 Rust 编译器 |

---

## 十、API 版本演进策略（WIT 接口）

插件 API（WIT 定义）发布后遵循语义化版本：

| 变更类型 | 示例 | 版本号变化 |
|---|---|---|
| 新增可选字段/函数 | 在 `traffic` interface 中新增 `get-history` 函数 | MINOR 升 |
| 新增必填字段 | `frps-profile` record 新增必填字段 | MAJOR 升 |
| 删除函数/字段 | 删除 `subscribe` 函数 | MAJOR 升 |
| 修改字段类型 | `server-port: u16` → `u32` | MAJOR 升 |

**兼容性承诺**：
- 同一个 MAJOR 版本内，插件不需要重新编译
- MAJOR 升时，提供至少一个 MAJOR 版本的过渡期（旧接口标记 `#[deprecated]`）
- 插件 `manifest.json` 中 `min_core_version` 声明最低核心版本

---

## 十一、不引入的依赖

| 不引入 | 原因 |
|---|---|
| `openssl` / `openssl-sys` | 交叉编译需要目标平台的 OpenSSL 头文件，在 ARM 嵌入式目标上极度痛苦。全部用 `rustls` |
| `sqlx` | 需要运行时数据库连接串。`rusqlite` 的编译时嵌入更契合"单文件"哲学 |
| `actix-web` | 自带 actor 运行时，与 tokio 生态不完全兼容 |
| `validator` | 为 HTTP 表单校验设计，拖入 Web 框架依赖链。FRP 配置校验手写解决 |
| `anyhow`（核心层） | 核心层需要结构化错误。`anyhow` 仅在应用层（monitor/gui）使用 |
| UI 组件库（Element Plus / Ant Design Vue / Vuetify 等） | 自带设计系统，强制风格一致性但破坏自主视觉。用 Headless UI + TailwindCSS |
