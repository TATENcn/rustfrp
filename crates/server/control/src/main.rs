//! RustFRP control server
//!
//! Pull mode collects /metrics from FRPS nodes, provides Web dashboard
//! and config template API for frps-agent nodes.
//! Absolutely read-only, absolutely stateless (ARCH-007).

mod health;
mod scraper;
mod web;

use clap::Parser;
use std::net::SocketAddr;

/// RustFRP control server
#[derive(Parser, Debug)]
#[command(name = "rustfrp-control", version, about)]
struct Cli {
    #[arg(long, default_value = "0.0.0.0:3000")]
    bind: String,

    #[arg(long, default_value = "15")]
    scrap_interval: u64,

    #[arg(long, default_value = "3")]
    scrap_timeout: u64,

    #[arg(long, default_value = "targets.json")]
    targets: String,

    /// Read-only directory containing <node-id>.toml agent templates.
    #[arg(long)]
    templates_dir: Option<std::path::PathBuf>,

    /// Environment variable containing the bearer token required by agents.
    #[arg(long, default_value = "RUSTFRP_AGENT_TOKEN")]
    agent_token_env: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    rustfrp_common::logging::init();

    let cli = Cli::parse();

    let addr: SocketAddr = cli
        .bind
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid bind address '{}': {e}", cli.bind))?;

    let targets = health::TargetRegistry::from_file(&cli.targets)?;

    tracing::info!(count = targets.len().await, "Target nodes loaded");

    let scraper = scraper::Scraper::new(
        targets.clone(),
        std::time::Duration::from_secs(cli.scrap_interval),
        std::time::Duration::from_secs(cli.scrap_timeout),
    )?;
    scraper.start().await;

    let agent_token = if cli.templates_dir.is_some() {
        let token = std::env::var(&cli.agent_token_env).map_err(|_| {
            anyhow::anyhow!(
                "templates are enabled but {} is not set",
                cli.agent_token_env
            )
        })?;
        anyhow::ensure!(!token.is_empty(), "agent token must not be empty");
        Some(token)
    } else {
        None
    };
    let router = web::create_router(targets, cli.templates_dir, agent_token);
    let listener = tokio::net::TcpListener::bind(addr).await?;

    tracing::info!("Web service started: http://{addr}");

    axum::serve(listener, router).await?;
    Ok(())
}
