//! Platform settings: the few switches an operator flips at runtime.
//!
//! Stored key/value rather than one column per setting so a new switch does not
//! need a schema change, and so a long-lived task can re-read the current value
//! instead of caching a decision made at startup.
//!
//! These settings are *not* FRP configuration: nothing here is serialized into a
//! generated TOML (ARCH-004).

use rusqlite::{params, OptionalExtension};

use crate::db::Database;
use crate::error::Result;

/// Whether the certificate ingest listener should accept pushes.
pub const CERT_INGEST_ENABLED: &str = "cert_ingest.enabled";

/// Loopback-or-any address the ingest listener binds.
pub const CERT_INGEST_LISTEN: &str = "cert_ingest.listen";

/// TCP port the ingest listener binds. Kept apart from the address so the UI can
/// offer "just change the port" without making an operator type a socket address.
pub const CERT_INGEST_PORT: &str = "cert_ingest.port";

/// Whether the ingest listen address may be reachable from other hosts.
pub const CERT_INGEST_EXPOSE: &str = "cert_ingest.expose";

/// Default ingest port. Chosen next to the control API's 7900 so the pair is
/// memorable, and above the ephemeral range so it does not collide with one.
pub const DEFAULT_INGEST_PORT: u16 = 7901;

impl Database {
    /// Read one setting, or `None` when it was never written.
    pub async fn platform_setting(&self, key: &str) -> Result<Option<String>> {
        let conn = self.lock().await;
        Ok(conn
            .query_row(
                "SELECT value FROM platform_setting WHERE key = ?1",
                params![key],
                |row| row.get::<_, String>(0),
            )
            .optional()?)
    }

    /// Read a setting as a boolean. Anything other than `true`/`1` is false; a
    /// missing row falls back to `default`.
    pub async fn platform_flag(&self, key: &str, default: bool) -> Result<bool> {
        match self.platform_setting(key).await? {
            Some(value) => Ok(matches!(value.as_str(), "true" | "1")),
            None => Ok(default),
        }
    }

    /// Upsert one setting, leaving every other key untouched.
    pub async fn set_platform_setting(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.lock().await;
        conn.execute(
            "INSERT INTO platform_setting (key, value, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![key, value, chrono::Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }
}

/// Resolved ingest listener configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestConfig {
    pub enabled: bool,
    /// Address actually bound, derived from `expose`.
    pub listen: String,
    pub port: u16,
    pub expose: bool,
}

impl IngestConfig {
    /// Full socket address for the listener.
    pub fn address(&self) -> String {
        format!("{}:{}", self.listen, self.port)
    }

    /// Read the stored configuration, applying defaults.
    ///
    /// The default is **enabled on loopback**: a push endpoint that only the
    /// local machine can reach is not an exposure, and requiring an operator to
    /// switch it on before Certd can be tried would leave the feature looking
    /// broken. Widening it to other hosts stays an explicit choice.
    pub async fn load(db: &Database) -> Result<Self> {
        let enabled = db.platform_flag(CERT_INGEST_ENABLED, true).await?;
        let expose = db.platform_flag(CERT_INGEST_EXPOSE, false).await?;
        let port = match db.platform_setting(CERT_INGEST_PORT).await? {
            Some(value) => value.parse::<u16>().unwrap_or(DEFAULT_INGEST_PORT),
            None => DEFAULT_INGEST_PORT,
        };
        let stored_listen = db.platform_setting(CERT_INGEST_LISTEN).await?;
        let listen = match stored_listen {
            // An explicit address wins, so a hand-configured deployment keeps
            // working even if `expose` disagrees with it.
            Some(value) if !value.trim().is_empty() => value.trim().to_string(),
            _ => {
                if expose {
                    "0.0.0.0".to_string()
                } else {
                    "127.0.0.1".to_string()
                }
            }
        };
        Ok(Self {
            enabled,
            listen,
            port,
            expose,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrate;

    async fn database() -> Database {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.db");
        let db = Database::open(path.to_str().unwrap()).await.unwrap();
        {
            let conn = db.lock().await;
            migrate::run(&conn).unwrap();
        }
        db
    }

    #[tokio::test]
    async fn ingest_defaults_to_loopback_and_enabled() {
        let db = database().await;
        let config = IngestConfig::load(&db).await.unwrap();
        assert!(config.enabled, "push must work out of the box on loopback");
        assert!(
            !config.expose,
            "default must not be reachable from other hosts"
        );
        assert_eq!(config.address(), "127.0.0.1:7901");
    }

    #[tokio::test]
    async fn exposing_changes_only_the_bind_address() {
        let db = database().await;
        db.set_platform_setting(CERT_INGEST_EXPOSE, "true")
            .await
            .unwrap();
        let config = IngestConfig::load(&db).await.unwrap();
        assert_eq!(config.address(), "0.0.0.0:7901");
        assert_eq!(config.port, DEFAULT_INGEST_PORT);
    }

    #[tokio::test]
    async fn settings_are_independent() {
        let db = database().await;
        db.set_platform_setting(CERT_INGEST_PORT, "17901")
            .await
            .unwrap();
        let config = IngestConfig::load(&db).await.unwrap();
        assert_eq!(config.port, 17901);
        assert!(config.enabled, "writing one key must not reset another");
    }

    #[tokio::test]
    async fn disabling_is_respected() {
        let db = database().await;
        db.set_platform_setting(CERT_INGEST_ENABLED, "false")
            .await
            .unwrap();
        assert!(!IngestConfig::load(&db).await.unwrap().enabled);
    }
}
