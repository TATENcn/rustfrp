# 显示基础设施命令
[no-cd]
default:
    @{{ just_executable() }} --list infra

# 构建并启动完整 Compose 服务
[no-cd]
up:
    docker compose up -d --build

# 停止 Compose 服务但保留数据卷
[no-cd]
down:
    docker compose down

# 重启全部或指定 Compose 服务
[no-cd]
restart service='':
    docker compose restart {{service}}

# 显示 Compose 服务状态
[no-cd]
status:
    docker compose ps

# 持续查看全部或指定服务的日志
[no-cd]
logs service='':
    docker compose logs -f {{service}}

# 验证并输出最终 Compose 配置
[no-cd]
config:
    docker compose config

# 拉取 Compose 配置引用的镜像
[no-cd]
pull:
    docker compose pull

# 只构建本地 daemon 镜像（不启动整套监控基础设施）
[no-cd]
docker-build:
    GRAFANA_ADMIN_PASSWORD=local-dev RUSTFRP_API_TOKEN_FILE=/dev/null docker compose build daemon

# 只启动本地 daemon，网页默认位于 http://localhost:7900
[no-cd]
docker-up:
    GRAFANA_ADMIN_PASSWORD=local-dev RUSTFRP_API_TOKEN_FILE=/dev/null docker compose up -d --no-deps daemon

# 重新构建并启动本地 daemon
[no-cd]
docker-rebuild:
    GRAFANA_ADMIN_PASSWORD=local-dev RUSTFRP_API_TOKEN_FILE=/dev/null docker compose up -d --no-deps --build daemon

# 查看本地 daemon 日志
[no-cd]
docker-logs:
    GRAFANA_ADMIN_PASSWORD=local-dev RUSTFRP_API_TOKEN_FILE=/dev/null docker compose logs -f daemon

# 检查本地 daemon 容器状态和 HTTP 健康端点
[no-cd]
docker-check:
    #!/usr/bin/env bash
    set -euo pipefail
    GRAFANA_ADMIN_PASSWORD=local-dev RUSTFRP_API_TOKEN_FILE=/dev/null docker compose ps daemon
    curl --fail --silent "http://127.0.0.1:${RUSTFRP_PORT:-7900}/api/v1/health"
    printf '\n'

# 在本地 daemon 容器中打开 shell
[no-cd]
docker-shell:
    GRAFANA_ADMIN_PASSWORD=local-dev RUSTFRP_API_TOKEN_FILE=/dev/null docker compose exec daemon /bin/sh
