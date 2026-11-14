//! LocalProxy CRUD
//!
//! 管理本地代理配置。字段 1:1 对应 FRP 官方 TOML 规范。

use crate::config::model::{LocalProxy, ProxyType};
use crate::db::Database;
use crate::error::{ClientError, Result};
use rusqlite::params;

/// Columns selected by every `local_proxy` read, in `row_to_proxy` order.
///
/// The projection, the row mapper and the insert column list must stay aligned;
/// keeping the read projection here means adding a field touches one place
/// instead of three.
const PROXY_COLUMNS: &str = "id, name, proxy_type, local_ip, local_port, remote_port, custom_domains, subdomain, use_encryption, use_compression, bandwidth_limit, bandwidth_limit_mode, health_check_type, health_check_timeout_s, health_check_max_failed, health_check_interval_s, health_check_path, health_check_http_headers, plugin_config, secret_key, locations, http_user, http_password, host_header_rewrite, request_headers, response_headers, route_by_http_user, annotations, metadatas, allow_users, nat_traversal_disable_assisted_addrs, proxy_protocol_version, multiplexer, created_at, updated_at";

impl Database {
    /// 插入新的 Proxy
    pub async fn insert_proxy(&self, proxy: &LocalProxy) -> Result<i64> {
        self.insert_proxy_for_tenant(proxy, "default").await
    }

    /// 插入新的 Proxy，并记录其租户归属
    pub async fn insert_proxy_for_tenant(&self, proxy: &LocalProxy, tenant: &str) -> Result<i64> {
        let conn = self.lock().await;
        conn.execute(
            "INSERT INTO local_proxy
                (name, proxy_type, local_ip, local_port, remote_port,
                 custom_domains, subdomain, use_encryption, use_compression,
                 bandwidth_limit, bandwidth_limit_mode,
                 health_check_type, health_check_timeout_s,
                 health_check_max_failed, health_check_interval_s,
                 health_check_path, health_check_http_headers, plugin_config,
                 secret_key,
                 locations, http_user, http_password, host_header_rewrite,
                 request_headers, response_headers, route_by_http_user,
                 annotations, metadatas,
                 allow_users, nat_traversal_disable_assisted_addrs,
                 proxy_protocol_version, multiplexer, tenant_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31, ?32, ?33)",
            params![
                proxy.name,
                proxy.proxy_type.to_string(),
                proxy.local_ip,
                proxy.local_port,
                proxy.remote_port,
                proxy.custom_domains.as_ref().map(|v| v.join(",")),
                proxy.subdomain,
                proxy.use_encryption as i32,
                proxy.use_compression as i32,
                proxy.bandwidth_limit,
                proxy.bandwidth_limit_mode,
                proxy.health_check_type,
                proxy.health_check_timeout_s,
                proxy.health_check_max_failed,
                proxy.health_check_interval_s,
                proxy.health_check_path,
                proxy.health_check_http_headers.as_ref().map(|v| serialize_json_or_warn(&proxy.name, "health_check_http_headers", v)),
                proxy.plugin_config.as_ref().map(|v| v.to_string()),
                proxy.secret_key,
                proxy.locations.as_ref().map(|v| v.join(",")),
                proxy.http_user,
                proxy.http_password,
                proxy.host_header_rewrite,
                proxy.request_headers,
                proxy.response_headers,
                proxy.route_by_http_user,
                proxy.annotations,
                proxy.metadatas,
                proxy.allow_users.as_ref().map(|v| v.join(",")),
                proxy.nat_traversal_disable_assisted_addrs.map(|v| v as i32),
                proxy.proxy_protocol_version,
                proxy.multiplexer,
                tenant,
            ],
        )
        .map_err(ClientError::DatabaseQuery)?;

        Ok(conn.last_insert_rowid())
    }

    /// 获取所有 Proxy
    pub async fn list_proxies(&self) -> Result<Vec<LocalProxy>> {
        self.list_proxies_for_tenant("default").await
    }

