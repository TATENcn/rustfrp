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
    /// Authentication token (redacted in API responses).
    #[serde(skip_serializing, default)]
    pub token: String,
    pub tls_enable: bool,
    pub tls_cert_file: Option<String>,
    pub tls_key_file: Option<String>,
    pub tls_trusted_ca_file: Option<String>,
    pub transport_protocol: String,
    pub heartbeat_interval: i64,
    pub heartbeat_timeout: i64,
    /// Dial server timeout in seconds (default 10).
    pub dial_server_timeout: Option<i64>,
    /// TCP keepalive interval for connection to server (seconds).
    pub dial_server_keepalive: Option<i64>,
    /// Local IP to bind when connecting to server.
    pub connect_server_local_ip: Option<String>,
    /// Proxy URL for connecting to server (http/socks5/ntlm).
    pub proxy_url: Option<String>,
    /// Connection pool size.
    pub pool_count: Option<i32>,
    /// Enable TCP multiplexing (default true).
    pub tcp_mux: Option<bool>,
    /// TCP mux keepalive interval.
    pub tcp_mux_keepalive_interval: Option<i64>,
    /// QUIC keepalive period in seconds.
    pub quic_keepalive_period: Option<i32>,
    /// QUIC max idle timeout in seconds.
    pub quic_max_idle_timeout: Option<i32>,
    /// QUIC max incoming streams.
    pub quic_max_incoming_streams: Option<i32>,
    /// Authentication method: "token" or "oidc". Default: token.
    pub auth_method: Option<String>,
    /// OIDC client ID (when auth_method = "oidc").
    pub oidc_client_id: Option<String>,
    /// OIDC client secret.
    pub oidc_client_secret: Option<String>,
    /// OIDC token endpoint URL.
    pub oidc_token_endpoint_url: Option<String>,
    /// OIDC audience.
    pub oidc_audience: Option<String>,
    /// OIDC scope.
    pub oidc_scope: Option<String>,
    /// Additional OIDC endpoint params (JSON).
    pub oidc_additional_endpoint_params: Option<String>,
    /// Username prefix to avoid proxy name conflicts across clients.
    pub user: Option<String>,
    /// Global metadata passed to server plugins.
    pub metadatas: Option<String>,
    /// Exit if first login fails (default true).
    pub login_fail_exit: Option<bool>,
    /// Start only specified proxy names.
    pub start: Option<Vec<String>>,
    /// Custom DNS server address.
    pub dns_server: Option<String>,
    /// STUN server for XTCP NAT hole punching.
    pub nat_hole_stun_server: Option<String>,
    /// UDP packet size (default 1500, must match server).
    pub udp_packet_size: Option<i32>,
    /// Include additional config directories.
    pub includes: Option<Vec<String>>,
    /// Persistent store path for runtime dynamic config.
    pub store_path: Option<String>,
    /// Feature gates, used to enable or disable experimental features (map[string]bool as JSON).
    pub feature_gates: Option<String>,
    #[serde(skip_deserializing)]
    pub created_at: String,
    #[serde(skip_deserializing)]
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
            dial_server_timeout: None,
            dial_server_keepalive: None,
            connect_server_local_ip: None,
            proxy_url: None,
            pool_count: None,
            tcp_mux: None,
            tcp_mux_keepalive_interval: None,
            quic_keepalive_period: None,
            quic_max_idle_timeout: None,
            quic_max_incoming_streams: None,
            auth_method: None,
            oidc_client_id: None,
            oidc_client_secret: None,
            oidc_token_endpoint_url: None,
            oidc_audience: None,
            oidc_scope: None,
            oidc_additional_endpoint_params: None,
            user: None,
            metadatas: None,
            login_fail_exit: None,
            start: None,
            dns_server: None,
            nat_hole_stun_server: None,
            udp_packet_size: None,
            includes: None,
            store_path: None,
            feature_gates: None,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }
}

// ============================================================================
// LocalProxy — 本地代理配置
// ============================================================================

/// HTTP header name-value pair (maps to FRP TOML `httpHeaders`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpHeader {
    pub name: String,
    pub value: String,
}

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
    Tcpmux,
    Sudp,
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
            ProxyType::Tcpmux => "tcpmux",
            ProxyType::Sudp => "sudp",
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
            "tcpmux" => Ok(ProxyType::Tcpmux),
            "sudp" => Ok(ProxyType::Sudp),
            other => Err(format!("Unknown proxy type: {other}")),
        }
    }
}

impl rusqlite::types::FromSql for ProxyType {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        let s = value.as_str()?;
        <Self as std::str::FromStr>::from_str(s).map_err(|e| {
            rusqlite::types::FromSqlError::Other(Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                e,
            )))
        })
    }
}

