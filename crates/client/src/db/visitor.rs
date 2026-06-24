//! LocalVisitor CRUD
//!
//! Manage local visitor configuration for STCP/XTCP/SUDP visitor types.
//! Fields map 1:1 to FRP official TOML spec.

use crate::config::model::{LocalVisitor, VisitorType};
use crate::db::Database;
use crate::error::{ClientError, Result};
use rusqlite::params;

impl Database {
    /// Insert a new Visitor
    pub async fn insert_visitor(&self, visitor: &LocalVisitor) -> Result<i64> {
        let conn = self.lock().await;
        conn.execute(
            "INSERT INTO local_visitor
                (name, visitor_type, server_name, server_user, bind_addr, bind_port,
                 secret_key, enabled, use_encryption, use_compression,
                 xtcp_protocol, keep_tunnel_open, max_retries_an_hour,
                 min_retry_interval, fallback_to, fallback_timeout_ms,
                 plugin_config, profile_id, annotations)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
            params![
                visitor.name,
                visitor.visitor_type.to_string(),
                visitor.server_name,
                visitor.server_user,
                visitor.bind_addr,
                visitor.bind_port,
                visitor.secret_key,
                visitor.enabled as i32,
                visitor.use_encryption as i32,
                visitor.use_compression as i32,
                visitor.xtcp_protocol,
                visitor.keep_tunnel_open.map(|v| v as i32),
                visitor.max_retries_an_hour,
                visitor.min_retry_interval,
                visitor.fallback_to,
                visitor.fallback_timeout_ms,
                visitor.plugin_config.as_ref().map(|v| v.to_string()),
                visitor.profile_id,
                visitor.annotations,
            ],
        )
        .map_err(ClientError::DatabaseQuery)?;

        Ok(conn.last_insert_rowid())
    }

    /// List all Visitors
    pub async fn list_visitors(&self) -> Result<Vec<LocalVisitor>> {
        let conn = self.lock().await;
        let mut stmt = conn
            .prepare(
                "SELECT id, name, visitor_type, server_name, server_user, bind_addr, bind_port,
                        secret_key, enabled, use_encryption, use_compression,
                        xtcp_protocol, keep_tunnel_open, max_retries_an_hour,
                        min_retry_interval, fallback_to, fallback_timeout_ms,
                        plugin_config, profile_id, annotations, created_at, updated_at
                 FROM local_visitor
                 ORDER BY name",
            )
            .map_err(ClientError::DatabaseQuery)?;

        let visitors: Vec<LocalVisitor> = stmt
            .query_map([], row_to_visitor)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(ClientError::DatabaseQuery)?;

        Ok(visitors)
    }

