# 显示开发命令
[no-cd]
default:
    @{{ just_executable() }} --list dev

# 安装 WebUI 依赖
[no-cd]
setup:
    cd plugins/webui && bun install

# 启动 daemon 与 HTTP API
[no-cd]
daemon *args:
    cargo run -p rustfrp-daemon -- {{args}}

# 启动 WebUI 开发服务器（需要在另一终端运行 dev daemon）
[no-cd]
webui *args:
    cd plugins/webui && bun run dev -- {{args}}

# 启动 control 服务
[no-cd]
control *args:
    cargo run -p rustfrp-control -- {{args}}

# 启动 FRPS agent
[no-cd]
agent *args:
    cargo run -p rustfrp-agent -- {{args}}
