---
doc_id: 06-DEPLOYMENT
version: 1.0.0
last_modified: 2026-06-23
modification_policy: operations
summary: 客户端分发方式、FRP 二进制管理、运行模式、路径约定、Docker 部署
---

# 部署指南

## 一、客户端分发方式

### 1.1 各平台打包格式

| 平台 | 格式 | 工具 | 说明 |
|---|---|---|---|
| Windows | `.msi`（推荐）/ `.exe` | Tauri bundler | MSI 支持静默安装和卸载 |
| macOS | `.dmg` | Tauri bundler | 需 Apple Developer 签名（否则用户需右键打开） |
| Linux | `.AppImage` / `.deb` | Tauri bundler | AppImage 无需 root，单文件直接运行 |
| 手动 | 单一二进制文件 | `cargo build --release` | 复制到 `~/.local/bin/` 即可运行。适合高级用户和嵌入式设备 |

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

```
rustfrp-manager gui --frp-version 0.61.0     # 指定 FRP 版本
rustfrp-manager gui --frp-path /usr/bin/frpc  # 使用已安装的路径
rustfrp-manager gui --no-auto-download         # 禁止自动下载（离线/安全严格环境）
```

### 版本兼容矩阵

| 本项目版本 | 支持 FRP 版本范围 | 说明 |
|---|---|---|
| v0.1.x | v0.52.0 ~ latest | MVP 阶段，跟随最新 FRP TOML 规范 |
| 未来版本 | 以 CHANGELOG 为准 | 若 FRP 引入 breaking TOML 变更，做适配并更新本文档 |

---

## 三、运行模式

本项目一个二进制支持四种运行模式：

```bash
# 模式 1：完整客户端（桌面 GUI）
rustfrp-manager gui

# 模式 2：无头客户端（守护进程 + 配置管理，适合 NAS/软路由）
rustfrp-manager daemon --db ~/.rustfrp/config.db

# 模式 3：监控服务器
rustfrp-manager monitor --scrape-targets /etc/rustfrp/targets.yaml --port 9090

# 模式 4：最小模式（生成 TOML → 拉起 frpc，不做持续管理）
rustfrp-manager run --db ~/.rustfrp/config.db
```

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

## 五、Docker 部署（仅监控服务器）

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

## 六、安装器行为

### Windows

```
rustfrp-manager_0.1.0_x64.msi
  ├── 安装位置：%ProgramFiles%\RustFRP Manager\
  ├── 开始菜单快捷方式
  ├── 可选：开机自启（安装时勾选）
  └── 卸载：控制面板 → 程序和功能 → 卸载（不残留 ~/.rustfrp 数据目录）
```

### macOS

```
rustfrp-manager_0.1.0_x64.dmg
  ├── 拖入 /Applications/
  └── 首次打开需右键 → 打开（如未签名）
```

### Linux

```
rustfrp-manager_0.1.0_amd64.AppImage
  ├── chmod +x 后直接运行
  └── 可选：集成到桌面环境（.desktop 文件）
```

---

## 七、升级策略

| 场景 | 行为 |
|---|---|
| 用户覆盖安装新版本 | 数据库自动 migration，不丢配置 |
| 降级到旧版本 | 检测到数据库版本高于当前代码 → 拒绝启动，提示升级或恢复备份 |
| FRP 二进制升级 | 用户可在设置中切换 FRP 版本，工具自动下载新版本到 `~/.rustfrp/binaries/` |
| 配置文件迁移 | 如果未来文件路径变动，首次启动时自动迁移旧路径到新路径 |
