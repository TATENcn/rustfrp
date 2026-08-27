//! Schema 校验器
//!
//! 启动前对配置进行校验，防止无效配置导致 frpc 启动失败。
//! MVP 阶段校验规则：
//! 1. IP 地址格式
//! 2. 端口范围 (1-65535)
//! 3. 必填字段非空
//!
//! 手写实现，不引入 validator/garde 等第三方校验框架（04-DEPENDENCIES.md）。

use crate::config::model::{
    BindingRule, FrpsProfile, LocalProxy, LocalVisitor, ProxyType, VisitorType,
};
use crate::error::{ClientError, Result};
use std::net::IpAddr;

impl FrpsProfile {
    /// 校验 Profile 配置
    ///
    /// 校验 Server 地址、端口、Token、心跳参数。
    pub fn validate(&self) -> Result<()> {
        // 名称必填
        if self.name.trim().is_empty() {
            return Err(ClientError::MissingRequiredField("name".into()));
        }

        // 服务端地址：合法 IP 或域名
        if !is_valid_host(&self.server_addr) {
            return Err(ClientError::InvalidIpAddress(self.server_addr.clone()));
        }

        // 端口范围
        if self.server_port == 0 {
            return Err(ClientError::InvalidPort("server_port cannot be 0".into()));
        }

        // Token 非空（生产环境基本要求）
        if self.token.trim().is_empty() {
            return Err(ClientError::MissingRequiredField(
                "token is required for FRP authentication".into(),
            ));
        }

        // 心跳参数合理性
        if self.heartbeat_interval < 1 {
            return Err(ClientError::ConfigValidation(
                "heartbeat_interval must be >= 1".into(),
            ));
        }
        if self.heartbeat_timeout < self.heartbeat_interval {
            return Err(ClientError::ConfigValidation(
                "heartbeat_timeout must be >= heartbeat_interval".into(),
            ));
        }

        // 传输协议
        if !["tcp", "kcp", "quic", "websocket", "wss"].contains(&self.transport_protocol.as_str()) {
            return Err(ClientError::ConfigValidation(format!(
                "Unsupported transport protocol: {}. Supported: tcp/kcp/quic/websocket/wss",
                self.transport_protocol
            )));
        }

        Ok(())
    }
}

impl LocalProxy {
    /// 校验 Proxy 配置
    ///
    /// 校验名称、类型、本地 IP、端口、健康检查参数。
    pub fn validate(&self) -> Result<()> {
        // 名称必填
        if self.name.trim().is_empty() {
            return Err(ClientError::MissingRequiredField("proxy name".into()));
        }

        // 本地 IP
        if !is_valid_host(&self.local_ip) {
            return Err(ClientError::InvalidIpAddress(self.local_ip.clone()));
        }

        // 端口范围
        if self.local_port == 0 {
            return Err(ClientError::InvalidPort("local_port cannot be 0".into()));
        }

        // remote_port：若提供则不能为 0
        if let Some(port) = self.remote_port {
            if port == 0 {
                return Err(ClientError::InvalidPort("remote_port cannot be 0".into()));
            }
        }

        // HTTP/HTTPS/TCPMux needs custom_domains or subdomain
        if matches!(
            self.proxy_type,
            ProxyType::Http | ProxyType::Https | ProxyType::Tcpmux
        ) && self.custom_domains.is_none()
            && self.subdomain.is_none()
        {
            return Err(ClientError::ConfigValidation(
                "HTTP/HTTPS/TCPMux proxy requires custom_domains or subdomain".into(),
            ));
        }

        // proxy_protocol_version must be "v1" or "v2" when set
        if let Some(ref ver) = self.proxy_protocol_version {
            if !["v1", "v2"].contains(&ver.as_str()) {
                return Err(ClientError::ConfigValidation(format!(
                    "proxy_protocol_version must be 'v1' or 'v2', got: {ver}"
                )));
            }
        }

        // STCP/XTCP/SUDP must have secret_key
        if matches!(
            self.proxy_type,
            ProxyType::Stcp | ProxyType::Xtcp | ProxyType::Sudp
        ) && self
            .secret_key
            .as_ref()
            .is_none_or(|k| k.trim().is_empty())
        {
            return Err(ClientError::MissingRequiredField(
                "secret_key is required for stcp/xtcp/sudp proxy types".into(),
            ));
        }

        // Domain 格式校验
        if let Some(ref domains) = self.custom_domains {
            for domain in domains {
                if !is_valid_domain(domain) {
                    return Err(ClientError::ConfigValidation(format!(
                        "Invalid domain: {domain}"
                    )));
                }
            }
        }

        // 健康检查参数
        if self.health_check_timeout_s < 1 {
            return Err(ClientError::ConfigValidation(
                "health_check_timeout_s must be >= 1".into(),
            ));
        }

        // 若声明了 plugin，必须包含 type 字段
        if let Some(ref plugin) = self.plugin_config {
            if plugin
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .is_empty()
            {
                return Err(ClientError::ConfigValidation(
                    "plugin_config must contain 'type' field".into(),
                ));
            }
        }

        Ok(())
    }
}

