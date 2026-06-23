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
plugins/gui/src/locales/
├── index.ts          # vue-i18n 初始化
├── zh-CN/
│   ├── common.json   # 通用文本（按钮、标签）
│   ├── error.json    # 错误消息（与 Rust error code 一一对应）
│   ├── servers.json  # 服务端页面文本
│   ├── proxies.json  # 代理页面文本
│   └── settings.json # 设置页面文本
└── en/
    ├── common.json
    ├── error.json
    └── ...
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

如果插值需要（如 `退出码 {code}`），通过 `vue-i18n` 的命名参数传递。

## 七、前端技术细节

| 层面 | 选型 | 理由 |
|---|---|---|
| 框架 | Vue 3 Composition API + `<script setup>` + TypeScript strict | 类型安全，生态成熟 |
| 状态管理 | Pinia | Vue 3 官方推荐 |
| 路由 | Vue Router 4 | 标准选择 |
| 样式 | TailwindCSS 3 | 原子化 CSS，`dark:` 变体支持主题切换 |
| 行为组件 | @headlessui/vue | 模态框/下拉菜单/列表的键盘导航和焦点管理，样式完全自控 |
| 图标 | lucide-vue-next | 开源，按需引入（tree-shaking），风格统一 |
| 图表 | ECharts 5（按需引入折线图/仪表盘） | 国内生态好，文档中文 |
| i18n | vue-i18n 9 | Vue 3 标准国际化方案 |
| 类型共享 | ts-rs（Rust 端自动生成 TypeScript 类型） | 防止 IPC 类型漂移 |
| 前端构建 | Vite 5 | 极速 HMR |
| 前端检查 | ESLint 9 + Prettier 3 | 代码风格统一 |

### 不使用的

| 不引入 | 理由 |
|---|---|
| UI 组件库（Element Plus / Ant Design Vue / Vuetify 等） | 自带设计系统，绑架视觉风格。本项目只使用无样式的 Headless 组件 |
| @heroicons/vue | 与 lucide-vue-next 功能重复 |
| Sass / Less | TailwindCSS + CSS 变量已覆盖所有样式需求 |
| Axios | Tauri 的 `invoke()` 替代 HTTP 请求 |