impl std::fmt::Display for ProxyType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_frp_str())
    }
}

// ============================================================================
// Visitor 类型与配置
// ============================================================================

/// Visitor type (mirrors FRP visitor types).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VisitorType {
    Stcp,
    Sudp,
    Xtcp,
}

impl VisitorType {
    /// Returns the FRP TOML string for this visitor type.
    pub fn as_frp_str(&self) -> &'static str {
        match self {
            VisitorType::Stcp => "stcp",
            VisitorType::Sudp => "sudp",
            VisitorType::Xtcp => "xtcp",
        }
    }
}

impl std::str::FromStr for VisitorType {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "stcp" => Ok(VisitorType::Stcp),
            "sudp" => Ok(VisitorType::Sudp),
            "xtcp" => Ok(VisitorType::Xtcp),
            other => Err(format!("Unknown visitor type: {other}")),
        }
    }
}

impl std::fmt::Display for VisitorType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_frp_str())
    }
}

impl rusqlite::types::FromSql for VisitorType {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        let s = value.as_str()?;
        <Self as std::str::FromStr>::from_str(s).map_err(|e| {
            rusqlite::types::FromSqlError::Other(Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                e,
            )))
        })
    }
}

/// Local visitor configuration (maps to FRP TOML `[[visitors]]`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalVisitor {
    pub id: Option<i64>,
    pub name: String,
    pub visitor_type: VisitorType,
    /// Target proxy name on the server side.
    pub server_name: String,
    /// Target proxy user on the server side (empty = current user).
    pub server_user: Option<String>,
    /// Visitor local bind address.
    pub bind_addr: Option<String>,
    /// Visitor local bind port. -1 means no physical port.
    pub bind_port: i32,
    /// Secret key (must match the proxy's secretKey).
    pub secret_key: Option<String>,
    /// Enabled flag.
    pub enabled: bool,
    /// Use encryption for transport.
    pub use_encryption: bool,
    /// Use compression for transport.
    pub use_compression: bool,
    /// XTCP-specific: underlying tunnel protocol (quic/kcp).
    pub xtcp_protocol: Option<String>,
    /// XTCP-specific: keep tunnel open.
    pub keep_tunnel_open: Option<bool>,
    /// XTCP-specific: max retries per hour.
    pub max_retries_an_hour: Option<i32>,
    /// XTCP-specific: min retry interval in seconds.
    pub min_retry_interval: Option<i32>,
    /// XTCP-specific: fallback visitor name.
    pub fallback_to: Option<String>,
    /// XTCP-specific: fallback timeout in ms.
    pub fallback_timeout_ms: Option<i32>,
    /// Visitor plugin configuration (JSON blob).
    pub plugin_config: Option<serde_json::Value>,
    /// Which profile this visitor belongs to.
    pub profile_id: i64,
    /// Annotations (map[string]string as JSON string).
    pub annotations: Option<String>,
    #[serde(skip_deserializing)]
    pub created_at: String,
    #[serde(skip_deserializing)]
    pub updated_at: String,
}

