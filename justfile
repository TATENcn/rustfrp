# RustFRP 开发命令

# --- 默认目标 ---
default:
    @just --list

# === 开发 ===

# 启动 GUI 开发模式
dev:
    cd plugins/gui && npm run tauri dev

# 启动客户端开发模式
dev-client:
    cargo run -p rustfrp-client

# 启动 daemon 开发模式（含 HTTP API）
dev-daemon:
    cargo run -p rustfrp-daemon

# 启动控制服务器开发模式
dev-control:
    cargo run -p rustfrp-control

# 启动 agent 开发模式
dev-agent:
    cargo run -p rustfrp-agent

# === 测试 ===

# 单元测试（每次 push 前跑）
test-fast:
    cargo test --lib --all

# 全量测试（含集成测试）
test-all:
    cargo test --all

# 带日志的测试
test-log:
    RUST_LOG=debug cargo test -- --nocapture

# === 代码质量 ===

# 格式化检查 + clippy
lint:
    cargo fmt --all -- --check
    cargo clippy --all-targets --all-features -- -D warnings

# 自动格式化
fmt:
    cargo fmt --all

# 安全审计
audit:
    cargo audit
    cargo deny check

# === WebUI 前端 ===

# Install WebUI frontend dependencies (Bun native TypeScript, 3-5x faster than npm)
setup-webui:
    cd plugins/webui && bun install

# Start WebUI dev mode (requires dev-daemon in another terminal)
dev-webui:
    cd plugins/webui && bun run dev

# Build WebUI + daemon binary (type-check + build + verify)
build-release-webui:
    cd plugins/webui && bun install --frozen-lockfile && bun x tsc --noEmit && bun run build
    @test -f plugins/webui/dist/index.html || (echo "ERROR: webui/dist/index.html not found!" && exit 1)
    cargo build -p rustfrp-daemon --release
    @echo "Binary: target/release/rustfrp-daemon"

# i18n translation key consistency check (Bun native TS execution, no pre-compilation)
check-i18n:
    cd plugins/webui && bun scripts/check-i18n-keys.ts

# === 构建 ===

# 发布构建
build-release:
    cargo build --release

# 交叉编译 armv7
build-armv7:
    cross build --release --target armv7-unknown-linux-gnueabihf

# 交叉编译 aarch64
build-aarch64:
    cross build --release --target aarch64-unknown-linux-gnueabihf

# === 基准测试 ===

# 运行基准
bench:
    cargo bench --all

# === 文档 ===

# 生成并打开文档
doc:
    cargo doc --no-deps --open

# === 清理 ===

# 清理构建产物
clean:
    cargo clean

# === CI 相关 ===

# CI 全套（等价于 CI pipeline）
ci:
    just lint
    just test-all
    just build-release
    just audit
