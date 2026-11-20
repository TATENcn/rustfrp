//! Desktop session — the daemon lifecycle used by the Tauri shell.
//!
//! The shell must not reimplement configuration management: it starts the same
//! `ClientCore` + axum stack the CLI daemon runs, then points a webview at the
//! bound loopback port. Keeping this orchestration here (instead of in the Tauri
//! crate) means it is testable without GTK/WebKit present.
//!
//! Startup order matters:
//!
//! 1. claim the data directory (one owner of the frpc children),
//! 2. open SQLite and the process manager,
//! 3. bind a loopback listener and learn its port,
//! 4. start serving with a freshly generated bearer token.
//!
//! The token exists only in memory and in the webview's `localStorage`, never in
//! the URL or on disk: any local process can reach a loopback port, so the token
//! is the only thing standing between another process and `/api/v1/config/export`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use rustfrp_client::core::ClientCore;
use tokio::net::TcpListener;
use tokio::sync::watch;
use tokio::task::JoinHandle;

use crate::api::{bind_api_listener, serve_with_listener, BearerToken};
use crate::instance::{InstanceLock, InstanceLockError};

/// How long to wait for the graceful shutdown to finish before giving up.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(15);

/// Listener address for a packaged build: an OS-assigned loopback port.
pub const EPHEMERAL_LOOPBACK: &str = "127.0.0.1:0";

/// Fixed loopback address used by `tauri dev`, where the Vite dev server proxies
/// `/api` to it (see `plugins/webui/vite.config.ts`).
pub const DEV_LOOPBACK: &str = "127.0.0.1:7900";

/// Everything the session needs to boot.
#[derive(Debug, Clone)]
pub struct DesktopOptions {
    /// Directory holding `config.db` and `instance.lock`.
    pub data_dir: PathBuf,
    /// Directory receiving generated `frpc.toml` files.
    pub config_dir: PathBuf,
    /// Address to bind; see [`EPHEMERAL_LOOPBACK`] and [`DEV_LOOPBACK`].
    pub listen: String,
    /// Explicit `frpc` binary, bypassing version management.
    pub frpc_path: Option<PathBuf>,
}

impl DesktopOptions {
    /// Options for `~/.rustfrp` with an OS-assigned loopback API port.
    ///
    /// The desktop shell passes its own paths; this exists so tests and
    /// embedders have one definition instead of repeating the defaults.
    pub fn with_data_dir(data_dir: impl Into<PathBuf>, listen: impl Into<String>) -> Self {
        let data_dir = data_dir.into();
        Self {
            config_dir: data_dir.join("runtime"),
            data_dir,
            listen: listen.into(),
            frpc_path: None,
        }
    }
}

