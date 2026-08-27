//! Deployment environments and profile membership.

use super::Database;
use crate::error::{ClientError, Result};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Environment {
    pub id: Option<i64>,
    pub name: String,
    pub description: Option<String>,
    pub color: String,
    pub is_default: bool,
    #[serde(skip_deserializing)]
    pub created_at: String,
    #[serde(skip_deserializing)]
    pub updated_at: String,
}

impl Default for Environment {
    fn default() -> Self {
        Self {
            id: None,
            name: String::new(),
            description: None,
            color: "#18a058".into(),
            is_default: false,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }
}

impl Database {
    pub async fn list_environments(&self) -> Result<Vec<Environment>> {
        let conn = self.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, name, description, color, is_default, created_at, updated_at
             FROM environment ORDER BY is_default DESC, name",
        )?;
        let environments = stmt
            .query_map([], row_to_environment)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(environments)
    }

    pub async fn insert_environment(&self, environment: &Environment) -> Result<i64> {
        validate(environment)?;
        let conn = self.lock().await;
        let tx = conn.unchecked_transaction()?;
        if environment.is_default {
            tx.execute("UPDATE environment SET is_default = 0", [])?;
        }
        let now = chrono::Utc::now().to_rfc3339();
        tx.execute(
            "INSERT INTO environment (name, description, color, is_default, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
            params![
                environment.name.trim(),
                environment.description,
                environment.color,
                environment.is_default as i32,
                now
            ],
        )
        .map_err(|e| {
            if e.to_string().contains("UNIQUE") {
                ClientError::RecordAlreadyExists(format!(
                    "environment '{}' already exists",
                    environment.name
                ))
            } else {
                ClientError::DatabaseQuery(e)
            }
        })?;
        let id = tx.last_insert_rowid();
        tx.commit()?;
        Ok(id)
    }

    pub async fn update_environment(&self, environment: &Environment) -> Result<()> {
        validate(environment)?;
        let id = environment
            .id
            .ok_or_else(|| ClientError::ConfigValidation("environment id is required".into()))?;
        let conn = self.lock().await;
        let tx = conn.unchecked_transaction()?;
        let current_default: Option<i64> = tx
            .query_row("SELECT id FROM environment WHERE is_default=1", [], |row| {
                row.get(0)
            })
            .optional()?;
        if current_default == Some(id) && !environment.is_default {
            return Err(ClientError::ConfigValidation(
                "select another default environment before demoting this one".into(),
            ));
        }
        if environment.is_default {
            tx.execute("UPDATE environment SET is_default = 0 WHERE id <> ?1", [id])?;
        }
        let changed = tx.execute(
            "UPDATE environment SET name=?1, description=?2, color=?3, is_default=?4, updated_at=?5 WHERE id=?6",
            params![environment.name.trim(), environment.description, environment.color, environment.is_default as i32, chrono::Utc::now().to_rfc3339(), id],
        )?;
        if changed == 0 {
            return Err(ClientError::RecordNotFound {
                table: "environment".into(),
                id,
            });
        }
        tx.commit()?;
        Ok(())
    }

    pub async fn delete_environment(&self, id: i64) -> Result<()> {
        let conn = self.lock().await;
        let tx = conn.unchecked_transaction()?;
        let is_default: Option<bool> = tx
            .query_row(
                "SELECT is_default FROM environment WHERE id=?1",
                [id],
                |row| row.get(0),
            )
            .optional()?;
        match is_default {
            None => {
                return Err(ClientError::RecordNotFound {
                    table: "environment".into(),
                    id,
                })
            }
            Some(true) => {
                return Err(ClientError::ConfigValidation(
                    "the default environment cannot be deleted".into(),
                ))
            }
            Some(false) => {}
        }
        let default_id: i64 =
            tx.query_row("SELECT id FROM environment WHERE is_default=1", [], |row| {
                row.get(0)
            })?;
        tx.execute(
            "UPDATE profile_environment SET environment_id=?1 WHERE environment_id=?2",
            params![default_id, id],
        )?;
        tx.execute("DELETE FROM environment WHERE id=?1", [id])?;
        tx.commit()?;
        Ok(())
    }

    pub async fn set_profile_environment(
        &self,
        profile_id: i64,
        environment_id: i64,
    ) -> Result<()> {
        let conn = self.lock().await;
        conn.execute(
            "INSERT INTO profile_environment (profile_id, environment_id) VALUES (?1, ?2)
             ON CONFLICT(profile_id) DO UPDATE SET environment_id=excluded.environment_id",
            params![profile_id, environment_id],
        )
        .map_err(ClientError::DatabaseQuery)?;
        Ok(())
    }

    pub async fn profile_environment_id(&self, profile_id: i64) -> Result<i64> {
        let conn = self.lock().await;
        conn.query_row(
            "SELECT environment_id FROM profile_environment WHERE profile_id=?1",
            [profile_id],
            |row| row.get(0),
        )
        .map_err(ClientError::DatabaseQuery)
    }

    pub async fn environment_profile_ids(&self, environment_id: i64) -> Result<Vec<i64>> {
        let conn = self.lock().await;
        let mut stmt = conn.prepare(
            "SELECT profile_id FROM profile_environment WHERE environment_id=?1 ORDER BY profile_id",
        )?;
        let ids = stmt
            .query_map([environment_id], |row| row.get(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(ids)
    }
}

fn validate(environment: &Environment) -> Result<()> {
    if environment.name.trim().is_empty() || environment.name.len() > 64 {
        return Err(ClientError::ConfigValidation(
            "environment name must be 1-64 characters".into(),
        ));
    }
    if !environment.color.starts_with('#')
        || environment.color.len() != 7
        || !environment.color[1..]
            .chars()
            .all(|c| c.is_ascii_hexdigit())
    {
        return Err(ClientError::ConfigValidation(
            "environment color must be #RRGGBB".into(),
        ));
    }
    Ok(())
}

fn row_to_environment(row: &rusqlite::Row<'_>) -> rusqlite::Result<Environment> {
    Ok(Environment {
        id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        color: row.get(3)?,
        is_default: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrate;

    #[tokio::test]
    async fn environment_lifecycle_reassigns_profiles_on_delete() {
        let db = Database::open(":memory:").await.unwrap();
        migrate::run(&*db.lock().await).unwrap();
        let profile_id = db
            .insert_profile(&crate::config::model::FrpsProfile::default())
            .await
            .unwrap();
        let environment_id = db
            .insert_environment(&Environment {
                name: "Production".into(),
                ..Environment::default()
            })
            .await
            .unwrap();
        db.set_profile_environment(profile_id, environment_id)
            .await
            .unwrap();
        assert_eq!(
            db.profile_environment_id(profile_id).await.unwrap(),
            environment_id
        );
        db.delete_environment(environment_id).await.unwrap();
        assert_eq!(db.profile_environment_id(profile_id).await.unwrap(), 1);
    }
}
