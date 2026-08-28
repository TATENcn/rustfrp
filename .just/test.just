# 显示测试命令
[no-cd]
default:
    @{{ just_executable() }} --list test

# 运行快速 Rust 库测试
[no-cd]
fast *args:
    cargo test --lib --all -- {{args}}

# 运行完整 Rust 测试套件
[no-cd]
all *args:
    cargo test --all -- {{args}}

# 使用调试日志和实时输出运行 Rust 测试
[no-cd]
log *args:
    RUST_LOG=debug cargo test -- {{args}} --nocapture

# 运行本地 FRP 穿透端到端测试
[no-cd]
e2e:
    bash scripts/e2e-local.sh