    /// 获取指定租户的 Proxy
    pub async fn list_proxies_for_tenant(&self, tenant: &str) -> Result<Vec<LocalProxy>> {
        let conn = self.lock().await;
        let mut stmt = conn
            .prepare(&format!(
                "SELECT {PROXY_COLUMNS} FROM local_proxy WHERE tenant_id = ?1 ORDER BY name"
            ))
            .map_err(ClientError::DatabaseQuery)?;

        let proxies: Vec<LocalProxy> = stmt
            .query_map(params![tenant], row_to_proxy)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(ClientError::DatabaseQuery)?;

        Ok(proxies)
    }

    /// 按 ID 获取单个 Proxy
    pub async fn get_proxy(&self, id: i64) -> Result<LocalProxy> {
        self.get_proxy_in_tenant(id, "default").await
    }

    /// 按 ID 获取单个 Proxy，并校验租户归属
    pub async fn get_proxy_in_tenant(&self, id: i64, tenant: &str) -> Result<LocalProxy> {
        let conn = self.lock().await;
        conn.query_row(
            &format!("SELECT {PROXY_COLUMNS} FROM local_proxy WHERE id = ?1 AND tenant_id = ?2"),
            params![id, tenant],
            row_to_proxy,
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => ClientError::RecordNotFound {
                table: "local_proxy".into(),
                id,
            },
            other => ClientError::DatabaseQuery(other),
        })
    }

    /// 更新 Proxy
    pub async fn update_proxy(&self, proxy: &LocalProxy) -> Result<()> {
        let id = proxy.id.ok_or_else(|| {
            ClientError::ConfigValidation("Proxy id is required for update".into())
        })?;

        let conn = self.lock().await;
        let affected = conn
            .execute(
                "UPDATE local_proxy SET
                    name = ?1, proxy_type = ?2, local_ip = ?3,
                    local_port = ?4, remote_port = ?5, custom_domains = ?6,
                    subdomain = ?7, use_encryption = ?8, use_compression = ?9,
                    bandwidth_limit = ?10, bandwidth_limit_mode = ?11,
                    health_check_type = ?12,
                    health_check_timeout_s = ?13, health_check_max_failed = ?14,
                    health_check_interval_s = ?15,
                    health_check_path = ?16, health_check_http_headers = ?17,
                    plugin_config = ?18, secret_key = ?19,
                    locations = ?20, http_user = ?21, http_password = ?22,
                    host_header_rewrite = ?23, request_headers = ?24,
                    response_headers = ?25, route_by_http_user = ?26,
                    annotations = ?27, metadatas = ?28,
                    allow_users = ?29,
                    nat_traversal_disable_assisted_addrs = ?30,
                    proxy_protocol_version = ?31, multiplexer = ?32,
                    updated_at = datetime('now')
                 WHERE id = ?33",
                params![
                    proxy.name,
                    proxy.proxy_type.to_string(),
                    proxy.local_ip,
                    proxy.local_port,
                    proxy.remote_port,
                    proxy.custom_domains.as_ref().map(|v| v.join(",")),
                    proxy.subdomain,
                    proxy.use_encryption as i32,
                    proxy.use_compression as i32,
                    proxy.bandwidth_limit,
                    proxy.bandwidth_limit_mode,
                    proxy.health_check_type,
                    proxy.health_check_timeout_s,
                    proxy.health_check_max_failed,
                    proxy.health_check_interval_s,
                    proxy.health_check_path,
                    proxy
                        .health_check_http_headers
                        .as_ref()
                        .map(|v| serialize_json_or_warn(
                            &proxy.name,
                            "health_check_http_headers",
                            v
                        )),
                    proxy.plugin_config.as_ref().map(|v| v.to_string()),
                    proxy.secret_key,
                    proxy.locations.as_ref().map(|v| v.join(",")),
                    proxy.http_user,
                    proxy.http_password,
                    proxy.host_header_rewrite,
                    proxy.request_headers,
                    proxy.response_headers,
                    proxy.route_by_http_user,
                    proxy.annotations,
                    proxy.metadatas,
                    proxy.allow_users.as_ref().map(|v| v.join(",")),
                    proxy.nat_traversal_disable_assisted_addrs.map(|v| v as i32),
                    proxy.proxy_protocol_version,
                    proxy.multiplexer,
                    id,
                ],
            )
            .map_err(ClientError::DatabaseQuery)?;

        if affected == 0 {
            return Err(ClientError::RecordNotFound {
                table: "local_proxy".into(),
                id,
            });
        }
        Ok(())
    }

    /// 删除 Proxy（级联删除关联的 BindingRule）
    pub async fn delete_proxy(&self, id: i64) -> Result<()> {
        let conn = self.lock().await;
        let affected = conn
            .execute("DELETE FROM local_proxy WHERE id = ?1", params![id])
            .map_err(ClientError::DatabaseQuery)?;

        if affected == 0 {
            return Err(ClientError::RecordNotFound {
                table: "local_proxy".into(),
                id,
            });
        }
        tracing::info!(proxy_id = id, "Proxy deleted");
        Ok(())
    }
}

