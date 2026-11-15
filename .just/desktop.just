# 桌面端（Tauri 外壳）
#
# 该 crate 不在根 workspace 内（GTK/WebKit 系统依赖不应成为所有 Rust 任务的前置），
# 因此命令显式切到 plugins/gui/src-tauri 执行。
[no-cd]
default:
    @{{ just_executable() }} --list desktop

# 安装 WebUI 与 Tauri CLI 依赖
[no-cd]
setup:
    cd plugins/webui && bun install --frozen-lockfile
    cd plugins/gui && bun install --frozen-lockfile

# 启动桌面端开发模式（Vite:5173 + API:7900 + 原生窗口）
[no-cd]
dev:
    cd plugins/gui && bun run dev

# 构建当前平台的安装包
[no-cd]
build:
    cd plugins/gui && bun run build

# 构建 Linux deb 与 AppImage
[no-cd]
build-linux:
    cd plugins/gui && bun run build:linux

# 检查桌面 crate 的格式与 Clippy
[no-cd]
check:
    cd plugins/gui/src-tauri && cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings

# 由 1024px 母版（app-icon.png）重新生成各平台图标
[no-cd]
icons:
    cd plugins/gui && bun run icon app-icon.png
