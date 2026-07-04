//! BindingRule CRUD
//!
//! 管理 Profile 与 Proxy 的多对多绑定关系。

use crate::config::model::{BindingRule, ProxyType};
use crate::db::Database;
use crate::error::{ClientError, Result};
use rusqlite::params;

impl Database {
    /// 插入新的绑定规则
    ///
    /// Checks STCP/XTCP conflict: these proxy types cannot be bound to multiple profiles
    /// (port conflict on local machine).
    pub async fn insert_binding(&self, binding: &BindingRule) -> Result<i64> {
        // Check STCP/XTCP conflict
        let proxy = self.get_proxy(binding.proxy_id).await?;
        if matches!(proxy.proxy_type, ProxyType::Stcp | ProxyType::Xtcp) {
            let existing = self.list_bindings_for_proxy(binding.proxy_id).await?;
            if !existing.is_empty() {
                return Err(ClientError::ConfigValidation(
                    "STCP/XTCP proxy cannot be bound to multiple profiles (port conflict)".into(),
                ));
            }
        }

        let conn = self.lock().await;
        conn.execute(
            "INSERT INTO binding_rule
                (profile_id, proxy_id, enabled, running, priority, group_name, group_key)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                binding.profile_id,
                binding.proxy_id,
                binding.enabled as i32,
                binding.running as i32,
                binding.priority,
                binding.group_name,
                binding.group_key,
            ],
        )
        .map_err(ClientError::DatabaseQuery)?;

