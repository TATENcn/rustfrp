# Bun 开发管理命令

在仓库根目录运行 `bun run mode:help`。管理器采用单一 `local` 方式，适用于 Linux，需要 Bun、Cargo（项目 rust-toolchain.toml 指定版本）、OpenSSL 和 util-linux 的 `flock`。无需安装根目录 npm 依赖。

## 启动与更新

```bash
bun run mode:start
bun run mode:status
bun run mode:reload
bun run mode:stop
```

- `start`：安装 WebUI 锁定依赖，调用已有 `bun run build`，增量执行 `cargo build --locked -p rustfrp-daemon --features rust-embed/debug-embed`，后台启动 daemon 和 HTTPS 代理。WebUI 内嵌到 debug 二进制，运行期间不读取可变 dist。已有实例时只显示状态，不重复启动。
- `reload`：先完成同样的构建，再停止旧实例、启动新实例。构建失败不会停止旧进程或替换旧页面。服务替换有短暂中断，不提供 HMR，也不承诺自动回滚数据库。
- `restart`：使用最近一次成功构建的产物重启，不重新编译。未构建时给出提示。
- `build`：只构建并保存产物，不启动或重启；运行实例使用独立的二进制副本。
- `stop`：停止已核验归属的本仓库进程组，保留数据库、日志、证书和缓存。
- `logs`：显示最后 80 行；`bun run mode:logs --follow` 持续跟踪。日志超过 10 MiB 自动轮转，保留 3 个历史文件。
- `health`：验证 daemon 进程、监听 socket 归属、HTTPS 证书、实例响应头与业务健康 JSON，异常返回非零。
- `test`：执行 `cargo test --lib`，参数按 argv 原样传递，例如 `bun run mode:test -- --nocapture`。

`start` / `reload` 在真正构建前做一次轻量空间检查：只有在文件系统达到临界水位（默认 92%）且可用空间低于所需预留时才拒绝构建，并提示先运行 `mode:clean`。其余情况照常使用增量缓存，不会每次构建前 prune。

不自动启动 frps、Control 或监控套件。需要完整部署时继续使用原有 Compose / just 命令。前端 HMR 开发仍使用 `just dev-webui`。

## 磁盘与缓存

```bash
bun run mode:clean              # 只预览，不删除
bun run mode:clean -- --execute # 实际回收候选
bun run mode:clean -- --execute --build  # 连同 Rust 构建产物回收
```

回收范围仅限本管理器能核验归属的可重建产物：`.local/mode/tmp`、`.local/mode/bun-cache`、`plugins/webui/node_modules`，以及仓库 `target/`（通过 `cargo clean`）。源码、`.local/mode/config.db`、证书私钥、API Token、日志正文与非本仓库目录一律排除。

策略按仓库文件系统使用率分级，阈值带滞回：

| 级别 | 触发 | 退回到 | 回收范围 |
| --- | --- | --- | --- |
| 正常 | < 75% | — | 仅超过 5 天的临时产物 |
| 偏高 | ≥ 75% | < 70% | 加 5 天 / 21 天的临时与下载缓存 |
| 高 | ≥ 85% | < 78% | 同上，按闲置时长放宽 |
| 临界 | ≥ 92% | < 85% | 加构建产物，闲置期限归零 |

闲置时间取管理器记录的**真实使用时间**（依赖安装、构建成功时刷新），不使用文件 atime。首次观察到的既有缓存从观察时刻起给宽限期，不会被立即判为过期；统计失败标记为未知并跳过。回收分批执行（默认每批 3 个、最多 8 批或 5 秒），每批后重新测量，达到退出线即停止。构建产物默认不参与，除非显式 `--build` 或处于临界级别。

缓存预算默认取文件系统容量的 2%，夹在 3–20 GiB 之间；预算只是兜底提示，不会据此删除其他项目的数据。

## 地址、认证与数据

实际地址由 `mode:status` 显示。HTTPS 绑定 `0.0.0.0`，面板列出回环及当前 IPv4 局域网地址；daemon 仅监听回环 HTTP。默认不下载 frpc，先打开管理界面，再按需要安装 FRP 版本。

首次访问时，将 `.local/mode/api-token` 的内容填入 WebUI 的 API Token 设置。Token 不输出到面板或日志。不要把此目录提交到 Git。

仓库身份来自规范化绝对路径的 SHA256。独立 clone/worktree 分别拥有自己的数据库、运行目录、证书、锁、日志与端口基线。符号链接指向相同仓库时身份相同；移动目录会改变身份，移动前请停止服务。