impl Default for LocalVisitor {
    fn default() -> Self {
        Self {
            id: None,
            name: String::new(),
            visitor_type: VisitorType::Stcp,
            server_name: String::new(),
            server_user: None,
            bind_addr: None,
            bind_port: -1,
            secret_key: None,
            enabled: true,
            use_encryption: true,
            use_compression: true,
            xtcp_protocol: None,
            keep_tunnel_open: None,
            max_retries_an_hour: None,
            min_retry_interval: None,
            fallback_to: None,
            fallback_timeout_ms: None,
            plugin_config: None,
            profile_id: 0,
            annotations: None,
            created_at: String::new(),
            updated_at: String::new(),
        }
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
    /// Bandwidth limit mode: "client" or "server". Default: client-side limiting.
    pub bandwidth_limit_mode: Option<String>,
    /// Secret key for STCP/XTCP/SUDP proxy types.
    /// Server and visitor must have matching keys.
    pub secret_key: Option<String>,
    /// URL locations for HTTP routing.
    pub locations: Option<Vec<String>>,
    /// HTTP Basic Auth username.
    pub http_user: Option<String>,
    /// HTTP Basic Auth password.
    pub http_password: Option<String>,
    /// Rewrite Host header.
    pub host_header_rewrite: Option<String>,
    /// Request header operations (JSON: {"set": {"X-Foo": "bar"}}).
    pub request_headers: Option<String>,
    /// Response header operations (JSON).
    pub response_headers: Option<String>,
    /// Route by HTTP Basic Auth user.
    pub route_by_http_user: Option<String>,
    /// Annotations displayed in server Dashboard (map[string]string as JSON).
    pub annotations: Option<String>,
    /// Additional metadata passed to server plugins (map[string]string as JSON).
    pub metadatas: Option<String>,
    /// List of visitor users allowed to access (stcp/xtcp/sudp only).
    /// Configure as "*" to allow any visitor.
    pub allow_users: Option<Vec<String>>,
    /// NAT traversal: disable assisted connections using local network interface addresses (xtcp only).
    pub nat_traversal_disable_assisted_addrs: Option<bool>,
    /// PROXY protocol version to enable ("v1" or "v2").
    pub proxy_protocol_version: Option<String>,
    /// HTTP health check path (e.g. "/health"). Only effective when health_check_type = "http".
    pub health_check_path: Option<String>,
    /// HTTP health check request headers. Only effective when health_check_type = "http".
    pub health_check_http_headers: Option<Vec<HttpHeader>>,
    /// FRP 原生插件配置（对应 TOML 中的 `[proxies.plugin]` 段）
    ///
    /// None 表示不使用 FRP 原生插件。
    /// 如：`{ "type": "https2http", "pluginLocalAddr": "127.0.0.1:80" }`
    pub plugin_config: Option<serde_json::Value>,
    #[serde(skip_deserializing)]
    pub created_at: String,
    #[serde(skip_deserializing)]
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
            bandwidth_limit_mode: None,
            secret_key: None,
            locations: None,
            http_user: None,
            http_password: None,
            host_header_rewrite: None,
            request_headers: None,
            response_headers: None,
            route_by_http_user: None,
            annotations: None,
            metadatas: None,
            allow_users: None,
            nat_traversal_disable_assisted_addrs: None,
            proxy_protocol_version: None,
            health_check_type: None,
            health_check_timeout_s: 3,
            health_check_max_failed: 3,
            health_check_interval_s: 10,
            health_check_path: None,
            health_check_http_headers: None,
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
    pub enabled: bool, // 配置是否已完成、允许被启动（资格）
    pub running: bool, // 代理是否正在 frpc 进程中运行（事实）
    pub priority: i32,
    pub group_name: Option<String>,
    pub group_key: Option<String>,
    #[serde(skip_deserializing)]
    pub created_at: String,
    #[serde(skip_deserializing)]
    pub updated_at: String,
}

// ============================================================================
// FRP TOML 输出模型（serde 序列化用）
// ============================================================================

/// Visitor transport configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisitorTransportConfig {
    #[serde(rename = "useEncryption")]
    pub use_encryption: bool,
    #[serde(rename = "useCompression")]
    pub use_compression: bool,
}

/// Proxy transport configuration.
///
/// Maps to `transport.*` dotted keys under each `[[proxies]]` entry.
/// In FRP TOML format (v0.52+), useEncryption, useCompression,
/// bandwidthLimit, bandwidthLimitMode, and proxyProtocolVersion
/// are nested under the transport section rather than at proxy level.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyTransportConfig {
    #[serde(rename = "useEncryption")]
    pub use_encryption: bool,
    #[serde(rename = "useCompression")]
    pub use_compression: bool,
    #[serde(rename = "bandwidthLimit", skip_serializing_if = "Option::is_none")]
    pub bandwidth_limit: Option<String>,
    #[serde(rename = "bandwidthLimitMode", skip_serializing_if = "Option::is_none")]
    pub bandwidth_limit_mode: Option<String>,
    #[serde(
        rename = "proxyProtocolVersion",
        skip_serializing_if = "Option::is_none"
    )]
    pub proxy_protocol_version: Option<String>,
}

