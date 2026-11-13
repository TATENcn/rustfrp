//! Per-proxy live status taken from the frpc admin API.
//!
//! `GET /api/status` on a profile's `[webServer]` listener is the only source
//! of truth for whether frps actually accepted a proxy: the daemon otherwise
//! only knows whether the frpc process is alive. A proxy can be "running" from
//! the process' point of view while frps rejected it (port taken, feature
//! disabled server-side, invalid domain).

use axum::extract::{Extension, Path, State};
use axum::Json;
use rustfrp_client::db::{admin_endpoint::AdminEndpoint, Database};
use serde::Serialize;
use std::time::Duration;

use super::auth::AuthIdentity;
use super::profiles::ensure_profile_tenant;
use super::response::ApiResponse;
use super::state::ApiState;

/// Timeout for the admin status request.
const STATUS_TIMEOUT_SECS: u64 = 3;

/// One proxy entry as reported by frpc.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ProxyStatus {
    /// Proxy type bucket as reported by frpc (`tcp`, `http`, ...).
    pub proxy_type: String,
    pub name: String,
    /// `running`, `wait start`, `start error`, ...
    pub status: String,
    /// frpc's error text when the proxy did not start.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub error: String,
    pub local_addr: String,
    pub remote_addr: String,
}

/// Collection of proxy statuses for one profile.
#[derive(Debug, Clone, Serialize)]
pub struct ProxyStatusReport {
    pub profile_id: i64,
    /// False when the admin endpoint could not be queried.
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub proxies: Vec<ProxyStatus>,
}

impl ProxyStatusReport {
    fn unavailable(profile_id: i64, error: impl Into<String>) -> Self {
        Self {
            profile_id,
            available: false,
            error: Some(error.into()),
            proxies: Vec::new(),
        }
    }
}

/// Query `/api/status` for a profile using its stored admin credentials.
///
/// Returns an `available: false` report rather than an error when the endpoint
/// is unreachable: a missing status view must never fail the caller's request.
pub async fn fetch_proxy_status(db: &Database, profile_id: i64) -> ProxyStatusReport {
    let endpoint = match db.stored_admin_endpoint(profile_id).await {
        Ok(Some(endpoint)) => endpoint,
        Ok(None) => {
            return ProxyStatusReport::unavailable(
                profile_id,
                "no frpc admin endpoint assigned yet",
            );
        }
        Err(error) => return ProxyStatusReport::unavailable(profile_id, error.to_string()),
    };

    fetch_from_endpoint(profile_id, &endpoint).await
}

async fn fetch_from_endpoint(profile_id: i64, endpoint: &AdminEndpoint) -> ProxyStatusReport {
    let url = format!("http://127.0.0.1:{}/api/status", endpoint.port);
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(STATUS_TIMEOUT_SECS))
        .build()
    {
        Ok(client) => client,
        Err(error) => return ProxyStatusReport::unavailable(profile_id, error.to_string()),
    };

    let response = match client
        .get(&url)
        .basic_auth("rustfrp", Some(&endpoint.password))
        .send()
        .await
    {
        Ok(response) => response,
        Err(error) => return ProxyStatusReport::unavailable(profile_id, error.to_string()),
    };

    if !response.status().is_success() {
        return ProxyStatusReport::unavailable(
            profile_id,
            format!("frpc admin API returned HTTP {}", response.status()),
        );
    }

    match response.json::<serde_json::Value>().await {
        Ok(body) => ProxyStatusReport {
            profile_id,
            available: true,
            error: None,
            proxies: parse_status_body(&body),
        },
        Err(error) => ProxyStatusReport::unavailable(profile_id, error.to_string()),
    }
}

/// Flatten `{"tcp":[{...}], "http":[{...}]}` into a single list.
fn parse_status_body(body: &serde_json::Value) -> Vec<ProxyStatus> {
    let Some(buckets) = body.as_object() else {
        return Vec::new();
    };
    let mut proxies = Vec::new();
    for (proxy_type, entries) in buckets {
        let Some(entries) = entries.as_array() else {
            continue;
        };
        for entry in entries {
            let field = |name: &str| {
                entry
                    .get(name)
                    .and_then(|value| value.as_str())
                    .unwrap_or_default()
                    .to_owned()
            };
            proxies.push(ProxyStatus {
                proxy_type: proxy_type.clone(),
                name: field("name"),
                status: field("status"),
                error: field("err"),
                local_addr: field("local_addr"),
                remote_addr: field("remote_addr"),
            });
        }
    }
    proxies.sort_by(|left, right| left.name.cmp(&right.name));
    proxies
}

/// `GET /api/v1/profiles/{id}/proxy-status` — per-proxy state from frpc.
pub async fn profile_proxy_status(
    State(state): State<ApiState>,
    Extension(identity): Extension<AuthIdentity>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse<ProxyStatusReport>>, (axum::http::StatusCode, Json<ApiResponse<()>>)> {
    ensure_profile_tenant(&state, id, &identity.tenant).await?;
    Ok(Json(ApiResponse::ok(
        fetch_proxy_status(&state.db, id).await,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_grouped_status_payload() {
        let body = serde_json::json!({
            "tcp": [
                {
                    "name": "echo",
                    "type": "tcp",
                    "status": "running",
                    "err": "",
                    "local_addr": "127.0.0.1:19080",
                    "remote_addr": "127.0.0.1:19081"
                },
                {
                    "name": "boxed",
                    "type": "tcp",
                    "status": "start error",
                    "err": "port already used",
                    "local_addr": "127.0.0.1:19080",
                    "remote_addr": ""
                }
            ],
            "tcpmux": [
                {
                    "name": "mux",
                    "type": "tcpmux",
                    "status": "start error",
                    "err": "tcpmux not enabled in server",
                    "local_addr": "127.0.0.1:22",
                    "remote_addr": ""
                }
            ]
        });

        let parsed = parse_status_body(&body);
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0].name, "boxed");
        assert_eq!(parsed[0].status, "start error");
        assert_eq!(parsed[0].error, "port already used");
        assert_eq!(parsed[1].name, "echo");
        assert_eq!(parsed[1].status, "running");
        assert_eq!(parsed[2].proxy_type, "tcpmux");
    }

    #[test]
    fn tolerates_unexpected_payload_shapes() {
        assert!(parse_status_body(&serde_json::json!({})).is_empty());
        assert!(parse_status_body(&serde_json::json!({"tcp": "nope"})).is_empty());
        // Missing optional keys must not panic.
        let parsed = parse_status_body(&serde_json::json!({"tcp": [{"name": "a"}]}));
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].status, "");
    }
}
