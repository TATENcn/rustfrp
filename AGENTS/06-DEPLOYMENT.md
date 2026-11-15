---
doc_id: 06-DEPLOYMENT
version: 1.0.0
last_modified: 2026-06-23
modification_policy: operations
summary: 客户端分发方式、FRP 二进制管理、运行模式、路径约定、桌面端部署、Docker 部署
---

# 部署指南

## 一、客户端分发方式

### 1.1 各平台打包格式

| 平台 | 格式 | 产物名 | 说明 |
|---|---|---|---|
| Windows x64 | `.msi`（推荐）/ `.exe` | `RustFRP_<ver>_x64_*.msi`、`RustFRP_<ver>_x64-setup.exe` | MSI 支持静默安装与卸载；NSIS `.exe` 适合无管理员权限安装 |
| macOS arm64 | `.dmg` | `RustFRP_<ver>_aarch64.dmg` | 未签名时需右键 → 打开；签名/公证走 `APPLE_*` secrets |
| macOS x64 | `.dmg` | `RustFRP_<ver>_x64.dmg` | 同上 |
| Linux x64 | `.deb`（推荐）/ `.AppImage` | `RustFRP_<ver>_amd64.deb`、`RustFRP_<ver>_amd64.AppImage` | deb 自动拉取 WebKitGTK/AppIndicator 依赖；AppImage 无需 root，但宿主需 `libfuse2` |
| 无桌面 | 三个单文件二进制 | `rustfrp_<平台>_<arch>.tar.gz` / `.zip` | daemon / agent / control，适合服务器与嵌入式 |

桌面端**不内嵌 frpc**，与无头部署共用同一套 `~/.rustfrp` 数据目录与 FRP 版本管理。

### 1.2 发行渠道

| 渠道 | 方式 |
|---|---|
| GitHub Releases | 主要渠道。每次打 tag 自动构建并上传产物 |
| 包管理器（未来） | `brew`（macOS）、`winget`（Windows）、`cargo install`（Rust 用户） |
| 官网下载（未来） | 静态页面 + CDN |

---

## 二、FRP 二进制管理

本工具 **不内嵌** FRP 二进制。启动时按以下流程处理：

```
启动
  ↓
检查 --frp-path 是否指定？
  ├─ 是 → 使用指定路径，跳过下载
  └─ 否 → 检查 ~/.rustfrp/binaries/frpc_{version} 是否存在？
            ├─ 存在 → SHA256 校验
            │         ├─ 通过 → 使用缓存
            │         └─ 失败 → 删除缓存，重新下载
            └─ 不存在 → 下载
                         ↓
                   从 GitHub Releases 拉取
                   https://github.com/fatedier/frp/releases/download/
                     v{version}/frp_{version}_{arch}.tar.gz
                         ↓
                   解压 → SHA256 校验 → 缓存到 ~/.rustfrp/binaries/
```

### 启动参数

```bash
# 无头 daemon
rustfrp-daemon --frpc-path /usr/bin/frpc        # 使用已安装的 frpc，跳过版本管理
rustfrp-daemon --auto-download                  # 允许启动期下载（默认关闭）
rustfrp-daemon --no-auto-download               # 显式禁止启动期下载

# 桌面端（同一份内核；不接受命令行参数）
RUSTFRP_DATA_DIR=~/.rustfrp-desktop rustfrp-desktop
```

桌面端使用系统默认 FRP 版本（`~/.rustfrp/frp`），在界面"系统状态 → FRP 版本"中安装与切换。

### 版本兼容矩阵

| 本项目版本 | 支持 FRP 版本范围 | 说明 |
|---|---|---|
| v0.1.x | v0.52.0 ~ latest | MVP 阶段，跟随最新 FRP TOML 规范 |
| 未来版本 | 以 CHANGELOG 为准 | 若 FRP 引入 breaking TOML 变更，做适配并更新本文档 |

---

## 三、运行模式

发布物包含四个二进制，按场景选用：

```bash
# 桌面端（原生窗口 + 内嵌 daemon，见第五节）
rustfrp-desktop

# 无头客户端（HTTP API + 配置管理 + frpc 守护，适合 NAS/软路由/容器）
rustfrp-daemon --db-path ~/.rustfrp/config.db --config-dir ~/.rustfrp/runtime \
               --api-listen 127.0.0.1:7900

# 控制服务器（多租户/多节点管理）
rustfrp-control

# FRPS 侧代理
rustfrp-agent
```

