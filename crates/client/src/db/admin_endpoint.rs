//! Per-profile frpc admin endpoint (`webServer`) allocation.
//!
//! frpc exposes its only documented hot-reload path (`/api/reload`) and the
//! per-proxy status (`/api/status`) through `webServer`. Every managed profile
//! therefore gets a loopback listener with a random password, persisted in
//! `profile_runtime` so that TOML generation stays reproducible across restarts.
//!
//! `webServer.user`/`webServer.password` are mandatory here: `/api/config`
//! returns the full configuration including the FRP token, so an unauthenticated
//! loopback listener would leak credentials to any local process.

use super::Database;
use crate::error::{ClientError, Result};
use rusqlite::OptionalExtension;
use std::net::SocketAddr;

/// First candidate port for a managed frpc admin listener.
const ADMIN_PORT_BASE: u16 = 7400;
/// Number of consecutive ports probed before giving up.
const ADMIN_PORT_SPAN: u16 = 400;

/// Loopback admin endpoint assigned to one profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdminEndpoint {
    pub port: u16,
    pub password: String,
}

impl Database {
    /// Return the stored admin endpoint for a profile, allocating one when missing.
    ///
    /// Allocation reserves a loopback port that is neither assigned to another
    /// profile nor currently bindable by another process, and stores a random
    /// password alongside it.
    pub async fn admin_endpoint(&self, profile_id: i64) -> Result<AdminEndpoint> {
        if let Some(endpoint) = self.stored_admin_endpoint(profile_id).await? {
            return Ok(endpoint);
        }

        let taken = self.assigned_admin_ports(profile_id).await?;
        let port = first_available_port(&taken).ok_or_else(|| {
            ClientError::ConfigValidation(format!(
                "no free loopback port found for the frpc admin endpoint (probed {ADMIN_PORT_BASE}..{})",
                u32::from(ADMIN_PORT_BASE) + u32::from(ADMIN_PORT_SPAN)
            ))
        })?;

        let endpoint = AdminEndpoint {
            port,
            password: uuid::Uuid::new_v4().simple().to_string(),
        };
        self.store_admin_endpoint(profile_id, &endpoint).await?;
        tracing::info!(
            profile_id,
            port,
            "Allocated frpc admin endpoint for profile"
        );
        Ok(endpoint)
    }

    /// Read the stored admin endpoint without allocating.
    pub async fn stored_admin_endpoint(&self, profile_id: i64) -> Result<Option<AdminEndpoint>> {
        let row = self
            .lock()
            .await
            .query_row(
                "SELECT admin_port, admin_password FROM profile_runtime WHERE profile_id = ?1",
                [profile_id],
                |row| {
                    Ok((
                        row.get::<_, Option<i64>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                    ))
                },
            )
            .optional()?;

        Ok(row.and_then(|(port, password)| match (port, password) {
            (Some(port), Some(password))
                if (1..=65_535).contains(&port) && !password.is_empty() =>
            {
                Some(AdminEndpoint {
                    port: port as u16,
                    password,
                })
            }
            _ => None,
        }))
    }

    /// Persist an admin endpoint for a profile.
    pub async fn store_admin_endpoint(
        &self,
        profile_id: i64,
        endpoint: &AdminEndpoint,
    ) -> Result<()> {
        self.lock()
            .await
            .execute(
                "INSERT INTO profile_runtime (profile_id, admin_port, admin_password, updated_at)
                 VALUES (?1, ?2, ?3, datetime('now'))
                 ON CONFLICT(profile_id) DO UPDATE SET
                     admin_port = excluded.admin_port,
                     admin_password = excluded.admin_password,
                     updated_at = excluded.updated_at",
                rusqlite::params![profile_id, i64::from(endpoint.port), endpoint.password],
            )
            .map_err(ClientError::DatabaseQuery)?;
        Ok(())
    }

    /// Ports already assigned to other profiles.
    async fn assigned_admin_ports(&self, profile_id: i64) -> Result<Vec<u16>> {
        let conn = self.lock().await;
        let mut statement = conn
            .prepare(
                "SELECT admin_port FROM profile_runtime
                 WHERE profile_id != ?1 AND admin_port IS NOT NULL",
            )
            .map_err(ClientError::DatabaseQuery)?;
        let ports = statement
            .query_map([profile_id], |row| row.get::<_, i64>(0))
            .map_err(ClientError::DatabaseQuery)?
            .collect::<std::result::Result<Vec<i64>, _>>()
            .map_err(ClientError::DatabaseQuery)?;
        Ok(ports
            .into_iter()
            .filter(|port| (1..=65_535).contains(port))
            .map(|port| port as u16)
            .collect())
    }
}

/// Pick the first port that is absent from `taken` and bindable on loopback.
fn first_available_port(taken: &[u16]) -> Option<u16> {
    (ADMIN_PORT_BASE..ADMIN_PORT_BASE.saturating_add(ADMIN_PORT_SPAN))
        .find(|port| !taken.contains(port) && loopback_port_is_free(*port))
}

fn loopback_port_is_free(port: u16) -> bool {
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    std::net::TcpListener::bind(address).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    async fn setup_db() -> Database {
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().to_string_lossy().to_string();
        let db = Database::open(&path).await.unwrap();
        crate::db::migrate::run(&*db.lock().await).unwrap();
        std::mem::forget(tmp);
        db
    }

    async fn insert_profile(db: &Database, name: &str) -> i64 {
        db.insert_profile(&crate::config::model::FrpsProfile {
            name: name.into(),
            ..Default::default()
        })
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn allocation_is_stable_and_unique_per_profile() {
        let db = setup_db().await;
        let first = insert_profile(&db, "first").await;
        let second = insert_profile(&db, "second").await;

        let first_endpoint = db.admin_endpoint(first).await.unwrap();
        let second_endpoint = db.admin_endpoint(second).await.unwrap();

        assert_ne!(first_endpoint.port, second_endpoint.port);
        assert!(first_endpoint.port >= ADMIN_PORT_BASE);
        assert!(!first_endpoint.password.is_empty());

        // Repeated lookups must not reallocate.
        assert_eq!(
            db.admin_endpoint(first).await.unwrap(),
            first_endpoint,
            "stored endpoint must be reused"
        );
    }

    #[tokio::test]
    async fn missing_credentials_trigger_reallocation() {
        let db = setup_db().await;
        let profile = insert_profile(&db, "profile").await;
        let endpoint = db.admin_endpoint(profile).await.unwrap();

        // Simulate a partially migrated row: port present, password lost.
        db.lock()
            .await
            .execute(
                "UPDATE profile_runtime SET admin_password = NULL WHERE profile_id = ?1",
                [profile],
            )
            .unwrap();
        assert!(db.stored_admin_endpoint(profile).await.unwrap().is_none());

        let reallocated = db.admin_endpoint(profile).await.unwrap();
        assert_eq!(reallocated.port, endpoint.port);
        assert_ne!(reallocated.password, endpoint.password);
    }
}
