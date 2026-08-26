//! FrpsProfile CRUD
//!
//! 管理 FRP 服务端连接配置。字段 1:1 对应 FRP 官方 TOML 规范。

use crate::config::model::FrpsProfile;
use crate::db::Database;
use crate::error::{ClientError, Result};
use rusqlite::params;

impl Database {
    /// 插入新的 Profile
    pub async fn insert_profile(&self, profile: &FrpsProfile) -> Result<i64> {
        let conn = self.lock().await;
        conn.execute(
            "INSERT INTO frps_profile
                (name, server_addr, server_port, token, tls_enable,
                 tls_cert_file, tls_key_file, tls_trusted_ca_file,
                 transport_protocol, heartbeat_interval, heartbeat_timeout,
                 dial_server_timeout, dial_server_keepalive,
                 connect_server_local_ip, proxy_url,
                 pool_count, tcp_mux, tcp_mux_keepalive_interval,
                 quic_keepalive_period, quic_max_idle_timeout,
                 quic_max_incoming_streams,
                 auth_method, oidc_client_id, oidc_client_secret,
                 oidc_token_endpoint_url, oidc_audience, oidc_scope,
                 oidc_additional_endpoint_params,
                 user, metadatas, login_fail_exit, dns_server,
                 nat_hole_stun_server, udp_packet_size,
                 start, includes, store_path, feature_gates)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31, ?32, ?33, ?34, ?35, ?36, ?37, ?38)",
            params![
                profile.name,
                profile.server_addr,
                profile.server_port,
                profile.token,
                profile.tls_enable as i32,
                profile.tls_cert_file,
                profile.tls_key_file,
                profile.tls_trusted_ca_file,
                profile.transport_protocol,
                profile.heartbeat_interval,
                profile.heartbeat_timeout,
                profile.dial_server_timeout,
                profile.dial_server_keepalive,
                profile.connect_server_local_ip,
                profile.proxy_url,
                profile.pool_count,
                profile.tcp_mux.map(|v| v as i32),
                profile.tcp_mux_keepalive_interval,
                profile.quic_keepalive_period,
                profile.quic_max_idle_timeout,
                profile.quic_max_incoming_streams,
                profile.auth_method,
                profile.oidc_client_id,
                profile.oidc_client_secret,
                profile.oidc_token_endpoint_url,
                profile.oidc_audience,
                profile.oidc_scope,
                profile.oidc_additional_endpoint_params,
                profile.user,
                profile.metadatas,
                profile.login_fail_exit.map(|v| v as i32),
                profile.dns_server,
                profile.nat_hole_stun_server,
                profile.udp_packet_size,
                profile.start.as_ref().map(|v| v.join(",")),
                profile.includes.as_ref().map(|v| v.join(",")),
                profile.store_path,
                profile.feature_gates,
            ],
        )
        .map_err(ClientError::DatabaseQuery)?;

