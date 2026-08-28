//! Explicit, one-shot migration from a modern frpc TOML file into SQLite.
//!
//! This is the only allowed TOML -> SQLite path under ARCH-003. The source
//! file is never watched and never becomes a runtime source of truth.

use crate::config::model::{HttpHeader, ProxyType, VisitorType};
use crate::db::Database;
use crate::error::{ClientError, Result};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportConfig {
    server_addr: String,
    #[serde(default = "default_server_port")]
    server_port: u16,
    #[serde(default)]
    user: Option<String>,
    #[serde(default)]
    token: Option<String>,
    #[serde(default)]
    auth: Option<ImportAuth>,
    #[serde(default)]
    transport: ImportTransport,
    #[serde(default)]
    login_fail_exit: Option<bool>,
    #[serde(default)]
    metadatas: Option<toml::Value>,
    #[serde(default)]
    dns_server: Option<String>,
    #[serde(default)]
    nat_hole_stun_server: Option<String>,
    #[serde(default)]
    udp_packet_size: Option<i32>,
    #[serde(default)]
    includes: Option<Vec<String>>,
    #[serde(default)]
    feature_gates: Option<toml::Value>,
    #[serde(default)]
    proxies: Vec<ImportProxy>,
    #[serde(default)]
    visitors: Vec<ImportVisitor>,
}

fn default_server_port() -> u16 {
    7000
}

