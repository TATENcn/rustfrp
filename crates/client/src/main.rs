//! RustFRP client binary entry point
//!
//! The client wraps frpc with configuration management,
//! TOML generation, and process supervision.

use clap::Parser;
use rustfrp_client::core::ClientCore;
use rustfrp_client::db::default_db_path;

// PERF-001: use mimalloc on ARM (routers), jemalloc on x86_64 (servers).
#[cfg(feature = "jemalloc")]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

#[cfg(feature = "mimalloc-dep")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

/// RustFRP client — frpc wrapper
#[derive(Parser, Debug)]
#[command(name = "rustfrp-client", version, about)]
struct Cli {
    /// Database path (default: ~/.rustfrp/config.db)
    #[arg(long)]
    db_path: Option<String>,

    /// Config output directory for generated frpc TOML files
    #[arg(long, default_value = "~/.rustfrp/runtime")]
    config_dir: String,
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
        "RustFRP client starting"
    );

    let core = ClientCore::new(&db_path, &cli.config_dir).await?;
    core.run().await?;

    Ok(())
}