        Ok(conn.last_insert_rowid())
    }

    /// 获取所有 Profile
    pub async fn list_profiles(&self) -> Result<Vec<FrpsProfile>> {
        let conn = self.lock().await;
        let mut stmt = conn
            .prepare(
                "SELECT id, name, server_addr, server_port, token, tls_enable,
                        tls_cert_file, tls_key_file, tls_trusted_ca_file,
                        transport_protocol, heartbeat_interval, heartbeat_timeout,
                        dial_server_timeout, dial_server_keepalive,
                        connect_server_local_ip, proxy_url,
                        pool_count, tcp_mux, tcp_mux_keepalive_interval,
                        quic_keepalive_period, quic_max_idle_timeout,
                        quic_max_incoming_streams,
                        auth_method, oidc_client_id, oidc_client_secret,
                        oidc_token_endpoint_url, oidc_audience, oidc_scope,
                        oidc_additional_endpoint_params,
                        user, metadatas, login_fail_exit, dns_server,
                        nat_hole_stun_server, udp_packet_size,
                        start, includes, store_path, feature_gates,
                        created_at, updated_at
                 FROM frps_profile
                 ORDER BY name",
            )
            .map_err(ClientError::DatabaseQuery)?;

        let profiles: Vec<FrpsProfile> = stmt
            .query_map([], row_to_profile)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(ClientError::DatabaseQuery)?;

        Ok(profiles)
    }

    /// 按 ID 获取单个 Profile
    pub async fn get_profile(&self, id: i64) -> Result<FrpsProfile> {
        let conn = self.lock().await;
        conn.query_row(
            "SELECT id, name, server_addr, server_port, token, tls_enable,
                    tls_cert_file, tls_key_file, tls_trusted_ca_file,
                    transport_protocol, heartbeat_interval, heartbeat_timeout,
                    dial_server_timeout, dial_server_keepalive,
                    connect_server_local_ip, proxy_url,
                    pool_count, tcp_mux, tcp_mux_keepalive_interval,
                    quic_keepalive_period, quic_max_idle_timeout,
                    quic_max_incoming_streams,
                    auth_method, oidc_client_id, oidc_client_secret,
                    oidc_token_endpoint_url, oidc_audience, oidc_scope,
                    oidc_additional_endpoint_params,
                    user, metadatas, login_fail_exit, dns_server,
                    nat_hole_stun_server, udp_packet_size,
                    start, includes, store_path, feature_gates,
                    created_at, updated_at
             FROM frps_profile WHERE id = ?1",
            params![id],
            row_to_profile,
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => ClientError::RecordNotFound {
                table: "frps_profile".into(),
                id,
            },
            other => ClientError::DatabaseQuery(other),
        })
    }

    /// 更新 Profile
    pub async fn update_profile(&self, profile: &FrpsProfile) -> Result<()> {
        let id = profile.id.ok_or_else(|| {
            ClientError::ConfigValidation("Profile id is required for update".into())
        })?;

        let conn = self.lock().await;
        let affected = conn
            .execute(
                "UPDATE frps_profile SET
                    name = ?1, server_addr = ?2, server_port = ?3,
                    token = COALESCE(NULLIF(?4, ''), token),
                    tls_enable = ?5, tls_cert_file = ?6, tls_key_file = ?7,
                    tls_trusted_ca_file = ?8, transport_protocol = ?9,
                    heartbeat_interval = ?10, heartbeat_timeout = ?11,
                    dial_server_timeout = ?12, dial_server_keepalive = ?13,
                    connect_server_local_ip = ?14, proxy_url = ?15,
                    pool_count = ?16, tcp_mux = ?17,
                    tcp_mux_keepalive_interval = ?18,
                    quic_keepalive_period = ?19,
                    quic_max_idle_timeout = ?20,
                    quic_max_incoming_streams = ?21,
                    auth_method = ?22, oidc_client_id = ?23,
                    oidc_client_secret = ?24, oidc_token_endpoint_url = ?25,
                    oidc_audience = ?26, oidc_scope = ?27,
                    oidc_additional_endpoint_params = ?28,
                    user = ?29, metadatas = ?30,
                    login_fail_exit = ?31, dns_server = ?32,
                    nat_hole_stun_server = ?33, udp_packet_size = ?34,
                    start = ?35, includes = ?36, store_path = ?37,
                    feature_gates = ?38,
                    updated_at = datetime('now')
                 WHERE id = ?39",
                params![
                    profile.name,
                    profile.server_addr,
                    profile.server_port,
                    profile.token,
                    profile.tls_enable as i32,
                    profile.tls_cert_file,
                    profile.tls_key_file,
                    profile.tls_trusted_ca_file,
                    profile.transport_protocol,
                    profile.heartbeat_interval,
                    profile.heartbeat_timeout,
                    profile.dial_server_timeout,
                    profile.dial_server_keepalive,
                    profile.connect_server_local_ip,
                    profile.proxy_url,
                    profile.pool_count,
                    profile.tcp_mux.map(|v| v as i32),
                    profile.tcp_mux_keepalive_interval,
                    profile.quic_keepalive_period,
                    profile.quic_max_idle_timeout,
                    profile.quic_max_incoming_streams,
                    profile.auth_method,
                    profile.oidc_client_id,
                    profile.oidc_client_secret,
                    profile.oidc_token_endpoint_url,
                    profile.oidc_audience,
                    profile.oidc_scope,
                    profile.oidc_additional_endpoint_params,
                    profile.user,
                    profile.metadatas,
                    profile.login_fail_exit.map(|v| v as i32),
                    profile.dns_server,
                    profile.nat_hole_stun_server,
                    profile.udp_packet_size,
                    profile.start.as_ref().map(|v| v.join(",")),
                    profile.includes.as_ref().map(|v| v.join(",")),
                    profile.store_path,
                    profile.feature_gates,
                    id,
                ],
            )
            .map_err(ClientError::DatabaseQuery)?;

        if affected == 0 {
            return Err(ClientError::RecordNotFound {
                table: "frps_profile".into(),
                id,
            });
        }
        Ok(())
    }

    /// 删除 Profile（级联删除关联的 BindingRule）
    pub async fn delete_profile(&self, id: i64) -> Result<()> {
        let conn = self.lock().await;
        let affected = conn
            .execute("DELETE FROM frps_profile WHERE id = ?1", params![id])
            .map_err(ClientError::DatabaseQuery)?;

        if affected == 0 {
            return Err(ClientError::RecordNotFound {
                table: "frps_profile".into(),
                id,
            });
        }
        tracing::info!(profile_id = id, "Profile deleted");
        Ok(())
    }
}

