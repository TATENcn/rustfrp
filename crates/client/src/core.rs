//! ClientCore — concrete implementation of ClientFacade
//!
//! Wires together Database, PluginManager, TOML generator, ProcessManager,
//! and SignalHandler into a working frpc wrapper daemon.

use crate::config::generator::{generate_all_frpc_tomls, profile_toml_path};
use crate::db::{migrate, Database};
use crate::error::{ClientError, Result};
use crate::process::manager::ProcessManager;
use crate::ClientFacade;
use crate::ClientState;
use rustfrp_common::plugin::manager::PluginManager;
use rustfrp_common::signal::SignalHandler;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Concrete client core that owns all subsystems.
pub struct ClientCore {
    db: Database,
    plugin_manager: PluginManager,
    process_manager: ProcessManager,
    signal_handler: SignalHandler,
    config_dir: PathBuf,
    frpc_path: PathBuf,
    state: Arc<RwLock<ClientState>>,
}

impl ClientCore {
    /// Create a new ClientCore.
    ///
    /// Opens (or creates) the SQLite database, runs migrations,
    pub async fn new(db_path: &str, config_dir: &str) -> Result<Self> {
        Self::new_with_frpc(db_path, config_dir, None, false).await
    }

    /// Create a ClientCore with explicit control over FRP binary discovery.
    ///
    /// When `auto_download` is false, initialization never accesses the
    /// network. The API remains available and an FRP version can be installed
    /// and activated later through the version management endpoints.
    pub async fn new_with_frpc(
        db_path: &str,
        config_dir: &str,
        frpc_path: Option<PathBuf>,
        auto_download: bool,
    ) -> Result<Self> {
        Self::new_with_manager(
            db_path,
            config_dir,
            frpc_path,
            auto_download,
            rustfrp_bin::manager::VersionManager::default(),
        )
        .await
    }

    async fn new_with_manager(
        db_path: &str,
        config_dir: &str,
        frpc_path: Option<PathBuf>,
        auto_download: bool,
        version_manager: rustfrp_bin::manager::VersionManager,
    ) -> Result<Self> {
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

        let frpc_path = if let Some(path) = frpc_path {
            path
        } else if auto_download {
            // Ensure the managed binary is present. This may access the
            // network only when the caller explicitly opts in.
            let selected_version = std::env::var("RUSTFRP_FRP_VERSION")
                .ok()
                .or(version_manager.active_version().await)
                .unwrap_or_else(|| rustfrp_bin::ensure::DEFAULT_FRP_VERSION.to_owned());
            let path =
                rustfrp_bin::ensure::ensure_binary("frpc", Some(&selected_version), None, None)
                    .await
                    .map_err(|e| ClientError::ProcessStart(format!("frpc unavailable: {e}")))?;
            version_manager
                .activate(&selected_version)
                .await
                .map_err(|e| ClientError::ProcessStart(format!("frpc activation failed: {e}")))?;
            path
        } else if let Some(active) = version_manager.active_version().await {
            version_manager
                .activate(&active)
                .await
                .map_err(|e| ClientError::ProcessStart(format!("active frpc unavailable: {e}")))?
        } else {
            tracing::warn!(
                "FRP auto-download is disabled and no active version is installed; the Web API will remain available until FRP is installed and activated"
            );
            PathBuf::from("frpc")
        };

        let plugin_manager = PluginManager::with_default_dir();
        let signal_handler = SignalHandler::new();
        let process_manager = ProcessManager::new(
            config_dir.clone(),
            frpc_path.clone(),
            signal_handler.clone(),
        )
        .with_database(db.clone());
        let state = Arc::new(RwLock::new(ClientState::Uninitialized));

        tracing::info!(
            db = %db_path,
            config_dir = %config_dir.display(),
            frpc = %frpc_path.display(),
            "ClientCore initialized"
        );

        Ok(Self {
            db,
            plugin_manager,
            process_manager,
            signal_handler,
            config_dir,
            frpc_path,
            state,
        })
    }