impl LocalVisitor {
    /// Validate visitor configuration
    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() {
            return Err(ClientError::MissingRequiredField("visitor name".into()));
        }
        if self.server_name.trim().is_empty() {
            return Err(ClientError::MissingRequiredField("server_name".into()));
        }
        // STCP/XTCP/SUDP must have secret_key
        if self
            .secret_key
            .as_ref()
            .is_none_or(|k| k.trim().is_empty())
        {
            return Err(ClientError::MissingRequiredField(
                "secret_key is required for visitor".into(),
            ));
        }
        // XTCP-specific field validations
        if matches!(self.visitor_type, VisitorType::Xtcp) {
            if let Some(ref proto) = self.xtcp_protocol {
                if !["quic", "kcp"].contains(&proto.as_str()) {
                    return Err(ClientError::ConfigValidation(
                        "xtcp_protocol must be 'quic' or 'kcp'".into(),
                    ));
                }
            }
        }
        // Profile ID must be valid
        if self.profile_id <= 0 {
            return Err(ClientError::ConfigValidation(
                "profile_id must be > 0".into(),
            ));
        }
        Ok(())
    }
}

impl BindingRule {
    /// 校验绑定规则
    pub fn validate(&self) -> Result<()> {
        if self.profile_id <= 0 {
            return Err(ClientError::ConfigValidation(
                "profile_id must be > 0".into(),
            ));
        }
        if self.proxy_id <= 0 {
            return Err(ClientError::ConfigValidation("proxy_id must be > 0".into()));
        }
        Ok(())
    }
}

/// 检查主机地址是否合法（IP 或域名）
///
/// 接受：
/// - IPv4 地址 (192.168.1.1)
/// - IPv6 地址 (::1, 2001:db8::1)
/// - 域名 (frp.example.com)
fn is_valid_host(host: &str) -> bool {
    if host.is_empty() {
        return false;
    }

    // IP 地址
    if host.parse::<IpAddr>().is_ok() {
        return true;
    }

    // 简单域名校验：不含非法字符，格式大致正确
    is_valid_domain(host)
}

/// 检查域名格式
///
/// 支持普通域名和通配符域名（*.example.com）。
fn is_valid_domain(domain: &str) -> bool {
    if domain.is_empty() || domain.len() > 253 {
        return false;
    }

    // 通配符域名
    if let Some(stripped) = domain.strip_prefix("*.") {
        return is_valid_domain(stripped);
    }

    // Validate each label
    for label in domain.split('.') {
        if label.is_empty() || label.len() > 63 || label.starts_with('-') || label.ends_with('-') {
            return false;
        }

        for ch in label.chars() {
            if !ch.is_ascii_alphanumeric() && ch != '-' {
                return false;
            }
        }
    }
    // Single-label domains are allowed (localhost, hostnames, short names)
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_valid_host_ipv4() {
        assert!(is_valid_host("192.168.1.1"));
        assert!(is_valid_host("127.0.0.1"));
        assert!(is_valid_host("0.0.0.0"));
    }

    #[test]
    fn test_is_valid_host_ipv6() {
        assert!(is_valid_host("::1"));
        assert!(is_valid_host("2001:db8::1"));
    }

    #[test]
    fn test_is_valid_host_domain() {
        assert!(is_valid_host("frp.example.com"));
        assert!(is_valid_host("my-frp.server.co.uk"));
        assert!(is_valid_host("localhost"));
        assert!(is_valid_host("my-server"));
        assert!(!is_valid_host(""));
        assert!(!is_valid_host("invalid..domain"));
    }

    #[test]
    fn test_validate_profile_empty_name() {
        let p = FrpsProfile {
            server_addr: "1.2.3.4".into(),
            server_port: 7000,
            token: "test".into(),
            ..Default::default()
        };
        assert!(p.validate().is_err()); // name 为空
    }

    #[test]
    fn test_validate_profile_invalid_port() {
        let p = FrpsProfile {
            name: "test".into(),
            server_addr: "1.2.3.4".into(),
            server_port: 0, // 非法端口
            token: "test".into(),
            ..Default::default()
        };
        assert!(p.validate().is_err());
    }

    #[test]
    fn test_validate_profile_valid() {
        let p = FrpsProfile {
            name: "My Server".into(),
            server_addr: "frp.example.com".into(),
            server_port: 7000,
            token: "secure_token".into(),
            ..Default::default()
        };
        assert!(p.validate().is_ok());
    }

    #[test]
    fn test_validate_proxy_empty_name() {
        let p = LocalProxy {
            local_ip: "127.0.0.1".into(),
            local_port: 3389,
            ..Default::default()
        };
        assert!(p.validate().is_err()); // name 为空
    }

    #[test]
    fn test_validate_proxy_http_missing_domain() {
        let p = LocalProxy {
            name: "web".into(),
            proxy_type: ProxyType::Http,
            local_ip: "127.0.0.1".into(),
            local_port: 80,
            remote_port: Some(8080),
            ..Default::default()
        };
        assert!(p.validate().is_err()); // HTTP 必须设置 domain
    }

    #[test]
    fn test_validate_proxy_valid_tcp() {
        let p = LocalProxy {
            name: "RDP".into(),
            local_ip: "127.0.0.1".into(),
            local_port: 3389,
            ..Default::default()
        };
        assert!(p.validate().is_ok());
    }
}
