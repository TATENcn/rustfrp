//! LocalProxy CRUD
//!
//! 管理本地代理配置。字段 1:1 对应 FRP 官方 TOML 规范。

use crate::config::model::{LocalProxy, ProxyType};
use crate::db::Database;
use crate::error::{ClientError, Result};
use rusqlite::params;

impl Database {
    /// 插入新的 Proxy
    pub async fn insert_proxy(&self, proxy: &LocalProxy) -> Result<i64> {
        let conn = self.lock().await;
        conn.execute(
            "INSERT INTO local_proxy
                (name, proxy_type, local_ip, local_port, remote_port,
                 custom_domains, subdomain, use_encryption, use_compression,
                 bandwidth_limit, health_check_type, health_check_timeout_s,
                 health_check_max_failed, health_check_interval_s, plugin_config)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
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
                proxy.health_check_type,
                proxy.health_check_timeout_s,
                proxy.health_check_max_failed,
                proxy.health_check_interval_s,
                proxy.plugin_config.as_ref().map(|v| v.to_string()),
            ],
        )
        .map_err(ClientError::DatabaseQuery)?;

        Ok(conn.last_insert_rowid())
    }

    /// 获取所有 Proxy
    pub async fn list_proxies(&self) -> Result<Vec<LocalProxy>> {
        let conn = self.lock().await;
        let mut stmt = conn
            .prepare(
                "SELECT id, name, proxy_type, local_ip, local_port, remote_port,
                        custom_domains, subdomain, use_encryption, use_compression,
                        bandwidth_limit, health_check_type, health_check_timeout_s,
                        health_check_max_failed, health_check_interval_s,
                        plugin_config, created_at, updated_at
                 FROM local_proxy
                 ORDER BY name",
            )
            .map_err(ClientError::DatabaseQuery)?;

        let proxies: Vec<LocalProxy> = stmt
            .query_map([], row_to_proxy)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(ClientError::DatabaseQuery)?;

        Ok(proxies)
    }

    /// 按 ID 获取单个 Proxy
    pub async fn get_proxy(&self, id: i64) -> Result<LocalProxy> {
        let conn = self.lock().await;
        conn.query_row(
            "SELECT id, name, proxy_type, local_ip, local_port, remote_port,
                    custom_domains, subdomain, use_encryption, use_compression,
                    bandwidth_limit, health_check_type, health_check_timeout_s,
                    health_check_max_failed, health_check_interval_s,
                    plugin_config, created_at, updated_at
             FROM local_proxy WHERE id = ?1",
            params![id],
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
                    bandwidth_limit = ?10, health_check_type = ?11,
                    health_check_timeout_s = ?12, health_check_max_failed = ?13,
                    health_check_interval_s = ?14, plugin_config = ?15,
                    updated_at = datetime('now')
                 WHERE id = ?16",
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
                    proxy.health_check_type,
                    proxy.health_check_timeout_s,
                    proxy.health_check_max_failed,
                    proxy.health_check_interval_s,
                    proxy.plugin_config.as_ref().map(|v| v.to_string()),
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

/// 将数据库行映射为 LocalProxy
fn row_to_proxy(row: &rusqlite::Row<'_>) -> rusqlite::Result<LocalProxy> {
    let domains: Option<String> = row.get(6)?;
    let custom_domains = domains
        .filter(|s| !s.is_empty())
        .map(|s| s.split(',').map(|d| d.to_string()).collect());

    let plugin_config: Option<String> = row.get(15)?;
    let plugin_config = plugin_config
        .filter(|s| !s.is_empty())
        .and_then(|s| serde_json::from_str(&s).ok());

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
        health_check_type: row.get(11)?,
        health_check_timeout_s: row.get(12)?,
        health_check_max_failed: row.get(13)?,
        health_check_interval_s: row.get(14)?,
        plugin_config,
        created_at: row.get(16)?,
        updated_at: row.get(17)?,
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
            health_check_type: None,
            health_check_timeout_s: 3,
            health_check_max_failed: 3,
            health_check_interval_s: 10,
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