    /// Start an frpc instance for every profile the user left running.
    ///
    /// `desired_running` is the user's intent, written by the start/stop switch
    /// (GUI) and by `POST /api/v1/profiles/{id}/start|stop`. Restoring on intent
    /// rather than on "a TOML exists" matters for unreachable servers: a failed
    /// `frpc` costs four spawns (initial + three backoffs), and doing that on
    /// every launch makes the desktop shell flash a window each time for a
    /// profile the user may not even care about.
    ///
    /// Returns `(started, skipped)` where `skipped` counts profiles the user had
    /// stopped.
    async fn restore_desired_running(&self) -> Result<(u32, u32)> {
        let mut started = 0;
        let mut skipped = 0;
        for profile in self.db.list_profiles().await? {
            let Some(id) = profile.id else { continue };
            if !self.db.profile_desired_running(id).await? {
                skipped += 1;
                continue;
            }
            if !profile_toml_path(&self.config_dir, id, &profile.name).exists() {
                continue;
            }
            match self.process_manager.start(id, &profile.name).await {
                Ok(()) => started += 1,
                Err(error) => tracing::warn!(
                    profile_id = id,
                    %error,
                    "frpc unavailable; keep management API available for repair"
                ),
            }
        }
        Ok((started, skipped))
    }

    /// Access the database (for daemon crate / API handlers).
    pub fn db(&self) -> &Database {
        &self.db
    }

    /// Access the rustfrp-managed frpc binary path.
    pub fn frpc_path(&self) -> &PathBuf {
        &self.frpc_path
    }
    pub fn process_manager(&self) -> &ProcessManager {
        &self.process_manager
    }

    /// Access the configuration directory.
    pub fn config_dir(&self) -> &PathBuf {
        &self.config_dir
    }

    /// Access the current daemon state (for ApiState consumption).
    pub fn state(&self) -> &Arc<RwLock<ClientState>> {
        &self.state
    }

    /// Access the shutdown flag observed by [`ClientCore::run`].
    ///
    /// Embedded hosts (desktop tray) request shutdown through this handle
    /// instead of sending themselves a signal.
    pub fn signal_handler(&self) -> &SignalHandler {
        &self.signal_handler
    }

