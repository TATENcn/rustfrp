//! RustFRP daemon binary entry point
//!
//! The daemon wraps frpc with configuration management,
//! TOML generation, process supervision, and (optionally) an HTTP API server.

use clap::Parser;
use rustfrp_client::core::ClientCore;
use rustfrp_client::db::default_db_path;

/// RustFRP daemon — frpc wrapper with optional HTTP API
#[derive(Parser, Debug)]
#[command(name = "rustfrp-daemon", version, about)]
struct Cli {
    /// Database path (default: ~/.rustfrp/config.db)
    #[arg(long)]
    db_path: Option<String>,

    /// Config output directory for generated frpc TOML files
    #[arg(long, default_value = "~/.rustfrp/runtime")]
    config_dir: String,

    /// HTTP API listen address (default: 127.0.0.1:7900)
    #[cfg(feature = "http-api")]
    #[arg(long, default_value = "127.0.0.1:7900")]
    api_listen: String,

    /// API token for bearer authentication (falls back to RUSTFRP_API_TOKEN)
    #[cfg(feature = "http-api")]
    #[arg(long)]
    api_token: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // CODE-004: production (RUSTFRP_LOG_MODE=json) → JSON to file;
    // development (default) → human-readable to console.
    rustfrp_common::logging::init();

    // Install panic hook for crash reporting
    rustfrp_common::panic_hook::install();

    let cli = Cli::parse();

    let db_path = cli
        .db_path
        .unwrap_or_else(|| default_db_path().to_string_lossy().to_string());

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        db_path = %db_path,
        config_dir = %cli.config_dir,
        "RustFRP daemon starting"
    );

    let core = ClientCore::new(&db_path, &cli.config_dir).await?;

    #[cfg(feature = "http-api")]
    {
        let api_token = cli.api_token.or_else(|| {
            std::env::var("RUSTFRP_API_TOKEN")
                .ok()
                .filter(|token| !token.is_empty())
        });

        match api_token {
            Some(token) => {
                rustfrp_daemon::serve_with_auth(core, &cli.api_listen, &token).await?;
            }
            None => {
                rustfrp_daemon::serve(core, &cli.api_listen).await?;
            }
        }
    }

    #[cfg(not(feature = "http-api"))]
    {
        core.run().await?;
    }

    Ok(())
}
