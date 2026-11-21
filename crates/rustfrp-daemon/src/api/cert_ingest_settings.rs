//! Certificate ingest settings.
//!
//! The push endpoint is a fixed, two-route surface that never returns certificate
//! material, so it ships enabled on loopback: reaching it requires a token, and
//! the token is limited to the domains it was issued for. Widening it to other
//! hosts (Docker, a desktop on a LAN, a tunnel) is an explicit choice here, and
//! the change takes effect immediately — the listener is rebound in place.
//!
//! Endpoints:
//! - `GET  /api/v1/cert-ingest` — current configuration and bound address
//! - `PUT  /api/v1/cert-ingest` — enable/disable, set port, allow other hosts

use axum::extract::{Extension, State};
use axum::http::HeaderMap;
use axum::Json;
use serde::{Deserialize, Serialize};

use rustfrp_client::db::settings::{
    IngestConfig, CERT_INGEST_ENABLED, CERT_INGEST_EXPOSE, CERT_INGEST_PORT,
};

use super::auth::AuthIdentity;
use super::response::{api_error, ApiResponse};
use super::state::ApiState;

type ApiErrorResponse = (axum::http::StatusCode, Json<ApiResponse<()>>);

/// Path the ingest listener and the control API both answer on.
pub const INGEST_PATH: &str = "/ingest/v1/certs";

/// Explicit public address for the push endpoint.
///
/// Only needed when neither the browser origin nor the bind address describes
/// how a certificate manager reaches this daemon — a reverse proxy that maps a
/// path, or a container whose published port differs from the internal one.
pub const PUBLIC_URL_ENV: &str = "RUSTFRP_CERT_INGEST_PUBLIC_URL";