    /// Run the full client lifecycle:
    ///
    /// 1. Load plugins
    /// 2. Generate frpc TOML configs from SQLite
    /// 3. Start frpc subprocesses for each active profile
    /// 4. Wait for shutdown signal (SIGTERM / SIGINT / Ctrl+C)
    /// 5. Graceful shutdown (stop all frpc instances)
    pub async fn run(&self) -> Result<()> {
        *self.state.write().await = ClientState::Ready;

        // 1. Load plugins
        let plugins = self.plugin_manager.load_all().await?;
        tracing::info!(count = plugins.len(), "plugins loaded");
        for (name, result) in self.plugin_manager.start_all().await {
            if let Err(error) = result {
                tracing::warn!(%name, %error, "plugin start failed; plugin isolated");
            }
        }

        // 2. Generate TOML configs
        let toml_files = generate_all_frpc_tomls(&self.db, &self.config_dir).await?;
        if toml_files.is_empty() {
            tracing::warn!(
                "no active bindings found — use the GUI or API to create profiles and proxies"
            );
        } else {
            tracing::info!(count = toml_files.len(), "frpc TOML configs generated");
        }

        // 3. Start frpc for each profile the user left running.
        let (started, skipped) = self.restore_desired_running().await?;
        if skipped > 0 {
            tracing::info!(
                count = skipped,
                "profiles not started (stopped by the user)"
            );
        }

        if started == 0 {
            tracing::info!(
                "no frpc instances to start (create profiles with active bindings first)"
            );
        } else {
            tracing::info!(count = started, "frpc instances started");
        }

        *self.state.write().await = ClientState::Running;

        // 4. Wait for shutdown signal
        tracing::info!("client running — press Ctrl+C to stop");
        while !self.signal_handler.is_shutdown_requested() {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }

        // 5. Graceful shutdown
        tracing::info!("shutting down...");
        *self.state.write().await = ClientState::Stopping;
        self.shutdown().await?;
        *self.state.write().await = ClientState::Stopped;
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
            let Some(id) = profile.id else { continue };
            let toml_path = profile_toml_path(&self.config_dir, id, &profile.name);
            if toml_path.exists() {
                if let Err(e) = self.process_manager.start(id, &profile.name).await {
                    tracing::warn!(profile_id = id, error = %e, "failed to start frpc");
                }
            }
        }
        Ok(())
    }

    async fn stop_all_frpc(&self) -> Result<()> {
        self.process_manager.shutdown_all().await
    }

    async fn shutdown(&self) -> Result<()> {
        self.plugin_manager.stop_all().await;
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

    /// Startup restores what the user left running, not "everything with a TOML".
    ///
    /// A stopped profile still has its generated TOML; restoring on TOML presence
    /// is what made an unreachable server retry on every launch.
    #[tokio::test]
    async fn restore_starts_only_profiles_the_user_left_running() {
        let temp = tempfile::tempdir().unwrap();
        let runtime = temp.path().join("runtime");
        std::fs::create_dir_all(&runtime).unwrap();

        // A stub frpc that records every invocation, so "was it attempted" is
        // observable without a real FRP binary.
        let marker = temp.path().join("invocations");
        let stub = temp
            .path()
            .join(if cfg!(windows) { "frpc.cmd" } else { "frpc" });
        #[cfg(unix)]
        std::fs::write(
            &stub,
            format!("#!/bin/sh\necho \"$2\" >> {}\nexit 0\n", marker.display()),
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        let core = ClientCore::new_with_manager(
            temp.path().join("config.db").to_str().unwrap(),
            runtime.to_str().unwrap(),
            Some(stub.clone()),
            false,
            rustfrp_bin::manager::VersionManager::new(temp.path().join("versions")),
        )
        .await
        .unwrap();

        let mut ids = Vec::new();
        for (name, desired) in [("kept-running", true), ("user-stopped", false)] {
            let profile = crate::config::model::FrpsProfile {
                name: name.into(),
                ..Default::default()
            };
            let id = core.db().insert_profile(&profile).await.unwrap();
            core.db()
                .set_profile_desired_running(id, desired)
                .await
                .unwrap();
            // Both have a generated TOML: presence alone must not decide.
            std::fs::write(
                crate::config::generator::profile_toml_path(&runtime, id, name),
                "",
            )
            .unwrap();
            ids.push((id, name));
        }

        let (_, skipped) = core.restore_desired_running().await.unwrap();
        assert_eq!(skipped, 1, "only the user-stopped profile is skipped");

        #[cfg(unix)]
        {
            let recorded = std::fs::read_to_string(&marker).unwrap_or_default();
            assert!(
                recorded.contains(&ids[0].0.to_string()),
                "the kept profile must be attempted, got: {recorded:?}"
            );
            assert!(
                !recorded.contains(&ids[1].0.to_string()),
                "the stopped profile must never be attempted, got: {recorded:?}"
            );
        }
    }

    #[tokio::test]
    async fn offline_initialization_without_frpc() {
        let temp = tempfile::tempdir().unwrap();
        let versions = temp.path().join("versions");
        let core = ClientCore::new_with_manager(
            temp.path().join("config.db").to_str().unwrap(),
            temp.path().join("runtime").to_str().unwrap(),
            None,
            false,
            rustfrp_bin::manager::VersionManager::new(versions.clone()),
        )
        .await
        .unwrap();
        assert_eq!(core.frpc_path(), &PathBuf::from("frpc"));
        assert!(
            !versions.exists(),
            "startup must not install any FRP binary"
        );
    }

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