/// Serialize a value to JSON string for DB storage, logging warning on failure.
fn serialize_json_or_warn<T: serde::Serialize>(
    proxy_name: &str,
    field_name: &str,
    value: &T,
) -> String {
    serde_json::to_string(value).unwrap_or_else(|e| {
        tracing::warn!(
            proxy = %proxy_name,
            field = %field_name,
            error = %e,
            "Failed to serialize field to JSON, storing empty"
        );
        String::new()
    })
}

/// 将数据库行映射为 LocalProxy
fn row_to_proxy(row: &rusqlite::Row<'_>) -> rusqlite::Result<LocalProxy> {
    let domains: Option<String> = row.get(6)?;
    let custom_domains = domains
        .filter(|s| !s.is_empty())
        .map(|s| s.split(',').map(|d| d.to_string()).collect());

    let health_check_http_headers: Option<String> = row.get(17)?;
    let health_check_http_headers = health_check_http_headers
        .filter(|s| !s.is_empty())
        .and_then(|s| serde_json::from_str(&s).ok());

    let plugin_config: Option<String> = row.get(18)?;
    let plugin_config = plugin_config
        .filter(|s| !s.is_empty())
        .and_then(|s| serde_json::from_str(&s).ok());

    let locations: Option<String> = row.get(20)?;
    let locations = locations
        .filter(|s| !s.is_empty())
        .map(|s| s.split(',').map(|d| d.to_string()).collect());

    Ok(LocalProxy {
        id: Some(row.get(0)?),
        name: row.get(1)?,
        proxy_type: row.get::<_, ProxyType>(2)?,
        local_ip: row.get(3)?,
        local_port: row.get(4)?,
        remote_port: row.get(5)?,
        custom_domains,
        subdomain: row.get(7)?,
        use_encryption: row.get::<_, i32>(8)? != 0,
        use_compression: row.get::<_, i32>(9)? != 0,
        bandwidth_limit: row.get(10)?,
        bandwidth_limit_mode: row.get(11)?,
        secret_key: row.get(19)?,
        locations,
        http_user: row.get(21)?,
        http_password: row.get(22)?,
        host_header_rewrite: row.get(23)?,
        request_headers: row.get(24)?,
        response_headers: row.get(25)?,
        route_by_http_user: row.get(26)?,
        annotations: row.get(27)?,
        metadatas: row.get(28)?,
        allow_users: {
            let raw: Option<String> = row.get(29)?;
            raw.filter(|s| !s.is_empty())
                .map(|s| s.split(',').map(|d| d.to_string()).collect())
        },
        nat_traversal_disable_assisted_addrs: {
            let raw: Option<i32> = row.get(30)?;
            raw.map(|v| v != 0)
        },
        proxy_protocol_version: row.get(31)?,
        multiplexer: row.get(32)?,
        health_check_type: row.get(12)?,
        health_check_timeout_s: row.get(13)?,
        health_check_max_failed: row.get(14)?,
        health_check_interval_s: row.get(15)?,
        health_check_path: row.get(16)?,
        health_check_http_headers,
        plugin_config,
        created_at: row.get(33)?,
        updated_at: row.get(34)?,
    })
}

#[cfg(test)]
mod tenant_tests {
    use super::*;
    use crate::config::model::FrpsProfile;

    async fn setup() -> Database {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_string_lossy().to_string();
        let db = Database::open(&path).await.unwrap();
        crate::db::migrate::run(&*db.lock().await).unwrap();
        std::mem::forget(tmp);
        db
    }

    fn proxy(name: &str) -> LocalProxy {
        LocalProxy {
            name: name.into(),
            local_port: 8080,
            remote_port: Some(18080),
            ..Default::default()
        }
    }