        Ok(conn.last_insert_rowid())
    }

    /// 获取所有绑定规则
    pub async fn list_bindings(&self) -> Result<Vec<BindingRule>> {
        let conn = self.lock().await;
        let mut stmt = conn
            .prepare(
                "SELECT id, profile_id, proxy_id, enabled, running, priority,
                        group_name, group_key, created_at, updated_at
                 FROM binding_rule
                 ORDER BY priority",
            )
            .map_err(ClientError::DatabaseQuery)?;

        let bindings: Vec<BindingRule> = stmt
            .query_map([], row_to_binding)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(ClientError::DatabaseQuery)?;

        Ok(bindings)
    }

    /// 按 ID 获取单个绑定
    pub async fn get_binding(&self, id: i64) -> Result<BindingRule> {
        let conn = self.lock().await;
        conn.query_row(
            "SELECT id, profile_id, proxy_id, enabled, running, priority,
                    group_name, group_key, created_at, updated_at
             FROM binding_rule WHERE id = ?1",
            params![id],
            row_to_binding,
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => ClientError::RecordNotFound {
                table: "binding_rule".into(),
                id,
            },
            other => ClientError::DatabaseQuery(other),
        })
    }

    /// 获取某个 Profile 下的所有绑定（含启用的和禁用的）
    pub async fn list_bindings_for_profile(&self, profile_id: i64) -> Result<Vec<BindingRule>> {
        let conn = self.lock().await;
        let mut stmt = conn
            .prepare(
                "SELECT id, profile_id, proxy_id, enabled, running, priority,
                        group_name, group_key, created_at, updated_at
                 FROM binding_rule WHERE profile_id = ?1
                 ORDER BY priority",
            )
            .map_err(ClientError::DatabaseQuery)?;

        let bindings: Vec<BindingRule> = stmt
            .query_map(params![profile_id], row_to_binding)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(ClientError::DatabaseQuery)?;

        Ok(bindings)
    }

    /// 获取某个 Proxy 下的所有绑定
    pub async fn list_bindings_for_proxy(&self, proxy_id: i64) -> Result<Vec<BindingRule>> {
        let conn = self.lock().await;
        let mut stmt = conn
            .prepare(
                "SELECT id, profile_id, proxy_id, enabled, running, priority,
                        group_name, group_key, created_at, updated_at
                 FROM binding_rule WHERE proxy_id = ?1
                 ORDER BY priority",
            )
            .map_err(ClientError::DatabaseQuery)?;

        let bindings: Vec<BindingRule> = stmt
            .query_map(params![proxy_id], row_to_binding)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(ClientError::DatabaseQuery)?;

        Ok(bindings)
    }

    /// 获取所有活跃的绑定（enabled=1 AND running=1，用于 TOML 生成）
    pub async fn list_active_bindings(&self) -> Result<Vec<BindingRule>> {
        let conn = self.lock().await;
        let mut stmt = conn
            .prepare(
                "SELECT id, profile_id, proxy_id, enabled, running, priority,
                        group_name, group_key, created_at, updated_at
                 FROM binding_rule WHERE enabled = 1 AND running = 1
                 ORDER BY priority",
            )
            .map_err(ClientError::DatabaseQuery)?;

        let bindings: Vec<BindingRule> = stmt
            .query_map([], row_to_binding)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(ClientError::DatabaseQuery)?;

        Ok(bindings)
    }

    /// 获取所有标记为 running 的绑定（用于 daemon 重启恢复）
    pub async fn list_running_bindings(&self) -> Result<Vec<BindingRule>> {
        let conn = self.lock().await;
        let mut stmt = conn
            .prepare(
                "SELECT id, profile_id, proxy_id, enabled, running, priority,
                        group_name, group_key, created_at, updated_at
                 FROM binding_rule WHERE running = 1
                 ORDER BY priority",
            )
            .map_err(ClientError::DatabaseQuery)?;

        let bindings: Vec<BindingRule> = stmt
            .query_map([], row_to_binding)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(ClientError::DatabaseQuery)?;

        Ok(bindings)
    }

    /// 获取某个 Profile 下所有 running 的绑定
    pub async fn list_running_bindings_for_profile(&self, profile_id: i64) -> Result<Vec<BindingRule>> {
        let conn = self.lock().await;
        let mut stmt = conn
            .prepare(
                "SELECT id, profile_id, proxy_id, enabled, running, priority,
                        group_name, group_key, created_at, updated_at
                 FROM binding_rule WHERE profile_id = ?1 AND running = 1
                 ORDER BY priority",
            )
            .map_err(ClientError::DatabaseQuery)?;

        let bindings: Vec<BindingRule> = stmt
            .query_map(params![profile_id], row_to_binding)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(ClientError::DatabaseQuery)?;

        Ok(bindings)
    }

    /// 设置绑定的 running 状态
    pub async fn set_running(&self, id: i64, running: bool) -> Result<()> {
        let conn = self.lock().await;
        let affected = conn
            .execute(
                "UPDATE binding_rule SET running = ?1, updated_at = datetime('now') WHERE id = ?2",
                params![running as i32, id],
            )
            .map_err(ClientError::DatabaseQuery)?;

        if affected == 0 {
            return Err(ClientError::RecordNotFound {
                table: "binding_rule".into(),
                id,
            });
        }
        Ok(())
    }

    /// 更新绑定规则
    pub async fn update_binding(&self, binding: &BindingRule) -> Result<()> {
        let id = binding.id.ok_or_else(|| {
            ClientError::ConfigValidation("Binding id is required for update".into())
        })?;

        let conn = self.lock().await;
        let affected = conn
            .execute(
                "UPDATE binding_rule SET
                    profile_id = ?1, proxy_id = ?2, enabled = ?3, running = ?4, priority = ?5,
                    group_name = ?6, group_key = ?7, updated_at = datetime('now')
                 WHERE id = ?8",
                params![
                    binding.profile_id,
                    binding.proxy_id,
                    binding.enabled as i32,
                    binding.running as i32,
                    binding.priority,
                    binding.group_name,
                    binding.group_key,
                    id,
                ],
            )
            .map_err(ClientError::DatabaseQuery)?;

        if affected == 0 {
            return Err(ClientError::RecordNotFound {
                table: "binding_rule".into(),
                id,
            });
        }
        Ok(())
    }

    /// 启用 / 禁用绑定
    pub async fn toggle_binding(&self, id: i64, enabled: bool) -> Result<()> {
        let conn = self.lock().await;
        let affected = conn
            .execute(
                "UPDATE binding_rule SET enabled = ?1, updated_at = datetime('now') WHERE id = ?2",
                params![enabled as i32, id],
            )
            .map_err(ClientError::DatabaseQuery)?;

        if affected == 0 {
            return Err(ClientError::RecordNotFound {
                table: "binding_rule".into(),
                id,
            });
        }
        Ok(())
    }

    /// 删除绑定规则
    pub async fn delete_binding(&self, id: i64) -> Result<()> {
        let conn = self.lock().await;
        let affected = conn
            .execute("DELETE FROM binding_rule WHERE id = ?1", params![id])
            .map_err(ClientError::DatabaseQuery)?;

        if affected == 0 {
            return Err(ClientError::RecordNotFound {
                table: "binding_rule".into(),
                id,
            });
        }
        tracing::info!(binding_id = id, "Binding deleted");
        Ok(())
    }
}