`rustfrp-desktop` 与 `rustfrp-daemon` 是**同一个 daemon 内核的两种外壳**，共用
`~/.rustfrp`，因此互斥（见第五节）。

### 启动时的数据库处理

```
首次启动：
  1. 检查 ~/.rustfrp/config.db 是否存在
  2. 不存在 → 创建空数据库 + 运行 migration 建表
  3. 存在 → 运行 migration（增量迁移，不覆盖已有数据）

后续启动：
  1. 运行 migration（新版本可能新增表/字段）
  2. migrations checksum 校验（防止迁移脚本被意外修改）
```

---

## 四、目录与文件约定

所有数据存储在用户目录下的 `~/.rustfrp/` 中，不污染系统目录。

| 路径 | 用途 | 生命周期 | 权限 |
|---|---|---|---|
| `~/.rustfrp/config.db` | SQLite 数据库（真理来源） | 持久 | `0600` |
| `~/.rustfrp/config.db-wal` | SQLite WAL 日志 | 自动管理 | `0600` |
| `~/.rustfrp/runtime/frpc.toml` | 运行时 TOML（每次启动重新生成） | 临时 | `0644` |
| `~/.rustfrp/binaries/` | FRP 二进制缓存（按版本存储） | 持久 | `0755` |
| `~/.rustfrp/instance.lock` | 数据目录独占锁（内容为持有者 PID，仅作诊断） | 进程生命周期 | `0644` |
| `~/.rustfrp/plugins/` | 用户安装的第三方插件 | 持久 | `0755` |
| `~/.rustfrp/logs/` | 应用日志 | 持久（支持轮转） | `0755` |
| `~/.rustfrp/logs/frpc.log` | frpc 子进程 stdout | 轮转（保留最近 7 天） | `0644` |
| `~/.rustfrp/logs/frpc_err.log` | frpc 子进程 stderr | 轮转（保留最近 7 天） | `0644` |
| `~/.rustfrp/logs/crash_*.txt` | 崩溃报告（panic hook 自动生成） | 持久（最多保留 10 个） | `0600` |

### Windows 路径映射

| Linux/macOS | Windows |
|---|---|
| `~/.rustfrp/` | `%APPDATA%\rustfrp\` |
| `~/.rustfrp/binaries/` | `%LOCALAPPDATA%\rustfrp\binaries\` |

路径获取通过 `dirs` crate 自动处理，不硬编码。

---

## 五、桌面端部署

### 安装

```bash
# Debian / Ubuntu
sudo apt install ./RustFRP_0.5.2_amd64.deb

# 任意发行版（无 root）
chmod +x RustFRP_0.5.2_amd64.AppImage && ./RustFRP_0.5.2_amd64.AppImage
```

Linux 运行时依赖：`libwebkit2gtk-4.1-0`、`libayatana-appindicator3-1`、`libgtk-3-0`。
deb 会声明依赖；AppImage 不声明，缺失 `libfuse2` 时需先安装。

### 启动行为

```
双击启动
  ↓
读取 ~/.rustfrp/instance.lock（独占 flock）
  ↓ 已被占用 → 弹窗说明"另一个 RustFRP 实例正在使用该数据目录"，退出码 1
  ↓
打开 SQLite → 绑定 127.0.0.1:<随机端口> → 生成会话 token
  ↓
创建窗口，加载 http://127.0.0.1:<端口>/
  ↓
