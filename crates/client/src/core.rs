//! ClientCore — concrete implementation of ClientFacade
//!
//! Wires together Database, PluginManager, TOML generator, ProcessManager,
//! and SignalHandler into a working frpc wrapper daemon.

use crate::config::generator::{generate_all_frpc_tomls, sanitize_filename};
use crate::db::{migrate, Database};
use crate::error::{ClientError, Result};
use crate::process::manager::ProcessManager;
use crate::ClientFacade;
use rustfrp_common::plugin::manager::PluginManager;
use rustfrp_common::signal::SignalHandler;
use std::path::PathBuf;

/// Concrete client core that owns all subsystems.
pub struct ClientCore {
    db: Database,
    plugin_manager: PluginManager,
    process_manager: ProcessManager,
    signal_handler: SignalHandler,
    config_dir: PathBuf,
}

impl ClientCore {
    /// Create a new ClientCore.
    ///
    /// Opens (or creates) the SQLite database, runs migrations,
    /// initializes the plugin manager and process manager.
    pub async fn new(db_path: &str, config_dir: &str) -> Result<Self> {
        let config_dir = expand_tilde(config_dir);

        // Ensure config directory exists
        std::fs::create_dir_all(&config_dir).map_err(|e| {
            ClientError::TomlWrite(format!("Failed to create config directory: {e}"))
        })?;

        // Open database and run migrations
        let db = Database::open(db_path).await?;
        {
            let conn = db.lock().await;
            migrate::run(&conn)?;
        }

        let plugin_manager = PluginManager::with_default_dir();
        let signal_handler = SignalHandler::new();
        let process_manager = ProcessManager::new(config_dir.clone(), signal_handler.clone());

        tracing::info!(
            db = %db_path,
            config_dir = %config_dir.display(),
            "ClientCore initialized"
        );

        Ok(Self {
            db,
            plugin_manager,
            process_manager,
            signal_handler,
            config_dir,
        })
    }

    /// Run the full client lifecycle:
    ///
    /// 1. Load plugins
    /// 2. Generate frpc TOML configs from SQLite
    /// 3. Start frpc subprocesses for each active profile
    /// 4. Wait for shutdown signal (SIGTERM / SIGINT / Ctrl+C)
    /// 5. Graceful shutdown (stop all frpc instances)
    pub async fn run(&self) -> Result<()> {
        // 1. Load plugins
        let plugins = self.plugin_manager.load_all().await?;
        tracing::info!(count = plugins.len(), "plugins loaded");

        // 2. Generate TOML configs
        let toml_files = generate_all_frpc_tomls(&self.db, &self.config_dir).await?;
        if toml_files.is_empty() {
            tracing::warn!(
                "no active bindings found — use the GUI or API to create profiles and proxies"
            );
        } else {
            tracing::info!(count = toml_files.len(), "frpc TOML configs generated");
        }

        // 3. Start frpc for each profile that has a generated TOML
        let profiles = self.db.list_profiles().await?;
        let mut started = 0;
        for profile in &profiles {
            let safe_name = sanitize_filename(&profile.name);
            let toml_path = self.config_dir.join(format!("{safe_name}.toml"));
            if toml_path.exists() {
                if let Some(id) = profile.id {
                    self.process_manager.start(id, &profile.name).await?;
                    started += 1;
                }
            }
        }

        if started == 0 {
            tracing::info!(
                "no frpc instances to start (create profiles with active bindings first)"
            );
        } else {
            tracing::info!(count = started, "frpc instances started");
        }

        // 4. Wait for shutdown signal
        tracing::info!("client running — press Ctrl+C to stop");
        while !self.signal_handler.is_shutdown_requested() {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }

        // 5. Graceful shutdown
        tracing::info!("shutting down...");
        self.shutdown().await?;
        tracing::info!("client stopped");

        Ok(())
    }
}

#[async_trait::async_trait]
impl ClientFacade for ClientCore {
    fn version(&self) -> &str {
        env!("CARGO_PKG_VERSION")
    }

    async fn init(&self) -> Result<()> {
        // Already initialized in new(). Re-verify.
        let conn = self.db.lock().await;
        migrate::run(&conn)?;
        Ok(())
    }

    async fn generate_all_configs(&self, output_dir: &str) -> Result<Vec<String>> {
        let dir = PathBuf::from(output_dir);
        let paths = generate_all_frpc_tomls(&self.db, &dir).await?;
        Ok(paths
            .into_iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect())
    }

    async fn start_frpc(&self, profile_id: i64) -> Result<()> {
        let profile = self.db.get_profile(profile_id).await?;
        self.process_manager.start(profile_id, &profile.name).await
    }

    async fn reload_frpc(&self, profile_id: i64) -> Result<()> {
        self.process_manager.reload(profile_id).await
    }

    async fn stop_frpc(&self, profile_id: i64) -> Result<()> {
        self.process_manager.stop(profile_id).await
    }

    async fn start_all_frpc(&self) -> Result<()> {
        let profiles = self.db.list_profiles().await?;
        for profile in &profiles {
            let safe_name = sanitize_filename(&profile.name);
            let toml_path = self.config_dir.join(format!("{safe_name}.toml"));
            if toml_path.exists() {
                if let Some(id) = profile.id {
                    if let Err(e) = self.process_manager.start(id, &profile.name).await {
                        tracing::warn!(profile_id = id, error = %e, "failed to start frpc");
                    }
                }
            }
        }
        Ok(())
    }

    async fn stop_all_frpc(&self) -> Result<()> {
        self.process_manager.shutdown_all().await
    }

    async fn shutdown(&self) -> Result<()> {
        self.process_manager.shutdown_all().await?;
        tracing::info!("all subsystems shut down");
        Ok(())
    }
}

/// Expand ~ to the user's home directory.
fn expand_tilde(path: &str) -> PathBuf {
    if let Some(stripped) = path.strip_prefix("~/") {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(stripped)
    } else {
        PathBuf::from(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_tilde() {
        let result = expand_tilde("~/test");
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        assert_eq!(result, home.join("test"));
    }

    #[test]
    fn test_expand_tilde_no_tilde() {
        let result = expand_tilde("/absolute/path");
        assert_eq!(result, PathBuf::from("/absolute/path"));
    }
}