/// 将数据库行映射为 BindingRule
fn row_to_binding(row: &rusqlite::Row<'_>) -> rusqlite::Result<BindingRule> {
    Ok(BindingRule {
        id: Some(row.get(0)?),
        profile_id: row.get(1)?,
        proxy_id: row.get(2)?,
        enabled: row.get::<_, i32>(3)? != 0,
        running: row.get::<_, i32>(4)? != 0,
        priority: row.get(5)?,
        group_name: row.get(6)?,
        group_key: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::model::ProxyType;
    use tempfile::NamedTempFile;

    async fn setup_db() -> Database {
        let tmp = NamedTempFile::new().unwrap();
        let db = Database::open(tmp.path().to_str().unwrap()).await.unwrap();
        crate::db::migrate::run(&*db.lock().await).unwrap();
        db
    }

    fn sample_binding(profile_id: i64, proxy_id: i64) -> BindingRule {
        BindingRule {
            id: None,
            profile_id,
            proxy_id,
            enabled: true,
            running: false,
            priority: 0,
            group_name: None,
            group_key: None,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    #[tokio::test]
    async fn test_insert_and_get_binding() {
        let db = setup_db().await;

        // 先创建 Profile 和 Proxy
        db.insert_profile(&crate::config::model::FrpsProfile::default())
            .await
            .unwrap();
        let proxy = crate::config::model::LocalProxy {
            id: None,
            name: "test".into(),
            proxy_type: ProxyType::Tcp,
            local_ip: "127.0.0.1".into(),
            local_port: 80,
            remote_port: Some(80),
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
        };
        let proxy_id = db.insert_proxy(&proxy).await.unwrap();

        let id = db
            .insert_binding(&sample_binding(1, proxy_id))
            .await
            .unwrap();
        assert!(id > 0);

        let binding = db.get_binding(id).await.unwrap();
        assert_eq!(binding.profile_id, 1);
        assert!(binding.enabled);
    }

    #[tokio::test]
    async fn test_toggle_binding() {
        let db = setup_db().await;

        db.insert_profile(&crate::config::model::FrpsProfile::default())
            .await
            .unwrap();
        let proxy = crate::config::model::LocalProxy::default();
        let proxy_id = db.insert_proxy(&proxy).await.unwrap();

        let id = db
            .insert_binding(&sample_binding(1, proxy_id))
            .await
            .unwrap();

        // 禁用
        db.toggle_binding(id, false).await.unwrap();
        let binding = db.get_binding(id).await.unwrap();
        assert!(!binding.enabled);

        // 重新启用
        db.toggle_binding(id, true).await.unwrap();
        let binding = db.get_binding(id).await.unwrap();
        assert!(binding.enabled);
    }
}
