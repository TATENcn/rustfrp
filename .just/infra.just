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
