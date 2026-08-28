---
doc_id: 07-UI-DESIGN
version: 1.0.0
last_modified: 2026-06-23
modification_policy: design
summary: 页面路由、布局结构、组件树、关键交互设计、i18n 架构、前端技术选型
---

# 界面设计

## 一、页面路由

```
/                          → 仪表盘（Dashboard）
/servers                   → 服务端列表（FrpsProfile 管理）
/servers/:id               → 服务端详情/编辑
/proxies                   → 代理规则列表（LocalProxy 管理）
/proxies/:id               → 代理规则详情/编辑
/bindings                  → 绑定规则（关联 Profile 和 Proxy）
/bindings/:id              → 绑定详情
/settings                  → 全局设置（主题、语言、开机自启、FRP 版本）
/settings/frp-version      → FRP 版本管理
/logs                      → 日志查看
/debug                     → 调试面板（仅 dev 构建，生产构建不可见）
/about                     → 关于
```

## 二、主布局

```
┌──────────────────────────────────────────────────────────┐
│  [Logo]  RustFRP Manager      [在线/离线]  [设置齿轮] [×] │  ← 自定义标题栏
├──────────┬───────────────────────────────────────────────┤
│          │                                               │
│  仪表盘  │             主内容区（<router-view>）             │
│  服务端  │                                               │
│  代理    │    ┌─────────────────────────────┐            │
│  绑定    │    │                             │            │
│  日志    │    │       卡片 / 表格 / 图表      │            │
│  设置    │    │                             │            │
│          │    └─────────────────────────────┘            │
│          │                                               │
│          │                                               │
├──────────┴───────────────────────────────────────────────┤
│  [🟢 frpc 运行中]  3 代理活跃  ↑ 1.2 MB/s  ↓ 0.8 MB/s    │  ← 状态栏
└──────────────────────────────────────────────────────────┘
```

### 布局说明

| 区域 | 组件 | 行为 |
|---|---|---|
| 标题栏 | `TitleBar.vue` | 跨平台统一风格（不使用系统原生标题栏，确保 Windows/macOS/Linux 一致）。显示连接状态灯 |
| 侧边栏 | `Sidebar.vue` | 固定宽度 64px（收起）/ 200px（展开）。包含导航项 + frpc 状态指示器 |
| 内容区 | `<router-view>` | 页面切换有过渡动画（fade） |
| 状态栏 | `StatusBar.vue` | 始终可见。显示 frpc 运行状态、活跃代理数、实时带宽 |

## 三、组件树

```
App.vue
├── TitleBar.vue               # 自定义标题栏
│   ├── ConnectionIndicator    # 在线/离线状态灯
│   └── WindowControls         # 最小化/最大化/关闭
├── Sidebar.vue                # 左侧导航
│   ├── NavItem.vue            # 单个导航项（图标 + 文字）
│   └── FrpcStatusDot.vue      # frpc 运行状态指示灯
├── <router-view>              # 内容区
│   │
│   ├── DashboardView.vue      # / — 仪表盘
│   │   ├── TrafficChart.vue          # ECharts 实时流量折线图（上/下行）
│   │   ├── StatCard.vue              # 统计卡片（连接数/代理数/在线时长）
│   │   ├── ActiveBindingsList.vue    # 当前活跃绑定列表（表格）
│   │   └── FrpcStatusCard.vue        # 进程状态卡片（PID/内存/重启次数）
│   │
│   ├── ServersView.vue        # /servers — 服务端列表
│   │   ├── ServerCard.vue            # 单个服务端信息卡片
│   │   ├── ServerFormDialog.vue      # 新增/编辑服务端弹窗表单
│   │   └── ServerDeleteConfirm.vue   # 删除确认弹窗
│   │
│   ├── ProxiesView.vue        # /proxies — 代理列表
│   │   ├── ProxyCard.vue
│   │   ├── ProxyFormDialog.vue
│   │   └── ProxyDeleteConfirm.vue
│   │
│   ├── BindingsView.vue       # /bindings — 绑定管理
│   │   ├── BindingTable.vue          # 多对多关联表格（Profile × Proxy）
│   │   ├── BindingFormDialog.vue     # 创建绑定弹窗（选择 Profile + Proxy）
│   │   └── BindingToggle.vue         # 一键启用/禁用
│   │
│   ├── SettingsView.vue       # /settings — 设置
│   │   ├── ThemeToggle.vue           # 深色/浅色主题切换
│   │   ├── LanguageSelect.vue        # 中/英文切换
│   │   ├── AutoStartToggle.vue       # 开机自启开关
│   │   ├── FrpVersionSelect.vue      # FRP 版本选择
│   │   └── ConfigImportExport.vue    # 配置导入/导出
│   │
│   ├── LogsView.vue           # /logs — 日志
│   │   ├── LogLevelFilter.vue        # 日志级别过滤
│   │   └── LogTable.vue              # 日志列表（虚拟滚动）
│   │
│   └── DebugPanel.vue         # /debug — 调试面板（仅 #[cfg(debug_assertions)]）
│       ├── DbStateViewer.vue
│       ├── TomlPreview.vue
│       ├── FrpcProcessInfo.vue
│       └── LoadedPluginsList.vue
│
└── StatusBar.vue              # 底部状态栏
    ├── FrpcStatusText.vue
    ├── ActiveProxyCount.vue
    └── TrafficIndicator.vue
```