/// What the UI renders. `address` is the address actually bound, or null when
/// the listener is off; `error` explains a configuration that could not be bound.
#[derive(Debug, Serialize)]
pub struct IngestSettings {
    pub enabled: bool,
    pub expose: bool,
    pub port: u16,
    pub listen: String,
    /// Address the listener is bound to right now, if any.
    pub address: Option<String>,
    /// Last bind failure, cleared on a successful apply.
    pub error: Option<String>,
    /// Ready-to-copy endpoint, derived from the address the browser used.
    pub url: Option<String>,
    /// Where `url` came from: `public_url`, `request`, `bind` or `none`.
    ///
    /// The UI shows this so an operator can tell "the daemon guessed your
    /// origin" from "you configured it", which is the difference between a
    /// suggestion and a promise.
    pub url_source: &'static str,
    /// The push route on the control API itself, always usable because the
    /// browser is already talking to it.
    pub api_path: String,
    /// Set when `RUSTFRP_CERT_INGEST_PUBLIC_URL` is in effect.
    pub public_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateIngestSettings {
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub expose: Option<bool>,
    #[serde(default)]
    pub port: Option<u16>,
}

pub async fn get(
    State(state): State<ApiState>,
    Extension(_identity): Extension<AuthIdentity>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<IngestSettings>>, ApiErrorResponse> {
    let config = IngestConfig::load(&state.db).await.map_err(api_error)?;
    let address = state.ingest.address().await;
    Ok(Json(ApiResponse::ok(settings_for(
        &config, address, &headers,
    ))))
}

pub async fn update(
    State(state): State<ApiState>,
    Extension(_identity): Extension<AuthIdentity>,
    headers: HeaderMap,
    Json(request): Json<UpdateIngestSettings>,
) -> Result<Json<ApiResponse<IngestSettings>>, ApiErrorResponse> {
    // Persist first, then bind: if the bind fails the stored choice still
    // reflects what the operator asked for, and the error names the conflict.
    if let Some(enabled) = request.enabled {
        state
            .db
            .set_platform_setting(CERT_INGEST_ENABLED, bool_str(enabled))
            .await
            .map_err(api_error)?;
    }
    if let Some(expose) = request.expose {
        state
            .db
            .set_platform_setting(CERT_INGEST_EXPOSE, bool_str(expose))
            .await
            .map_err(api_error)?;
    }
    if let Some(port) = request.port {
        state
            .db
            .set_platform_setting(CERT_INGEST_PORT, &port.to_string())
            .await
            .map_err(api_error)?;
    }

    let config = IngestConfig::load(&state.db).await.map_err(api_error)?;
    let applied = state.ingest.apply(&config).await;
    let (address, error) = match applied {
        Ok(address) => (address, None),
        Err(error) => (None, Some(error)),
    };

    tracing::info!(
        enabled = config.enabled,
        expose = config.expose,
        port = config.port,
        bound = address.as_deref().unwrap_or("none"),
        "Certificate ingest settings updated"
    );

    let mut settings = settings_for(&config, address, &headers);
    settings.error = error;
    Ok(Json(ApiResponse::ok(settings)))
}

fn settings_for(
    config: &IngestConfig,
    address: Option<String>,
    headers: &HeaderMap,
) -> IngestSettings {
    let public_url = public_url_override();
    let (url, url_source) = match (&public_url, address.as_deref()) {
        (Some(public), _) => (Some(public.clone()), "public_url"),
        (None, Some(bound)) => match request_origin(headers) {
            Some(origin) => (Some(format!("{origin}{INGEST_PATH}")), "request"),
            None => (Some(endpoint_url(bound, config)), "bind"),
        },
        (None, None) => (None, "none"),
    };
    IngestSettings {
        enabled: config.enabled,
        expose: config.expose,
        port: config.port,
        listen: config.listen.clone(),
        address,
        error: None,
        url,
        url_source,
        api_path: INGEST_PATH.to_string(),
        public_url,
    }
}

/// `scheme://authority` the caller actually used, honouring a reverse proxy.
///
/// The `Host` header is what the browser addressed; `X-Forwarded-*` is what a
/// proxy in front tells us it terminated. Both are attacker-controlled, but this
/// value is only ever echoed back to the same authenticated caller as a
/// convenience — it is never used for routing, authentication, or a redirect.
fn request_origin(headers: &HeaderMap) -> Option<String> {
    let header = |name: &str| {
        headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
            .filter(|value| !value.is_empty())
    };

    // `Forwarded: for=..;proto=https;host=example.com` — take the first element.
    let forwarded = header("forwarded").and_then(|value| {
        value.split(',').next().map(|element| {
            element
                .split(';')
                .filter_map(|pair| pair.split_once('='))
                .map(|(key, value)| {
                    (
                        key.trim().to_ascii_lowercase(),
                        value.trim().trim_matches('"').to_string(),
                    )
                })
                .collect::<std::collections::HashMap<_, _>>()
        })
    });

    let scheme = header("x-forwarded-proto")
        .and_then(|value| value.split(',').next())
        .map(|value| value.trim().to_ascii_lowercase())
        .or_else(|| {
            forwarded
                .as_ref()
                .and_then(|map| map.get("proto"))
                .map(|value| value.to_ascii_lowercase())
        })
        .filter(|scheme| scheme == "http" || scheme == "https");

    let host = header("x-forwarded-host")
        .and_then(|value| value.split(',').next())
        .map(str::trim)
        .map(str::to_string)
        .or_else(|| forwarded.as_ref().and_then(|map| map.get("host").cloned()))
        .or_else(|| header("host").map(str::to_string));

    let host = host?;
    if !is_plausible_host(&host) {
        return None;
    }
    // Without a proxy hint the connection is plain HTTP: the control API binds
    // loopback or a LAN address and does not terminate TLS itself.
    Some(format!(
        "{}://{host}",
        scheme.unwrap_or_else(|| "http".into())
    ))
}

/// Reject anything that would turn the echoed value into an injection vector.
fn is_plausible_host(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 255
        && host.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b':' | b'[' | b']' | b'_')
        })
}

fn public_url_override() -> Option<String> {
    let value = std::env::var(PUBLIC_URL_ENV).ok()?;
    let trimmed = value.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    // A configured value must already name the ingest route or a bare origin;
    // either way it is normalised so the UI never shows a doubled path.
    let base = trimmed
        .strip_suffix(INGEST_PATH)
        .unwrap_or(trimmed)
        .trim_end_matches('/');
    let base = base.strip_suffix("/ingest/v1").unwrap_or(base);
    Some(format!("{}{INGEST_PATH}", base.trim_end_matches('/')))
}

