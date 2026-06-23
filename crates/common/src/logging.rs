//! Logging initialization
//!
//! CODE-004: production mode (RUSTFRP_LOG_MODE=json) → JSON to file;
//! development mode (default) → human-readable to console.

use std::fs::OpenOptions;
use std::sync::Mutex;
use tracing_subscriber::EnvFilter;

/// Initialize tracing subscriber.
///
/// Behavior controlled by environment variables:
/// - `RUST_LOG`: tracing filter (default: `info`)
/// - `RUSTFRP_LOG_MODE`: `json` = JSON to `~/.rustfrp/logs/rustfrp.log` (append);
///    anything else = human-readable to stderr.
pub fn init() {
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let log_mode = std::env::var("RUSTFRP_LOG_MODE").unwrap_or_default();

    if log_mode == "json" {
        let log_dir = dirs::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join(".rustfrp")
            .join("logs");

        // Best-effort: if we can't create the log dir or open the file,
        // fall back to console so the user sees what's happening.
        match create_log_file(&log_dir) {
            Ok(log_file) => {
                tracing_subscriber::fmt()
                    .json()
                    .with_env_filter(env_filter)
                    .with_writer(Mutex::new(log_file))
                    .init();
            }
            Err(e) => {
                // Fallback: console output so the startup error is visible
                tracing_subscriber::fmt()
                    .with_env_filter(env_filter)
                    .init();
                tracing::warn!(
                    dir = %log_dir.display(),
                    error = %e,
                    "Cannot open JSON log file, falling back to console logging"
                );
            }
        }
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .init();
    }
}

/// Create (or open for append) the log file, ensuring the directory exists.
fn create_log_file(log_dir: &std::path::Path) -> std::io::Result<std::fs::File> {
    std::fs::create_dir_all(log_dir)?;
    OpenOptions::new()
        .append(true)
        .create(true)
        .open(log_dir.join("rustfrp.log"))
}
