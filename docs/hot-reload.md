# 热重载与逐代理状态

RustFRP 通过 frpc 自带的 admin API 完成热重载与逐代理状态查询。两者共享每个服务器配置的
`[webServer]` 监听，因此不需要额外端口规划或额外的 frpc 参数。

## 为什么不用信号

frpc 只注册 `SIGINT` / `SIGTERM`（`cmd/frpc/sub/root.go`）。`SIGHUP` 走的是进程默认动作，
会直接终止 frpc（实测退出码 129）。官方热重载路径是 admin API：

```
frpc reload -c frpc.toml   →  GET http://<webServer.addr>:<port>/api/reload
```

该请求由 frpc 自行增删代理，已建立的连接不受影响。RustFRP 的 `ProcessGuard::reload`
调用 `frpc reload`，因此 `PERF-002`「热重载不得中断已有连接」成立。

## `[webServer]` 的分配与保护

- 每个已保存的服务器配置在首次生成 TOML 时分配一个回环端口，记录在
  `profile_runtime.admin_port`；同时生成随机密码存入 `profile_runtime.admin_password`。
- 端口从 7400 起向上探测，跳过已被其他配置占用的端口以及当前无法绑定的端口。
- 监听地址固定 `127.0.0.1`，并要求 Basic 认证：`GET /api/config` 会回显完整配置，包括
  FRP token，无认证的回环监听会把凭据暴露给任意本地进程。
- 端口与密码随服务器配置删除而级联删除；配置迁移到新机器时重新分配。

生成的片段形如：

```toml
[webServer]
addr = "127.0.0.1"
port = 7401
user = "rustfrp"
password = "<random>"
```

端口被其他程序占用时 frpc 启动失败，错误会通过启动诊断返回，而不是静默降级。

## 逐代理状态

`GET /api/v1/profiles/{id}/proxy-status` 读取 frpc 的 `/api/status` 并展开按类型分组的结果：

```json
{
  "profile_id": 1,
  "available": true,
  "proxies": [
    {
      "proxy_type": "tcp",
      "name": "echo",
      "status": "running",
      "local_addr": "127.0.0.1:19080",
      "remote_addr": "127.0.0.1:19081"
    }
  ]
}
```

- `available: false` 表示 admin 端口不可达（进程未运行、端口被占用或凭据失配），此时
  `error` 给出原因，`proxies` 为空。查询失败不会让请求失败。
- `status` 直接来自 frpc，取值包括 `running`、`wait start`、`start error`。
- `start error` 常见的两类原因：目标端口已被 frps 上其他客户端占用；服务端未启用所请求的
  特性（例如 `tcpmux` 需要 frps 开启 `tcpmuxHTTPConnectPort`）。
- 代理状态无法映射回 `local_proxy` 的 `id` 时按 `name` 匹配——这是 frpc 唯一对外暴露的键。

## 前端表现

「代理分配」页在实例运行期间每 5 秒读取一次该接口，为每个代理显示 frpc 报告的真实远端地址
与状态；未运行时退回显示配置中的类型与本地端口。代理被 frps 拒绝时显示红色徽标，悬停可看到
frpc 的原始错误文本。

## 校验

生成后、启动前会调用 `frpc verify -c`：

- `verify` 不绑定端口、不连接服务器，因此在实例运行期间也可以安全调用。
- 它能拦住结构性错误，例如 `type = "tcpmux"` 缺少 `multiplexer`、非法健康检查类型、
  当前 frp 版本不认识的字段（如仅存在于 frp master 的 `transport.tls.useSystemCa`）。
- 失败时不会拉进程，错误直接作为启动失败返回，避免「进程起来了但配置被 frpc 丢弃」。

回归覆盖：

```sh
cargo test -p rustfrp-client --lib
cargo test -p rustfrp-daemon --lib
```

`frp 0.71.0` 实测：双代理配置修改后 frpc PID 不变、两条隧道持续可用。