/// Build the URL from the bind address, for the case where there is no request
/// context to learn anything better from.
///
/// A wildcard bind is reported with a placeholder: the point of the hint is
/// "this is where the endpoint answers", and `0.0.0.0` is not dialable.
fn endpoint_url(address: &str, config: &IngestConfig) -> String {
    let host = match address.rsplit_once(':') {
        Some(("0.0.0.0", _)) | Some(("[::]", _)) => "<this-host>",
        Some((host, _)) => host,
        None => return INGEST_PATH.to_string(),
    };
    format!("http://{host}:{}{INGEST_PATH}", config.port)
}

fn bool_str(value: bool) -> &'static str {
    if value {
        "true"
    } else {
        "false"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcard_bind_is_reported_as_a_placeholder_host() {
        let config = IngestConfig {
            enabled: true,
            listen: "0.0.0.0".into(),
            port: 7901,
            expose: true,
        };
        let url = endpoint_url("0.0.0.0:7901", &config);
        assert!(url.contains("<this-host>:7901"), "{url}");
        assert!(!url.contains("0.0.0.0"), "{url}");
    }

    #[test]
    fn loopback_bind_reports_its_real_host() {
        let config = IngestConfig {
            enabled: true,
            listen: "127.0.0.1".into(),
            port: 7901,
            expose: false,
        };
        assert_eq!(
            endpoint_url("127.0.0.1:7901", &config),
            "http://127.0.0.1:7901/ingest/v1/certs"
        );
    }

    fn config() -> IngestConfig {
        IngestConfig {
            enabled: true,
            listen: "127.0.0.1".into(),
            port: 7901,
            expose: false,
        }
    }

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.insert(
                axum::http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                value.parse().unwrap(),
            );
        }
        map
    }

    /// The browser's own origin wins over the loopback bind, which is the whole
    /// point: an operator on another host must never be told `127.0.0.1`.
    #[test]
    fn request_origin_beats_the_bind_address() {
        let settings = settings_for(
            &config(),
            Some("127.0.0.1:7901".into()),
            &headers(&[("host", "arch-rustfrp.powercess.com")]),
        );
        assert_eq!(
            settings.url.as_deref(),
            Some("http://arch-rustfrp.powercess.com/ingest/v1/certs")
        );
        assert_eq!(settings.url_source, "request");
    }

    /// A reverse proxy terminating TLS must produce `https`, not `http`.
    #[test]
    fn forwarded_proto_selects_the_scheme() {
        let settings = settings_for(
            &config(),
            Some("127.0.0.1:7901".into()),
            &headers(&[
                ("host", "127.0.0.1:7900"),
                ("x-forwarded-host", "arch-rustfrp.powercess.com"),
                ("x-forwarded-proto", "https"),
            ]),
        );
        assert_eq!(
            settings.url.as_deref(),
            Some("https://arch-rustfrp.powercess.com/ingest/v1/certs")
        );
    }

    #[test]
    fn forwarded_header_is_honoured() {
        let settings = settings_for(
            &config(),
            Some("127.0.0.1:7901".into()),
            &headers(&[("forwarded", "for=10.0.0.2;proto=https;host=example.com")]),
        );
        assert_eq!(
            settings.url.as_deref(),
            Some("https://example.com/ingest/v1/certs")
        );
    }

    /// A host header is echoed to the caller, so it must not carry a payload.
    #[test]
    fn implausible_host_falls_back_to_the_bind_address() {
        let settings = settings_for(
            &config(),
            Some("127.0.0.1:7901".into()),
            &headers(&[("host", "evil.com/</script>")]),
        );
        assert_eq!(settings.url_source, "bind");
        assert_eq!(
            settings.url.as_deref(),
            Some("http://127.0.0.1:7901/ingest/v1/certs")
        );
    }

    /// With the listener off there is nothing to advertise, and the UI falls
    /// back to the control API path the browser is already using.
    #[test]
    fn a_stopped_listener_advertises_no_url() {
        let settings = settings_for(&config(), None, &headers(&[("host", "example.com")]));
        assert!(settings.url.is_none());
        assert_eq!(settings.url_source, "none");
        assert_eq!(settings.api_path, INGEST_PATH);
    }
}
