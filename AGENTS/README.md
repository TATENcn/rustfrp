# AGENTS — 参考文档索引

> 本目录存放 Agent 开发时需要的全部参考文档。接到任务后，按下面的路由表定位到正确的文档和章节。

## 项目铁律（每次任务前检查）

1. **单体优先** — 一个二进制 + 一个 SQLite，不引入 etcd/K8s/消息队列
2. **SQLite 唯一真理** — 绝不从 TOML 反写 SQLite
3. **监控只读无状态** — 监控宕机不影响穿透
4. **微内核克制** — 核心只做 CRUD + TOML 生成 + 进程守护，不含 UI/业务逻辑
5. **1:1 映射 FRP** — 表字段必须对 FRP 官方 TOML 规范，不发明新字段
6. **原子操作** — 文件写入走 tmp+rename，进程启停走完整信号链
7. **插件隔离** — 插件崩溃不拖垮核心

---

## 任务路由

| 改动范围                                  | 必读文档（按优先级排序）                                                                                                                               |
| ----------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `crates/rustfrp-core/**`                  | ① `01-ARCHITECTURE.md` 第二、四章 ② `02-CONSTRAINTS.md` 全部 P0（ARCH-001~008 + PERF-001~003 + CODE-003~004） ③ `03-DEVELOPMENT.md` 第二、五、六、七节 |
| `plugins/**` 或 `crates/rustfrp-sdk/**`   | ① `01-ARCHITECTURE.md` 第二章插件部分 ② `02-CONSTRAINTS.md` PLG-001~005 ③ `03-DEVELOPMENT.md` 第三节（三种插件模板） ④ `08-SECURITY.md` 全文           |
| `plugins/gui/src/**`（前端）              | ① `07-UI-DESIGN.md` ② `04-DEPENDENCIES.md` 第六章 GUI 依赖 ③ `03-DEVELOPMENT.md` 第十节（类型共享）                                                    |
| `crates/rustfrp-monitor/**`               | ① `01-ARCHITECTURE.md` 第四章 ② `02-CONSTRAINTS.md` ARCH-007 + PERF-004                                                                                |
| `crates/rustfrp-frp/**`                   | ① `03-DEVELOPMENT.md` 第十二节（版本管理）                                                                                                             |
| `AGENTS/**`（本文档目录）                 | 检查下方「文档修改权限」——此目录部分文件禁止 Agent 修改                                                                                                |
| FRP 功能适配（补齐缺失字段/类型/Visitor） | `09-FRP-ADAPTATION-PLAN.md` — 按 Phase 顺序实施                                                                                                        |
| 跨目录大范围改动                          | 先读完本文件全部路由条目，再按涉及模块逐项读取                                                                                                         |

---

## 文档修改权限

| 级别               | 文件                                                        | 规则                                                                   |
| ------------------ | ----------------------------------------------------------- | ---------------------------------------------------------------------- |
| **宪法（禁改）**   | `01-ARCHITECTURE.md`, `02-CONSTRAINTS.md`, `08-SECURITY.md` | Agent 不可修改。必须人工 PR + owner 审批，受 `.github/CODEOWNERS` 保护 |
| **参考（可改）**   | `03-DEVELOPMENT.md`, `04-DEPENDENCIES.md`                   | 可修改，附理由，PR 合入后审查                                          |
| **运维（可改）**   | `05-CICD.md`, `06-DEPLOYMENT.md`                            | 可修改，变更需 CI 验证通过                                             |
| **设计（有限改）** | `07-UI-DESIGN.md`                                           | 可新增内容，改页面路由/组件树需人工确认                                |

---

## 场景索引

**写核心代码时**：

1. 打开 `02-CONSTRAINTS.md` 末尾速查表，对照所有 P0 约束
2. 参考 `03-DEVELOPMENT.md` 第二节的代码模板（错误类型、SQLite 模型、TOML 生成器、进程守护、校验器）
3. TOML 写入必须走 `write(tmp) → rename`，进程退出必须走 SIGTERM → 等 3s → SIGKILL
4. 新增 CoreError 变体必须同时定义 `code()` 和 `user_message_key()`

**写插件时**：

1. 先确定插件类型（WASM / Native / Sidecar）→ 参考 `01-ARCHITECTURE.md` 第二章对比表
2. 创建 `manifest.json`，声明最小权限集
3. 实现完整生命周期 `init → start → stop → unload`
4. 禁止 `unwrap()` / `expect()`，所有公共方法返回 `Result`
5. 写完后用 `08-SECURITY.md` 的威胁矩阵逐项检查

**改数据库 Schema 时**：

1. 新字段必须对应 FRP 官方 TOML 规范（`02-CONSTRAINTS.md` ARCH-004）
2. 三表（frps_profile / local_proxy / binding_rule）独立，不合并（ARCH-005）
3. 迁移只做增量，checksum 校验每次启动执行（`03-DEVELOPMENT.md` 第七节）

**新增依赖时**：

1. 核心层尽量不加依赖，不引入网络 I/O 类 crate
2. 评估体积和编译时间影响
3. 记录到 `04-DEPENDENCIES.md`

---

## 开发命令速查

```
just dev           # 启动 GUI 开发模式
just test-fast     # 单元测试（快，每次 push 前跑）
just test-all      # 全量测试（含集成测试）
just lint          # fmt --check + clippy -- -D warnings
just build-release # 发布构建
```

完整命令列表见 `03-DEVELOPMENT.md` 第九章。