    /// Get a single Visitor by ID
    pub async fn get_visitor(&self, id: i64) -> Result<LocalVisitor> {
        let conn = self.lock().await;
        conn.query_row(
            "SELECT id, name, visitor_type, server_name, server_user, bind_addr, bind_port,
                    secret_key, enabled, use_encryption, use_compression,
                    xtcp_protocol, keep_tunnel_open, max_retries_an_hour,
                    min_retry_interval, fallback_to, fallback_timeout_ms,
                    plugin_config, profile_id, annotations, created_at, updated_at
             FROM local_visitor WHERE id = ?1",
            params![id],
            row_to_visitor,
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => ClientError::RecordNotFound {
                table: "local_visitor".into(),
                id,
            },
            other => ClientError::DatabaseQuery(other),
        })
    }

    /// Update a Visitor
    pub async fn update_visitor(&self, visitor: &LocalVisitor) -> Result<()> {
        let id = visitor.id.ok_or_else(|| {
            ClientError::ConfigValidation("Visitor id is required for update".into())
        })?;

        let conn = self.lock().await;
        let affected = conn
            .execute(
                "UPDATE local_visitor SET
                    name = ?1, visitor_type = ?2, server_name = ?3,
                    server_user = ?4, bind_addr = ?5, bind_port = ?6,
                    secret_key = ?7, enabled = ?8, use_encryption = ?9,
                    use_compression = ?10, xtcp_protocol = ?11,
                    keep_tunnel_open = ?12, max_retries_an_hour = ?13,
                    min_retry_interval = ?14, fallback_to = ?15,
                    fallback_timeout_ms = ?16, plugin_config = ?17,
                    profile_id = ?18, annotations = ?19,
                    updated_at = datetime('now')
                 WHERE id = ?20",
                params![
                    visitor.name,
                    visitor.visitor_type.to_string(),
                    visitor.server_name,
                    visitor.server_user,
                    visitor.bind_addr,
                    visitor.bind_port,
                    visitor.secret_key,
                    visitor.enabled as i32,
                    visitor.use_encryption as i32,
                    visitor.use_compression as i32,
                    visitor.xtcp_protocol,
                    visitor.keep_tunnel_open.map(|v| v as i32),
                    visitor.max_retries_an_hour,
                    visitor.min_retry_interval,
                    visitor.fallback_to,
                    visitor.fallback_timeout_ms,
                    visitor.plugin_config.as_ref().map(|v| v.to_string()),
                    visitor.profile_id,
                    visitor.annotations,
                    id,
                ],
            )
            .map_err(ClientError::DatabaseQuery)?;

        if affected == 0 {
            return Err(ClientError::RecordNotFound {
                table: "local_visitor".into(),
                id,
            });
        }
        Ok(())
    }

    /// Delete a Visitor
    pub async fn delete_visitor(&self, id: i64) -> Result<()> {
        let conn = self.lock().await;
        let affected = conn
            .execute("DELETE FROM local_visitor WHERE id = ?1", params![id])
            .map_err(ClientError::DatabaseQuery)?;

        if affected == 0 {
            return Err(ClientError::RecordNotFound {
                table: "local_visitor".into(),
                id,
            });
        }
        tracing::info!(visitor_id = id, "Visitor deleted");
        Ok(())
    }

    /// List visitors for a specific profile
    pub async fn list_visitors_for_profile(&self, profile_id: i64) -> Result<Vec<LocalVisitor>> {
        let conn = self.lock().await;
        let mut stmt = conn
            .prepare(
                "SELECT id, name, visitor_type, server_name, server_user, bind_addr, bind_port,
                        secret_key, enabled, use_encryption, use_compression,
                        xtcp_protocol, keep_tunnel_open, max_retries_an_hour,
                        min_retry_interval, fallback_to, fallback_timeout_ms,
                        plugin_config, profile_id, annotations, created_at, updated_at
                 FROM local_visitor WHERE profile_id = ?1
                 ORDER BY name",
            )
            .map_err(ClientError::DatabaseQuery)?;

        let visitors: Vec<LocalVisitor> = stmt
            .query_map(params![profile_id], row_to_visitor)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(ClientError::DatabaseQuery)?;

        Ok(visitors)
    }

    /// List active (enabled) visitors across all profiles
    pub async fn list_active_visitors(&self) -> Result<Vec<LocalVisitor>> {
        let conn = self.lock().await;
        let mut stmt = conn
            .prepare(
                "SELECT id, name, visitor_type, server_name, server_user, bind_addr, bind_port,
                        secret_key, enabled, use_encryption, use_compression,
                        xtcp_protocol, keep_tunnel_open, max_retries_an_hour,
                        min_retry_interval, fallback_to, fallback_timeout_ms,
                        plugin_config, profile_id, annotations, created_at, updated_at
                 FROM local_visitor WHERE enabled = 1
                 ORDER BY name",
            )
            .map_err(ClientError::DatabaseQuery)?;

        let visitors: Vec<LocalVisitor> = stmt
            .query_map([], row_to_visitor)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(ClientError::DatabaseQuery)?;

        Ok(visitors)
    }
}

