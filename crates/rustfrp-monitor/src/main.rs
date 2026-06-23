//! RustFRP central monitoring server
//!
//! Pull mode collects /metrics from FRPS nodes, provides Web dashboard.
//! Absolutely read-only, absolutely stateless (ARCH-007).

mod health;
mod scraper;
mod web;

use clap::Parser;
use std::net::SocketAddr;
use tracing_subscriber::EnvFilter;

/// RustFRP monitoring server
#[derive(Parser, Debug)]
#[command(name = "rustfrp-monitor", version, about)]
struct Cli {
    #[arg(long, default_value = "0.0.0.0:3000")]
    bind: String,

    #[arg(long, default_value = "15")]
    scrap_interval: u64,

    #[arg(long, default_value = "3")]
    scrap_timeout: u64,

    #[arg(long, default_value = "targets.json")]
    targets: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    let addr: SocketAddr = cli.bind.parse()
        .map_err(|e| anyhow::anyhow!("Invalid bind address '{}': {e}", cli.bind))?;

    let targets = health::TargetRegistry::from_file(&cli.targets)?;

    tracing::info!(count = targets.len().await, "Target nodes loaded");

    let scraper = scraper::Scraper::new(
        targets.clone(),
        std::time::Duration::from_secs(cli.scrap_interval),
        std::time::Duration::from_secs(cli.scrap_timeout),
    );
    scraper.start().await;

    let router = web::create_router(targets);
    let listener = tokio::net::TcpListener::bind(addr).await?;

    tracing::info!("Web service started: http://{addr}");

    axum::serve(listener, router).await?;
    Ok(())
}
