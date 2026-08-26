//! HTTP API state — shared application state for all handlers
//!
//! `ApiState` holds all dependencies needed by axum handlers:
//! database, process manager, reload task tracking, and client lifecycle state.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

use rustfrp_client::db::Database;
use rustfrp_client::process::manager::ProcessManager;
use rustfrp_client::ClientState;

/// Reload async task status
#[derive(Debug, Clone, serde::Serialize)]
pub struct ReloadTaskStatus {
    pub status: ReloadPhase,
    pub profiles_affected: u32,
    pub errors: Vec<String>,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReloadPhase {
    Running,
    Completed,
    Failed,
}

/// Shared state for HTTP API handlers
///
/// `ProcessManager` is held directly (no `Arc<RwLock<>>` wrapper) because
/// it already contains internal locking (`guards: Arc<RwLock<HashMap<...>>>`).
/// Wrapping it again would create a double-lock contention pattern.
#[derive(Clone)]
pub struct ApiState {
    pub db: Database,
    pub process_manager: ProcessManager,
    pub config_dir: PathBuf,
    pub start_time: Instant,
    pub app_state: Arc<RwLock<ClientState>>,
    pub reload_tasks: Arc<RwLock<HashMap<String, ReloadTaskStatus>>>,
}

impl ApiState {
    /// Create a new ApiState from its components.
    pub fn new(
        db: Database,
        process_manager: ProcessManager,
        config_dir: PathBuf,
        app_state: Arc<RwLock<ClientState>>,
    ) -> Self {
        Self {
            db,
            process_manager,
            config_dir,
            start_time: Instant::now(),
            app_state,
            reload_tasks: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustfrp_client::ClientState;
    use rustfrp_common::signal::SignalHandler;
    use std::sync::Arc;
    use tempfile::TempDir;
    use tokio::sync::RwLock;

    async fn setup_test_db() -> Database {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let db = Database::open(tmp.path().to_str().unwrap()).await.unwrap();
        rustfrp_client::db::migrate::run(&*db.lock().await).unwrap();
        db
    }

    #[tokio::test]
    async fn test_api_state_new_initializes_all_fields() {
        let db = setup_test_db().await;
        let tmp_dir = TempDir::new().unwrap();
        let config_dir = tmp_dir.path().to_path_buf();
        let signal = SignalHandler::new();
        let process_manager = ProcessManager::new(
            config_dir.clone(),
            std::path::PathBuf::from("/nonexistent/frpc"),
            signal,
        );
        let app_state = Arc::new(RwLock::new(ClientState::Uninitialized));

        let state = ApiState::new(
            db.clone(),
            process_manager,
            config_dir.clone(),
            app_state.clone(),
        );

        // Verify fields are accessible
        let db_path = state.db.path();
        assert!(!db_path.as_os_str().is_empty());
        assert_eq!(state.config_dir, config_dir);

        // start_time should be set to now
        let elapsed = state.start_time.elapsed();
        assert!(elapsed.as_secs() < 5, "start_time should be recent");

        // reload_tasks starts empty
        let tasks = state.reload_tasks.read().await;
        assert!(tasks.is_empty());
    }

    #[tokio::test]
    async fn test_reload_tasks_starts_empty() {
        let db = setup_test_db().await;
        let tmp_dir = TempDir::new().unwrap();
        let config_dir = tmp_dir.path().to_path_buf();
        let signal = SignalHandler::new();
        let process_manager = ProcessManager::new(
            config_dir.clone(),
            std::path::PathBuf::from("/nonexistent/frpc"),
            signal,
        );
        let app_state = Arc::new(RwLock::new(ClientState::Ready));

        let state = ApiState::new(db, process_manager, config_dir, app_state);

        let tasks = state.reload_tasks.read().await;
        assert!(tasks.is_empty());
        assert_eq!(tasks.len(), 0);
    }

    #[test]
    fn test_reload_phase_serialization() {
        let status = ReloadTaskStatus {
            status: ReloadPhase::Running,
            profiles_affected: 0,
            errors: vec![],
            started_at: chrono::Utc::now(),
            completed_at: None,
        };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("\"running\""));
        assert!(json.contains("\"profiles_affected\":0"));
    }
}