/// Map database row to LocalVisitor
fn row_to_visitor(row: &rusqlite::Row<'_>) -> rusqlite::Result<LocalVisitor> {
    let plugin_config: Option<String> = row.get(17)?;
    let plugin_config = plugin_config
        .filter(|s| !s.is_empty())
        .and_then(|s| serde_json::from_str(&s).ok());

    Ok(LocalVisitor {
        id: Some(row.get(0)?),
        name: row.get(1)?,
        visitor_type: row.get::<_, VisitorType>(2)?,
        server_name: row.get(3)?,
        server_user: row.get(4)?,
        bind_addr: row.get(5)?,
        bind_port: row.get(6)?,
        secret_key: row.get(7)?,
        enabled: row.get::<_, i32>(8)? != 0,
        use_encryption: row.get::<_, i32>(9)? != 0,
        use_compression: row.get::<_, i32>(10)? != 0,
        xtcp_protocol: row.get(11)?,
        keep_tunnel_open: row.get::<_, Option<i32>>(12)?.map(|v| v != 0),
        max_retries_an_hour: row.get(13)?,
        min_retry_interval: row.get(14)?,
        fallback_to: row.get(15)?,
        fallback_timeout_ms: row.get(16)?,
        plugin_config,
        profile_id: row.get(18)?,
        annotations: row.get(19)?,
        created_at: row.get(20)?,
        updated_at: row.get(21)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::model::FrpsProfile;
    use tempfile::NamedTempFile;

    async fn setup_db() -> Database {
        let tmp = NamedTempFile::new().unwrap();
        let db = Database::open(tmp.path().to_str().unwrap()).await.unwrap();
        crate::db::migrate::run(&*db.lock().await).unwrap();
        // Create a profile for FK constraint
        db.insert_profile(&FrpsProfile::default()).await.unwrap();
        db
    }

    fn sample_visitor(name: &str, profile_id: i64) -> LocalVisitor {
        LocalVisitor {
            id: None,
            name: name.into(),
            visitor_type: VisitorType::Stcp,
            server_name: "secret-ssh".into(),
            server_user: None,
            bind_addr: Some("127.0.0.1".into()),
            bind_port: 6000,
            secret_key: Some("abcdefg".into()),
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
            profile_id,
            annotations: None,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    #[tokio::test]
    async fn test_insert_and_get_visitor() {
        let db = setup_db().await;
        let id = db
            .insert_visitor(&sample_visitor("STCP-Visitor", 1))
            .await
            .unwrap();
        assert!(id > 0);

        let visitor = db.get_visitor(id).await.unwrap();
        assert_eq!(visitor.name, "STCP-Visitor");
        assert_eq!(visitor.bind_port, 6000);
        assert_eq!(visitor.server_name, "secret-ssh");
    }

    #[tokio::test]
    async fn test_update_visitor() {
        let db = setup_db().await;
        let id = db
            .insert_visitor(&sample_visitor("Original", 1))
            .await
            .unwrap();

        let mut visitor = db.get_visitor(id).await.unwrap();
        visitor.name = "Updated".into();
        visitor.bind_port = 7000;
        db.update_visitor(&visitor).await.unwrap();

        let updated = db.get_visitor(id).await.unwrap();
        assert_eq!(updated.name, "Updated");
        assert_eq!(updated.bind_port, 7000);
    }

    #[tokio::test]
    async fn test_delete_visitor() {
        let db = setup_db().await;
        let id = db
            .insert_visitor(&sample_visitor("ToDelete", 1))
            .await
            .unwrap();
        db.delete_visitor(id).await.unwrap();

        let result = db.get_visitor(id).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_list_visitors_for_profile() {
        let db = setup_db().await;
        db.insert_visitor(&sample_visitor("V1", 1)).await.unwrap();
        db.insert_visitor(&sample_visitor("V2", 1)).await.unwrap();

        let list = db.list_visitors_for_profile(1).await.unwrap();
        assert_eq!(list.len(), 2);
    }

    #[tokio::test]
    async fn test_list_active_visitors() {
        let db = setup_db().await;
        let id = db
            .insert_visitor(&sample_visitor("Active", 1))
            .await
            .unwrap();

        // Disable it
        let mut v = db.get_visitor(id).await.unwrap();
        v.enabled = false;
        db.update_visitor(&v).await.unwrap();

        let active = db.list_active_visitors().await.unwrap();
        assert!(active.is_empty());
    }
}
