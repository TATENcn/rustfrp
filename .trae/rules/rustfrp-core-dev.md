---
alwaysApply: true
---

# RustFRP Core 开发约束

> **适用场景**：编写或修改 `crates/rustfrp-core/` 下的任何 `.rs` 文件时，必须遵守以下 P0 级强制约束。
> 本文档是速查表，完整细节见 `AGENTS/02-CONSTRAINTS.md`。

---

## P0 约束速查表

| ID | 约束 | 文档 |
|---|---|---|
| ARCH-001 | 核心最小化：只含 SQLite CRUD + TOML 生成 + 进程守护 + 插件管理，禁止 UI 和业务逻辑 | `AGENTS/02-CONSTRAINTS.md#一架构约束` |
| ARCH-002 | 插件隔离：插件崩溃不得导致核心进程退出，WASM 沙箱 / Native C ABI / Sidecar 进程级隔离 | `AGENTS/02-CONSTRAINTS.md#一架构约束` |
| ARCH-003 | SQLite 为唯一真理来源：**禁止从 TOML 反向解析写入 SQLite**，TOML 仅为运行时产物 | `AGENTS/02-CONSTRAINTS.md#一架构约束` |
| ARCH-004 | 字段 1:1 映射 FRP 官方 TOML 规范：不发明 FRP 不支持的配置项 | `AGENTS/02-CONSTRAINTS.md#一架构约束` |
| ARCH-005 | 三表独立解耦：frps_profile / local_proxy / binding_rule 必须分表，禁止合并 | `AGENTS/02-CONSTRAINTS.md#一架构约束` |
| ARCH-006 | FRP 子进程运行：SIGTERM → 等 3s → SIGKILL，崩溃重启 ≤3 次 | `AGENTS/02-CONSTRAINTS.md#一架构约束` |
| ARCH-007 | 监控与配置分离：监控只读不写，监控宕机不影响穿透 | `AGENTS/02-CONSTRAINTS.md#一架构约束` |
| ARCH-008 | 单体部署：不引入 etcd / K8s / 消息队列等中间件 | `AGENTS/02-CONSTRAINTS.md#一架构约束` |
| PERF-001 | Core 内存 < 10MB，低配设备（128MB ARMv7）可用 | `AGENTS/02-CONSTRAINTS.md#二性能约束` |
| PERF-002 | 热重载不得中断已有连接；重载失败 → 继续用旧配置 | `AGENTS/02-CONSTRAINTS.md#二性能约束` |
| PERF-003 | TOML 原子写入：先写 `.frpc.toml.tmp`，成功后 `rename`，**禁止直接 write 目标文件** | `AGENTS/02-CONSTRAINTS.md#二性能约束` |
| CODE-003 | 所有 CoreError 变体必须实现 `code()` 和 `user_message_key()` | `AGENTS/02-CONSTRAINTS.md#四代码规范` |
| CODE-004 | 敏感字段（Token/密码/TLS 私钥）必须 `#[instrument(skip(...))]`，禁止 `println!` | `AGENTS/02-CONSTRAINTS.md#四代码规范` |

---

## 关键路径专项

### 1. TOML 生成路径 (`config/generator.rs`)

**正确模式**：
```rust
// 原子写入：先写 tmp，再 rename
fs::write(tmp_path, toml_content).await?;
fs::rename(tmp_path, output_path).await?;
```

**禁止模式**：
```rust
fs::write(output_path, toml_content).await?; // 直接覆盖！崩溃会损坏配置
```

**验证方法**：grep 代码中是否存在 `toml::from_str` → `INSERT/UPDATE` 的调用链（违反 ARCH-003）。

### 2. 进程管理路径 (`process/guard.rs`)

**正确模式**：
```rust
// 优雅退出
child.send_signal(Signal::SIGTERM)?;
match tokio::time::timeout(Duration::from_secs(3), child.wait()).await {
    Ok(Ok(status)) => { /* 正常退出 */ }
    Ok(Err(e)) => { /* 进程错误 */ }
    Err(_elapsed) => { child.kill().await?; } // 超时 → SIGKILL
}
```

**禁止模式**：
```rust
child.kill().await?; // 跳过 SIGTERM，直接 SIGKILL
child.wait().await?; // 无超时等待，可能永久阻塞
```

**崩溃重启**：上限 3 次，超过则停止重启 + 记录 ERROR 日志。frpc 子进程 stderr 必须异步监视，错误关键词升格为 WARN/ERROR。

### 3. 插件管理路径 (`plugin/`)

- 插件 panic 必须被 `std::panic::catch_unwind` 捕获，不得导致核心退出
- 插件管理器支持热插拔（load / unload 不重启核心）

### 4. 数据库路径 (`db/`)

- 三表独立：`frps_profile` / `local_proxy` / `binding_rule`
- 迁移只做增量（`ALTER TABLE ... ADD COLUMN`），不改已有列，checksum 校验每次启动执行
- 禁止在核心层直接读写 TOML 文件（一切通过 SQLite → generator 路径）

---

## 常见陷阱

| # | 陷阱 | 反面示例 |
|---|---|---|
| 1 | 在核心层引入 UI 依赖（如直接依赖 Tauri 的 `tauri::command`） | `use tauri::command` 出现在 core crate 中 |
| 2 | 从 TOML 文件反序列化后写入 SQLite | `let cfg: Config = toml::from_str(&s)?; db.insert(&cfg)?;` |
| 3 | TOML 写入跳过 tmp+rename，直接覆盖 | `fs::write("frpc.toml", content)` |
| 4 | 子进程退出不使用超时，直接 `wait()` | `child.wait().await` 无 timeout 包裹 |
| 5 | 新增 CoreError 变体忘记实现 `code()` 和 `user_message_key()` | `CoreError::NewVariant` 缺少这两个方法 |
| 6 | 日志中打印 Token 或密码原文 | `info!(token = raw_token, "connecting")` |

> 完整约束细节见 `AGENTS/02-CONSTRAINTS.md`。架构背景见 `AGENTS/01-ARCHITECTURE.md`。
