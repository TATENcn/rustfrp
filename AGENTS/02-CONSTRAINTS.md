---
doc_id: 02-CONSTRAINTS
version: 1.0.0
last_modified: 2026-06-23
modification_policy: constitution
---

# 开发约束

约束分两级：**P0（强制，违反即阻断）** 和 **P1（推荐，违反需说明理由）**。

---

## 约束速查索引

### P0（强制）

| ID | 内容 | 位置 |
|---|---|---|
| ARCH-001 | 核心最小化，不含 UI 和业务逻辑 | [→](#一架构约束) |
| ARCH-002 | 插件隔离，崩溃不影响核心 | [→](#一架构约束) |
| ARCH-003 | SQLite 唯一真理来源；仅允许显式一次性迁移导入 | [→](#一架构约束) |
| ARCH-004 | 字段 1:1 映射 FRP 规范 | [→](#一架构约束) |
| ARCH-005 | Profile/Proxy/Binding 三表解耦 | [→](#一架构约束) |
| ARCH-006 | FRP 子进程运行，优雅退出 | [→](#一架构约束) |
| ARCH-007 | 监控只读无状态，配置下发走 Agent Pull（监控不写回，Agent 自主拉取） | [→](#一架构约束) |
| ARCH-008 | 单体部署，不引入中间件 | [→](#一架构约束) |
| ARCH-009 | 支持多 frpc 实例（一个 Profile 一个进程） | [→](#一架构约束) |
| ARCH-010 | FrpsProfile 与 LocalProxy 通过 BindingRule 多对多解耦 | [→](#一架构约束) |
| PERF-001 | Core < 10MB，低配设备可用 | [→](#二性能约束) |
| PERF-002 | 配置原子生成，热重载不断连 | [→](#二性能约束) |
| PERF-003 | TOML 原子写入（tmp + rename） | [→](#二性能约束) |
| PLG-001 | manifest.json 必填 | [→](#三插件约束) |
| PLG-002 | 权限声明与调用前校验 | [→](#三插件约束) |
| PLG-003 | 完整生命周期 | [→](#三插件约束) |
| PLG-004 | 禁止 panic | [→](#三插件约束) |
| PLG-005 | 资源限制 | [→](#三插件约束) |
| CODE-006 | 代码字符串全英文 | [→](#四代码规范) |
| CODE-007 | 禁止 query_map 静默吞错 | [→](#四代码规范) |
| CFG-001 | LocalProxy 支持 FRP 原生插件配置（plugin_config JSON blob） | [→](#五配置约束) |

### P1（推荐）

| ID | 内容 | 位置 |
|---|---|---|
| PERF-004 | 监控拉取超时 3s，熔断 | [→](#二性能约束) |
| PLG-006 | 插件 API 向后兼容 | [→](#三插件约束) |
| PLG-007 | Permission 枚举在 core 统一定义，sdk 通过 re-export 引用，禁止重复 | [→](#三插件约束) |
| CODE-001 | 异步优先 | [→](#四代码规范) |
| CODE-002 | Result 错误处理 | [→](#四代码规范) |
| CODE-003 | 文档注释 | [→](#四代码规范) |
| PERF-005 | 日志文件 append 模式，不截断历史日志 | [→](#二性能约束) |

---

## 一、架构约束

### ARCH-001：微内核最小化

**优先级**：P0

**规定**：
- 核心层只包含：SQLite CRUD、TOML 序列化生成、frpc/frps 进程管理、插件管理器
- 核心层禁止包含：任何 UI 代码、业务逻辑（流量统计/智能调度/告警推送等）
- 业务功能一律作为插件实现

**验证**：审查 `core/` 目录，非上述四类的模块需移到插件层。

---

### ARCH-002：插件隔离

**优先级**：P0

**规定**：
- WASM 插件在 Wasmtime 沙箱中运行，仅通过 Host Functions 与核心交互
- 动态库插件通过 C ABI 接口交互
- Sidecar 插件作为独立进程运行
- 插件崩溃不得导致核心进程退出

**验证**：单元测试中模拟插件 panic，断言核心仍正常运行。

---

### ARCH-003：SQLite 为唯一真理来源

**优先级**：P0

**规定**：
- SQLite 是配置的唯一持久化存储
- 禁止运行时自动或持续执行 TOML → SQLite 双向同步
- 允许用户显式执行一次性迁移导入；导入必须先解析和校验，并在单个事务中写入 SQLite
- 导入完成后，源 TOML 不再参与运行时配置管理，也不会被持续监听
- TOML 文件仅为运行时产物，每次启动/重载时从 SQLite 重新生成
- 用户永远不需要手动编辑生成的 TOML 文件

**验证**：除显式迁移入口外，不得存在 TOML → SQLite 写入路径；导入失败测试必须断言事务完整回滚。

---

### ARCH-004：数据模型 1:1 映射 FRP 规范

**优先级**：P0

**规定**：
- SQLite 中 FrpsProfile / LocalProxy 表的字段必须与 FRP 官方 TOML 规范一一对应
- 不发明任何 FRP 不支持的配置项
- 工具的角色是"翻译器"——把 GUI 表单翻译为标准 FRP TOML

**验证**：对比 FRP 官方文档的 TOML 字段列表与 SQLite 表结构，新增字段需对应。

---

### ARCH-005：配置模型三表解耦

**优先级**：P0

**规定**：
- FrpsProfile（服务端连接配置）、LocalProxy（本地代理配置）、BindingRule（绑定规则）必须分为三张独立表
- 禁止将 Profile 和 Proxy 合并存储
- BindingRule 支持多对多关联

**验证**：

```sql
-- 必须存在这三张独立表
SELECT name FROM sqlite_master WHERE type='table'
  AND name IN ('frps_profile', 'local_proxy', 'binding_rule');
-- 结果必须恰好 3 行
```

---

### ARCH-006：进程管理

**优先级**：P0

**规定**：
- FRP 二进制以子进程方式运行，不编译进核心层
- 支持多版本 FRP 二进制
- 优雅退出：SIGTERM → 等 3 秒 → SIGKILL
- 崩溃自动重启最多 3 次

**验证**：单元测试中 kill 子进程，断言 5 秒内自动重启。

---

### ARCH-007：监控只读无状态，配置下发走 Agent Pull

**优先级**：P0

**规定**：
- 监控服务器（rustfrp-monitor）绝对只读，拉取 /metrics 不写回任何数据
- 控制面可提供配置模板 API（只读 GET），供 frps-agent 定期拉取
- Agent 宕机 → frps 继续运行；控制面宕机 → Agent 缓存上次配置
- 配置变更只能通过客户端 GUI 用户主动触发
- **禁止**控制面直接向 frps 进程写入配置或发信号
- 监控服务器宕机不得影响 FRPS/FRPC 穿透

**验证**：停掉监控服务器，确认穿透链路正常。

---

### ARCH-008：单体部署

**优先级**：P0

**规定**：
- 项目作为单体应用运行，一个二进制 + 一个 SQLite 文件即可
- 不引入 etcd、Consul、消息队列、分布式数据库等重量级中间件
- 不为 HA 而将工具本体微服务化

---

### ARCH-009：支持多 frpc 实例

**优先级**：P0

**规定**：
- 系统支持同时运行多个 frpc 子进程，每个对应一个 FrpsProfile
- 一个 FrpsProfile → 一个 `frpc_{name}.toml` → 一个 ProcessGuard 实例
- ProcessManager 管理所有 ProcessGuard 的生命周期
- 新增/修改/删除 Profile 时自动增减对应的 frpc 实例

**验证**：测试中同时创建 3 个 Profile 并绑定 Proxy，断言 3 个 frpc.toml 文件和 3 个进程 Guard。

---

### ARCH-010：FrpsProfile / LocalProxy / BindingRule 三表多对多解耦

**优先级**：P0

**规定**：
- 一个 FrpsProfile 可绑定多个 LocalProxy
- 一个 LocalProxy 可绑定到多个 FrpsProfile（同一服务暴露到多个服务器）
- 绑定关系通过 BindingRule 表达，包含 enabled / priority / group 元数据
- TOML 生成时按 Profile 分组，每组生成独立的 TOML 文件

**验证**：创建一个 Profile 绑定 2 个 Proxy，再创建第二个 Profile 绑定其中 1 个 Proxy + 另 1 个新 Proxy，断言 TOML 分组正确。

---

## 二、性能约束

### PERF-001：低配设备可用

**优先级**：P0

**规定**：
- 仅加载 Core（无插件）时：内存 < 10MB，CPU 空闲时 < 2%
- 目标：128MB 内存的 ARMv7 路由器上能稳定运行
- 使用 jemalloc 或 mimalloc 替换系统默认分配器

**验证**：在目标设备或模拟环境中运行，监控 RSS 和 CPU 占用。

---

### PERF-002：配置生成与热重载

**优先级**：P0

**规定**：
- 从 SQLite 读取并生成 TOML 的过程应是毫秒级
- 热重载不得中断已有的正常运行连接
- 热重载失败（配置错误）→ frpc 继续使用旧配置，GUI 弹窗提示

**验证**：生成 100 条代理规则对应的 TOML 并计时；热重载期间检查已有连接状态。

---

### PERF-003：原子写入 I/O

**优先级**：P0

**规定**：
- TOML 生成必须先写临时文件，成功后再原子重命名
- 禁止直接覆盖正在使用的 frpc.toml

**验证**：检查代码中 TOML 写入路径，确认使用 `write(tmp) + rename` 模式。

---

### PERF-004：IPC 与监控拉取

**优先级**：P1

**规定**：
- 监控服务器拉取 /metrics 超时设为 3 秒
- 单节点拉取失败不影响其他节点的采集
- 连续失败 N 次 → 降低该节点拉取频率（熔断）

---

### PERF-005：日志文件 append 模式

**优先级**：P1

**规定**：
- frpc 子进程的 stdout/stderr 日志文件使用 append 模式打开
- 禁止使用 `File::create()` 截断已有日志
- 使用 `OpenOptions::new().append(true).create(true).open()`

**验证**：检查 `process/guard.rs` 中所有 `File::create` 调用均已替换为 append 模式。

---

## 三、插件约束

### PLG-001：清单文件

**优先级**：P0

**规定**：每个插件必须提供 `manifest.json`：

```json
{
  "name": "plugin-name",
  "version": "1.0.0",
  "type": "wasm | native | sidecar",
  "entry": "plugin.wasm | plugin.so | plugin",
  "description": "...",
  "permissions": ["read-traffic", "subscribe-events"],
  "dependencies": [],
  "min_core_version": "1.0.0"
}
```

必填字段：`name`（唯一）、`version`（语义化）、`type`、`entry`、`permissions`。

---

### PLG-002：权限声明与校验

**优先级**：P0

**规定**：

权限类型：
- `read-config` — 读取配置
- `write-config` — 修改配置
- `read-traffic` — 读取流量数据
- `control-process` — 启停 FRP 进程
- `subscribe-events` — 订阅事件
- `network-access` — 网络访问
- `filesystem-access` — 文件系统访问

插件在 manifest 中声明权限，核心层在每次调用前校验。权限不足 → 操作被拒绝并记录日志。

---

### PLG-003：生命周期

**优先级**：P0

**规定**：插件必须实现 `init → start → (运行) → stop → unload` 完整生命周期。

调用顺序约束：
- `init` 必须在 `start` 之前
- `stop` 必须在 `unload` 之前
- `init` 失败后不得调用 `start`
- 违反生命周期顺序 → 插件被卸载

---

### PLG-004：禁止 panic

**优先级**：P0

**规定**：
- 所有插件公共方法返回 `Result<T, PluginError>`
- 插件内部禁止 `unwrap()` 和 `expect()`（测试代码除外）
- 插件 panic 被捕获后强制卸载，不得影响核心

---

### PLG-005：资源限制

**优先级**：P0

**规定**：

| 资源 | 限制 |
|---|---|
| WASM 插件内存 | < 50MB |
| 文件描述符 | < 100 |
| 网络连接数 | < 10 |
| IPC 消息大小 | < 1MB |

超限 → 插件被强制停止。

---

### PLG-006：核心 API 向后兼容

**优先级**：P1

**规定**：插件 API 接口（WIT 定义）发布后保持向后兼容。若需 breaking change，必须通过版本号区分。

---

### PLG-007：Permission 枚举在 core 统一定义，sdk 通过 re-export 引用

**优先级**：P1

**规定**：Permission 枚举在 common 统一定义（`crates/common/src/plugin/manifest.rs`），sdk 通过 `pub use rustfrp_common::plugin::manifest::Permission;` 重新导出。禁止在 sdk 中重复定义 Permission 枚举。

**验证**：`grep -r 'pub enum Permission' crates/` 预期只匹配 common 中的定义。

---

## 四、代码规范

### CODE-001：异步优先

**优先级**：P1

**规定**：文件 I/O 使用 `tokio::fs`，网络 I/O 使用 `tokio::net`，进程 I/O 使用 `tokio::process`。初始化阶段可用同步 API。

### CODE-002：错误处理

**优先级**：P1

**规定**：
- 公共 API 全部返回 `Result<T, E>`
- 使用 `thiserror` 定义错误类型，携带上下文
- 使用 `?` 传播错误
- 用 `tracing` 记录结构化日志，敏感信息（Token/密码）需脱敏

### CODE-003：错误码规范

**优先级**：P0

**规定**：
- 所有 `CoreError` 变体必须实现 `code()` 方法，返回唯一错误码（如 `DB_001`、`CFG_001`、`PROC_001`）
- 错误码格式：`{模块}_{序号}`。模块前缀：`DB`（数据库）、`CFG`（配置）、`PROC`（进程）、`PLG`（插件）、`NET`（网络）
- 所有 `CoreError` 变体必须实现 `user_message_key()` 方法，返回 i18n 翻译键
- 错误消息区分两个层次：`Display` 给开发者（英文），`user_message_key()` 给用户（前端查 i18n 字典）
- 新增 CoreError 变体时必须同时新增错误码

**验证**：审查 `core/src/error.rs`，每个变体检查 `code()` 和 `user_message_key()` 均已实现。

---

### CODE-004：日志规范与脱敏

**优先级**：P0

**规定**：
- 所有日志使用 `tracing` 结构化日志，禁止使用 `println!` / `eprintln!`
- 敏感字段（Token、密码、TLS 私钥、用户自定义 Header）必须使用 `#[instrument(skip(token, ...))]` 跳过
- 日志级别遵循：`ERROR`=需人工介入 | `WARN`=自动恢复的异常 | `INFO`=关键生命周期 | `DEBUG`=问题定位细节 | `TRACE`=函数级调用链
- 生产环境日志输出 JSON 格式到文件（`~/.rustfrp/logs/`），开发环境输出人类可读格式到控制台
- frpc 子进程的 stdout/stderr 写入独立日志文件（`frpc.log` / `frpc_err.log`），核心层异步监视 stderr 中的错误关键词并升格为 WARN/ERROR

**验证**：审查所有 `tracing` 宏调用，确认敏感字段出现在日志中即违规。CI 中增加 Clippy lint 检查。


### CODE-005：文档

**优先级**：P1

**规定**：公共函数需有文档注释（功能、参数、返回值）。架构决策需有 ADR 记录。

---

### CODE-006：代码字符串全英文

**优先级**：P0

**规定**：
- 所有 Rust 代码中的字符串字面量必须为英文，包括但不限于：
  - `CoreError::Display` 信息（`#[error("...")]`）
  - `CoreError::ConfigValidation("...")` 和所有校验返回值
  - 插件 manifest 校验的返回消息（`Vec<String>`）
  - `tracing` 宏中的日志消息
  - `anyhow::bail!` / `anyhow::Context` 等错误上下文
- `user_message_key()` 使用英文 i18n key（面向翻译框架）
- 仅注释（`//` / `///` / `/* */`）和文档可使用中文（面向中文团队协作）
- 测试数据中的非关键字符串（如 `name` 字段）可以使用中文

**验证**：`grep -rP '[\x{4e00}-\x{9fff}]' crates/ --include='*.rs' | grep -v '//' | grep -v '///' | grep -v '/\*'` 预期零输出（注释内的中文除外）。

---

### CODE-007：禁止 query_map 静默吞错

**优先级**：P0

**规定**：
- 数据库 `query_map` 结果必须用 `collect::<Result<Vec<_>, _>>()` 向上传播错误
- 禁止使用 `.filter_map(|r| r.ok())` 丢弃反序列化错误
- 若确有业务理由必须跳过个别错误行，必须同时打 `tracing::warn!` 记录原因

**验证**：`grep -r 'filter_map.*\.ok()'` 在 db/ 目录下零匹配。

---

## 五、配置约束

### CFG-001：支持 FRP 原生插件配置

**优先级**：P0

**规定**：
- `LocalProxy` 数据模型必须包含 `plugin_config: Option<serde_json::Value>` 字段
- 字段 1:1 对应 FRP TOML 中的 `[proxies.plugin]` 段
- 写入时校验：若 `plugin_config` 不为 None，必须至少包含 `type` 字段
- TOML 生成时将其序列化到对应 `[[proxies]]` 条目下

**验证**：创建包含 `https2http` plugin_config 的 Proxy，断言生成的 TOML 包含 `[proxies.plugin]` 段。

---

## 六、约束速查表

| ID | 内容 | 级别 |
|---|---|---|
| ARCH-001 | 核心最小化，不含 UI 和业务逻辑 | P0 |
| ARCH-002 | 插件隔离，崩溃不影响核心 | P0 |
| ARCH-003 | SQLite 唯一真理来源；仅允许显式一次性迁移导入 | P0 |
| ARCH-004 | 字段 1:1 映射 FRP 规范 | P0 |
| ARCH-005 | Profile/Proxy/Binding 三表解耦 | P0 |
| ARCH-006 | FRP 子进程运行，优雅退出 | P0 |
| ARCH-007 | 监控只读无状态，配置下发走 Agent Pull | P0 |
| ARCH-008 | 单体部署，不引入中间件 | P0 |
| ARCH-009 | 支持多 frpc 实例（一个 Profile 一个进程） | P0 |
| ARCH-010 | Profile/Proxy/Binding 三表多对多解耦，TOML 按 Profile 分组 | P0 |
| PERF-001 | Core < 10MB，低配可用 | P0 |
| PERF-002 | 配置原子生成，热重载不断连 | P0 |
| PERF-003 | TOML 原子写入（tmp + rename） | P0 |
| PERF-004 | 监控拉取超时 3s，熔断 | P1 |
| PERF-005 | 日志文件 append 模式，不截断历史日志 | P1 |
| PLG-001 | manifest.json 必填 | P0 |
| PLG-002 | 权限声明与调用前校验 | P0 |
| PLG-003 | 完整生命周期 | P0 |
| PLG-004 | 禁止 panic | P0 |
| PLG-005 | 资源限制 | P0 |
| PLG-006 | 插件 API 向后兼容 | P1 |
| PLG-007 | Permission 枚举在 core 统一定义，sdk re-export | P1 |
| CODE-001 | 异步优先 | P1 |
| CODE-002 | Result 错误处理 | P1 |
| CODE-003 | 错误码规范 | P0 |
| CODE-004 | 日志规范与脱敏 | P0 |
| CODE-005 | 文档注释 | P1 |
| CODE-006 | 核心层错误/校验信息全英文 | P0 |
| CODE-007 | 禁止 query_map 静默吞错 | P0 |
| CFG-001 | 支持 FRP 原生插件配置（plugin_config JSON blob） | P0 |
