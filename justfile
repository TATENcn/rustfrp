# RustFRP 开发命令

# 开发进程与依赖初始化
mod dev '.just/dev.just'

# Docker Compose 基础设施
mod infra '.just/infra.just'

# 测试套件
mod test '.just/test.just'

# 格式化、代码检查与安全审计
mod check '.just/check.just'

# 调试、发布与交叉编译
mod build '.just/build.just'

# 显示全部命令
default:
    @{{ just_executable() }} help

# 显示中文分组帮助
help:
    #!/usr/bin/env bash
    cat <<'EOF'
    RustFRP 本地开发命令

    常用
      just help                 显示这份帮助
      just doctor               检查本地开发工具
      just setup                安装项目依赖
      just ci                   运行完整本地质量门禁

    开发
      just dev                  查看开发命令
      just dev daemon [参数]    启动 daemon 与 HTTP API
      just dev webui [参数]     启动 WebUI 开发服务器
      just dev control [参数]   启动 control 服务
      just dev agent [参数]     启动 FRPS agent

    基础设施
      just infra                查看基础设施命令
      just infra up             构建并启动 Compose 服务
      just infra down           停止 Compose 服务
      just infra status         查看服务状态
      just infra logs [服务]    持续查看日志

    测试
      just test                 查看测试命令
      just test fast [参数]     运行快速库测试
      just test all [参数]      运行完整 Rust 测试
      just test log [参数]      带调试日志运行测试
      just test e2e             运行本地端到端测试

    代码质量
      just check                查看检查命令
      just check fmt            检查 Rust 格式
      just check format         自动格式化 Rust 代码
      just check lint           运行 Clippy
      just check i18n           检查 WebUI 翻译键
      just check audit          运行依赖安全与策略审计
      just check all            运行常规检查

    构建
      just build                查看构建命令
      just build debug [参数]   调试构建
      just build release [参数] 发布构建
      just build webui          构建 WebUI
      just build webui-release  构建 WebUI 与 release daemon
      just build armv7          交叉编译 ARMv7
      just build aarch64        交叉编译 AArch64

    其他
      just bench                运行基准测试
      just doc                  生成并打开 Rust 文档
      just clean                清理 Cargo 构建产物

    使用 just --list 查看包含旧版兼容入口在内的完整命令索引。
    EOF

# 检查本地开发工作流所需工具
doctor:
    #!/usr/bin/env bash
    set -u

    missing=0
    for tool in cargo bun; do
        if command -v "$tool" >/dev/null 2>&1; then
            printf '[正常] %s\n' "$tool"
        else
            printf '[缺失] %s（必需）\n' "$tool"
            missing=1
        fi
    done

    if command -v docker >/dev/null 2>&1 && docker compose version >/dev/null 2>&1; then
        printf '[正常] docker compose\n'
    else
        printf '[缺失] docker compose（infra 命令需要）\n'
    fi

    for tool in cross cargo-audit cargo-deny; do
        if command -v "$tool" >/dev/null 2>&1; then
            printf '[正常] %s\n' "$tool"
        else
            printf '[缺失] %s（可选）\n' "$tool"
        fi
    done

    if ((missing == 0)); then
        printf '\n核心开发环境已就绪。\n'
    else
        printf '\n核心开发环境尚未就绪，请安装上面标记为“必需”的工具。\n'
    fi

    exit "$missing"

# 安装本地项目依赖
setup:
    {{ just_executable() }} dev setup

# 运行与旧版 CI 配方一致的完整本地门禁
ci:
    {{ just_executable() }} check legacy-lint
    {{ just_executable() }} test all
    {{ just_executable() }} build release
    {{ just_executable() }} check audit

# 运行全部基准测试
bench:
    cargo bench --all

# 生成并打开 Rust 文档
doc:
    cargo doc --no-deps --open

# 清理 Cargo 构建产物
clean:
    cargo clean

# 说明旧版 client 开发命令为何不可用
dev-client:
    @echo "rustfrp-client 是 library crate，无法通过 cargo run 启动。"
    @exit 2

# 兼容旧版 WebUI 初始化命令
setup-webui:
    {{ just_executable() }} dev setup

# 兼容旧版 daemon 命令
dev-daemon:
    {{ just_executable() }} dev daemon

# 兼容旧版 control 命令
dev-control:
    {{ just_executable() }} dev control

# 兼容旧版 agent 命令
dev-agent:
    {{ just_executable() }} dev agent

# 兼容旧版 WebUI 命令
dev-webui:
    {{ just_executable() }} dev webui

# 兼容旧版快速测试命令
test-fast:
    {{ just_executable() }} test fast

# 兼容旧版完整测试命令
test-all:
    {{ just_executable() }} test all

# 兼容旧版日志测试命令
test-log:
    {{ just_executable() }} test log

# 兼容旧版格式化命令
fmt:
    {{ just_executable() }} check format

# 兼容旧版审计命令
audit:
    {{ just_executable() }} check audit

# 兼容旧版 i18n 检查命令
check-i18n:
    {{ just_executable() }} check i18n

# 兼容旧版发布构建命令
build-release:
    {{ just_executable() }} build release

# 兼容旧版 WebUI 发布构建命令
build-release-webui:
    {{ just_executable() }} build webui-release

# 兼容旧版 ARMv7 构建命令
build-armv7:
    {{ just_executable() }} build armv7

# 兼容旧版 AArch64 构建命令
build-aarch64:
    {{ just_executable() }} build aarch64

# 兼容旧版格式化与 Clippy 组合检查
lint:
    {{ just_executable() }} check legacy-lint
