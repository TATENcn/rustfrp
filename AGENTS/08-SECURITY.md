---
doc_id: 08-SECURITY
version: 1.0.0
last_modified: 2026-06-23
modification_policy: design
summary: 安全威胁模型、插件权限边界、敏感数据保护、通信安全
---

# 安全设计

## 一、威胁矩阵（MVP 必须覆盖）

以下威胁场景按严重程度排序，每个威胁列明对策与残余风险。

### 1.1 插件安全

| 威胁 | 攻击向量 | 对策 | 残余风险 |
|---|---|---|---|
| WASM 插件越权读写文件 | 插件内调用文件系统 API | Host Functions 白名单：仅暴露 `get_traffic()` / `get_config()` / `subscribe_event()` / `publish_event()`。不暴露任何文件、网络、进程操作函数 | 恶意插件在白名单范围内读取流量数据 → 仅限当前会话数据泄露，无法持久化 |
| 动态库插件代码注入 | 用户下载了被篡改的 `.so/.dll` 插件 | 加载前 SHA256 校验 + 用户显式授权弹窗（列出插件请求的权限清单） | 用户点击「同意」后的责任归用户。未来可加入签名链验证 |
| Sidecar 插件伪造 | 恶意程序伪装成 Sidecar 插件与核心通信 | Sidecar 通过 stdin/stdout JSON-RPC 协议通信，核心层不信任任何来自 Sidecar 的数据。所有通过 Sidecar 的操作均需再次经过核心层权限校验 | Sidecar 可发送大量垃圾数据导致 CPU 占用 → 通过 PLG-005 资源限制（消息 < 1MB）控制 |
| 插件崩溃拖垮核心 | WASM 插件 panic 或死循环 | Wasmtime 沙箱隔离 + `catch_unwind` + 燃料计量（fuel metering）限制 WASM 执行步数 | 理论上 Wasmtime 自身可能存在逃逸漏洞，但概率极低 |

### 1.2 数据安全

| 威胁 | 攻击向量 | 对策 | 残余风险 |
|---|---|---|---|
| SQLite 中 Token/密码明文泄露 | SQLite 文件被拷贝 | **MVP 暂不做加密**。理由：Token 与设备强绑定（设备已被攻破则无所谓 Token 泄露）。数据库文件权限设为 `0600`（仅 owner 可读写） | 若攻击者通过其他方式获取 SQLite 文件且设备未攻破，可读取 Token |
| 生成的 TOML 文件中包含敏感信息 | TOML 生成时的临时文件被读取 | 原子写入（tmp → rename），临时文件写入后立即 rename。生成的 TOML 中 Token 使用 FRP 原生变量替换 `${FRP_TOKEN}`，由核心层在启动时通过安全临时文件注入实际值 | 若 FRP 本身的变量替换机制有缺陷，Token 可被读取 |
| 配置导出包泄露 | 用户导出的加密配置包被截获 | 导出时使用 AES-256-GCM 加密，口令由用户设置。导出文件扩展名 `.rrp`（RustFRP encrypted package） | 口令强度由用户决定 |
| 日志中 Token/密码泄露 | tracing span 中记录了 Token 字段 | 使用 `tracing` 的 `#[instrument(skip(token))]` 跳过敏感字段。日志输出层面增加全局脱敏过滤器 | 开发者忘记在新增日志点加 `skip` → 通过 code review 和 Clippy lint 检查 |

### 1.3 通信安全

| 威胁 | 攻击向量 | 对策 | 残余风险 |
|---|---|---|---|
| `/metrics` 端点被公网访问 | FRPS 暴露在公网，`/metrics` 无鉴权 | 文档明确要求：内网拉取，或前置 Basic Auth 反向代理（Nginx/Caddy） | 用户未按文档部署则自行承担 |
| 监控服务器拉取数据被中间人截获 | 拉取路径上的网络设备被控制 | 使用 HTTPS 拉取 `/metrics`（若 FRPS 前端 Nginx 配置了 TLS） | 纯内网环境无 TLS → 信任内网安全性 |
| 客户端与监控服务器之间通信被窃听 | 未来可能的配置同步功能 | **当前不做**。监控服务器绝对只读，不向客户端回写任何数据 | — |

