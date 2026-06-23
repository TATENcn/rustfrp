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
                 transport_protocol, heartbeat_interval, heartbeat_timeout)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
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
                    name = ?1, server_addr = ?2, server_port = ?3, token = ?4,
                    tls_enable = ?5, tls_cert_file = ?6, tls_key_file = ?7,
                    tls_trusted_ca_file = ?8, transport_protocol = ?9,
                    heartbeat_interval = ?10, heartbeat_timeout = ?11,
                    updated_at = datetime('now')
                 WHERE id = ?12",
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
        tracing::info!(profile_id = id, "Profile 已删除");
        Ok(())
    }
}

/// 将数据库行映射为 FrpsProfile
fn row_to_profile(row: &rusqlite::Row<'_>) -> rusqlite::Result<FrpsProfile> {
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
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
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
