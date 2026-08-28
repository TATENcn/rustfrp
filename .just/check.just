# 显示代码质量检查命令
[no-cd]
default:
    @{{ just_executable() }} --list check

# 检查 Rust 格式但不修改文件
[no-cd]
fmt:
    cargo fmt --all -- --check

# 格式化全部 Rust 代码
[no-cd]
format:
    cargo fmt --all

# 对全部目标和功能运行 Clippy
[no-cd]
lint:
    cargo clippy --all-targets --all-features -- -D warnings

# 运行旧版格式化与 Clippy 组合门禁
[no-cd]
legacy-lint: fmt lint

# 检查 WebUI 翻译键一致性
[no-cd]
i18n:
    cd plugins/webui && bun scripts/check-i18n-keys.ts

# 运行依赖漏洞与策略审计
[no-cd]
audit:
    cargo audit
    cargo deny check

# 运行格式化、Clippy 和翻译检查
[no-cd]
all: fmt lint i18n