## 四、关键交互设计

| 交互 | 描述 | 实现方式 |
|---|---|---|
| 表单实时校验 | 输入 IP/端口/Token 时即时校验格式 | Rust 后端通过 Tauri command 校验（避免在前端重复校验逻辑），前端实时显示错误提示。输入防抖 300ms |
| 一键启停 | 仪表盘大按钮 `▶ 启动` / `⏹ 停止` | 操作有确认弹窗（防误触）。启动失败 → 弹窗展示 frpc stderr。停止走 SIGTERM → 等 → SIGKILL |
| 绑定创建 | 在绑定页面点击「新建绑定」→ 弹窗中选择 Profile（下拉）和 Proxy（下拉）→ 保存 | 两个级联下拉框，选择后即时预览生成的 TOML 片段 |
| 拖拽绑定（v2） | 从「代理池」拖动代理到「服务端」上完成绑定 | HTML5 Drag & Drop API + Vue 指令。v1 不做，v2 实现 |
| 系统托盘 | 最小化到托盘，托盘图标颜色反映 frpc 状态 | Tauri tray-icon 插件。绿色=运行中，灰色=已停止，红色=异常（点击托盘图标恢复窗口） |
| 深色/浅色主题 | 设置页面切换，即时生效 | TailwindCSS `dark:` 变体 + CSS 自定义属性。主题选择持久化到 `tauri-plugin-store` |
| 配置导入导出 | 加密导出为 `.rrp` 文件 | 导出：SQLite dump → AES-256-GCM 加密 → 写文件。导入：读文件 → 解密 → 合并到 SQLite（冲突时提示用户选择覆盖/跳过） |
| 热重载反馈 | 修改配置后点击「应用」→ 等待 SIGHUP 结果 | 成功 → Toast 绿提示「配置已重载」。失败 → 弹窗红提示 + frpc stderr 原文 |

## 五、状态管理（Pinia Stores）

```
stores/
├── app.ts          # 全局状态：主题、语言、frpc 运行状态
├── profiles.ts     # FrpsProfile CRUD + 列表
├── proxies.ts      # LocalProxy CRUD + 列表
├── bindings.ts     # BindingRule CRUD + 列表
├── traffic.ts      # 实时流量数据（通过事件订阅自动更新）
└── logs.ts         # 日志条目（环形缓冲区，最多保留 1000 条）
```

### Store 设计原则

- 每个 store 通过 Tauri `invoke()` 与 Rust 后端通信，不直接操作 SQLite
- `traffic` store 通过事件监听自动更新，不需要轮询
- Store 之间不直接引用——跨 store 操作通过组件协调（或 Pinia `getters` 引用只读数据）

## 六、i18n 架构

### 目录结构

```
plugins/webui/src/i18n/
├── index.ts          # 轻量 Vue provide/inject i18n，持久化当前 locale
├── locale.ts         # SupportedLocale 与 Naive UI locale 映射
├── format.ts         # Intl 数据格式化与兼容性 fallback
├── format.test.ts    # Bun 单元测试
├── types.ts          # 翻译键和插值参数类型
└── messages/
    ├── zh.json
    └── en.json
```

### 翻译键命名规范