### 1.4 进程安全

| 威胁 | 攻击向量 | 对策 | 残余风险 |
|---|---|---|---|
| frpc 子进程权限过高 | frpc 以父进程权限运行 | frpc 以当前用户权限运行（无 privilege escalation）。未来可考虑 `seccomp` 限制系统调用 | 当前用户权限 = frpc 权限 |
| 信号注入 | 恶意进程向 frpc 发送伪造信号 | 核心层仅监听来自自身的信号。SIGHUP 仅由核心层在原子写入 TOML 后发送 | 其他进程可发送 SIGTERM 强制停止 frpc → OS 级权限问题，非本项目范围 |

---

## 二、插件权限模型

### 2.1 权限类型

| 权限标识 | 含义 | WIT 中对应的 Host Function |
|---|---|---|
| `read-config` | 读取配置（只读） | `get_active_profiles()` / `get_active_bindings()` |
| `read-traffic` | 读取流量数据（只读） | `get_current()` / `subscribe(callback)` |
| `subscribe-events` | 订阅核心事件 | `subscribe(event_type, callback)` |
| `publish-events` | 发布事件给其他插件 | `publish_event(event)` |
| `network-access` | 访问网络（严格限制） | 暂不开放给 WASM 插件 |
| `filesystem-access` | 访问文件系统（严格限制） | 暂不开放给 WASM 插件 |

### 2.2 权限校验流程

```
插件调用 Host Function
        ↓
核心层检查：调用方插件的 manifest.permissions 是否包含所需权限
        ↓ 否                    ↓ 是
操作被拒绝 + 记录日志          执行操作 + 记录日志
"plugin X attempted            "plugin X read traffic
 to access filesystem           stats"
 without permission"
```

### 2.3 权限最小化原则

- 插件 `manifest.json` 中 `permissions` 字段声明所需权限
- **不申请不需要的权限**——这是插件审核的重要检查项
- 核心层在每次 Host Function 调用时都做权限校验（不缓存权限结果）

---

## 三、敏感数据保护

### 3.1 数据分级

| 级别 | 内容 | 保护措施 |
|---|---|---|
| 高敏感 | FRP Token、TLS 证书私钥、用户密码 | 日志中脱敏，不进入环境变量，不序列化到 TOML（用变量替换） |
| 中敏感 | FRPS 地址列表、代理规则拓扑 | 不进入日志（或仅记录数量），导出时加密 |
| 低敏感 | 连接数、流量统计 | 可进入日志和监控大盘 |

### 3.2 脱敏实现

```rust
// 方式 1：tracing 宏跳过字段
#[instrument(skip(token, tls_key))]
pub fn create_profile(&self, profile: FrpsProfile) -> Result<()> { ... }

// 方式 2：Display 实现中替换
impl fmt::Display for FrpsProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FrpsProfile({}, {}:{}", self.name, self.server_addr, self.server_port)
        // 不输出 token
    }
}

// 方式 3：全局日志过滤器
// tracing-subscriber 的 filter layer 匹配 token 字段并替换为 [REDACTED]
```

---

## 四、未来安全增强（Phase 2+）

| 增强项 | 说明 | 优先级 |
|---|---|---|
| mTLS 双向认证 | 客户端与控制面之间双向证书验证 | 企业级需求时实现 |
| 插件签名验证 | 插件发布时签名，加载前验证签名链 | 当第三方插件生态形成时必要 |
| SQLite 加密 | 使用 SQLCipher 加密数据库文件 | 当 Token 加密存储成为强需求时 |
| seccomp 沙箱 | frpc 子进程的系统调用过滤 | 当服务端部署在不可信环境时 |
| OIDC 集成 | 监控大盘的 SSO 登录 | 当多用户访问监控时 |
| 零信任 Web 鉴权 | 穿透后对 Web 服务的访问鉴权（类似 Cloudflare Access） | 企业级方向 |