/// 将数据库行映射为 FrpsProfile
fn row_to_profile(row: &rusqlite::Row<'_>) -> rusqlite::Result<FrpsProfile> {
    let tcp_mux: Option<i32> = row.get(17)?;
    let login_fail_exit: Option<i32> = row.get(31)?;
    let start: Option<String> = row.get(35)?;
    let includes: Option<String> = row.get(36)?;
    Ok(FrpsProfile {
        id: Some(row.get(0)?),
        name: row.get(1)?,
        server_addr: row.get(2)?,
        server_port: row.get(3)?,
        token: row.get(4)?,
        tls_enable: row.get::<_, i32>(5)? != 0,
        tls_cert_file: row.get(6)?,
        tls_key_file: row.get(7)?,
        tls_trusted_ca_file: row.get(8)?,
        transport_protocol: row.get(9)?,
        heartbeat_interval: row.get(10)?,
        heartbeat_timeout: row.get(11)?,
        dial_server_timeout: row.get(12)?,
        dial_server_keepalive: row.get(13)?,
        connect_server_local_ip: row.get(14)?,
        proxy_url: row.get(15)?,
        pool_count: row.get(16)?,
        tcp_mux: tcp_mux.map(|v| v != 0),
        tcp_mux_keepalive_interval: row.get(18)?,
        quic_keepalive_period: row.get(19)?,
        quic_max_idle_timeout: row.get(20)?,
        quic_max_incoming_streams: row.get(21)?,
        auth_method: row.get(22)?,
        oidc_client_id: row.get(23)?,
        oidc_client_secret: row.get(24)?,
        oidc_token_endpoint_url: row.get(25)?,
        oidc_audience: row.get(26)?,
        oidc_scope: row.get(27)?,
        oidc_additional_endpoint_params: row.get(28)?,
        user: row.get(29)?,
        metadatas: row.get(30)?,
        login_fail_exit: login_fail_exit.map(|v| v != 0),
        dns_server: row.get(32)?,
        nat_hole_stun_server: row.get(33)?,
        udp_packet_size: row.get(34)?,
        start: start
            .filter(|s| !s.is_empty())
            .map(|s| s.split(',').map(|d| d.to_string()).collect()),
        includes: includes
            .filter(|s| !s.is_empty())
            .map(|s| s.split(',').map(|d| d.to_string()).collect()),
        store_path: row.get(37)?,
        feature_gates: row.get(38)?,
        created_at: row.get(39)?,
        updated_at: row.get(40)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    async fn setup_db() -> Database {
        let tmp = NamedTempFile::new().unwrap();
        let db = Database::open(tmp.path().to_str().unwrap()).await.unwrap();
        crate::db::migrate::run(&*db.lock().await).unwrap();
        db
    }

    fn sample_profile(name: &str) -> FrpsProfile {
        FrpsProfile {
            id: None,
            name: name.into(),
            server_addr: "1.2.3.4".into(),
            server_port: 7000,
            token: "test_token".into(),
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

    #[tokio::test]
    async fn test_insert_and_get_profile() {
        let db = setup_db().await;
        let id = db
            .insert_profile(&sample_profile("测试服务器"))
            .await
            .unwrap();
        assert!(id > 0);

        let profile = db.get_profile(id).await.unwrap();
        assert_eq!(profile.name, "测试服务器");
        assert_eq!(profile.server_addr, "1.2.3.4");
    }

    #[tokio::test]
    async fn test_list_profiles() {
        let db = setup_db().await;
        db.insert_profile(&sample_profile("A")).await.unwrap();
        db.insert_profile(&sample_profile("B")).await.unwrap();

        let list = db.list_profiles().await.unwrap();
        assert_eq!(list.len(), 2);
    }

    #[tokio::test]
    async fn test_update_profile() {
        let db = setup_db().await;
        let id = db.insert_profile(&sample_profile("原始")).await.unwrap();

        let mut profile = db.get_profile(id).await.unwrap();
        profile.name = "更新后".into();
        db.update_profile(&profile).await.unwrap();

        let updated = db.get_profile(id).await.unwrap();
        assert_eq!(updated.name, "更新后");
    }

    #[tokio::test]
    async fn test_delete_profile() {
        let db = setup_db().await;
        let id = db.insert_profile(&sample_profile("待删除")).await.unwrap();
        db.delete_profile(id).await.unwrap();

        let result = db.get_profile(id).await;
        assert!(result.is_err());
    }
}
