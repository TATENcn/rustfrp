# 系统架构设计

## 一、架构总览

项目采用 **微内核 + 插件化** 的单体架构。一个二进制 + 一个 SQLite 数据库即可运行，不引入微服务、分布式集群、消息队列等任何重量级中间件。

核心公式：

> **SQLite 是真理来源 → TOML 是运行时产物 → 原生 FRP 是穿透引擎 → Pull 模式独立监控**

系统由三个独立模块组成：

```
┌──────────────────────────────────────────────────────┐
│         中央监控服务器 (Observability Center)          │
│         Pull 模式 · 只读 · 绝对无状态                   │
│         Prometheus + Grafana                         │
└──────────────────────┬───────────────────────────────┘
                       │ Pull /metrics（超时 3s，熔断）
          ┌────────────┼────────────┐
          ▼            ▼            ▼
 ┌───────────┐  ┌───────────┐  ┌───────────┐
 │  FRPS #1  │  │  FRPS #2  │  │  FRPS #N  │  数据面
 │ /metrics  │  │ /metrics  │  │ /metrics  │  原生 frps，零侵入，零依赖
 └───────────┘  └───────────┘  └───────────┘
       ▲               ▲              ▲
       │               │              │  FRP 原生 Proxy Group
       └───────────────┼──────────────┘  （未来 HA 插件驱动）
                       │
 ┌─────────────────────┴──────────────────────────┐
 │           客户端（智能 frpc 包装器）               │
 │                                                  │
 │  ┌──────────┐   ┌──────────┐   ┌─────────────┐  │
 │  │  SQLite  │──▶│ TOML 生成 │──▶│ 原生 frpc    │  │
 │  │(真理来源) │   │(运行时产物)│   │(子进程)      │  │
 │  └──────────┘   └──────────┘   └─────────────┘  │
 │                                                  │
 │  GUI: Tauri v2（Web 前端 + Rust 后端）             │
 │  插件运行时: Wasmtime（WASM）/ libloading（动态库） │
 └──────────────────────────────────────────────────┘
```

---

## 二、模块一：客户端（智能 frpc 包装器）

**定位**：配置管理中心 + 进程守护 + 运行时生成器。不处理任何流量转发。

### 2.1 核心职责

| 职责 | 实现 |
|---|---|
| 高自由度配置管理 | FrpsProfile × LocalProxy × BindingRule 多对多解耦模型，支持模板克隆、多环境切换 |
| 本地真理引擎 | SQLite 单文件存储全部配置，提供 CRUD 和版本历史 |
| 运行时生成器 | 从 SQLite 读取 → serde 序列化 → 生成标准 frpc.toml |
| 进程守护 | tokio::process 管理子进程；配置变更后原子写入 TOML → 发 SIGHUP 热重载 |
| 插件化扩展 | Wasmtime 运行 WASM 插件 / libloading 加载动态库 / Sidecar 独立子进程 |

### 2.2 数据模型（SQLite，唯一真理来源）

三张表完全解耦，实现多对多自由绑定。**字段必须 1:1 对应 FRP 官方 TOML 规范，不发明任何 FRP 不认识的字段**。

```
FrpsProfile（服务端连接配置）
├── id                INTEGER PRIMARY KEY
├── name              TEXT    （"公司服务器"、"家庭 NAS"）
├── server_addr       TEXT
├── server_port       INTEGER
├── token             TEXT
├── tls_enable        INTEGER
├── transport_protocol TEXT
├── ...               （其余字段与 FRP 官方 TOML 规范一一对应）
└── created_at / updated_at

LocalProxy（本地代理配置）
├── id                INTEGER PRIMARY KEY
├── name              TEXT    （"办公室 RDP"、"家庭摄像头"）
├── type              TEXT    （tcp / udp / http / https / stcp / xtcp）
├── local_ip          TEXT
├── local_port        INTEGER
├── remote_port       INTEGER
├── custom_domains    TEXT
├── health_check_type TEXT
├── ...
└── created_at / updated_at

BindingRule（绑定规则，多对多关联）
├── id                INTEGER PRIMARY KEY
├── profile_id        INTEGER → FrpsProfile.id
├── proxy_id          INTEGER → LocalProxy.id
├── enabled           INTEGER (0/1)
├── priority          INTEGER
├── group_name        TEXT    （Proxy Group，HA 场景用）
├── group_key         TEXT
└── created_at / updated_at
```

### 2.3 SQLite → TOML 双轨制（核心机制）

```
用户在 GUI 操作
       ↓
   SQLite（真理来源，人的操作台）
       ↓ serde 序列化
   .frpc.toml.tmp（临时文件）
       ↓ 原子重命名
   frpc.toml（运行时产物，FRP 的控制台）
       ↓
   原生 frpc 读取启动
```