    /// A token issued for one tenant must not see another tenant's proxies, and
    /// must not be able to read them by id: foreign ids report "not found" so
    /// they cannot be probed.
    #[tokio::test]
    async fn proxies_are_invisible_across_tenants() {
        let db = setup().await;
        let mine = db
            .insert_proxy_for_tenant(&proxy("mine"), "alpha")
            .await
            .unwrap();
        let theirs = db
            .insert_proxy_for_tenant(&proxy("theirs"), "beta")
            .await
            .unwrap();

        let alpha_view = db.list_proxies_for_tenant("alpha").await.unwrap();
        assert_eq!(alpha_view.len(), 1);
        assert_eq!(alpha_view[0].id, Some(mine));

        assert!(matches!(
            db.get_proxy_in_tenant(theirs, "alpha").await,
            Err(ClientError::RecordNotFound { .. })
        ));
        assert!(db.get_proxy_in_tenant(theirs, "beta").await.is_ok());
    }

    /// Visitors, bindings and profiles inherit their tenant from the profile row.
    #[tokio::test]
    async fn related_rows_follow_their_profile_tenant() {
        let db = setup().await;
        let profile = db
            .insert_profile_for_tenant(
                &FrpsProfile {
                    name: "beta-server".into(),
                    server_addr: "frp.example.com".into(),
                    ..Default::default()
                },
                "beta",
            )
            .await
            .unwrap();
        let proxy_id = db
            .insert_proxy_for_tenant(&proxy("svc"), "beta")
            .await
            .unwrap();
        db.insert_binding(&crate::config::model::BindingRule {
            profile_id: profile,
            proxy_id,
            enabled: true,
            ..Default::default()
        })
        .await
        .unwrap();
        db.insert_visitor(&crate::config::model::LocalVisitor {
            name: "visitor".into(),
            server_name: "svc".into(),
            bind_port: 9000,
            secret_key: Some("k".into()),
            profile_id: profile,
            ..Default::default()
        })
        .await
        .unwrap();

        assert_eq!(db.list_bindings_for_tenant("beta").await.unwrap().len(), 1);
        assert!(db
            .list_bindings_for_tenant("alpha")
            .await
            .unwrap()
            .is_empty());
        assert_eq!(db.list_visitors_for_tenant("beta").await.unwrap().len(), 1);
        assert!(db
            .list_visitors_for_tenant("alpha")
            .await
            .unwrap()
            .is_empty());
        assert_eq!(db.list_profiles_for_tenant("beta").await.unwrap().len(), 1);
        assert!(db
            .list_profiles_for_tenant("alpha")
            .await
            .unwrap()
            .is_empty());
    }
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

    fn sample_proxy(name: &str) -> LocalProxy {
        LocalProxy {
            id: None,
            name: name.into(),
            proxy_type: ProxyType::Tcp,
            local_ip: "127.0.0.1".into(),
            local_port: 3389,
            remote_port: Some(3389),
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
            multiplexer: None,
            plugin_config: None,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    #[tokio::test]
    async fn test_insert_and_get_proxy() {
        let db = setup_db().await;
        let id = db.insert_proxy(&sample_proxy("RDP")).await.unwrap();
        assert!(id > 0);

        let proxy = db.get_proxy(id).await.unwrap();
        assert_eq!(proxy.name, "RDP");
        assert_eq!(proxy.local_port, 3389);
    }

    #[tokio::test]
    async fn test_update_proxy() {
        let db = setup_db().await;
        let id = db.insert_proxy(&sample_proxy("原始")).await.unwrap();

        let mut proxy = db.get_proxy(id).await.unwrap();
        proxy.name = "更新后".into();
        proxy.local_port = 8080;
        db.update_proxy(&proxy).await.unwrap();

        let updated = db.get_proxy(id).await.unwrap();
        assert_eq!(updated.name, "更新后");
        assert_eq!(updated.local_port, 8080);
    }

    #[tokio::test]
    async fn test_delete_proxy() {
        let db = setup_db().await;
        let id = db.insert_proxy(&sample_proxy("待删除")).await.unwrap();
        db.delete_proxy(id).await.unwrap();

        let result = db.get_proxy(id).await;
        assert!(result.is_err());
    }
}
