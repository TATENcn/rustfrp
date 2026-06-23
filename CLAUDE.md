# RustFRP — AI Agent 入口路由

> 本文档在每次对话开始时自动加载。不包含具体规则——规则由 Skill 在代码改动时注入。
> 详细文档索引见 `AGENTS/README.md`。

## 项目铁律（每次任务前检查）

1. **单体优先** — 一个二进制 + 一个 SQLite，不引入 etcd/K8s/消息队列
2. **SQLite 唯一真理** — 绝不从 TOML 反写 SQLite
3. **监控只读无状态** — 监控宕机不影响穿透
4. **微内核克制** — 核心只做 CRUD + TOML 生成 + 进程守护，不含 UI/业务逻辑
5. **1:1 映射 FRP** — 表字段必须对应 FRP 官方 TOML 规范，不发明新字段
6. **原子操作** — 文件写入走 tmp+rename，进程启停走完整信号链
7. **插件隔离** — 插件崩溃不拖垮核心

## 任务路由

| 改动范围 | 必读文档 |
|---|---|
| `crates/rustfrp-core/` | `AGENTS/01-ARCHITECTURE.md` 第二章 + `AGENTS/02-CONSTRAINTS.md` 全部 P0 + `AGENTS/03-DEVELOPMENT.md` 第二节 |
| `plugins/` 或 `crates/rustfrp-sdk/` | `AGENTS/01-ARCHITECTURE.md` 第二章插件部分 + `AGENTS/02-CONSTRAINTS.md` PLG-001~005 + `AGENTS/03-DEVELOPMENT.md` 第三节 + `AGENTS/08-SECURITY.md` |
| `plugins/gui/src/`（前端） | `AGENTS/07-UI-DESIGN.md` + `AGENTS/04-DEPENDENCIES.md` 第六章 + `AGENTS/03-DEVELOPMENT.md` 第十节 |
| `crates/rustfrp-monitor/` | `AGENTS/01-ARCHITECTURE.md` 第四章 + `AGENTS/02-CONSTRAINTS.md` ARCH-007 + PERF-004 |
| `crates/rustfrp-frp/` | `AGENTS/03-DEVELOPMENT.md` 第十二节 |
| `AGENTS/`（文档目录） | `AGENTS/README.md` 文档修改权限表（宪法级文件不可直接修改） |
| 跨目录大范围改动 | `AGENTS/README.md` 快速启动指南 → 按涉及模块查上表 |

## 文档修改权限

| 级别 | 文件 | 规则 |
|---|---|---|
| **宪法（禁改）** | `01-ARCHITECTURE.md`, `02-CONSTRAINTS.md`, `08-SECURITY.md` | Agent 不可修改。需人工 PR + owner 审批 |
| **参考（可改）** | `03-DEVELOPMENT.md`, `04-DEPENDENCIES.md` | 可修改，附理由 |
| **运维（可改）** | `05-CICD.md`, `06-DEPLOYMENT.md` | 可修改，需 CI 验证 |
| **设计（有限改）** | `07-UI-DESIGN.md` | 可新增内容，改路由/组件树需人工确认 |

## 开发命令速查

```
just dev           # 启动 GUI 开发模式
just test-fast     # 单元测试（每次 push 前跑）
just test-all      # 全量测试（含集成测试）
just lint          # fmt --check + clippy -- -D warnings
just build-release # 发布构建
```