**硬性规则**：

- **禁止双向同步**：系统绝不反向解析 TOML 写回 SQLite。用户绝不手动编辑生成的 TOML。SQLite 是唯一真理来源。
- **原子写入**：先写 `.frpc.toml.tmp`，成功后才 OS 原子重命名替换。防止中途崩溃损坏配置。
- **启动前 Schema 校验**：生成 TOML 前校验 IP 格式、端口范围、必填字段。防止无效配置导致 frpc 启动失败。

### 2.4 进程管理

Rust 包装器通过 `tokio::process` 拉起原生 frpc 子进程。

| 阶段 | 操作 |
|---|---|
| 启动 | SQLite 读取 → Schema 校验 → 原子写入 TOML → 启动 frpc 子进程 |
| 热重载 | GUI 保存 → 更新 SQLite → 原子写入新 TOML → 发 SIGHUP |
| 退出 | 捕获退出信号 → 向 frpc 发 SIGTERM → 等 3 秒 → 未退出则 SIGKILL |

**异常处理**：

- frpc 非零退出 → 捕获 Stderr → GUI 弹窗
- SIGHUP 被拒绝（配置错误）→ frpc 继续用旧配置 → 捕获 Stderr → GUI 警告
- 包装器崩溃 → OS 自动回收 frpc 子进程，不产生僵尸

### 2.5 插件架构

核心只做 SQLite → TOML → 进程管理。高级功能全部插件化，按需加载。

```
┌──────────────────────────────────┐
│          插件层（按需加载）         │
│  ┌────────┐ ┌────────┐ ┌──────┐  │
│  │Failover│ │流量统计│ │消息推送│  │
│  │HA 插件 │ │ 插件   │ │ 插件  │  │
│  └────────┘ └────────┘ └──────┘  │
│  ┌────────┐                      │
│  │ eBPF   │  ...                 │
│  │可观测性│                      │
│  └────────┘                      │
├──────────────────────────────────┤
│         微内核（Core）             │
│  SQLite CRUD + TOML 生成          │
│  + 进程守护 + 插件管理器           │
└──────────────────────────────────┘
```

**三种插件形态**：

| 形态 | 运行时 | 场景 | 隔离级别 |
|---|---|---|---|
| WASM | Wasmtime | 流量统计、测速算法、配置校验等纯逻辑 | 沙箱，仅通过 Host Functions 读状态 |
| 动态库 | libloading | GUI 渲染、硬件交互等需原生性能 | 进程内，严格权限控制 |
| Sidecar | Stdio/Socket | 消息推送、第三方 API 对接 | 进程级隔离 |

**插件规则**：

- WASM 插件严格限制 Host Functions：只读状态，不可操作网络和文件系统。
- 插件管理器支持热插拔。
- 插件暴露的 API（WIT 定义）一旦发布必须向后兼容。

---

## 三、模块二：服务端（极简 frps 节点）

**定位**：纯粹的数据转发面。完全保留原生 FRP，零侵入、零包装。

| 事项 | 做法 |
|---|---|
| 部署 | 原生 frps 二进制，不做修改、不嵌入、不包装 |
| 可观测性 | 开启 FRP 原生 /metrics 端点 |
| 依赖 | 零额外依赖，不引入数据库/MQ/配置中心 |

FRPS 间的负载均衡通过 FRP 原生 Proxy Group 实现，由客户端配置驱动，服务端无感知。此能力作为未来可选插件。

---

## 四、模块三：中央监控服务器（Observability Center）

**定位**：绝对只读、绝对无状态的全局可观测中心。

| 事项 | 做法 |
|---|---|
| 数据采集 | 定时 Pull 各 FRPS 的 `/metrics`，绝不接收 Push |
| 可视化 | Prometheus + Grafana 渲染连接数/带宽/健康度 |
| 状态 | 不存业务配置，不提供配置下发接口，不向被监控节点回写数据 |

**数据流**：`FRPS /metrics → Prometheus → Grafana 大盘`

**硬性规则**：

- `/metrics` 默认无鉴权，必须内网拉取或前置 Basic Auth 反向代理。
- 拉取超时 3 秒。连续失败 N 次 → 标记离线 → 降低该节点拉取频率。
- 监控服务器宕机不影响 FRPS/FRPC 穿透，互不依赖。

---

## 五、明确不做（设计底线）

以下方案经评估后明确排除，任何开发阶段均不引入：

