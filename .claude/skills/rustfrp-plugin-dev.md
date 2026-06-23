---
name: rustfrp-plugin-dev
description: RustFRP 插件开发约束 — 修改 plugins/ 或 crates/rustfrp-sdk/ 时自动注入
triggers:
  - "plugins/**/*"
  - "crates/rustfrp-sdk/**/*.rs"
---

# RustFRP 插件开发约束

> 触发条件：修改 `plugins/` 下任何文件，或 `crates/rustfrp-sdk/` 下任何 `.rs` 文件时自动加载。
> 本文档是 P0 级强制约束的速查表，详情见文档锚点链接。

---

## P0 约束速查表

| ID | 约束 | 文档 |
|---|---|---|
| PLG-001 | manifest.json 必填字段：`name`（唯一）、`version`（语义化）、`type`、`entry`、`permissions` | `AGENTS/02-CONSTRAINTS.md#三插件约束` |
| PLG-002 | 权限声明与调用前校验：7 种权限，核心层每次调用前校验，权限不足 → 拒绝 + 记日志 | `AGENTS/02-CONSTRAINTS.md#三插件约束` |
| PLG-003 | 完整生命周期：`init → start → (运行) → stop → unload`，顺序不可违反 | `AGENTS/02-CONSTRAINTS.md#三插件约束` |
| PLG-004 | 禁止 `unwrap()` / `expect()`（测试除外），所有公共方法返回 `Result<T, PluginError>` | `AGENTS/02-CONSTRAINTS.md#三插件约束` |
| PLG-005 | 资源限制：WASM < 50MB / FD < 100 / 连接 < 10 / IPC < 1MB | `AGENTS/02-CONSTRAINTS.md#三插件约束` |
| PLG-006 | 插件 API（WIT 定义）向后兼容（P1 推荐，非强制） | `AGENTS/02-CONSTRAINTS.md#三插件约束` |

---

## 三种插件形态对照

| 属性 | WASM | Native（动态库） | Sidecar |
|---|---|---|---|
| 运行时 | Wasmtime | libloading / C ABI | 独立子进程 |
| 场景 | 纯逻辑（流量统计、配置校验） | 需原生性能（GUI、硬件交互） | 完全解耦（消息推送、第三方 API） |
| 隔离级别 | 沙箱，仅 Host Functions | 进程内，严格权限 | 进程级隔离 |
| manifest.type | `"wasm"` | `"native"` | `"sidecar"` |
| 交互方式 | Host Functions 调用 | C ABI 函数指针表 | stdin/stdout JSON-RPC 行协议 |
| 文件系统 | 禁止直接访问 | 需声明 `filesystem-access` | 需声明 `filesystem-access` |
| 网络 | 禁止直接访问 | 需声明 `network-access` | 需声明 `network-access` |
| 退出信号 | Host 调用 `stop()` | Host 调用 `stop()` | SIGTERM（超时 5s → SIGKILL） |

---

## 关键代码模板

### manifest.json 最小模板

```json
{
  "name": "my-plugin",
  "version": "1.0.0",
  "type": "wasm",
  "entry": "my_plugin.wasm",
  "description": "插件功能描述",
  "permissions": ["read-traffic"],
  "dependencies": [],
  "min_core_version": "1.0.0"
}
```

### 生命周期接口（所有插件类型必须实现）

```rust
trait Plugin {
    fn init(&mut self, ctx: &PluginContext) -> Result<(), PluginError>;
    fn start(&mut self) -> Result<(), PluginError>;
    fn stop(&mut self) -> Result<(), PluginError>;
    fn unload(&mut self) -> Result<(), PluginError>;
}
```

### 权限声明（7 种）

| 权限 | 能力 |
|---|---|
| `read-config` | 读取配置 |
| `write-config` | 修改配置 |
| `read-traffic` | 读取流量数据 |
| `control-process` | 启停 FRP 进程 |
| `subscribe-events` | 订阅事件 |
| `network-access` | 网络访问 |
| `filesystem-access` | 文件系统访问 |

---

## 常见陷阱

| # | 陷阱 | 反面示例 |
|---|---|---|
| 1 | manifest.json 缺少必填字段 | `{"name": "x", "version": "1.0"}` — 缺少 type/entry/permissions |
| 2 | 在插件代码中使用 `unwrap()` 或 `expect()` | `let config = read_config().unwrap();` — 应用 `?` 或 `map_err` |
| 3 | 生命周期顺序错误：先 `start` 再 `init` | 核心层检测到顺序违规 → 插件被强制卸载 |
| 4 | WASM 插件尝试直接访问文件或网络 | WASM 插件只能通过 Host Functions 与核心交互 |
| 5 | Sidecar 插件不处理 SIGTERM，导致僵尸进程 | Sidecar 必须在 5s 内响应 SIGTERM，否则被 SIGKILL |
| 6 | 声明过多权限（应遵循最小权限原则） | 一个纯流量统计插件声明了 `write-config` 和 `network-access` |

> 完整约束细节见 `AGENTS/02-CONSTRAINTS.md`。插件架构背景见 `AGENTS/01-ARCHITECTURE.md#二模块一客户端智能-frpc-包装器`。安全审查清单见 `AGENTS/08-SECURITY.md`。