所有管理器状态保存在 `.local/mode/`。数据库是 `.local/mode/config.db`，与默认 `~/.rustfrp/config.db` 分离。**daemon 自身已有的 FRP 版本库及插件目录仍沿用用户级目录**；在界面安装、切换 FRP 版本可能影响同一用户的其他 daemon，不应将其当作完全隔离的 FRP 沙箱。

HTTPS 实际绑定时最多尝试 16 个连续端口。后端端口由独立服务名派生，冲突时启动失败并写日志，不终止占用者。停止操作核验 PID 创建时间、进程组、工作目录和实例启动参数；身份不符时拒绝发送信号。并发生命周期操作通过仓库锁互斥，忙时返回失败，不无限等待。

## 本地配置

唯一保存开发者选择的文件是仓库根的 `.local/mode.json`（已被 Git 忽略，帮助与状态面板显示绝对路径）。文件不存在时使用内置默认值；损坏、JSON 非法或不支持的 `version` 会直接报错，不会静默重置。局部更新保留其他字段，写入走临时文件 + 原子重命名，且只在持有仓库锁时发生。

```json
{
  "version": 1,
  "storage": {
    "watermarks": {
      "elevated": 75, "high": 85, "critical": 92,
      "exitElevated": 70, "exitHigh": 78, "exitCritical": 85
    },
    "idleDays": {"tmp": 5, "build": 10, "download": 21},
    "budgetBytes": 0,
    "buildHeadroomBytes": 2147483648,
    "coldHeadroomBytes": 4294967296,
    "log": {"maxBytes": 10485760, "keep": 3},
    "maintenance": {"maxBatches": 8, "maxPerBatch": 3, "maxMs": 5000}
  }
}
```

`budgetBytes: 0` 表示按容量自动推导。阈值必须有序：退出线必须低于对应的触发线，否则加载时拒绝启动。PID、日志、证书、缓存使用记录（`usage.json`）、清理状态（`storage.json`）、构建增长记录、锁和生成产物都不在这里，而是分别存放在 `.local/mode/`。

环境变量覆盖仍与 daemon 自身规则一致：管理器启动子进程时设置 `RUSTFRP_API_TOKEN_FILE`、`RUSTFRP_NO_AUTO_DOWNLOAD`，并显式清除 `RUSTFRP_API_TOKEN` 与 `RUSTFRP_AUTH_POLICY_FILE`，避免宿主机上的密钥串入开发实例。

## HTTPS 信任

OpenSSL 生成 30 天有效的随机自签证书，SAN 包括 localhost、127.0.0.1 和启动时的局域网 IPv4。证书临近到期或网卡地址改变时，下次启动重新签发；更换网络后运行 `mode:restart` 更新证书及地址。

管理器健康客户端只信任本仓库证书并验证主机名，不关闭 TLS 校验。浏览器信任需要自行设置；证书路径为 `.local/mode/cert.pem`。例如 Debian/Ubuntu 可复制证书到 `/usr/local/share/ca-certificates/rustfrp-<仓库标识>.crt` 后执行 `sudo update-ca-certificates`；其他发行版使用各自的证书管理工具。管理器不会更改系统信任库。浏览器可能有独立证书库，局域网其他设备也需要单独信任；证书更新后需重新导入。

## 输出与验证

```bash
bun run mode:status
bun run mode:clean
NO_COLOR=1 bun run mode:help
FORCE_COLOR=1 bun run mode:status
bun run --silent mode:status --json
bun run --silent mode:health --json
bun run --silent mode:clean --json
bun test scripts/mode/*.test.mjs
```

默认输出中文状态面板和分组帮助；TTY 使用语义颜色，重定向只去色、不改变布局。`NO_COLOR` 优先于 `FORCE_COLOR`。机器输出必须显式指定 `--json`；`--silent` 去掉 Bun 的命令回显，保证 stdout 只有单个 JSON 值。

## 已知边界

- 只有生命周期命令会在执行时清理过期缓存；长期不启动的仓库不会自行回收，需要显式运行 `mode:clean -- --execute`。
- 空间查询与归属核验依赖 Linux `statfs`、`/proc` 与 `flock`；其他平台请使用 `cargo clean` 与手动清理。
- `mode:clean` 不删除 Docker 镜像、卷或其他项目的缓存，也不会尝试整机范围的清理。
