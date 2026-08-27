//! RustFRP agent binary entry point.

use anyhow::{Context, Result};
use clap::Parser;
use rustfrp_agent::config::{AppliedConfig, ConfigStore};
use rustfrp_agent::pull::{ConfigPuller, PullResult};
use rustfrp_agent::supervisor::FrpsSupervisor;
use std::path::{Path, PathBuf};
use tokio::time::{sleep, Duration};

#[derive(Parser, Debug)]
#[command(name = "rustfrp-agent", version, about)]
struct Cli {
    #[arg(long)]
    node_id: String,
    #[arg(long, default_value = "http://localhost:3000")]
    control_url: String,
    #[arg(long, default_value = "~/.rustfrp/agent/runtime")]
    runtime_dir: String,
    #[arg(long, default_value_t = 60)]
    pull_interval: u64,
    /// Environment variable containing the control-plane bearer token.
    #[arg(long, default_value = "RUSTFRP_AGENT_TOKEN")]
    token_env: String,
    /// Explicit frps binary; otherwise a verified official binary is managed.
    #[arg(long)]
    frps_path: Option<PathBuf>,
    #[arg(long)]
    frp_version: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    rustfrp_common::logging::init();
    rustfrp_common::panic_hook::install();
    let cli = Cli::parse();
    anyhow::ensure!(
        cli.pull_interval > 0,
        "pull interval must be greater than zero"
    );

    let runtime_dir = expand_tilde(&cli.runtime_dir);
    let token = std::env::var(&cli.token_env).with_context(|| {
        format!(
            "required agent token environment variable {} is not set",
            cli.token_env
        )
    })?;
    anyhow::ensure!(!token.is_empty(), "agent token must not be empty");
    let puller = ConfigPuller::new(&cli.control_url, &cli.node_id, token)?;
    let store = ConfigStore::new(runtime_dir.clone());

    let frps = match cli.frps_path {
        Some(path) => path,
        None => rustfrp_bin::ensure::ensure_binary("frps", cli.frp_version.as_deref(), None, None)
            .await
            .context("ensure verified frps binary")?,
    };
    let mut supervisor = FrpsSupervisor::new(frps, runtime_dir);

    let mut etag = None;
    let mut active = match pull_and_apply(&puller, &store, &supervisor, None).await {
        Ok((config, next_etag)) => {
            etag = next_etag;
            config
        }
        Err(error) => {
            tracing::warn!(%error, "Initial control-plane pull failed; trying last valid cache");
            store
                .load_cached()
                .await?
                .context("no valid cached frps configuration is available")?
        }
    };
    supervisor.verify_config(&active.path).await?;
    match supervisor.ensure_running(&active.path).await {
        Ok(outcome) => {
            if active.changed && outcome.adopted {
                supervisor.restart(&active.path).await?;
            }
            store.finalize().await;
        }
        Err(error) => {
            tracing::error!(%error, "Failed to start pulled configuration; rolling back");
            if let Some(previous) = store.rollback().await? {
                supervisor.ensure_running(&previous.path).await?;
                active = previous;
            } else {
                return Err(error);
            }
        }
    }

    tracing::info!(node_id = cli.node_id, pid = ?supervisor.pid(), config = %active.path.display(), "RustFRP agent is supervising frps");

    let mut pulls = tokio::time::interval(Duration::from_secs(cli.pull_interval));
    pulls.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    pulls.tick().await;
    let mut restarts = 0_u32;

    loop {
        tokio::select! {
            _ = shutdown_signal() => {
                tracing::info!(pid = ?supervisor.pid(), "Agent stopping; leaving frps running per ADR-004");
                return Ok(());
            }
            _ = pulls.tick() => {
                match pull_and_apply(&puller, &store, &supervisor, etag.as_deref()).await {
                    Ok((candidate, next_etag)) => {
                        etag = next_etag.or(etag);
                        if candidate.changed {
                            match supervisor.restart(&candidate.path).await {
                                Ok(pid) => {
                                    active = candidate;
                                    store.finalize().await;
                                    restarts = 0;
                                    tracing::info!(pid, digest = %active.digest, "Applied new frps configuration");
                                }
                                Err(error) => {
                                    tracing::error!(%error, "Failed to restart frps after configuration update; rolling back");
                                    if let Some(previous) = store.rollback().await? {
                                        supervisor.ensure_running(&previous.path).await?;
                                        active = previous;
                                    }
                                }
                            }
                        }
                    }
                    Err(error) => tracing::warn!(%error, "Config pull rejected; retaining last valid configuration"),
                }
            }
            _ = sleep(Duration::from_millis(500)) => {
                if let Some(exit_code) = supervisor.poll_exit().await? {
                    if restarts >= 3 {
                        anyhow::bail!("frps exited with code {exit_code}; restart budget exhausted");
                    }
                    let delay = 1_u64 << restarts;
                    restarts += 1;
                    tracing::warn!(exit_code, attempt = restarts, delay, "frps exited; scheduling restart");
                    sleep(Duration::from_secs(delay)).await;
                    supervisor.ensure_running(&active.path).await?;
                }
            }
        }
    }
}

async fn pull_and_apply(
    puller: &ConfigPuller,
    store: &ConfigStore,
    supervisor: &FrpsSupervisor,
    etag: Option<&str>,
) -> Result<(AppliedConfig, Option<String>)> {
    match puller.pull(etag).await? {
        PullResult::NotModified => {
            let cached = store
                .load_cached()
                .await?
                .context("control returned not-modified without a local cache")?;
            Ok((cached, etag.map(str::to_owned)))
        }
        PullResult::Updated { contents, etag } => {
            let staged = store.stage(&contents).await?;
            if staged.changed {
                if let Err(error) = supervisor.verify_config(&staged.path).await {
                    store.discard(&staged).await;
                    return Err(error);
                }
            }
            Ok((store.commit(staged).await?, etag))
        }
    }
}

fn expand_tilde(path: &str) -> PathBuf {
    if path == "~" {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    }
    if let Some(relative) = path.strip_prefix("~/") {
        return dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(relative);
    }
    Path::new(path).to_path_buf()
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("install SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {},
            _ = terminate.recv() => {},
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_home_prefix_only() {
        assert!(!expand_tilde("~/agent").starts_with("~"));
        assert_eq!(
            expand_tilde("relative/path"),
            PathBuf::from("relative/path")
        );
    }
}