```
# 分层命名：{模块}.{组件}.{字段}
common.save       → "保存" / "Save"
common.cancel     → "取消" / "Cancel"
common.delete     → "删除" / "Delete"

error.DB_001      → "数据库连接丢失，请检查磁盘空间和文件权限"
error.CFG_001     → "IP 地址格式不正确"
error.PROC_001    → "frpc 进程异常退出（退出码 {code}）"

servers.title     → "服务端管理"
servers.add       → "添加服务端"
servers.form.server_addr → "服务器地址"
```

### 与 Rust 错误码的集成

```
Rust CoreError::code()          → 前端 error.{code} 翻译键
Rust CoreError::user_message_key()  → 前端按当前语言查找翻译
```

如果插值需要（如 `退出码 {code}`），通过内置 `t()` 的命名参数传递。

### 区域化数据格式

WebUI 的用户可见数字、持续时间和单位统一由
`plugins/webui/src/i18n/format.ts` 格式化，并显式传入当前
`SupportedLocale`。组件不得依赖浏览器默认 locale，也不得各自拼接
`h`、`m`、`%`、`B` 等展示字符串。

| 数据 | 格式化策略 |
|---|---|
| daemon uptime | `Intl.DurationFormat`；先按 86400/3600/60 拆分，旧浏览器使用内置 fallback |
| 普通数字 | `Intl.NumberFormat` |
| 后端 CPU 百分比 | 后端值为 0~100，除以 100 后使用 `Intl.NumberFormat` 的 `percent` 样式 |
| 内存与流量 | 应用层按 1024 换算 IEC 单位（KiB/MiB/GiB），`Intl` 只格式化数字部分 |
| 日期时间（新增展示时） | `Intl.DateTimeFormat`，API 保持 ISO 8601/UTC，不改变传输格式 |

IP、端口、PID、版本号、SHA256、Token、配置文件、日志原文和 Prometheus
指标属于机器数据，不做 locale 格式化。`Intl.DurationFormat` 是 Baseline
2025 能力；除非浏览器支持范围明确提升，必须保留 feature detection fallback。

区域化格式化逻辑使用 `bun test` 验证，中英文输出测试应断言语义片段，避免
把 ICU/浏览器允许的标点和空格差异写成脆弱的完整字符串快照。

## 七、前端技术细节

| 层面 | 选型 | 理由 |
|---|---|---|
| 框架 | Vue 3 Composition API + `<script setup>` + TypeScript strict | 类型安全，生态成熟 |
| 状态管理 | Pinia | Vue 3 官方推荐 |
| 路由 | Vue Router 4 | 标准选择 |
| UI 组件 | Naive UI 2 | 组件、主题及中英文 locale/date-locale 集成 |
| 图表 | 轻量内联 SVG | 当前指标历史图无需引入大型图表依赖 |
| 文本 i18n | 内置 typed message map | 仅中英文、零运行时依赖，编译期约束翻译键 |
| 数据 i18n | ECMA-402 `Intl` | 持续时间、数字、百分比和单位按当前 UI locale 展示 |
| 前端构建 | Vite 8 + Bun | 快速构建，Bun 同时运行前端单元测试 |
| 前端检查 | TypeScript 6 strict + 翻译键检查脚本 | 类型安全并防止中英文键漂移 |

### 主题与图标

- 页面布局、响应式和语义化设计令牌使用 Tailwind CSS 4；复杂交互组件继续使用 Naive UI。
- 浅色、深色和跟随系统模式由 `stores/theme.ts` 统一管理，主题色通过 CSS variables 与 Naive UI `themeOverrides` 同步。
- 功能性图标使用 Iconify 的构建时离线方案：`unplugin-icons` + 单独的 `@iconify-json/lucide` 图标集。
- 页面只能通过 `AppIcon` 的语义名称使用图标；具体 Iconify 导入集中在 `components/icon/registry.ts`。
- 禁止 Emoji、图标字体、运行时 Iconify API、页面手写 UI SVG 和混用多个图标集。指标图表等数据可视化 SVG 不属于 UI 图标。
- 图标策略由 `bun run check:icons` 检查，生产构建必须先通过该检查。

### 不使用的

| 不引入 | 理由 |
|---|---|
| 第二套 UI 组件库（Element Plus / Ant Design Vue / Vuetify 等） | 与 Naive UI 重复，增加体积和交互差异 |
| 大型图表库 | 当前指标图由内联 SVG 满足，避免增加前端包体 |
| vue-i18n | 当前仅支持中英文，内置 typed message map 已满足需求 |
| Axios | `ofetch` 已提供统一 HTTP 客户端能力 |
