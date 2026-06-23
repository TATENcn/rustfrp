//! 数据模型
//!
//! 所有字段 1:1 对应 FRP 官方 TOML 规范（ARCH-004），
//! 不发明任何 FRP 不支持的配置项。

use serde::{Deserialize, Serialize};

// ============================================================================
// FrpsProfile — 服务端连接配置
// ============================================================================

/// FRP 服务端连接配置
///
/// 对应 FRP TOML 中的 `[common]` 段。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrpsProfile {
    pub id: Option<i64>,
    pub name: String,
    pub server_addr: String,
    pub server_port: u16,
    pub token: String,
    pub tls_enable: bool,
    pub tls_cert_file: Option<String>,
    pub tls_key_file: Option<String>,
    pub tls_trusted_ca_file: Option<String>,
    pub transport_protocol: String,
    pub heartbeat_interval: i64,
    pub heartbeat_timeout: i64,
    pub created_at: String,
    pub updated_at: String,
}

impl Default for FrpsProfile {
    fn default() -> Self {
        Self {
            id: None,
            name: String::new(),
            server_addr: "0.0.0.0".into(),
            server_port: 7000,
            token: String::new(),
            tls_enable: false,
            tls_cert_file: None,
            tls_key_file: None,
            tls_trusted_ca_file: None,
            transport_protocol: "tcp".into(),
            heartbeat_interval: 30,
            heartbeat_timeout: 90,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }
}

// ============================================================================
// LocalProxy — 本地代理配置
// ============================================================================

/// 代理类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProxyType {
    Tcp,
    Udp,
    Http,
    Https,
    Stcp,
    Xtcp,
}

impl ProxyType {
    /// 返回 FRP TOML 中使用的字符串
    pub fn as_frp_str(&self) -> &'static str {
        match self {
            ProxyType::Tcp => "tcp",
            ProxyType::Udp => "udp",
            ProxyType::Http => "http",
            ProxyType::Https => "https",
            ProxyType::Stcp => "stcp",
            ProxyType::Xtcp => "xtcp",
        }
    }
}

impl std::str::FromStr for ProxyType {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "tcp" => Ok(ProxyType::Tcp),
            "udp" => Ok(ProxyType::Udp),
            "http" => Ok(ProxyType::Http),
            "https" => Ok(ProxyType::Https),
            "stcp" => Ok(ProxyType::Stcp),
            "xtcp" => Ok(ProxyType::Xtcp),
            other => Err(format!("Unknown proxy type: {other}")),
        }
    }
}

impl rusqlite::types::FromSql for ProxyType {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        let s = value.as_str()?;
        <Self as std::str::FromStr>::from_str(s).map_err(|e| {
            rusqlite::types::FromSqlError::Other(Box::new(
                std::io::Error::new(std::io::ErrorKind::InvalidData, e)
            ))
        })
    }
}

impl std::fmt::Display for ProxyType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_frp_str())
    }
}

/// 本地代理配置
///
/// 对应 FRP TOML 中的每个 `[[proxies]]` 条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalProxy {
    pub id: Option<i64>,
    pub name: String,
    pub proxy_type: ProxyType,
    pub local_ip: String,
    pub local_port: u16,
    pub remote_port: Option<u16>,
    pub custom_domains: Option<Vec<String>>,
    pub subdomain: Option<String>,
    pub use_encryption: bool,
    pub use_compression: bool,
    pub bandwidth_limit: Option<String>,
    pub health_check_type: Option<String>,
    pub health_check_timeout_s: i64,
    pub health_check_max_failed: i64,
    pub health_check_interval_s: i64,
    /// FRP 原生插件配置（对应 TOML 中的 `[proxies.plugin]` 段）
    ///
    /// None 表示不使用 FRP 原生插件。
    /// 如：`{ "type": "https2http", "pluginLocalAddr": "127.0.0.1:80" }`
    pub plugin_config: Option<serde_json::Value>,
    pub created_at: String,
    pub updated_at: String,
}

