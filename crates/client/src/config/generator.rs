//! SQLite → TOML 生成器
//!
//! 从 SQLite 读取配置，按 Profile 分组生成独立的 frpc TOML 文件。
//! 原子写入：tmp → rename（PERF-003）。

use crate::config::model::{
    AuthConfig, FrpcConfig, HealthCheckConfig, LoadBalancerConfig, LogConfig, NatTraversalConfig,
    OidcClientConfig, ProxyEntry, ProxyTransportConfig, QuicConfig, TlsConfig, TransportConfig,
    VirtualNetConfig, VisitorEntry, VisitorTransportConfig, WebServerConfig,
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

                let output_path = profile_toml_path(output_dir, *profile_id, &profile.name);

                let toml_str = toml::to_string_pretty(&config)
                    .map_err(|e| ClientError::TomlSerialization(e.to_string()))?;

                atomic_write(&output_path, &toml_str)?;
                remove_stale_profile_tomls(output_dir, *profile_id, &output_path);

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

/// 为单个 Profile 重新生成 frpc TOML 文件
///
/// 用于 Start/Stop 操作后的按需重生成，避免全量重建所有 profile 的 TOML。
///
/// # Returns
///
/// 生成的 TOML 文件路径。若该 profile 下无 running 的 binding，则删除已有的 TOML 文件并返回 None。
pub async fn generate_frpc_toml_for_profile(
    db: &Database,
    profile_id: i64,
    output_dir: &Path,
) -> Result<Option<PathBuf>> {
    let profile = db.get_profile(profile_id).await?;
    let output_path = profile_toml_path(output_dir, profile_id, &profile.name);

    let bindings = db.list_enabled_bindings_for_profile(profile_id).await?;

    if bindings.is_empty() {
        // No enabled membership means there is no runnable Profile config.
        if output_path.exists() {
            std::fs::remove_file(&output_path)
                .map_err(|e| ClientError::TomlWrite(format!("Failed to remove TOML: {e}")))?;
            tracing::info!(
                path = %output_path.display(),
                profile = %profile.name,
                "TOML removed (no enabled bindings)"
            );
        }
        return Ok(None);
    }

    let binding_refs: Vec<&crate::config::model::BindingRule> = bindings.iter().collect();
    let config = build_frpc_config(&profile, &binding_refs, db).await?;

    let toml_str = toml::to_string_pretty(&config)
        .map_err(|e| ClientError::TomlSerialization(e.to_string()))?;

    atomic_write(&output_path, &toml_str)?;

    // Renaming a profile changes its artifact id, so the previous file would
    // linger and look like another runnable profile.
    remove_stale_profile_tomls(output_dir, profile_id, &output_path);

    tracing::info!(
        path = %output_path.display(),
        profile = %profile.name,
        proxies = config.proxies.len(),
        "frpc TOML regenerated for profile"
    );

    Ok(Some(output_path))
}

/// Delete earlier TOML files belonging to `profile_id` but carrying another name.
fn remove_stale_profile_tomls(output_dir: &Path, profile_id: i64, keep: &Path) {
    let prefix = format!("{profile_id}-");
    let Ok(entries) = std::fs::read_dir(output_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let belongs_to_other_id = name
            .split_once('-')
            .and_then(|(head, _)| head.parse::<i64>().ok())
            .is_some_and(|other| other != profile_id);
        if name.ends_with(".toml")
            && !belongs_to_other_id
            && name.starts_with(&prefix)
            && path != keep
        {
            match std::fs::remove_file(&path) {
                Ok(()) => {
                    tracing::info!(path = %path.display(), "Removed stale profile TOML")
                }
                Err(error) => {
                    tracing::warn!(path = %path.display(), %error, "Failed to remove stale TOML")
                }
            }
        }
    }
}

/// 为单个 Profile 构建 FrpcConfig
async fn build_frpc_config(
    profile: &crate::config::model::FrpsProfile,
    bindings: &[&crate::config::model::BindingRule],
    db: &Database,
) -> Result<FrpcConfig> {
    let tls = if profile.tls_enable
        || profile.tls_server_name.is_some()
        || profile.tls_disable_custom_first_byte.is_some()
    {
        Some(TlsConfig {
            enable: profile.tls_enable,
            cert_file: profile.tls_cert_file.clone(),
            key_file: profile.tls_key_file.clone(),
            trusted_ca_file: profile.tls_trusted_ca_file.clone(),
            server_name: profile.tls_server_name.clone(),
            disable_custom_first_byte: profile.tls_disable_custom_first_byte,
        })
    } else {
        None
    };

    let transport = Some(TransportConfig {
        protocol: profile.transport_protocol.clone(),
        wire_protocol: profile.wire_protocol.clone(),
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

    // Every persisted profile needs a loopback admin endpoint: `frpc reload`
    // (the only documented hot-reload path) and per-proxy status both require it.
    // Unsaved profiles (unit tests, previews) have no row to attach it to.
    let web_server = match profile.id {
        Some(id) => {
            let admin = db.admin_endpoint(id).await?;
            Some(WebServerConfig {
                addr: "127.0.0.1".into(),
                port: admin.port,
                user: "rustfrp".into(),
                password: admin.password,
            })
        }
        None => None,
    };

    let mut proxies: Vec<ProxyEntry> = Vec::new();
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
                    // frpc rejects the whole file when two proxies share a name
                    // (`proxy name [x] is duplicated`), which would take down an
                    // otherwise healthy profile.
                    if proxies.iter().any(|seen| seen.name == entry.name) {
                        return Err(ClientError::ConfigValidation(format!(
                            "duplicate proxy name '{}' in profile '{}': frpc rejects duplicated names",
                            entry.name, profile.name
                        )));
                    }
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
    let auth = if profile.auth_method.as_deref() == Some("none") {
        None
    } else if profile.auth_method.as_deref() == Some("oidc") {
        if let (Some(ref client_id), Some(ref client_secret), Some(ref token_url)) = (
            &profile.oidc_client_id,
            &profile.oidc_client_secret,
            &profile.oidc_token_endpoint_url,
        ) {
            Some(AuthConfig {
                method: Some("oidc".into()),
                token: None,
                additional_scopes: profile.auth_additional_scopes.clone(),
                oidc: Some(OidcClientConfig {
                    client_id: client_id.clone(),
                    client_secret: client_secret.clone(),
                    audience: profile.oidc_audience.clone(),
                    scope: profile.oidc_scope.clone(),
                    token_endpoint_url: token_url.clone(),
                    additional_endpoint_params: parse_json_field(
                        &profile.name,
                        "oidc_additional_params",
                        &profile.oidc_additional_params,
                    ),
                    trusted_ca_file: profile.oidc_trusted_ca_file.clone(),
                    insecure_skip_verify: profile.oidc_insecure_skip_verify,
                    proxy_url: profile.oidc_proxy_url.clone(),
                }),
            })
        } else {
            return Err(ClientError::ConfigValidation(
                "OIDC client ID, client secret, and token endpoint URL are required".into(),
            ));
        }
    } else if profile.auth_method.as_deref() == Some("token") || profile.auth_method.is_none() {
        // An empty token means frps has no `auth.token`; omit the block and let
        // frpc use its default (token auth with an empty secret).
        token.clone().map(|token| AuthConfig {
            method: Some("token".into()),
            token: Some(token),
            additional_scopes: profile.auth_additional_scopes.clone(),
            oidc: None,
        })
    } else {
        return Err(ClientError::ConfigValidation(format!(
            "Unsupported authentication method: {}",
            profile.auth_method.as_deref().unwrap_or_default()
        )));
    };

    // When using auth block, token goes inside auth, not at top level
    let top_level_token = if auth.is_some() || profile.auth_method.as_deref() == Some("none") {
        None
    } else {
        token
    };

    Ok(FrpcConfig {
        server_addr: profile.server_addr.clone(),
        server_port: profile.server_port,
        user: profile.user.clone(),
        token: top_level_token,
        transport,
        auth,
        web_server,
        login_fail_exit: profile.login_fail_exit,
        metadatas: parse_json_field(&profile.name, "metadatas", &profile.metadatas),
        dns_server: profile.dns_server.clone(),
        nat_hole_stun_server: profile.nat_hole_stun_server.clone(),
        udp_packet_size: profile.udp_packet_size,
        includes: profile.includes.clone(),
        feature_gates: parse_json_field(&profile.name, "feature_gates", &profile.feature_gates),
        client_id: profile.client_id.clone(),
        start: profile.start.clone().filter(|names| !names.is_empty()),
        log: build_log_config(profile),
        virtual_net: profile
            .virtual_net_address
            .as_ref()
            .filter(|address| !address.trim().is_empty())
            .map(|address| VirtualNetConfig {
                address: address.clone(),
            }),
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

    // Build transport sub-config (FRP TOML v0.52+ uses `transport.*` dotted keys)
    let transport = if proxy.use_encryption
        || proxy.use_compression
        || proxy.bandwidth_limit.is_some()
        || proxy.bandwidth_limit_mode.is_some()
        || proxy.proxy_protocol_version.is_some()
    {
        Some(ProxyTransportConfig {
            use_encryption: proxy.use_encryption,
            use_compression: proxy.use_compression,
            bandwidth_limit: proxy.bandwidth_limit.clone(),
            bandwidth_limit_mode: proxy.bandwidth_limit_mode.clone(),
            proxy_protocol_version: proxy.proxy_protocol_version.clone(),
        })
    } else {
        None
    };

    let entry = ProxyEntry {
        name: proxy.name.clone(),
        proxy_type: proxy.proxy_type.to_string(),
        local_ip: proxy.local_ip.clone(),
        local_port: proxy.local_port,
        remote_port: proxy.remote_port,
        custom_domains: proxy.custom_domains.clone(),
        subdomain: proxy.subdomain.clone(),
        multiplexer: proxy.multiplexer.clone(),
        transport,
        secret_key: proxy.secret_key.clone(),
        locations: proxy.locations.clone(),
        http_user: proxy.http_user.clone(),
        http_password: proxy.http_password.clone(),
        host_header_rewrite: proxy.host_header_rewrite.clone(),
        request_headers: parse_json_field(&proxy.name, "request_headers", &proxy.request_headers),
        response_headers: parse_json_field(
            &proxy.name,
            "response_headers",
            &proxy.response_headers,
        ),
        route_by_http_user: proxy.route_by_http_user.clone(),
        annotations: parse_json_field(&proxy.name, "annotations", &proxy.annotations),
        metadatas: parse_json_field(&proxy.name, "metadatas", &proxy.metadatas),
        allow_users: proxy.allow_users.clone(),
        nat_traversal: proxy.nat_traversal_disable_assisted_addrs.map(|disable| {
            NatTraversalConfig {
                disable_assisted_addrs: disable,
            }
        }),
        load_balancer: None, // populated by caller from binding
        plugin: proxy.plugin_config.clone(),
        health_check,
    };

    vec![entry]
}

/// Build the `[log]` section only when the user configured log options.
///
/// RustFRP always redirects frpc stdout/stderr into its own log files, so
/// emitting the section is optional and left to explicit user settings.
fn build_log_config(profile: &crate::config::model::FrpsProfile) -> Option<LogConfig> {
    if profile.log_to.is_none() && profile.log_level.is_none() && profile.log_max_days.is_none() {
        return None;
    }
    Some(LogConfig {
        to: profile.log_to.clone(),
        level: profile.log_level.clone(),
        max_days: profile.log_max_days,
    })
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

    if ![
        "tcp", "udp", "http", "https", "stcp", "xtcp", "tcpmux", "sudp",
    ]
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

/// Length cap for the human-readable suffix in generated artifact names.
const ARTIFACT_NAME_MAX: usize = 40;

/// Identifier shared by a profile's TOML file and its frpc logs.
///
/// The profile id is the anchor: `sanitize_filename` maps distinct names onto the
/// same string (`"Test Server"` and `"Test_Server"` both become `Test_Server`),
/// so a name-only scheme lets two profiles share one TOML and one log file. The
/// readable suffix stays for operators, truncated so a long name cannot exceed
/// filesystem limits.
pub fn profile_artifact_id(profile_id: i64, profile_name: &str) -> String {
    let sanitized = sanitize_filename(profile_name);
    let trimmed: String = sanitized.chars().take(ARTIFACT_NAME_MAX).collect();
    if trimmed.is_empty() {
        return profile_id.to_string();
    }
    format!("{profile_id}-{trimmed}")
}

/// Path of the generated TOML for one profile.
pub fn profile_toml_path(output_dir: &Path, profile_id: i64, name: &str) -> PathBuf {
    output_dir.join(format!("{}.toml", profile_artifact_id(profile_id, name)))
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
            user: Some("tenant-user".into()),
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
            running: true,
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
        assert!(content.contains("user = \"tenant-user\""));
        // 文件名以 profile id 为锚，附带 sanitized name
        assert!(paths[0]
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains(&format!("{profile_id}-Test_Server")));
    }

    /// Fields that only matter for advanced deployments must still round-trip
    /// from SQLite into the generated TOML.
    #[tokio::test]
    async fn advanced_common_config_round_trips_into_toml() {
        let db = setup_db().await;
        let profile_id = db
            .insert_profile(&FrpsProfile {
                name: "advanced".into(),
                server_addr: "frp.example.com".into(),
                token: "t".into(),
                client_id: Some("edge-1".into()),
                wire_protocol: Some("v2".into()),
                auth_additional_scopes: Some(vec!["HeartBeats".into()]),
                start: Some(vec!["svc".into()]),
                log_level: Some("warn".into()),
                tls_server_name: Some("frps.example.com".into()),
                ..Default::default()
            })
            .await
            .unwrap();
        let proxy_id = db
            .insert_proxy(&LocalProxy {
                name: "svc".into(),
                local_port: 8080,
                ..Default::default()
            })
            .await
            .unwrap();
        db.insert_binding(&BindingRule {
            profile_id,
            proxy_id,
            enabled: true,
            running: true,
            ..Default::default()
        })
        .await
        .unwrap();

        let output_dir = TempDir::new().unwrap();
        let paths = generate_all_frpc_tomls(&db, output_dir.path())
            .await
            .unwrap();
        let content = std::fs::read_to_string(&paths[0]).unwrap();

        for expected in [
            "clientID = \"edge-1\"",
            "wireProtocol = \"v2\"",
            "additionalScopes = [\"HeartBeats\"]",
            "start = [\"svc\"]",
            "level = \"warn\"",
            "serverName = \"frps.example.com\"",
        ] {
            assert!(
                content.contains(expected),
                "missing {expected} in:\n{content}"
            );
        }
    }

    #[tokio::test]
    async fn test_no_auth_omits_stored_token() {
        let db = setup_db().await;
        let profile = FrpsProfile {
            name: "No Auth".into(),
            server_addr: "frp.example.com".into(),
            token: "dormant-token".into(),
            auth_method: Some("none".into()),
            ..Default::default()
        };

        let config = build_frpc_config(&profile, &[], &db).await.unwrap();
        assert!(config.auth.is_none());
        assert!(config.token.is_none());
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
                running: true,
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
        // Artifact names are profile-id anchored so they cannot collide.
        assert!(names.contains(&format!("{p1_id}-Server_Alpha")));
        assert!(names.contains(&format!("{p2_id}-Server_Beta")));
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

    /// frpc refuses a file with duplicated proxy names, so generation must fail
    /// early instead of producing a profile that cannot start.
    #[tokio::test]
    async fn duplicate_proxy_names_in_one_profile_are_rejected() {
        let db = setup_db().await;
        let profile_id = db
            .insert_profile(&FrpsProfile {
                name: "dup".into(),
                server_addr: "frp.example.com".into(),
                token: "t".into(),
                ..Default::default()
            })
            .await
            .unwrap();
        for local_port in [8080u16, 8081] {
            let proxy_id = db
                .insert_proxy(&LocalProxy {
                    name: "same".into(),
                    local_port,
                    remote_port: Some(local_port + 1000),
                    ..Default::default()
                })
                .await
                .unwrap();
            db.insert_binding(&BindingRule {
                profile_id,
                proxy_id,
                enabled: true,
                running: true,
                ..Default::default()
            })
            .await
            .unwrap();
        }

        let output_dir = TempDir::new().unwrap();
        let error = generate_all_frpc_tomls(&db, output_dir.path())
            .await
            .expect_err("duplicate proxy names must not generate a TOML");
        assert!(
            error.to_string().contains("duplicate proxy name 'same'"),
            "unexpected error: {error}"
        );
    }

    /// Two profiles whose names sanitize to the same string must not share a TOML
    /// file or a log name; sharing let one load the other's configuration after a
    /// global reload.
    #[tokio::test]
    async fn colliding_profile_names_get_distinct_artifacts() {
        let db = setup_db().await;
        let mut ids = Vec::new();
        for name in ["Test Server", "Test_Server"] {
            let profile_id = db
                .insert_profile(&FrpsProfile {
                    name: name.into(),
                    server_addr: "frp.example.com".into(),
                    token: "t".into(),
                    ..Default::default()
                })
                .await
                .unwrap();
            let proxy_id = db
                .insert_proxy(&LocalProxy {
                    name: format!("p{profile_id}"),
                    local_port: 8080,
                    remote_port: Some(18000 + profile_id as u16),
                    ..Default::default()
                })
                .await
                .unwrap();
            db.insert_binding(&BindingRule {
                profile_id,
                proxy_id,
                enabled: true,
                running: true,
                ..Default::default()
            })
            .await
            .unwrap();
            ids.push(profile_id);
        }

        let output_dir = TempDir::new().unwrap();
        let paths = generate_all_frpc_tomls(&db, output_dir.path())
            .await
            .unwrap();
        assert_eq!(paths.len(), 2, "each profile needs its own TOML");

        let mut names: Vec<String> = paths
            .iter()
            .map(|path| path.file_stem().unwrap().to_string_lossy().to_string())
            .collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), 2, "artifact names must be unique: {names:?}");

        for path in &paths {
            let stem = path.file_stem().unwrap().to_string_lossy().to_string();
            let profile_id = ids
                .iter()
                .find(|id| stem.starts_with(&format!("{id}-")))
                .unwrap_or_else(|| panic!("unexpected artifact name {stem}"));
            let content = std::fs::read_to_string(path).unwrap();
            assert!(
                content.contains(&format!("name = \"p{profile_id}\"")),
                "{} must contain its own proxy",
                path.display()
            );
        }
    }

    /// Renaming a profile must not leave its previous TOML behind: a stale file
    /// looks like another runnable profile and is picked up on the next boot.
    #[tokio::test]
    async fn renaming_a_profile_removes_its_previous_toml() {
        let db = setup_db().await;
        let profile_id = db
            .insert_profile(&FrpsProfile {
                name: "before".into(),
                server_addr: "frp.example.com".into(),
                token: "t".into(),
                ..Default::default()
            })
            .await
            .unwrap();
        let proxy_id = db
            .insert_proxy(&LocalProxy {
                name: "svc".into(),
                local_port: 8080,
                remote_port: Some(18080),
                ..Default::default()
            })
            .await
            .unwrap();
        db.insert_binding(&BindingRule {
            profile_id,
            proxy_id,
            enabled: true,
            running: true,
            ..Default::default()
        })
        .await
        .unwrap();

        let output_dir = TempDir::new().unwrap();
        generate_all_frpc_tomls(&db, output_dir.path())
            .await
            .unwrap();
        let stale = output_dir.path().join(format!("{profile_id}-before.toml"));
        assert!(stale.exists());
        let unrelated = output_dir.path().join("999-other.toml");
        std::fs::write(&unrelated, "serverAddr = \"x\"\n").unwrap();

        let mut profile = db.get_profile(profile_id).await.unwrap();
        profile.name = "after".into();
        db.update_profile(&profile).await.unwrap();
        generate_all_frpc_tomls(&db, output_dir.path())
            .await
            .unwrap();

        assert!(!stale.exists(), "renamed profile left a stale TOML");
        assert!(output_dir
            .path()
            .join(format!("{profile_id}-after.toml"))
            .exists());
        assert!(unrelated.exists(), "cleanup must not touch other profiles");
    }

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("My Server"), "My_Server");
        assert_eq!(sanitize_filename("home-nas"), "home-nas");
        assert_eq!(sanitize_filename("a/b:c"), "a_b_c");
        assert_eq!(sanitize_filename("公司服务器"), "_____");
    }

    /// frpc refuses `type = "tcpmux"` without `multiplexer`; the generated TOML
    /// must carry it or the whole profile fails to start.
    #[tokio::test]
    async fn tcpmux_proxy_emits_multiplexer() {
        let db = setup_db().await;
        let profile_id = db
            .insert_profile(&FrpsProfile {
                name: "mux".into(),
                server_addr: "frp.example.com".into(),
                token: "t".into(),
                ..Default::default()
            })
            .await
            .unwrap();
        let proxy_id = db
            .insert_proxy(&LocalProxy {
                name: "mux".into(),
                proxy_type: crate::config::model::ProxyType::Tcpmux,
                local_port: 22,
                custom_domains: Some(vec!["mux.example.com".into()]),
                multiplexer: Some("httpconnect".into()),
                ..Default::default()
            })
            .await
            .unwrap();
        db.insert_binding(&BindingRule {
            profile_id,
            proxy_id,
            enabled: true,
            running: true,
            ..Default::default()
        })
        .await
        .unwrap();

        let output_dir = TempDir::new().unwrap();
        let paths = generate_all_frpc_tomls(&db, output_dir.path())
            .await
            .unwrap();
        let content = std::fs::read_to_string(&paths[0]).unwrap();
        assert!(
            content.contains("multiplexer = \"httpconnect\""),
            "generated TOML must carry the tcpmux multiplexer:\n{content}"
        );
    }

    /// Reload and per-proxy status run through `[webServer]`, so every persisted
    /// profile must get a loopback listener with credentials.
    #[tokio::test]
    async fn persisted_profile_emits_authenticated_loopback_admin_endpoint() {
        let db = setup_db().await;
        let profile_id = db
            .insert_profile(&FrpsProfile {
                name: "admin".into(),
                server_addr: "frp.example.com".into(),
                token: "t".into(),
                ..Default::default()
            })
            .await
            .unwrap();
        let proxy_id = db
            .insert_proxy(&LocalProxy {
                name: "svc".into(),
                local_port: 8080,
                ..Default::default()
            })
            .await
            .unwrap();
        db.insert_binding(&BindingRule {
            profile_id,
            proxy_id,
            enabled: true,
            running: true,
            ..Default::default()
        })
        .await
        .unwrap();

        let output_dir = TempDir::new().unwrap();
        let paths = generate_all_frpc_tomls(&db, output_dir.path())
            .await
            .unwrap();
        let content = std::fs::read_to_string(&paths[0]).unwrap();
        let endpoint = db.admin_endpoint(profile_id).await.unwrap();

        assert!(content.contains("[webServer]"), "{content}");
        assert!(content.contains("addr = \"127.0.0.1\""), "{content}");
        assert!(
            content.contains(&format!("port = {}", endpoint.port)),
            "{content}"
        );
        assert!(content.contains("user = \"rustfrp\""), "{content}");
        assert!(
            content.contains(&format!("password = \"{}\"", endpoint.password)),
            "{content}"
        );
    }
}