/// Single visitor entry in TOML (maps to `[[visitors]]`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisitorEntry {
    pub name: String,
    #[serde(rename = "type")]
    pub visitor_type: String,
    #[serde(rename = "serverName")]
    pub server_name: String,
    #[serde(rename = "serverUser", skip_serializing_if = "Option::is_none")]
    pub server_user: Option<String>,
    #[serde(rename = "bindAddr", skip_serializing_if = "Option::is_none")]
    pub bind_addr: Option<String>,
    #[serde(rename = "bindPort")]
    pub bind_port: i32,
    #[serde(rename = "secretKey", skip_serializing_if = "Option::is_none")]
    pub secret_key: Option<String>,
    #[serde(rename = "transport", skip_serializing_if = "Option::is_none")]
    pub transport: Option<VisitorTransportConfig>,
    /// XTCP-specific: underlying tunnel protocol (quic/kcp).
    #[serde(rename = "protocol", skip_serializing_if = "Option::is_none")]
    pub xtcp_protocol: Option<String>,
    /// XTCP-specific: keep tunnel open.
    #[serde(rename = "keepTunnelOpen", skip_serializing_if = "Option::is_none")]
    pub keep_tunnel_open: Option<bool>,
    /// XTCP-specific: max retries per hour.
    #[serde(rename = "maxRetriesAnHour", skip_serializing_if = "Option::is_none")]
    pub max_retries_an_hour: Option<i32>,
    /// XTCP-specific: min retry interval in seconds.
    #[serde(rename = "minRetryInterval", skip_serializing_if = "Option::is_none")]
    pub min_retry_interval: Option<i32>,
    /// XTCP-specific: fallback visitor name.
    #[serde(rename = "fallbackTo", skip_serializing_if = "Option::is_none")]
    pub fallback_to: Option<String>,
    /// XTCP-specific: fallback timeout in ms.
    #[serde(rename = "fallbackTimeoutMs", skip_serializing_if = "Option::is_none")]
    pub fallback_timeout_ms: Option<i32>,
    /// Visitor plugin config.
    #[serde(rename = "plugin", skip_serializing_if = "Option::is_none")]
    pub plugin: Option<serde_json::Value>,
}

/// Authentication configuration for FRP TOML output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    pub method: Option<String>,
    pub token: Option<String>,
    #[serde(rename = "oidc", skip_serializing_if = "Option::is_none")]
    pub oidc: Option<OidcClientConfig>,
}

/// OIDC client configuration for FRP TOML output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcClientConfig {
    #[serde(rename = "clientID")]
    pub client_id: String,
    #[serde(rename = "clientSecret")]
    pub client_secret: String,
    #[serde(rename = "audience", skip_serializing_if = "Option::is_none")]
    pub audience: Option<String>,
    #[serde(rename = "scope", skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(rename = "tokenEndpointURL")]
    pub token_endpoint_url: String,
}

/// 生成 frpc.toml 时使用的完整配置结构
///
/// 此结构仅用于 TOML 生成，不存储在 SQLite 中（ARCH-003）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrpcConfig {
    #[serde(rename = "serverAddr")]
    pub server_addr: String,
    #[serde(rename = "serverPort")]
    pub server_port: u16,
    #[serde(rename = "user", skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    pub token: Option<String>,
    #[serde(rename = "transport")]
    pub transport: Option<TransportConfig>,
    #[serde(rename = "auth", skip_serializing_if = "Option::is_none")]
    pub auth: Option<AuthConfig>,
    #[serde(rename = "loginFailExit", skip_serializing_if = "Option::is_none")]
    pub login_fail_exit: Option<bool>,
    #[serde(rename = "metadatas", skip_serializing_if = "Option::is_none")]
    pub metadatas: Option<serde_json::Value>,
    #[serde(rename = "dnsServer", skip_serializing_if = "Option::is_none")]
    pub dns_server: Option<String>,
    #[serde(rename = "natHoleStunServer", skip_serializing_if = "Option::is_none")]
    pub nat_hole_stun_server: Option<String>,
    #[serde(rename = "udpPacketSize", skip_serializing_if = "Option::is_none")]
    pub udp_packet_size: Option<i32>,
    #[serde(rename = "includes", skip_serializing_if = "Option::is_none")]
    pub includes: Option<Vec<String>>,
    #[serde(rename = "featureGates", skip_serializing_if = "Option::is_none")]
    pub feature_gates: Option<serde_json::Value>,
    #[serde(rename = "proxies")]
    pub proxies: Vec<ProxyEntry>,
    #[serde(rename = "visitors", skip_serializing_if = "Vec::is_empty")]
    pub visitors: Vec<VisitorEntry>,
}

/// 传输层配置
///
/// 对应 FRP TOML 中的 `[transport]` 段。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportConfig {
    pub protocol: String,
    #[serde(rename = "tls")]
    pub tls: Option<TlsConfig>,
    #[serde(rename = "heartbeatInterval", skip_serializing_if = "Option::is_none")]
    pub heartbeat_interval: Option<i64>,
    #[serde(rename = "heartbeatTimeout", skip_serializing_if = "Option::is_none")]
    pub heartbeat_timeout: Option<i64>,
    #[serde(rename = "dialServerTimeout", skip_serializing_if = "Option::is_none")]
    pub dial_server_timeout: Option<i64>,
    #[serde(
        rename = "dialServerKeepalive",
        skip_serializing_if = "Option::is_none"
    )]
    pub dial_server_keepalive: Option<i64>,
    #[serde(
        rename = "connectServerLocalIP",
        skip_serializing_if = "Option::is_none"
    )]
    pub connect_server_local_ip: Option<String>,
    #[serde(rename = "proxyURL", skip_serializing_if = "Option::is_none")]
    pub proxy_url: Option<String>,
    #[serde(rename = "poolCount", skip_serializing_if = "Option::is_none")]
    pub pool_count: Option<i32>,
    #[serde(rename = "tcpMux", skip_serializing_if = "Option::is_none")]
    pub tcp_mux: Option<bool>,
    #[serde(
        rename = "tcpMuxKeepaliveInterval",
        skip_serializing_if = "Option::is_none"
    )]
    pub tcp_mux_keepalive_interval: Option<i64>,
    #[serde(rename = "quic", skip_serializing_if = "Option::is_none")]
    pub quic: Option<QuicConfig>,
}

