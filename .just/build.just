# 显示构建命令
[no-cd]
default:
    @{{ just_executable() }} --list build

# 以调试模式构建完整 workspace
[no-cd]
debug *args:
    cargo build --all {{args}}

# 以发布模式构建完整 workspace
[no-cd]
release *args:
    cargo build --release {{args}}

# 构建 WebUI
[no-cd]
webui:
    cd plugins/webui && bun run build

# 构建 WebUI 与 release daemon 二进制
[no-cd]
webui-release:
    cd plugins/webui && bun install --frozen-lockfile && bun x tsc --noEmit && bun run build
    @test -f plugins/webui/dist/index.html || (echo "ERROR: webui/dist/index.html not found!" && exit 1)
    cargo build -p rustfrp-daemon --release
    @echo "Binary: target/release/rustfrp-daemon"

# 为 ARMv7 Linux 交叉编译 release workspace
[no-cd]
armv7:
    cross build --release --target armv7-unknown-linux-gnueabihf

# 为 AArch64 Linux 交叉编译 release workspace
[no-cd]
aarch64:
    cross build --release --target aarch64-unknown-linux-gnueabihf
