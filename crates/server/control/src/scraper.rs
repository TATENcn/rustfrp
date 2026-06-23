//! /metrics scraper
//!
//! Periodically pulls /metrics from FRPS nodes.
//! Timeout 3s, consecutive failures → circuit breaker (PERF-004).

use crate::health::{NodeState, TargetRegistry};
use prometheus::{Encoder, TextEncoder};
use std::sync::Arc;
use std::time::Duration;

/// Metrics scraper
pub struct Scraper {
    /// Target node registry (shared state)
    targets: Arc<TargetRegistry>,
    /// Polling interval
    interval: Duration,
    /// Single pull timeout
    timeout: Duration,
    /// Circuit breaker threshold: consecutive failures before reducing frequency
    circuit_breaker_threshold: u32,
    /// Reusable HTTP client (connection pooling)
    client: reqwest::Client,
}

impl std::fmt::Debug for Scraper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Scraper")
            .field("interval", &self.interval)
            .field("timeout", &self.timeout)
            .field("circuit_breaker_threshold", &self.circuit_breaker_threshold)
            .finish_non_exhaustive()
    }
}

impl Scraper {
    /// Create a new scraper
    ///
    /// Returns an error if the HTTP client cannot be initialised (e.g. TLS backend failure).
    pub fn new(
        targets: Arc<TargetRegistry>,
        interval: Duration,
        timeout: Duration,
    ) -> Result<Self, anyhow::Error> {
        let client = reqwest::Client::builder().timeout(timeout).build()?;
        Ok(Self {
            targets,
            interval,
            timeout,
            circuit_breaker_threshold: 5,
            client,
        })
    }

    /// Start the scraping loop
    pub async fn start(&self) {
        let targets = self.targets.clone();
        let interval = self.interval;
        let threshold = self.circuit_breaker_threshold;
        let client = self.client.clone();

        tokio::spawn(async move {
            let mut tick = tokio::time::interval(interval);

            loop {
                tick.tick().await;
                let nodes = targets.list_nodes().await;

                for node in nodes {
                    let node_id = node.node.id.clone();
                    let url = node.node.metrics_url.clone();

                    // Circuit breaker: reduce frequency after consecutive failures
                    if node.consecutive_failures >= threshold
                        && node.consecutive_failures % (threshold * 2) != 0
                    {
                        continue;
                    }

                    let result = fetch_metrics(&client, &url).await;

                    match result {
                        Ok(body) => {
                            targets
                                .update_state(
                                    &node_id,
                                    NodeState::Healthy {
                                        last_scrap: chrono::Utc::now(),
                                        metrics_body: body,
                                    },
                                )
                                .await;
                        }
                        Err(e) => {
                            tracing::warn!(
                                node = %node_id,
                                error = %e,
                                failures = node.consecutive_failures + 1,
                                "Metrics pull failed"
                            );
                            targets
                                .update_state(
                                    &node_id,
                                    NodeState::Unhealthy {
                                        last_error: e.to_string(),
                                        consecutive_failures: node.consecutive_failures + 1,
                                    },
                                )
                                .await;
                        }
                    }
                }
            }
        });
    }
}

/// Fetch /metrics from a node using the shared client
async fn fetch_metrics(client: &reqwest::Client, url: &str) -> Result<String, anyhow::Error> {
    let response = client.get(url).send().await?;

    if !response.status().is_success() {
        anyhow::bail!("HTTP {}", response.status());
    }

    let body = response.text().await?;
    Ok(body)
}

/// Prometheus metrics registry
#[derive(Debug, Default)]
#[allow(dead_code)]
pub struct MetricsRegistry {
    registry: prometheus::Registry,
}

#[allow(dead_code)]
impl MetricsRegistry {
    pub fn new() -> Self {
        Self {
            registry: prometheus::Registry::new(),
        }
    }

    /// Export all metrics as Prometheus text format
    pub fn export_text(&self) -> Result<String, anyhow::Error> {
        let metric_families = self.registry.gather();
        let mut buffer = Vec::new();
        let encoder = TextEncoder::new();
        encoder.encode(&metric_families, &mut buffer)?;
        Ok(String::from_utf8(buffer)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_registry_export() {
        let registry = MetricsRegistry::new();
        let text = registry.export_text().unwrap();
        assert!(text.is_empty() || text.contains("# TYPE") || text.is_empty());
    }
}