关闭窗口 = 隐藏到托盘（frpc 继续运行）
托盘"退出" = 逐个 frpc SIGTERM → 等 3s → SIGKILL，然后退出
```

- 数据目录可用 `RUSTFRP_DATA_DIR` 覆盖，默认 `~/.rustfrp`。
- 桌面端与 `rustfrp-daemon` **互斥**：两者共用数据目录时会各自启动同一 profile 的 frpc，
  因此锁文件保证只有一个进程持有该目录。要同时运行请给其中一方指定不同的数据目录。
- 启动不做任何网络访问（不自动下载 frpc）。首次使用需在"系统状态 → FRP 版本"里安装并激活。

### 与无头部署的关系

| 场景 | 建议 |
|---|---|
| 同机先跑 daemon 又装桌面端 | 停掉 systemd 服务（`systemctl --user stop rustfrp-daemon`），否则桌面端会因锁被占用而拒绝启动 |
| 想同时用 | `RUSTFRP_DATA_DIR=~/.rustfrp-desktop` 启动桌面端，得到独立配置库 |
| 只想远程访问 | 用 daemon + 浏览器，桌面端不是必需的 |

### 故障排查

| 现象 | 原因 | 处理 |
|---|---|---|
| 启动即退出，无窗口 | 数据目录被占用 | 看 `~/.rustfrp/logs/rustfrp.log` 中的 `instance.lock` 记录，关掉占用进程 |
| 窗口空白 | 后端端口未就绪 | 同上日志搜 `desktop session started`；若报 bind 失败则是端口/权限问题 |
| 托盘图标不显示 | 桌面环境缺 appindicator | 安装 `libayatana-appindicator`/`gnome-shell-extension-appindicator`；不影响窗口使用 |
| macOS 提示"无法验证开发者" | 未签名 | 右键 → 打开，或 `xattr -dr com.apple.quarantine /Applications/RustFRP.app` |

日志默认写 `~/.rustfrp/logs/rustfrp.log`（JSON），因为 GUI 进程在 Windows/macOS 上没有控制台。
需要看实时输出时用环境变量覆盖：`RUSTFRP_LOG_MODE=console`。

---

## 六、Docker 部署（仅监控服务器）

客户端不需要 Docker。监控服务器提供 Docker 化选项：

```dockerfile
# monitor/Dockerfile
FROM rust:1.85-alpine AS builder
RUN apk add --no-cache musl-dev
WORKDIR /app
COPY . .
RUN cargo build --release -p rustfrp-monitor

FROM alpine:3.20
COPY --from=builder /app/target/release/rustfrp-monitor /usr/local/bin/
EXPOSE 9090
ENTRYPOINT ["rustfrp-monitor", "monitor"]
```

```yaml
# docker-compose.yml（示例）
version: '3.8'
services:
  monitor:
    image: rustfrp-monitor:latest
    ports:
      - "9090:9090"
    volumes:
      - ./scrape-targets.yaml:/etc/rustfrp/targets.yaml:ro
    command: monitor --scrape-targets /etc/rustfrp/targets.yaml --port 9090

  prometheus:
    image: prom/prometheus:latest
    volumes:
      - ./prometheus.yml:/etc/prometheus/prometheus.yml:ro
    ports:
      - "9091:9090"

  grafana:
    image: grafana/grafana:latest
    ports:
      - "3000:3000"
    volumes:
      - grafana-storage:/var/lib/grafana

volumes:
  grafana-storage:
```

---

## 七、安装器行为

### Windows

```
RustFRP_0.5.2_x64_en-US.msi
  ├── 安装位置：%ProgramFiles%\RustFRP\
  ├── 开始菜单快捷方式
  └── 卸载：控制面板 → 程序和功能（不删除 %USERPROFILE%\.rustfrp 数据目录）

RustFRP_0.5.2_x64-setup.exe   # NSIS，可无管理员权限安装到用户目录
```

### macOS

```
RustFRP_0.5.2_aarch64.dmg
  ├── 拖入 /Applications/
  ├── 首次打开需右键 → 打开（未签名构建）
  └── 已签名/公证构建可直接打开
```

### Linux

```
RustFRP_0.5.2_amd64.deb        # 推荐：自动拉取 WebKitGTK 与 AppIndicator 依赖
RustFRP_0.5.2_amd64.AppImage   # 无 root：chmod +x 后运行；宿主需 libfuse2

deb 内容：
  ├── /usr/bin/rustfrp-desktop
  ├── /usr/share/applications/RustFRP.desktop
  └── /usr/share/icons/hicolor/{32x32,128x128,256x256@2}/apps/rustfrp-desktop.png
```

---

## 八、升级策略

| 场景 | 行为 |
|---|---|
| 用户覆盖安装新版本 | 数据库自动 migration，不丢配置 |
| 降级到旧版本 | 检测到数据库版本高于当前代码 → 拒绝启动，提示升级或恢复备份 |
| FRP 二进制升级 | 用户可在设置中切换 FRP 版本，工具自动下载新版本到 `~/.rustfrp/binaries/` |
| 配置文件迁移 | 如果未来文件路径变动，首次启动时自动迁移旧路径到新路径 |