impl Default for LocalProxy {
    fn default() -> Self {
        Self {
            id: None,
            name: String::new(),
            proxy_type: ProxyType::Tcp,
            local_ip: "127.0.0.1".into(),
            local_port: 0,
            remote_port: None,
            custom_domains: None,
            subdomain: None,
            use_encryption: true,
            use_compression: true,
            bandwidth_limit: None,
            health_check_type: None,
            health_check_timeout_s: 3,
            health_check_max_failed: 3,
            health_check_interval_s: 10,
            plugin_config: None,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }
}

// ============================================================================
// BindingRule — 绑定规则（多对多关联）
// ============================================================================

/// 绑定规则
///
/// 将 Profile 和 Proxy 关联在一起，支持多对多。
/// 设备 A 的 Proxy X 可以绑定到多个 Profile，实现多服务器同时穿透。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindingRule {
    pub id: Option<i64>,
    pub profile_id: i64,
    pub proxy_id: i64,
    pub enabled: bool,
    pub priority: i32,
    pub group_name: Option<String>,
    pub group_key: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

// ============================================================================
// FRP TOML 输出模型（serde 序列化用）
// ============================================================================

/// 生成 frpc.toml 时使用的完整配置结构
///
/// 此结构仅用于 TOML 生成，不存储在 SQLite 中（ARCH-003）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrpcConfig {
    #[serde(rename = "serverAddr")]
    pub server_addr: String,
    #[serde(rename = "serverPort")]
    pub server_port: u16,
    pub token: Option<String>,
    #[serde(rename = "transport")]
    pub transport: Option<TransportConfig>,
    #[serde(rename = "proxies")]
    pub proxies: Vec<ProxyEntry>,
}

/// 传输层配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportConfig {
    pub protocol: String,
    #[serde(rename = "tls")]
    pub tls: Option<TlsConfig>,
}

/// TLS 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    #[serde(rename = "enable")]
    pub enable: bool,
    #[serde(rename = "certFile", skip_serializing_if = "Option::is_none")]
    pub cert_file: Option<String>,
    #[serde(rename = "keyFile", skip_serializing_if = "Option::is_none")]
    pub key_file: Option<String>,
    #[serde(rename = "trustedCaFile", skip_serializing_if = "Option::is_none")]
    pub trusted_ca_file: Option<String>,
}

/// 单个代理条目
///
/// 对应 FRP TOML 中 `[[proxies]]` 下的一个条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyEntry {
    pub name: String,
    #[serde(rename = "type")]
    pub proxy_type: String,
    #[serde(rename = "localIP")]
    pub local_ip: String,
    #[serde(rename = "localPort")]
    pub local_port: u16,
    #[serde(rename = "remotePort", skip_serializing_if = "Option::is_none")]
    pub remote_port: Option<u16>,
    #[serde(
        rename = "customDomains",
        skip_serializing_if = "Option::is_none"
    )]
    pub custom_domains: Option<Vec<String>>,
    #[serde(rename = "subdomain", skip_serializing_if = "Option::is_none")]
    pub subdomain: Option<String>,
    #[serde(rename = "useEncryption")]
    pub use_encryption: bool,
    #[serde(rename = "useCompression")]
    pub use_compression: bool,
    #[serde(
        rename = "bandwidthLimit",
        skip_serializing_if = "Option::is_none"
    )]
    pub bandwidth_limit: Option<String>,
    #[serde(rename = "healthCheck")]
    pub health_check: Option<HealthCheckConfig>,
    /// FRP 原生插件（对应 TOML 的 `[proxies.plugin]` 段）
    #[serde(rename = "plugin", skip_serializing_if = "Option::is_none")]
    pub plugin: Option<serde_json::Value>,
}

/// 健康检查配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckConfig {
    #[serde(rename = "type")]
    pub check_type: String,
    #[serde(rename = "timeoutSeconds")]
    pub timeout_s: i64,
    #[serde(rename = "maxFailed")]
    pub max_failed: i64,
    #[serde(rename = "intervalSeconds")]
    pub interval_s: i64,
}