| 排除项 | 原因 |
|---|---|
| 服务端 TUI 配置编辑 | TUI 只适合监控，不适合复杂表单。配置管理统一走客户端 GUI |
| 客户端侧负载均衡 | FRP 是隧道工具非流量调度器。HA 通过 Proxy Group + health_check |
| DDNS 做灾害转移 | DNS TTL 导致切换延迟不可控（分钟到小时级），只做寻址不做容灾 |
| 环境变量传配置 | OS 大小限制；`/proc/pid/environ` 泄露敏感信息；无法表达嵌套结构 |
| 中央控制面配发 | 控制面宕机 → 全部穿透中断，引入单点故障 |
| 微服务/集群化本体 | 工具本身是单体，不为 HA 引入 etcd/K8s/消息队列 |

---

## 六、按硬件能力的分级运行

| 场景 | 硬件 | 加载 | 内存 | 功能 |
|---|---|---|---|---|
| 嵌入式/老旧路由 | 128MB, ARMv7 | Core only | < 5MB | 纯粹 FRP 守护，基础热重载 |
| 现代 NAS/软路由 | 2GB | Core + 流量监控 + 消息推送 | ~30MB | 历史流量、断线告警 |
| 个人电脑 | 8GB+ | Core + GUI + 全插件 | ~100MB | 完整桌面应用，拖拽配置，图表 |

---

## 七、技术栈

| 层 | 选型 |
|---|---|
| 语言 | Rust |
| GUI | Tauri v2 + Web 前端 |
| 配置存储 | SQLite（rusqlite） |
| 序列化 | serde + toml |
| 异步 | tokio |
| WASM 运行时 | Wasmtime |
| 动态库加载 | libloading |
| 内存分配 | jemalloc 或 mimalloc |
| 监控采集 | FRP 原生 /metrics + Prometheus |
| 前端图表 | ECharts 或 Chart.js |
| 交叉编译 | cross + GitHub Actions（armv7/aarch64/x86_64） |

---

## 八、开发路线图

| 阶段 | 目标 | 产出 |
|---|---|---|
| Phase 1 | 微内核 + GUI 客户端 | SQLite CRUD、TOML 生成、frpc 进程守护、基础 Tauri GUI |
| Phase 2 | 客户端智能化 | 热重载、Schema 校验、本地流量图表、多环境切换 |
| Phase 3 | 中央监控 | Pull 采集、全局大盘、离线告警 |
| Phase 4 | 插件系统 | Wasmtime 集成、插件管理器、官方 Failover 插件 |
| Phase 5 | 企业级（未来） | eBPF 可观测性、多租户、Zero Trust |

---

## 九、架构决策记录（ADR）

以下决策是架构设计推敲后的最终结论。每个决策意味着对其他方案的明确否定，后续开发不应重新讨论已否决方案。

### ADR-001：子进程管理 FRP 二进制，而非库集成

- **决策**：通过 `tokio::process` 以子进程方式运行原生 frpc/frps 二进制
- **否决方案**：将 FRP 源码编译进本项目
- **理由**：
  1. 协议兼容性——使用官方二进制确保与 FRP 生态 100% 兼容
  2. 版本灵活性——用户可自由切换 FRP 版本，不受本项目发布周期约束
  3. 稳定性隔离——FRP 崩溃不拖垮管理进程，守护进程可自动重启
  4. 不重新发明轮子——本项目价值在"管理"，不在"穿透"

### ADR-002：WASM 作为主要插件格式

- **决策**：以 WASM (Wasmtime) 为默认插件运行时，同时保留动态库和 Sidecar 作为补充
- **否决方案**：纯动态库插件、脚本语言插件（Lua/Python）
- **理由**：
  1. 沙箱安全——WASM 插件无法直接访问文件系统和网络，需通过 Host Functions
  2. 跨平台——一次编译，所有目标架构可用
  3. 体积小——典型 WASM 插件 < 1MB

### ADR-003：单体部署，不微服务化

- **决策**：一个二进制 + 一个 SQLite 文件即可运行全部功能
- **否决方案**：将配置管理、监控、插件管理拆分为独立微服务
- **理由**：
  1. 目标场景是路由器/NAS/个人电脑，不是数据中心
  2. 引入 etcd/K8s/消息队列会彻底破坏"下限极低"的核心定位
  3. 单体架构的运维复杂度远低于分布式系统

### ADR-004：服务端零包装

- **决策**：服务端使用未经修改的原生 frps 二进制，本项目不做任何包装
- **否决方案**：为 frps 开发 TUI 管理界面
- **理由**：
  1. TUI 适合状态监控，不适合复杂表单配置——配置管理统一走客户端 GUI
  2. 零侵入意味着 frps 可独立升级，不受本项目制约
  3. 服务端只需暴露 /metrics，观测需求已满足
