use super::Database;
use crate::error::Result;
use rusqlite::OptionalExtension;
impl Database {
    pub async fn profile_frpc_version(&self, id: i64) -> Result<Option<String>> {
        Ok(self
            .lock()
            .await
            .query_row(
                "SELECT frpc_version FROM profile_runtime WHERE profile_id = ?1",
                [id],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten())
    }
    pub async fn set_profile_frpc_version(&self, id: i64, version: Option<&str>) -> Result<()> {
        self.lock().await.execute("INSERT INTO profile_runtime (profile_id, frpc_version) VALUES (?1, ?2) ON CONFLICT(profile_id) DO UPDATE SET frpc_version=excluded.frpc_version, updated_at=datetime('now')", rusqlite::params![id, version])?;
        Ok(())
    }
    pub async fn profiles_using_version(&self, version: &str) -> Result<Vec<i64>> {
        let conn = self.lock().await;
        let mut stmt =
            conn.prepare("SELECT profile_id FROM profile_runtime WHERE frpc_version = ?1")?;
        let rows = stmt
            .query_map([version], |row| row.get(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn version_selection_preserves_runtime_flags_and_cascades_on_delete() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(tmp.path().join("test.db").to_str().unwrap())
            .await
            .unwrap();
        super::super::migrate::run(&*db.lock().await).unwrap();
        let id = db
            .insert_profile(&crate::config::model::FrpsProfile {
                name: "version-test".into(),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(db.profile_frpc_version(id).await.unwrap(), None);
        db.set_profile_desired_running(id, true).await.unwrap();
        db.set_profile_frpc_version(id, Some("0.70.1"))
            .await
            .unwrap();
        assert!(db.profile_desired_running(id).await.unwrap());
        assert_eq!(db.profiles_using_version("0.70.1").await.unwrap(), vec![id]);
        db.set_profile_frpc_version(id, None).await.unwrap();
        assert!(db
            .profiles_using_version("0.70.1")
            .await
            .unwrap()
            .is_empty());
        db.set_profile_frpc_version(id, Some("0.70.1"))
            .await
            .unwrap();
        db.delete_profile(id).await.unwrap();
        assert!(db
            .profiles_using_version("0.70.1")
            .await
            .unwrap()
            .is_empty());
    }
}
