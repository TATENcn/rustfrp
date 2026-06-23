//! Cross-platform signal handling
//!
//! Handles SIGINT / SIGTERM for graceful shutdown.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Signal handler
///
/// Listens for system signals and provides `is_shutdown_requested()` for other modules.
#[derive(Clone)]
pub struct SignalHandler {
    shutdown_requested: Arc<AtomicBool>,
}

impl SignalHandler {
    /// Create a signal handler and begin listening
    ///
    /// Handles SIGINT (Ctrl+C) and SIGTERM.
    pub fn new() -> Self {
        let shutdown_requested = Arc::new(AtomicBool::new(false));
        let flag = shutdown_requested.clone();

        tokio::spawn(async move {
            #[cfg(unix)]
            {
                use tokio::signal::unix::{signal, SignalKind};

                let mut sigterm = match signal(SignalKind::terminate()) {
                    Ok(s) => Some(s),
                    Err(e) => {
                        tracing::warn!("Failed to register SIGTERM handler: {e}");
                        None
                    }
                };
                let mut sigint = match signal(SignalKind::interrupt()) {
                    Ok(s) => Some(s),
                    Err(e) => {
                        tracing::warn!("Failed to register SIGINT handler: {e}");
                        None
                    }
                };

                // At least one signal handler must be registered
                if sigterm.is_none() && sigint.is_none() {
                    tracing::error!("No signal handlers registered, shutting down");
                    flag.store(true, Ordering::SeqCst);
                    return;
                }

                // Wait for whichever signal arrives first.
                // A signal may be None if registration failed; skip it.
                tokio::select! {
                    _ = async {
                        match sigterm.as_mut() {
                            Some(s) => s.recv().await,
                            None => std::future::pending().await,
                        }
                    } => {
                        tracing::info!("Received SIGTERM, preparing graceful shutdown");
                    }
                    _ = async {
                        match sigint.as_mut() {
                            Some(s) => s.recv().await,
                            None => std::future::pending().await,
                        }
                    } => {
                        tracing::info!("Received SIGINT, preparing graceful shutdown");
                    }
                }
            }

            #[cfg(not(unix))]
            {
                use tokio::signal;
                match signal::ctrl_c().await {
                    Ok(()) => {
                        tracing::info!("Received Ctrl+C, preparing graceful shutdown");
                    }
                    Err(e) => {
                        tracing::error!("Failed to register Ctrl+C handler: {e}");
                    }
                }
            }

            flag.store(true, Ordering::SeqCst);
        });

        Self { shutdown_requested }
    }

    /// Whether a shutdown has been requested
    pub fn is_shutdown_requested(&self) -> bool {
        self.shutdown_requested.load(Ordering::SeqCst)
    }
}

impl Default for SignalHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for SignalHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SignalHandler")
            .field("shutdown_requested", &self.shutdown_requested)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_signal_handler_default_state() {
        let handler = SignalHandler::new();
        assert!(!handler.is_shutdown_requested());
    }
}