#[derive(Debug, Clone, Default, Deserialize)]
struct ImportAuth {
    #[serde(default)]
    method: Option<String>,
    #[serde(default)]
    token: Option<String>,
    #[serde(default)]
    oidc: Option<ImportOidc>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportOidc {
    #[serde(default)]
    client_id: Option<String>,
    #[serde(default)]
    client_secret: Option<String>,
    #[serde(default)]
    token_endpoint_url: Option<String>,
    #[serde(default)]
    audience: Option<String>,
    #[serde(default)]
    scope: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportTransport {
    #[serde(default = "default_protocol")]
    protocol: String,
    #[serde(default)]
    tls: ImportTls,
    #[serde(default = "default_heartbeat_interval")]
    heartbeat_interval: i64,
    #[serde(default = "default_heartbeat_timeout")]
    heartbeat_timeout: i64,
    #[serde(default)]
    dial_server_timeout: Option<i64>,
    #[serde(default)]
    dial_server_keepalive: Option<i64>,
    #[serde(default)]
    connect_server_local_ip: Option<String>,
    #[serde(default)]
    proxy_url: Option<String>,
    #[serde(default)]
    pool_count: Option<i32>,
    #[serde(default)]
    tcp_mux: Option<bool>,
    #[serde(default)]
    tcp_mux_keepalive_interval: Option<i64>,
    #[serde(default)]
    quic: ImportQuic,
}

impl Default for ImportTransport {
    fn default() -> Self {
        Self {
            protocol: default_protocol(),
            tls: ImportTls::default(),
            heartbeat_interval: default_heartbeat_interval(),
            heartbeat_timeout: default_heartbeat_timeout(),
            dial_server_timeout: None,
            dial_server_keepalive: None,
            connect_server_local_ip: None,
            proxy_url: None,
            pool_count: None,
            tcp_mux: None,
            tcp_mux_keepalive_interval: None,
            quic: ImportQuic::default(),
        }
    }
}

fn default_protocol() -> String {
    "tcp".into()
}
fn default_heartbeat_interval() -> i64 {
    30
}
fn default_heartbeat_timeout() -> i64 {
    90
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportTls {
    #[serde(default)]
    enable: bool,
    #[serde(default)]
    cert_file: Option<String>,
    #[serde(default)]
    key_file: Option<String>,
    #[serde(default)]
    trusted_ca_file: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportQuic {
    #[serde(default)]
    keepalive_period: Option<i32>,
    #[serde(default)]
    max_idle_timeout: Option<i32>,
    #[serde(default)]
    max_incoming_streams: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportProxy {
    name: String,
    #[serde(rename = "type")]
    proxy_type: String,
    #[serde(default = "default_local_ip")]
    local_ip: String,
    #[serde(default)]
    local_port: u16,
    #[serde(default)]
    remote_port: Option<u16>,
    #[serde(default)]
    custom_domains: Option<Vec<String>>,
    #[serde(default)]
    subdomain: Option<String>,
    #[serde(default)]
    transport: ImportProxyTransport,
    #[serde(default)]
    secret_key: Option<String>,
    #[serde(default)]
    locations: Option<Vec<String>>,
    #[serde(default)]
    http_user: Option<String>,
    #[serde(default)]
    http_password: Option<String>,
    #[serde(default)]
    host_header_rewrite: Option<String>,
    #[serde(default)]
    request_headers: Option<toml::Value>,
    #[serde(default)]
    response_headers: Option<toml::Value>,
    #[serde(default)]
    route_by_http_user: Option<String>,
    #[serde(default)]
    annotations: Option<toml::Value>,
    #[serde(default)]
    metadatas: Option<toml::Value>,
    #[serde(default)]
    allow_users: Option<Vec<String>>,
    #[serde(default)]
    nat_traversal: ImportNatTraversal,
    #[serde(default)]
    load_balancer: ImportLoadBalancer,
    #[serde(default)]
    health_check: Option<ImportHealthCheck>,
    #[serde(default)]
    plugin: Option<toml::Value>,
}

fn default_local_ip() -> String {
    "127.0.0.1".into()
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportProxyTransport {
    #[serde(default)]
    use_encryption: bool,
    #[serde(default)]
    use_compression: bool,
    #[serde(default)]
    bandwidth_limit: Option<String>,
    #[serde(default)]
    bandwidth_limit_mode: Option<String>,
    #[serde(default)]
    proxy_protocol_version: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportNatTraversal {
    #[serde(default)]
    disable_assisted_addrs: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportLoadBalancer {
    #[serde(default)]
    group: Option<String>,
    #[serde(default)]
    group_key: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportHealthCheck {
    #[serde(rename = "type")]
    check_type: String,
    #[serde(default = "default_health_timeout")]
    timeout_seconds: i64,
    #[serde(default = "default_health_failed")]
    max_failed: i64,
    #[serde(default = "default_health_interval")]
    interval_seconds: i64,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    http_headers: Option<Vec<HttpHeader>>,
}

fn default_health_timeout() -> i64 {
    3
}
fn default_health_failed() -> i64 {
    3
}
fn default_health_interval() -> i64 {
    10
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportVisitor {
    name: String,
    #[serde(rename = "type")]
    visitor_type: String,
    server_name: String,
    #[serde(default)]
    server_user: Option<String>,
    #[serde(default)]
    bind_addr: Option<String>,
    #[serde(default = "default_bind_port")]
    bind_port: i32,
    #[serde(default)]
    secret_key: Option<String>,
    #[serde(default)]
    transport: ImportVisitorTransport,
    #[serde(default)]
    protocol: Option<String>,
    #[serde(default)]
    keep_tunnel_open: Option<bool>,
    #[serde(default)]
    max_retries_an_hour: Option<i32>,
    #[serde(default)]
    min_retry_interval: Option<i32>,
    #[serde(default)]
    fallback_to: Option<String>,
    #[serde(default)]
    fallback_timeout_ms: Option<i32>,
    #[serde(default)]
    plugin: Option<toml::Value>,
}

fn default_bind_port() -> i32 {
    -1
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportVisitorTransport {
    #[serde(default)]
    use_encryption: bool,
    #[serde(default)]
    use_compression: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportSummary {
    pub profile_id: i64,
    pub profile_name: String,
    pub proxies_imported: usize,
    pub visitors_imported: usize,
    pub renamed_items: Vec<Rename>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Rename {
    pub kind: &'static str,
    pub from: String,
    pub to: String,
}

fn json(value: &Option<toml::Value>) -> Option<String> {
    value.as_ref().and_then(|v| serde_json::to_string(v).ok())
}

fn unique_name(existing: &mut HashSet<String>, requested: &str) -> String {
    if existing.insert(requested.to_string()) {
        return requested.to_string();
    }
    let mut suffix = 2;
    loop {
        let candidate = format!("{requested}-{suffix}");
        if existing.insert(candidate.clone()) {
            return candidate;
        }
        suffix += 1;
    }
}

impl Database {
    /// Import a complete modern frpc TOML document as one profile transaction.
    pub async fn import_frpc_toml(
        &self,
        requested_profile_name: &str,
        source: &str,
    ) -> Result<ImportSummary> {
        if requested_profile_name.trim().is_empty() {
            return Err(ClientError::ConfigValidation(
                "Import profile name must not be empty".into(),
            ));
        }
        let config: ImportConfig = toml::from_str(source)
            .map_err(|e| ClientError::ConfigValidation(format!("Invalid frpc TOML: {e}")))?;
        if config.server_addr.trim().is_empty() {
            return Err(ClientError::ConfigValidation(
                "serverAddr must not be empty".into(),
            ));
        }

        for proxy in &config.proxies {
            proxy
                .proxy_type
                .parse::<ProxyType>()
                .map_err(ClientError::ConfigValidation)?;
            if proxy.name.trim().is_empty() {
                return Err(ClientError::ConfigValidation(
                    "Proxy name must not be empty".into(),
                ));
            }
            if proxy.local_port == 0 && proxy.plugin.is_none() {
                return Err(ClientError::ConfigValidation(format!(
                    "Proxy '{}' requires localPort",
                    proxy.name
                )));
            }
        }
        for visitor in &config.visitors {
            visitor
                .visitor_type
                .parse::<VisitorType>()
                .map_err(ClientError::ConfigValidation)?;
        }

        let mut conn = self.lock().await;
        let tx = conn.transaction().map_err(ClientError::DatabaseQuery)?;
        let mut profile_names = tx
            .prepare("SELECT name FROM frps_profile")
            .map_err(ClientError::DatabaseQuery)?
            .query_map([], |row| row.get(0))
            .map_err(ClientError::DatabaseQuery)?
            .collect::<std::result::Result<HashSet<String>, _>>()
            .map_err(ClientError::DatabaseQuery)?;
        let mut renamed_items = Vec::new();
        let profile_name = unique_name(&mut profile_names, requested_profile_name.trim());
        if profile_name != requested_profile_name.trim() {
            renamed_items.push(Rename {
                kind: "profile",
                from: requested_profile_name.trim().into(),
                to: profile_name.clone(),
            });
        }

        let auth = config.auth.as_ref();
        let oidc = auth.and_then(|a| a.oidc.as_ref());
        let token = auth
            .and_then(|a| a.token.clone())
            .or(config.token.clone())
            .unwrap_or_default();
        let auth_method = auth
            .and_then(|a| a.method.clone())
            .or_else(|| Some(if token.is_empty() { "none" } else { "token" }.into()));
        tx.execute(
            "INSERT INTO frps_profile (name, server_addr, server_port, token, tls_enable, tls_cert_file, tls_key_file, tls_trusted_ca_file, transport_protocol, heartbeat_interval, heartbeat_timeout, dial_server_timeout, dial_server_keepalive, connect_server_local_ip, proxy_url, pool_count, tcp_mux, tcp_mux_keepalive_interval, quic_keepalive_period, quic_max_idle_timeout, quic_max_incoming_streams, auth_method, oidc_client_id, oidc_client_secret, oidc_token_endpoint_url, oidc_audience, oidc_scope, user, metadatas, login_fail_exit, dns_server, nat_hole_stun_server, udp_packet_size, includes, feature_gates) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25,?26,?27,?28,?29,?30,?31,?32,?33,?34,?35)",
            params![profile_name, config.server_addr, config.server_port, token, config.transport.tls.enable as i32, config.transport.tls.cert_file, config.transport.tls.key_file, config.transport.tls.trusted_ca_file, config.transport.protocol, config.transport.heartbeat_interval, config.transport.heartbeat_timeout, config.transport.dial_server_timeout, config.transport.dial_server_keepalive, config.transport.connect_server_local_ip, config.transport.proxy_url, config.transport.pool_count, config.transport.tcp_mux.map(|v| v as i32), config.transport.tcp_mux_keepalive_interval, config.transport.quic.keepalive_period, config.transport.quic.max_idle_timeout, config.transport.quic.max_incoming_streams, auth_method, oidc.and_then(|o| o.client_id.clone()), oidc.and_then(|o| o.client_secret.clone()), oidc.and_then(|o| o.token_endpoint_url.clone()), oidc.and_then(|o| o.audience.clone()), oidc.and_then(|o| o.scope.clone()), config.user, json(&config.metadatas), config.login_fail_exit.map(|v| v as i32), config.dns_server, config.nat_hole_stun_server, config.udp_packet_size, config.includes.as_ref().map(|v| v.join(",")), json(&config.feature_gates)]
        ).map_err(ClientError::DatabaseQuery)?;
        let profile_id = tx.last_insert_rowid();

        for (priority, proxy) in config.proxies.iter().enumerate() {
            // Proxy names are scoped by an frpc profile at runtime. Preserve them
            // exactly; renaming would silently change public FRP endpoint names.
            let name = proxy.name.clone();
            let health = proxy.health_check.as_ref();
            tx.execute(
                "INSERT INTO local_proxy (name, proxy_type, local_ip, local_port, remote_port, custom_domains, subdomain, use_encryption, use_compression, bandwidth_limit, bandwidth_limit_mode, health_check_type, health_check_timeout_s, health_check_max_failed, health_check_interval_s, health_check_path, health_check_http_headers, plugin_config, secret_key, locations, http_user, http_password, host_header_rewrite, request_headers, response_headers, route_by_http_user, annotations, metadatas, allow_users, nat_traversal_disable_assisted_addrs, proxy_protocol_version) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25,?26,?27,?28,?29,?30,?31)",
                params![name, proxy.proxy_type, proxy.local_ip, proxy.local_port, proxy.remote_port, proxy.custom_domains.as_ref().map(|v| v.join(",")), proxy.subdomain, proxy.transport.use_encryption as i32, proxy.transport.use_compression as i32, proxy.transport.bandwidth_limit, proxy.transport.bandwidth_limit_mode, health.map(|h| h.check_type.clone()), health.map(|h| h.timeout_seconds).unwrap_or(3), health.map(|h| h.max_failed).unwrap_or(3), health.map(|h| h.interval_seconds).unwrap_or(10), health.and_then(|h| h.path.clone()), health.and_then(|h| h.http_headers.as_ref()).and_then(|v| serde_json::to_string(v).ok()), json(&proxy.plugin), proxy.secret_key, proxy.locations.as_ref().map(|v| v.join(",")), proxy.http_user, proxy.http_password, proxy.host_header_rewrite, json(&proxy.request_headers), json(&proxy.response_headers), proxy.route_by_http_user, json(&proxy.annotations), json(&proxy.metadatas), proxy.allow_users.as_ref().map(|v| v.join(",")), proxy.nat_traversal.disable_assisted_addrs.map(|v| v as i32), proxy.transport.proxy_protocol_version]
            ).map_err(ClientError::DatabaseQuery)?;
            let proxy_id = tx.last_insert_rowid();
            tx.execute("INSERT INTO binding_rule (profile_id, proxy_id, enabled, running, priority, group_name, group_key) VALUES (?1,?2,1,0,?3,?4,?5)", params![profile_id, proxy_id, priority as i32, proxy.load_balancer.group, proxy.load_balancer.group_key]).map_err(ClientError::DatabaseQuery)?;
        }

        for visitor in &config.visitors {
            let name = visitor.name.clone();
            tx.execute("INSERT INTO local_visitor (name, visitor_type, server_name, server_user, bind_addr, bind_port, secret_key, enabled, use_encryption, use_compression, xtcp_protocol, keep_tunnel_open, max_retries_an_hour, min_retry_interval, fallback_to, fallback_timeout_ms, plugin_config, profile_id) VALUES (?1,?2,?3,?4,?5,?6,?7,1,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)", params![name, visitor.visitor_type, visitor.server_name, visitor.server_user, visitor.bind_addr, visitor.bind_port, visitor.secret_key, visitor.transport.use_encryption as i32, visitor.transport.use_compression as i32, visitor.protocol, visitor.keep_tunnel_open.map(|v| v as i32), visitor.max_retries_an_hour, visitor.min_retry_interval, visitor.fallback_to, visitor.fallback_timeout_ms, json(&visitor.plugin), profile_id]).map_err(ClientError::DatabaseQuery)?;
        }

        tx.commit().map_err(ClientError::DatabaseQuery)?;
        Ok(ImportSummary {
            profile_id,
            profile_name,
            proxies_imported: config.proxies.len(),
            visitors_imported: config.visitors.len(),
            renamed_items,
        })
    }

    /// Create a transactionally consistent SQLite backup and return its bytes.
    pub async fn export_backup(&self) -> Result<Vec<u8>> {
        let backup_path = self
            .path()
            .with_extension(format!("{}.backup.sqlite", uuid::Uuid::new_v4()));
        {
            let conn = self.lock().await;
            if let Err(error) =
                conn.execute("VACUUM INTO ?1", [backup_path.to_string_lossy().as_ref()])
            {
                let _ = std::fs::remove_file(&backup_path);
                return Err(ClientError::DatabaseQuery(error));
            }
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Err(error) =
                std::fs::set_permissions(&backup_path, std::fs::Permissions::from_mode(0o600))
            {
                let _ = std::fs::remove_file(&backup_path);
                return Err(ClientError::TomlWrite(error.to_string()));
            }
        }
        let result = std::fs::read(&backup_path).map_err(|e| ClientError::TomlWrite(e.to_string()));
        if let Err(error) = std::fs::remove_file(&backup_path) {
            tracing::warn!(%error, path = %backup_path.display(), "Failed to remove temporary backup");
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn db() -> (tempfile::TempDir, Database) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.db");
        let db = Database::open(path.to_str().unwrap()).await.unwrap();
        let conn = db.lock().await;
        crate::db::migrate::run(&conn).unwrap();
        drop(conn);
        (dir, db)
    }

    #[tokio::test]
    async fn imports_profile_proxies_visitors_and_renames_conflicts() {
        let (_dir, db) = db().await;
        let source = r#"
serverAddr = "example.com"
serverPort = 7000
[auth]
method = "token"
token = "secret"
[[proxies]]
name = "ssh"
type = "tcp"
localIP = "127.0.0.1"
localPort = 22
remotePort = 6022
[[visitors]]
name = "private"
type = "stcp"
serverName = "private-server"
bindPort = 9000
"#;
        let first = db.import_frpc_toml("imported", source).await.unwrap();
        assert_eq!(first.proxies_imported, 1);
        assert_eq!(first.visitors_imported, 1);
        let second = db.import_frpc_toml("imported", source).await.unwrap();
        assert_eq!(second.profile_name, "imported-2");
        assert_eq!(db.list_profiles().await.unwrap().len(), 2);
        assert_eq!(db.list_proxies().await.unwrap().len(), 2);
        assert_eq!(db.list_visitors().await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn invalid_input_does_not_write_partial_data() {
        let (_dir, db) = db().await;
        let source = r#"serverAddr = "example.com"
[[proxies]]
name = "broken"
type = "tcp"
"#;
        assert!(db.import_frpc_toml("bad", source).await.is_err());
        assert!(db.list_profiles().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn exports_valid_sqlite_backup() {
        let (_dir, db) = db().await;
        let bytes = db.export_backup().await.unwrap();
        assert!(bytes.starts_with(b"SQLite format 3\0"));
    }
}