/// Errors that stop a desktop session from starting.
#[derive(Debug, thiserror::Error)]
pub enum DesktopError {
    /// Another RustFRP process owns the data directory.
    #[error(transparent)]
    InstanceLock(#[from] InstanceLockError),

    /// SQLite, the process manager, or the listener could not be prepared.
    #[error(transparent)]
    Core(#[from] anyhow::Error),
}

/// A running desktop session.
///
/// Dropping it without calling [`DesktopSession::shutdown`] leaves frpc children
/// running; the Tauri shell always shuts down explicitly on exit.
pub struct DesktopSession {
    data_dir: PathBuf,
    token: String,
    url: String,
    /// Address the certificate push endpoint is listening on, if it started.
    ingest: Option<String>,
    shutdown: rustfrp_common::signal::SignalHandler,
    server: JoinHandle<anyhow::Result<()>>,
    // Flipped to `true` when the server task ends, however it ends. The shell
    // uses this to close the window instead of leaving a dead UI on screen.
    stopped: watch::Receiver<bool>,
    // Held for the session's lifetime; released when this struct drops.
    _lock: InstanceLock,
}

impl DesktopSession {
    /// Claim the data directory and start the embedded API server.
    pub async fn start(options: DesktopOptions) -> Result<Self, DesktopError> {
        let lock = InstanceLock::acquire(&options.data_dir)?;

        let core = ClientCore::new_with_frpc(
            options
                .data_dir
                .join("config.db")
                .to_str()
                .unwrap_or("config.db"),
            options.config_dir.to_str().unwrap_or("runtime"),
            options.frpc_path,
            false,
        )
        .await
        .map_err(anyhow::Error::from)?;

        // Cloned before the core moves into the server task: this is the handle
        // that turns a tray "Quit" into the normal teardown path.
        let shutdown = core.signal_handler().clone();
        let token = generate_token();

        let listener: TcpListener = bind_api_listener(&options.listen).await?;
        let url = format!(
            "http://{}",
            listener.local_addr().map_err(anyhow::Error::from)?
        );

        // The push endpoint is started here too: a desktop install is the case
        // where Certd has no other way to reach the machine, and the endpoint is
        // the same three-route surface with the same token check.
        let ingest_state = crate::cert_ingest::IngestState {
            db: core.db().clone(),
        };
        let ingest_server = crate::cert_ingest::IngestServer::new(ingest_state.clone());
        let ingest = match rustfrp_client::db::settings::IngestConfig::load(&ingest_state.db).await
        {
            Ok(config) => match ingest_server.apply(&config).await {
                Ok(address) => address,
                Err(error) => {
                    tracing::warn!(%error, "certificate ingest endpoint is not listening");
                    None
                }
            },
            Err(error) => {
                tracing::warn!(%error, "could not read ingest settings");
                None
            }
        };

        let auth = BearerToken::new(token.clone());
        let (stop_tx, stopped) = watch::channel(false);
        let server = tokio::spawn(async move {
            let result = serve_with_listener(core, listener, auth, "desktop bearer token").await;
            // Announce the stop even if the server errored: a dead backend means
            // the window has nothing left to talk to.
            let _ = stop_tx.send(true);
            result
        });

        tracing::info!(
            %url,
            data_dir = %options.data_dir.display(),
            ingest = ingest.as_deref().unwrap_or("none"),
            "desktop session started"
        );

        Ok(Self {
            data_dir: options.data_dir,
            token,
            url,
            ingest,
            shutdown,
            server,
            stopped,
            _lock: lock,
        })
    }

    /// Base URL of the embedded UI and API, e.g. `http://127.0.0.1:51234`.
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Bearer token for API calls from the webview.
    pub fn token(&self) -> &str {
        &self.token
    }

    /// Address the certificate push endpoint listens on, once it is up.
    ///
    /// A desktop user needs this to paste into Certd; it is loopback unless the
    /// stored settings were changed to expose the endpoint.
    pub fn ingest_address(&self) -> Option<&str> {
        self.ingest.as_deref()
    }

    /// Data directory (`config.db`, `logs/`, `frp/`).
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// Script injected before the webview loads the UI.
    ///
    /// The token reaches the page through `localStorage` only, so it never
    /// appears in the URL, history, or referer.
    pub fn initialization_script(&self) -> String {
        format!(
            "localStorage.setItem('api_token', {token}); window.__RUSTFRP_DESKTOP__ = true;",
            token = serde_json::to_string(&self.token).expect("token is a plain string"),
        )
    }

    /// Resolves once the embedded API server has stopped, whatever the reason
    /// (own shutdown, a signal delivered to the process, or a server error).
    ///
    /// The desktop shell watches this so a daemon that dies never leaves an
    /// unusable window behind.
    pub fn stopped(&self) -> watch::Receiver<bool> {
        self.stopped.clone()
    }

    /// Ask the core to stop, then wait for it to stop frpc children.
    ///
    /// Uses the signal handler rather than a real signal: sending SIGTERM to
    /// ourselves would also kill the webview before teardown completes.
    pub async fn shutdown(self) {
        self.shutdown.request_shutdown();
        match tokio::time::timeout(SHUTDOWN_GRACE, self.server).await {
            Ok(Ok(Err(error))) => tracing::error!(%error, "desktop API server stopped with error"),
            Ok(Err(error)) => tracing::error!(%error, "desktop API server task panicked"),
            Ok(Ok(Ok(()))) => tracing::info!("desktop session stopped"),
            Err(_) => tracing::error!(
                grace_secs = SHUTDOWN_GRACE.as_secs(),
                "desktop session did not stop in time; frpc children may still be running"
            ),
        }
    }
}

/// 256 bits of randomness, hex-encoded: two v4 UUIDs.
///
/// `Uuid::new_v4` draws from the OS CSPRNG, which is the only requirement here —
/// this is a bearer secret for a loopback port, not a persisted credential.
fn generate_token() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn session(dir: &Path) -> DesktopSession {
        DesktopSession::start(DesktopOptions {
            data_dir: dir.join("data"),
            config_dir: dir.join("runtime"),
            listen: EPHEMERAL_LOOPBACK.to_owned(),
            frpc_path: Some(PathBuf::from("/nonexistent/frpc")),
        })
        .await
        .expect("desktop session starts without network access")
    }

    #[tokio::test]
    async fn session_serves_the_ui_and_requires_the_generated_token() {
        let dir = tempfile::tempdir().unwrap();
        let session = session(dir.path()).await;

        assert!(session.url().starts_with("http://127.0.0.1:"));
        assert!(
            !session.url().contains(session.token()),
            "token must not leak into the URL"
        );
        assert!(session.token().len() >= 64, "token should be 256-bit hex");

        let client = reqwest::Client::new();
        let anonymous = client
            .get(format!("{}/api/v1/status", session.url()))
            .send()
            .await
            .unwrap();
        assert_eq!(anonymous.status(), reqwest::StatusCode::UNAUTHORIZED);

        let authorized = client
            .get(format!("{}/api/v1/status", session.url()))
            .bearer_auth(session.token())
            .send()
            .await
            .unwrap();
        assert_eq!(authorized.status(), reqwest::StatusCode::OK);

        session.shutdown().await;
    }

    #[tokio::test]
    async fn initialization_script_hands_the_token_to_the_page() {
        let dir = tempfile::tempdir().unwrap();
        let session = session(dir.path()).await;
        let script = session.initialization_script();
        assert!(script.contains("localStorage.setItem('api_token'"));
        assert!(script.contains(session.token()));
        assert!(script.contains("__RUSTFRP_DESKTOP__"));
        session.shutdown().await;
    }

    #[tokio::test]
    async fn stop_signal_fires_when_the_server_ends_on_its_own() {
        let dir = tempfile::tempdir().unwrap();
        let session = session(dir.path()).await;
        let mut stopped = session.stopped();
        assert!(!*stopped.borrow(), "not stopped before anything happens");

        // Mimic a signal arriving from outside the process (service manager,
        // `kill`): the client core run loop exits, the select arm finishes, and
        // the shell must learn about it.
        session.shutdown.request_shutdown();

        tokio::time::timeout(SHUTDOWN_GRACE, stopped.changed())
            .await
            .expect("the stop signal must fire once the server ends")
            .expect("sender is alive until the task returns");
        assert!(*stopped.borrow());

        session.shutdown().await;
    }

    #[tokio::test]
    async fn second_session_on_the_same_data_directory_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let first = session(dir.path()).await;

        let error = DesktopSession::start(DesktopOptions {
            data_dir: dir.path().join("data"),
            config_dir: dir.path().join("runtime2"),
            listen: EPHEMERAL_LOOPBACK.to_owned(),
            frpc_path: Some(PathBuf::from("/nonexistent/frpc")),
        })
        .await
        .err()
        .expect("the data directory is already claimed");

        assert!(matches!(
            error,
            DesktopError::InstanceLock(InstanceLockError::AlreadyLocked(_))
        ));

        first.shutdown().await;
    }
}
