# frpc 版本与安装渠道

## 入口与行为

- **系统状态 → FRP 版本**：安装版本、查看后台进度与来源、设置系统默认或删除未使用版本。
- **服务器配置 → 每行的 frpc 版本按钮**：选择“跟随系统默认”或固定已安装且校验通过的版本。新建配置默认跟随系统，保存配置后可单独设置。
- **代理分配**：显示服务器当前运行版本；版本变更未应用时显示“待重启”，点击跳转服务器配置。

一个服务器配置对应一个 frpc 进程，其代理共用版本。安装不激活版本，也不重启进程。保存服务器版本选择不影响当前连接；点击“重启并应用版本”后，该服务器下所有代理连接会短暂中断。

切换系统默认版本时只重启跟随默认的运行实例，固定版本的实例保留原进程。切换启动失败会恢复原默认版本并尝试恢复相关实例。被配置引用、当前默认或仍有进程运行的版本禁止删除；因此保存新选择后，旧进程仍在运行时旧版本不能删除。

版本策略保存在 SQLite `profile_runtime.frpc_version`（V15 增量迁移），不写进 FRP TOML。数据库备份包含该策略，单纯导出 frpc.toml 不包含版本选择。固定版本缺失或校验失败时明确报错，不自动替换为其他版本。后台自动拉起与手动启动使用同一版本选择逻辑。

## 下载渠道

- GitHub 官方：`https://github.com/fatedier/frp/releases/download`
- GHFast：`https://ghfast.top/https://github.com/fatedier/frp/releases/download`
- GH-Proxy：`https://gh-proxy.com/https://github.com/fatedier/frp/releases/download`
- 自定义 HTTPS Release 根地址：其后会拼接 `/v版本号/frp_版本号_平台.tar.gz`（Windows 为 zip）。不允许账号、密码、查询参数或片段。

默认使用官方渠道，不自动切换第三方。公共加速器的速度、可用性随网络变化；失败时可在界面查看原因并选择其他渠道重试。版本建议来自官方 GitHub API，建议列表不可用时可以手动输入版本号。

校验信息优先读取官方 GitHub Release API 的资产 SHA-256 digest，缺少 digest 或 API 不可用时读取官方 Release 校验清单。不从加速器读取可信校验值，也不跳过校验。无法取得官方校验信息、校验不符、解压失败均停止安装。

下载采用流式写入，限制单包 256 MiB；完整下载后重命名，随后校验、解压和安装。后台任务展示下载字节数和阶段；页面关闭后继续运行，守护进程重启会中断任务，需要重新提交。只保留当前/最近一次任务状态，不是永久安装日志。已验证的缓存版本直接复用，不改写原来源；旧版本未记录来源时显示“来源未记录”。

## API

- `GET/PUT /api/v1/profiles/:id/frpc-version`：读取版本与待重启状态；提交 `{"version": null}` 跟随默认，或指定版本字符串。按租户检查配置权限。
- `GET/POST /api/v1/frp/installations`：查询任务或创建任务；提交 `version`、`channel`（`official`、`ghfast`、`gh-proxy`、`custom`）及可选 `mirror_base`。
- 保留旧同步安装 `POST /api/v1/frp/versions` 以兼容现有调用方。
- 版本注册表和安装任务使用平台级读写权限；服务器版本选择使用对应租户读写权限。

## 验证

Rust 测试覆盖渠道地址验证、版本引用与删除、默认切换不重启固定版本实例、保存待重启状态、实际切换进程、跨租户拒绝以及后台缓存安装任务。

可手动运行隔离的真实下载测试（下载到临时目录，不影响当前安装）：

```sh
cargo test -p rustfrp-bin install_via_ghfast_with_official_verification --locked -- --ignored --nocapture
```
