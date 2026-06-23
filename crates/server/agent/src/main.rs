//! RustFRP agent binary entry point
//!
//! The agent is an optional component deployed alongside frps on server nodes.
//! It pulls configuration templates from the control server,
//! generates frps.toml locally, manages the frps child process,
//! and provides a plugin runtime for server-side extensions.
//!
//! Per ADR-004: agent crash does not affect frps (frps continues with last TOML).
//! Per ADR-005: agent pulls config from control plane (no push).

use clap::Parser;

/// RustFRP agent — frps wrapper
#[derive(Parser, Debug)]
#[command(name = "rustfrp-agent", version, about)]
struct Cli {
    /// Control server URL for config template pull
    #[arg(long, default_value = "http://localhost:3000")]
    control_url: String,

    /// frps config output directory
    #[arg(long, default_value = "~/.rustfrp/agent/runtime")]
    config_dir: String,

    /// Pull interval in seconds
    #[arg(long, default_value = "60")]
    pull_interval: u64,
}

fn main() {
    rustfrp_common::logging::init();

    let cli = Cli::parse();

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        control_url = cli.control_url,
        "RustFRP agent starting"
    );

    // Install panic hook for crash reporting
    rustfrp_common::panic_hook::install();

    tracing::info!(
        config_dir = cli.config_dir,
        pull_interval = cli.pull_interval,
        "Agent initialized (full implementation pending)"
    );

    // TODO: Pull config from control server, generate frps.toml,
    //       manage frps process, load plugins.
    //       This will be implemented in Phase 3-4.
}
