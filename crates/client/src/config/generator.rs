//! SQLite → TOML 生成器
//!
//! 从 SQLite 读取配置，按 Profile 分组生成独立的 frpc TOML 文件。
//! 原子写入：tmp → rename（PERF-003）。

use crate::config::model::{
    AuthConfig, FrpcConfig, HealthCheckConfig, LoadBalancerConfig, NatTraversalConfig,
    OidcClientConfig, ProxyEntry, QuicConfig, TlsConfig, TransportConfig, VisitorEntry,
    VisitorTransportConfig,
};
use crate::db::Database;
use crate::error::{ClientError, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// 从 SQLite 读取配置，按 Profile 分组生成多个 frpc TOML 文件
///
/// 一个 FrpsProfile → 一个 `{safe_name}.toml` → 一个 frpc 进程实例（ARCH-009, ARCH-010）。
///
/// # Arguments
///
/// * `db` - 数据库实例
/// * `output_dir` - 输出目录（TOML 文件将写入此目录下）
///
/// # Returns
///
/// 成功生成的 TOML 文件路径列表。若无启用的绑定则返回空列表。
pub async fn generate_all_frpc_tomls(db: &Database, output_dir: &Path) -> Result<Vec<PathBuf>> {
    let bindings = db.list_active_bindings().await?;

    if bindings.is_empty() {
        tracing::info!("No active bindings, skipping TOML generation");
        return Ok(Vec::new());
    }

    // 按 profile_id 分组
    let mut groups: HashMap<i64, Vec<_>> = HashMap::new();
    for binding in &bindings {
        groups.entry(binding.profile_id).or_default().push(binding);
    }

    std::fs::create_dir_all(output_dir)
        .map_err(|e| ClientError::TomlWrite(format!("Failed to create output directory: {e}")))?;

    let mut generated = Vec::new();

    for (profile_id, group_bindings) in &groups {
        match db.get_profile(*profile_id).await {
            Ok(profile) => {
                let config = build_frpc_config(&profile, group_bindings, db).await?;

                let safe_name = sanitize_filename(&profile.name);
                let output_path = output_dir.join(format!("{safe_name}.toml"));

                let toml_str = toml::to_string_pretty(&config)
                    .map_err(|e| ClientError::TomlSerialization(e.to_string()))?;

                atomic_write(&output_path, &toml_str)?;

                tracing::info!(
                    path = %output_path.display(),
                    profile = %profile.name,
                    proxies = config.proxies.len(),
                    "frpc TOML generated"
                );

                generated.push(output_path);
            }
            Err(e) => {
                tracing::warn!(
                    profile_id,
                    error = %e,
                    "Skipping profile: not found"
                );
                continue;
            }
        }
    }

    if generated.is_empty() {
        tracing::warn!("No TOML files generated (all profiles missing or no proxies)");
    }

    Ok(generated)
}

/// 为单个 Profile 构建 FrpcConfig
async fn build_frpc_config(
    profile: &crate::config::model::FrpsProfile,
    bindings: &[&crate::config::model::BindingRule],
    db: &Database,
) -> Result<FrpcConfig> {
    let tls = if profile.tls_enable {
        Some(TlsConfig {
            enable: true,
            cert_file: profile.tls_cert_file.clone(),
            key_file: profile.tls_key_file.clone(),
            trusted_ca_file: profile.tls_trusted_ca_file.clone(),
        })
    } else {
        None
    };

    let transport = Some(TransportConfig {
        protocol: profile.transport_protocol.clone(),
        tls,
        heartbeat_interval: Some(profile.heartbeat_interval),
        heartbeat_timeout: Some(profile.heartbeat_timeout),
        dial_server_timeout: profile.dial_server_timeout,
        dial_server_keepalive: profile.dial_server_keepalive,
        connect_server_local_ip: profile.connect_server_local_ip.clone(),
        proxy_url: profile.proxy_url.clone(),
        pool_count: profile.pool_count,
        tcp_mux: profile.tcp_mux,
        tcp_mux_keepalive_interval: profile.tcp_mux_keepalive_interval,
        quic: build_quic_config(profile),
    });

    let token = if profile.token.is_empty() {
        None
    } else {
        Some(profile.token.clone())
    };

    let mut proxies = Vec::new();
    for binding in bindings {
        match db.get_proxy(binding.proxy_id).await {
            Ok(proxy) => {
                let mut entries = build_proxy_entries(&proxy);
                // Inject load balancer group info from binding
                if let Some(ref group_name) = binding.group_name {
                    for entry in &mut entries {
                        entry.load_balancer = Some(LoadBalancerConfig {
                            group: group_name.clone(),
                            group_key: binding.group_key.clone(),
                        });
                    }
                }
                for entry in &entries {
                    validate_proxy_entry(entry)?;
                }
                proxies.extend(entries);
            }
            Err(e) => {
                tracing::warn!(
                    proxy_id = binding.proxy_id,
                    error = %e,
                    "Skipping invalid proxy"
                );
                continue;
            }
        }
    }

    // Load visitors for this profile
    let profile_id = profile.id.unwrap_or(0);
    let visitors = match db.list_visitors_for_profile(profile_id).await {
        Ok(visitors) => visitors
            .iter()
            .filter(|v| v.enabled)
            .map(build_visitor_entry)
            .collect(),
        Err(e) => {
            tracing::warn!(
                profile_id,
                error = %e,
                "Failed to load visitors for profile, using empty list"
            );
            Vec::new()
        }
    };

    // Build auth config (OIDC or token-based).
    //
    // FRP TOML supports two styles:
    //   1. Top-level `token = "..."` (legacy, simple).
    //   2. `[auth]` block with method="token" or method="oidc" (preferred).
    //
    // We emit the [auth] block when OIDC is configured (all fields present),
    // or when token auth is needed. When [auth] is present, we omit the
    // top-level token to avoid duplication (frpc handles both equivalently).
    let auth = if profile.auth_method.as_deref() == Some("oidc") {
        if let (Some(ref client_id), Some(ref client_secret), Some(ref token_url)) = (
            &profile.oidc_client_id,
            &profile.oidc_client_secret,
            &profile.oidc_token_endpoint_url,
        ) {
            Some(AuthConfig {
                method: Some("oidc".into()),
                token: None,
                oidc: Some(OidcClientConfig {
                    client_id: client_id.clone(),
                    client_secret: client_secret.clone(),
                    audience: profile.oidc_audience.clone(),
                    scope: profile.oidc_scope.clone(),
                    token_endpoint_url: token_url.clone(),
                }),
            })
        } else {
            None
        }
    } else if token.is_some() {
        Some(AuthConfig {
            method: Some("token".into()),
            token: token.clone(),
            oidc: None,
        })
    } else {
        None
    };

    // When using auth block, token goes inside auth, not at top level
    let top_level_token = if auth.is_some() { None } else { token };

    Ok(FrpcConfig {
        server_addr: profile.server_addr.clone(),
        server_port: profile.server_port,
        user: profile.user.clone(),
        token: top_level_token,
        transport,
        auth,
        login_fail_exit: profile.login_fail_exit,
        metadatas: parse_json_field(&profile.name, "metadatas", &profile.metadatas),
        dns_server: profile.dns_server.clone(),
        nat_hole_stun_server: profile.nat_hole_stun_server.clone(),
        udp_packet_size: profile.udp_packet_size,
        includes: profile.includes.clone(),
        feature_gates: parse_json_field(&profile.name, "feature_gates", &profile.feature_gates),
        proxies,
        visitors,
    })
}

/// 将 LocalProxy 转换为 ProxyEntry 列表
fn build_proxy_entries(proxy: &crate::config::model::LocalProxy) -> Vec<ProxyEntry> {
    let health_check = proxy
        .health_check_type
        .as_ref()
        .map(|ht| HealthCheckConfig {
            check_type: ht.clone(),
            timeout_s: proxy.health_check_timeout_s,
            max_failed: proxy.health_check_max_failed,
            interval_s: proxy.health_check_interval_s,
            path: proxy.health_check_path.clone(),
            http_headers: proxy.health_check_http_headers.clone(),
        });

    let entry = ProxyEntry {
        name: proxy.name.clone(),
        proxy_type: proxy.proxy_type.to_string(),
        local_ip: proxy.local_ip.clone(),
        local_port: proxy.local_port,
        remote_port: proxy.remote_port,
        custom_domains: proxy.custom_domains.clone(),
        subdomain: proxy.subdomain.clone(),
        use_encryption: proxy.use_encryption,
        use_compression: proxy.use_compression,
        bandwidth_limit: proxy.bandwidth_limit.clone(),
        bandwidth_limit_mode: proxy.bandwidth_limit_mode.clone(),
        secret_key: proxy.secret_key.clone(),
        locations: proxy.locations.clone(),
        http_user: proxy.http_user.clone(),
        http_password: proxy.http_password.clone(),
        host_header_rewrite: proxy.host_header_rewrite.clone(),
        request_headers: parse_json_field(&proxy.name, "request_headers", &proxy.request_headers),
        response_headers: parse_json_field(&proxy.name, "response_headers", &proxy.response_headers),
        route_by_http_user: proxy.route_by_http_user.clone(),
        annotations: parse_json_field(&proxy.name, "annotations", &proxy.annotations),
        metadatas: parse_json_field(&proxy.name, "metadatas", &proxy.metadatas),
        allow_users: proxy.allow_users.clone(),
        nat_traversal: proxy
            .nat_traversal_disable_assisted_addrs
            .map(|disable| NatTraversalConfig {
                disable_assisted_addrs: disable,
            }),
        proxy_protocol_version: proxy.proxy_protocol_version.clone(),
        load_balancer: None, // populated by caller from binding
        plugin: proxy.plugin_config.clone(),
        health_check,
    };

    vec![entry]
}

/// Build QuicConfig from FrpsProfile, returning None if no QUIC fields are set.
fn build_quic_config(profile: &crate::config::model::FrpsProfile) -> Option<QuicConfig> {
    if profile.quic_keepalive_period.is_none()
        && profile.quic_max_idle_timeout.is_none()
        && profile.quic_max_incoming_streams.is_none()
    {
        return None;
    }
    Some(QuicConfig {
        keepalive_period: profile.quic_keepalive_period,
        max_idle_timeout: profile.quic_max_idle_timeout,
        max_incoming_streams: profile.quic_max_incoming_streams,
    })
}

/// Convert LocalVisitor to VisitorEntry for TOML output
fn build_visitor_entry(visitor: &crate::config::model::LocalVisitor) -> VisitorEntry {
    let transport = if visitor.use_encryption || visitor.use_compression {
        Some(VisitorTransportConfig {
            use_encryption: visitor.use_encryption,
            use_compression: visitor.use_compression,
        })
    } else {
        None
    };

    VisitorEntry {
        name: visitor.name.clone(),
        visitor_type: visitor.visitor_type.as_frp_str().to_string(),
        server_name: visitor.server_name.clone(),
        server_user: visitor.server_user.clone(),
        bind_addr: visitor.bind_addr.clone(),
        bind_port: visitor.bind_port,
        secret_key: visitor.secret_key.clone(),
        transport,
        xtcp_protocol: visitor.xtcp_protocol.clone(),
        keep_tunnel_open: visitor.keep_tunnel_open,
        max_retries_an_hour: visitor.max_retries_an_hour,
        min_retry_interval: visitor.min_retry_interval,
        fallback_to: visitor.fallback_to.clone(),
        fallback_timeout_ms: visitor.fallback_timeout_ms,
        plugin: visitor.plugin_config.clone(),
    }
}

/// 校验 ProxyEntry 基本合法性
fn validate_proxy_entry(entry: &ProxyEntry) -> Result<()> {
    if entry.name.trim().is_empty() {
        return Err(ClientError::ConfigValidation(
            "Proxy name cannot be empty".into(),
        ));
    }

    if !["tcp", "udp", "http", "https", "stcp", "xtcp", "tcpmux", "sudp"]
        .contains(&entry.proxy_type.as_str())
    {
        return Err(ClientError::ConfigValidation(format!(
            "Unsupported proxy type: {}",
            entry.proxy_type
        )));
    }

    if entry.local_port == 0 {
        return Err(ClientError::InvalidPort(format!(
            "Proxy '{}' local_port cannot be 0",
            entry.name
        )));
    }

    Ok(())
}

/// 原子写入文件（tmp → rename）
fn atomic_write(path: &Path, content: &str) -> Result<()> {
    let tmp_path = path.with_extension("toml.tmp");

    std::fs::write(&tmp_path, content)
        .map_err(|e| ClientError::TomlWrite(format!("Failed to write temp file: {e}")))?;

    std::fs::rename(&tmp_path, path)
        .map_err(|e| ClientError::TomlWrite(format!("Atomic rename failed: {e}")))?;

    Ok(())
}

/// Parse a JSON field from an Option<String>, logging a warning on failure.
///
/// Many fields are stored as JSON strings in SQLite (annotations, metadatas,
/// request_headers, response_headers, plugin_config). This helper parses them
/// while surfacing malformed data instead of silently dropping it.
fn parse_json_field(
    entity_name: &str,
    field_name: &str,
    raw: &Option<String>,
) -> Option<serde_json::Value> {
    raw.as_ref().and_then(|s| {
        serde_json::from_str(s)
            .map_err(|e| {
                tracing::warn!(
                    entity = %entity_name,
                    field = %field_name,
                    error = %e,
                    "Invalid JSON in field, value will be omitted from TOML"
                );
            })
            .ok()
    })
}

/// 文件名安全处理：替换空格、去除非安全字符
pub(crate) fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => c,
            ' ' => '_',
            _ => '_',
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::model::{BindingRule, FrpsProfile, LocalProxy};
    use tempfile::TempDir;

    async fn setup_db() -> Database {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let db = Database::open(tmp.path().to_str().unwrap()).await.unwrap();
        crate::db::migrate::run(&*db.lock().await).unwrap();
        db
    }

    #[tokio::test]
    async fn test_generate_empty_tomls() {
        let db = setup_db().await;
        let output_dir = TempDir::new().unwrap();

        let paths = generate_all_frpc_tomls(&db, output_dir.path())
            .await
            .unwrap();
        // 无绑定，返回空列表
        assert!(paths.is_empty());
    }

    #[tokio::test]
    async fn test_generate_with_single_profile() {
        let db = setup_db().await;

        let profile = FrpsProfile {
            name: "Test Server".into(),
            server_addr: "frp.example.com".into(),
            server_port: 7000,
            token: "test123".into(),
            ..Default::default()
        };
        let profile_id = db.insert_profile(&profile).await.unwrap();

        let proxy = LocalProxy {
            name: "RDP".into(),
            local_ip: "127.0.0.1".into(),
            local_port: 3389,
            remote_port: Some(13389),
            ..Default::default()
        };
        let proxy_id = db.insert_proxy(&proxy).await.unwrap();

        let binding = BindingRule {
            id: None,
            profile_id,
            proxy_id,
            enabled: true,
            priority: 0,
            group_name: None,
            group_key: None,
            created_at: String::new(),
            updated_at: String::new(),
        };
        db.insert_binding(&binding).await.unwrap();

        let output_dir = TempDir::new().unwrap();
        let paths = generate_all_frpc_tomls(&db, output_dir.path())
            .await
            .unwrap();

        assert_eq!(paths.len(), 1);
        let content = std::fs::read_to_string(&paths[0]).unwrap();
        assert!(content.contains("frp.example.com"));
        assert!(content.contains("RDP"));
        assert!(content.contains("3389"));
        // 文件名应为 sanitized profile name
        assert!(paths[0]
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains("Test_Server"));
    }

    #[tokio::test]
    async fn test_generate_multiple_profiles() {
        let db = setup_db().await;

        // 创建两个 Profile
        let p1 = FrpsProfile {
            name: "Server Alpha".into(),
            server_addr: "alpha.example.com".into(),
            server_port: 7000,
            token: "tok1".into(),
            ..Default::default()
        };
        let p1_id = db.insert_profile(&p1).await.unwrap();

        let p2 = FrpsProfile {
            name: "Server Beta".into(),
            server_addr: "beta.example.com".into(),
            server_port: 7000,
            token: "tok2".into(),
            ..Default::default()
        };
        let p2_id = db.insert_profile(&p2).await.unwrap();

        // 各绑定一个 Proxy
        let proxy = LocalProxy {
            name: "Web".into(),
            local_ip: "127.0.0.1".into(),
            local_port: 80,
            remote_port: Some(8080),
            ..Default::default()
        };
        let proxy_id = db.insert_proxy(&proxy).await.unwrap();

        for &pid in &[p1_id, p2_id] {
            db.insert_binding(&BindingRule {
                id: None,
                profile_id: pid,
                proxy_id,
                enabled: true,
                priority: 0,
                group_name: None,
                group_key: None,
                created_at: String::new(),
                updated_at: String::new(),
            })
            .await
            .unwrap();
        }

        let output_dir = TempDir::new().unwrap();
        let paths = generate_all_frpc_tomls(&db, output_dir.path())
            .await
            .unwrap();

        assert_eq!(paths.len(), 2);
        let names: Vec<String> = paths
            .iter()
            .map(|p| p.file_stem().unwrap().to_string_lossy().to_string())
            .collect();
        assert!(names.contains(&"Server_Alpha".to_string()));
        assert!(names.contains(&"Server_Beta".to_string()));
    }

    #[tokio::test]
    async fn test_atomic_write_no_orphan_temp() {
        let db = setup_db().await;
        let output_dir = TempDir::new().unwrap();

        // 无绑定时不生成文件，tmp 也不应残留
        let paths = generate_all_frpc_tomls(&db, output_dir.path())
            .await
            .unwrap();
        assert!(paths.is_empty());

        // 检查目录中无 tmp 残留
        let tmp_files: Vec<_> = std::fs::read_dir(output_dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".toml.tmp"))
            .collect();
        assert!(tmp_files.is_empty());
    }

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("My Server"), "My_Server");
        assert_eq!(sanitize_filename("home-nas"), "home-nas");
        assert_eq!(sanitize_filename("a/b:c"), "a_b_c");
        assert_eq!(sanitize_filename("公司服务器"), "_____");
    }
}
