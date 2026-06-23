//! RustFRP client — frpc wrapper
//!
//! Provides configuration management, TOML generation, and frpc process supervision.
//! Uses `rustfrp_common` for plugin infrastructure and signal handling.

pub mod config;
pub mod db;
pub mod error;
pub mod process;

pub use error::{ClientError, Result};

/// Client facade — unified API for the frpc wrapper
///
/// All client operations are exposed through this trait.
#[async_trait::async_trait]
pub trait ClientFacade: Send + Sync {
    /// Get client version
    fn version(&self) -> &str;

    /// Initialize client (open/create database, run migrations)
    async fn init(&self) -> Result<()>;

    /// Generate all frpc TOML configs (grouped by Profile) and atomically write
    async fn generate_all_configs(&self, output_dir: &str) -> Result<Vec<String>>;

    /// Start frpc child process for the given profile
    async fn start_frpc(&self, profile_id: i64) -> Result<()>;

    /// Hot-reload frpc for the given profile (send SIGHUP)
    async fn reload_frpc(&self, profile_id: i64) -> Result<()>;

    /// Stop frpc child process for the given profile
    async fn stop_frpc(&self, profile_id: i64) -> Result<()>;

    /// Start frpc instances for all configured profiles
    async fn start_all_frpc(&self) -> Result<()>;

    /// Stop all frpc instances
    async fn stop_all_frpc(&self) -> Result<()>;

    /// Shutdown client, release resources
    async fn shutdown(&self) -> Result<()>;
}

/// Client state
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientState {
    /// Not initialized
    Uninitialized,
    /// Initialized, database ready
    Ready,
    /// frpc running
    Running,
    /// Stopping
    Stopping,
    /// Stopped
    Stopped,
    /// Error state
    Error(String),
}

impl std::fmt::Display for ClientState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientState::Uninitialized => write!(f, "uninitialized"),
            ClientState::Ready => write!(f, "ready"),
            ClientState::Running => write!(f, "running"),
            ClientState::Stopping => write!(f, "stopping"),
            ClientState::Stopped => write!(f, "stopped"),
            ClientState::Error(e) => write!(f, "error: {e}"),
        }
    }
}
