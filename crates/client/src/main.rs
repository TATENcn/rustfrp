//! RustFRP client binary entry point
//!
//! The client wraps frpc with configuration management,
//! TOML generation, and process supervision.
//!
//! Future: Tauri GUI integration.

use clap::Parser;

// PERF-001: use mimalloc on ARM (routers), jemalloc on x86_64 (servers).
// Activate via: `cargo build --features mimalloc-dep` (ARM) or `--features jemalloc` (x86_64).
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

    /// Config output directory
    #[arg(long, default_value = "~/.rustfrp/runtime")]
    config_dir: String,
}

fn main() {
    // CODE-004: production (RUSTFRP_LOG_MODE=json) → JSON to file;
    // development (default) → human-readable to console.
    rustfrp_common::logging::init();


    let cli = Cli::parse();

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        "RustFRP client starting"
    );

    // Install panic hook for crash reporting
    rustfrp_common::panic_hook::install();

    tracing::info!(
        db_path = cli.db_path,
        config_dir = cli.config_dir,
        "Client initialized (full implementation pending)"
    );

    // TODO: Initialize database, load plugins, generate TOML, start frpc
    // This will be implemented when the GUI or daemon mode is added.
}