/// QUIC transport configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuicConfig {
    #[serde(rename = "keepalivePeriod", skip_serializing_if = "Option::is_none")]
    pub keepalive_period: Option<i32>,
    #[serde(rename = "maxIdleTimeout", skip_serializing_if = "Option::is_none")]
    pub max_idle_timeout: Option<i32>,
    #[serde(rename = "maxIncomingStreams", skip_serializing_if = "Option::is_none")]
    pub max_incoming_streams: Option<i32>,
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
    #[serde(rename = "customDomains", skip_serializing_if = "Option::is_none")]
    pub custom_domains: Option<Vec<String>>,
    #[serde(rename = "subdomain", skip_serializing_if = "Option::is_none")]
    pub subdomain: Option<String>,
    /// Transport-level configuration (useEncryption, useCompression, bandwidth, etc.).
    /// In FRP TOML format v0.52+, these fields are nested under `transport.*` keys.
    #[serde(rename = "transport", skip_serializing_if = "Option::is_none")]
    pub transport: Option<ProxyTransportConfig>,
    #[serde(rename = "secretKey", skip_serializing_if = "Option::is_none")]
    pub secret_key: Option<String>,
    #[serde(rename = "locations", skip_serializing_if = "Option::is_none")]
    pub locations: Option<Vec<String>>,
    #[serde(rename = "httpUser", skip_serializing_if = "Option::is_none")]
    pub http_user: Option<String>,
    #[serde(rename = "httpPassword", skip_serializing_if = "Option::is_none")]
    pub http_password: Option<String>,
    #[serde(rename = "hostHeaderRewrite", skip_serializing_if = "Option::is_none")]
    pub host_header_rewrite: Option<String>,
    #[serde(rename = "requestHeaders", skip_serializing_if = "Option::is_none")]
    pub request_headers: Option<serde_json::Value>,
    #[serde(rename = "responseHeaders", skip_serializing_if = "Option::is_none")]
    pub response_headers: Option<serde_json::Value>,
    #[serde(rename = "routeByHTTPUser", skip_serializing_if = "Option::is_none")]
    pub route_by_http_user: Option<String>,
    #[serde(rename = "annotations", skip_serializing_if = "Option::is_none")]
    pub annotations: Option<serde_json::Value>,
    #[serde(rename = "metadatas", skip_serializing_if = "Option::is_none")]
    pub metadatas: Option<serde_json::Value>,
    #[serde(rename = "allowUsers", skip_serializing_if = "Option::is_none")]
    pub allow_users: Option<Vec<String>>,
    #[serde(rename = "natTraversal", skip_serializing_if = "Option::is_none")]
    pub nat_traversal: Option<NatTraversalConfig>,
    /// Load balancer config. Group proxies with same group name for round-robin.
    #[serde(rename = "loadBalancer", skip_serializing_if = "Option::is_none")]
    pub load_balancer: Option<LoadBalancerConfig>,
    #[serde(rename = "healthCheck")]
    pub health_check: Option<HealthCheckConfig>,
    /// FRP 原生插件（对应 TOML 的 `[proxies.plugin]` 段）
    #[serde(rename = "plugin", skip_serializing_if = "Option::is_none")]
    pub plugin: Option<serde_json::Value>,
}

/// NAT traversal configuration (XTCP only).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NatTraversalConfig {
    #[serde(rename = "disableAssistedAddrs")]
    pub disable_assisted_addrs: bool,
}

/// Load balancer config. Group proxies with same group name for round-robin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadBalancerConfig {
    #[serde(rename = "group")]
    pub group: String,
    #[serde(rename = "groupKey", skip_serializing_if = "Option::is_none")]
    pub group_key: Option<String>,
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
    #[serde(rename = "path", skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(rename = "httpHeaders", skip_serializing_if = "Option::is_none")]
    pub http_headers: Option<Vec<HttpHeader>>,
}
