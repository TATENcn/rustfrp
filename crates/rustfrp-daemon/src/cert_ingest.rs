//! Dedicated certificate ingest listener.
//!
//! Kept apart from the control API on purpose. The control API is a
//! configuration plane bound to loopback with scoped bearer tokens; the ingest
//! listener is the one surface that may be reachable from another host (a Certd
//! instance, a renew hook), so it exposes exactly two routes and nothing else:
//!
//! ```text
//! POST /ingest/v1/certs      — accept a certificate bundle
//! GET  /ingest/v1/healthz    — liveness, no information
//! ```
//!
//! Reaching it grants no read access: there is no route that returns a
//! certificate, a key, a proxy, or a setting. The worst a leaked token can do is
//! overwrite the certificate material of a domain it is explicitly allowed to
//! serve — and only through the paths an operator already configured.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{DefaultBodyLimit, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;

use rustfrp_client::cert::{domain, parse_bundle};
use rustfrp_client::db::certificate::{ImportRequest, SOURCE_WEBHOOK};
use rustfrp_client::db::settings::IngestConfig;
use rustfrp_client::db::Database;
use rustfrp_client::ClientError;

/// Header carrying the ingest token. Never a path segment: URLs end up in the
/// caller's logs, request headers do not.
pub const TOKEN_HEADER: &str = "x-cert-ingest-token";

/// A bundle is a few kilobytes; this bounds what one request can make us parse.
const MAX_BODY_BYTES: usize = 64 * 1024;

/// Shared state for the ingest listener. Deliberately not `ApiState`: this
/// listener must not be able to reach the process manager or the version
/// registry, even by accident.
#[derive(Clone)]
pub struct IngestState {
    pub db: Database,
}

/// Accepted request body. Both fields are PEM or bare base64 DER; `chain` is
/// accepted for callers that split the fullchain themselves.
#[derive(Debug, Deserialize)]
pub struct IngestRequest {
    #[serde(alias = "cert", alias = "one")]
    pub crt: String,
    pub key: String,
    #[serde(default)]
    pub chain: Option<String>,
    #[serde(default)]
    pub domains: Option<Vec<String>>,
}

/// Response body. Metadata only — never the material that was submitted.
#[derive(Debug, serde::Serialize)]
struct IngestResponse {
    ok: bool,
    certificate_id: i64,
    primary_domain: String,
    not_after: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    warnings: Vec<String>,
    /// Hex of the stored fingerprint, so a caller can log what it sent.
    fingerprint_sha256: String,
}

/// Build the ingest router. Two routes, no fallback, no static assets.
pub fn create_ingest_router(state: IngestState) -> Router {
    Router::new()
        .route("/ingest/v1/certs", post(ingest_certificate))
        .route("/ingest/v1/healthz", get(healthz))
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .with_state(state)
}

/// Bind and serve until shutdown.
pub async fn serve_ingest(state: IngestState, listen: &str) -> anyhow::Result<()> {
    let listener = bind_ingest(listen).await?;
    axum::serve(listener, create_ingest_router(state)).await?;
    Ok(())
}

/// One running ingest listener, replaceable at runtime.
///
/// The listener can be switched on, off, or moved to another port while the
/// daemon keeps running, so "open the endpoint" in the UI takes effect without a
/// restart. Rebinding is serialised: two concurrent changes must not both think
/// they own the port.
pub struct IngestServer {
    state: IngestState,
    binding: Arc<tokio::sync::Mutex<Option<Running>>>,
}

struct Running {
    address: String,
    shutdown: tokio::sync::oneshot::Sender<()>,
    task: tokio::task::JoinHandle<()>,
}

impl IngestServer {
    pub fn new(state: IngestState) -> Self {
        Self {
            state,
            binding: Arc::new(tokio::sync::Mutex::new(None)),
        }
    }

    /// Currently bound address, if the listener is running.
    pub async fn address(&self) -> Option<String> {
        self.binding
            .lock()
            .await
            .as_ref()
            .map(|running| running.address.clone())
    }

    /// Start, stop, or move the listener to match `config`.
    ///
    /// A bind failure is reported, not fatal: the daemon's job is to serve frpc,
    /// and a taken port must not stop it from starting. The caller surfaces the
    /// error so an operator sees why the endpoint is not reachable.
    pub async fn apply(&self, config: &IngestConfig) -> Result<Option<String>, String> {
        let mut binding = self.binding.lock().await;

        if !config.enabled {
            if let Some(running) = binding.take() {
                stop(running).await;
                tracing::info!("Certificate ingest listener stopped (disabled)");
            }
            return Ok(None);
        }

        let wanted = config.address();
        if binding
            .as_ref()
            .is_some_and(|running| running.address == wanted)
        {
            return Ok(Some(wanted));
        }

        // Any address change goes through a full stop: rebinding the same port
        // in place would race with the old listener still holding it.
        if let Some(running) = binding.take() {
            stop(running).await;
        }

        match bind_ingest(&wanted).await {
            Ok(listener) => {
                let router = create_ingest_router(self.state.clone());
                let (shutdown, signal) = tokio::sync::oneshot::channel();
                let task = tokio::spawn(async move {
                    let served = axum::serve(listener, router)
                        .with_graceful_shutdown(async {
                            let _ = signal.await;
                        })
                        .await;
                    if let Err(error) = served {
                        tracing::error!(%error, "certificate ingest listener stopped");
                    }
                });
                *binding = Some(Running {
                    address: wanted.clone(),
                    shutdown,
                    task,
                });
                Ok(Some(wanted))
            }
            Err(error) => {
                // Leaving `binding` empty keeps the reported state honest: the
                // endpoint really is not listening.
                tracing::warn!(address = %wanted, %error, "certificate ingest listener could not bind");
                Err(format!("could not listen on {wanted}: {error}"))
            }
        }
    }
}

async fn stop(running: Running) {
    let _ = running.shutdown.send(());
    // Bounded wait: a slow in-flight push must not block a settings change
    // forever, and the listener is idempotent to abandon.
    if tokio::time::timeout(std::time::Duration::from_secs(5), running.task)
        .await
        .is_err()
    {
        tracing::warn!("certificate ingest listener did not stop cleanly");
    }
}

async fn bind_ingest(address: &str) -> anyhow::Result<tokio::net::TcpListener> {
    let addr: SocketAddr = address.parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(
        bound = %listener.local_addr()?,
        "Certificate ingest listener started (header token only, 2 routes)"
    );
    Ok(listener)
}

async fn healthz() -> impl IntoResponse {
    Json(serde_json::json!({ "ok": true }))
}

async fn ingest_certificate(
    State(state): State<IngestState>,
    headers: HeaderMap,
    Json(request): Json<IngestRequest>,
) -> Response {
    let Some(presented) = headers
        .get(TOKEN_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        // No token shape hint beyond "unauthorized": a prober learns nothing
        // about whether the header or the endpoint is missing.
        return unauthorized("ingest token required");
    };

    let resolved = match state.db.authenticate_source_token(presented).await {
        Ok(resolved) => resolved,
        Err(error) => {
            tracing::error!(%error, "ingest token lookup failed");
            return failure(
                StatusCode::INTERNAL_SERVER_ERROR,
                "CERT_002",
                &error.to_string(),
            );
        }
    };
    let Some(grant) = resolved else {
        return unauthorized("ingest token is unknown, revoked, or expired");
    };
    let tenant = grant.tenant.clone();

    let parsed = match parse_bundle(&request.crt, &request.key) {
        Ok(parsed) => parsed,
        Err(error) => {
            return failure(
                StatusCode::UNPROCESSABLE_ENTITY,
                error.code(),
                &error.to_string(),
            )
        }
    };

    // A token may only submit certificates for the domains it was issued for.
    // Check every SAN, not just the primary one: a wildcard or multi-SAN
    // certificate must not become a way to serve an unrelated host.
    let disallowed =
        domain::disallowed(&parsed.sans, &parsed.primary_domain, &grant.allowed_domains);
    if !disallowed.is_empty() {
        tracing::warn!(
            domain = %parsed.primary_domain,
            rejected = ?disallowed,
            "ingest token submitted a certificate for a domain it does not cover"
        );
        return failure(
            StatusCode::FORBIDDEN,
            "CERT_006",
            "the certificate covers domains this token is not allowed to serve",
        );
    }

    // A token attached to one certificate must not be able to replace a
    // different certificate's material just because the domains overlap.
    if let Some(attached) = grant.certificate_id {
        let expected = match state.db.certificate_in_tenant(attached, &tenant).await {
            Ok(certificate) => certificate,
            Err(error) => return failure(StatusCode::NOT_FOUND, error.code(), &error.to_string()),
        };
        if !domain::covers_target(
            &parsed.sans,
            &parsed.primary_domain,
            &expected.primary_domain,
        ) {
            tracing::warn!(
                certificate_id = attached,
                expected = %expected.primary_domain,
                presented = %parsed.primary_domain,
                "ingest token submitted a certificate for a different domain"
            );
            return failure(
                StatusCode::FORBIDDEN,
                "CERT_008",
                "the certificate does not cover the domain this token is bound to",
            );
        }
    }

    let certificate = match state
        .db
        .import_certificate(
            &tenant,
            ImportRequest {
                parsed,
                source: SOURCE_WEBHOOK.to_string(),
                source_detail: request
                    .domains
                    .as_ref()
                    .map(|domains| domains.join(","))
                    .filter(|detail| !detail.is_empty()),
            },
        )
        .await
    {
        Ok(certificate) => certificate,
        Err(error) => {
            let status = match &error {
                ClientError::CertificateParse(_) | ClientError::CertificateKeyMismatch(_) => {
                    StatusCode::UNPROCESSABLE_ENTITY
                }
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            return failure(status, error.code(), &error.to_string());
        }
    };

    // Write to any proxy already bound to this certificate, but never create a
    // binding here: which proxy serves which domain is an operator decision.
    if !certificate.bindings.is_empty() {
        match state.db.apply_certificate(certificate.id, &tenant).await {
            Ok((applied, failed)) => tracing::info!(
                certificate_id = certificate.id,
                applied = applied.len(),
                failed = failed.len(),
                "Certificate ingested and applied"
            ),
            Err(error) => tracing::warn!(
                certificate_id = certificate.id,
                %error,
                "Certificate ingested but apply failed"
            ),
        }
    }

    (
        StatusCode::OK,
        Json(IngestResponse {
            ok: true,
            certificate_id: certificate.id,
            primary_domain: certificate.primary_domain,
            not_after: certificate.not_after,
            warnings: certificate.warnings,
            fingerprint_sha256: certificate.fingerprint_sha256,
        }),
    )
        .into_response()
}

fn unauthorized(message: &str) -> Response {
    failure(StatusCode::UNAUTHORIZED, "CERT_007", message)
}

fn failure(status: StatusCode, code: &str, message: &str) -> Response {
    (
        status,
        Json(serde_json::json!({
            "ok": false,
            "error": { "code": code, "message": message },
        })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    /// The ingest listener must not grow a control-plane surface: any path other
    /// than the two declared routes is a 404, including the API prefix.
    #[tokio::test]
    async fn exposes_only_the_two_declared_routes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.db");
        let db = Database::open(path.to_str().unwrap()).await.unwrap();
        {
            let conn = db.lock().await;
            rustfrp_client::db::migrate::run(&conn).unwrap();
        }
        let router = create_ingest_router(IngestState { db });

        for (uri, expected) in [
            ("/ingest/v1/healthz", StatusCode::OK),
            ("/api/v1/certificates", StatusCode::NOT_FOUND),
            ("/api/v1/proxies", StatusCode::NOT_FOUND),
            ("/api/v1/config/export", StatusCode::NOT_FOUND),
            ("/", StatusCode::NOT_FOUND),
            ("/ingest/v1/certs/anything", StatusCode::NOT_FOUND),
        ] {
            let response = router
                .clone()
                .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), expected, "unexpected status for {uri}");
        }
    }

    #[tokio::test]
    async fn rejects_missing_and_unknown_tokens() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.db");
        let db = Database::open(path.to_str().unwrap()).await.unwrap();
        {
            let conn = db.lock().await;
            rustfrp_client::db::migrate::run(&conn).unwrap();
        }
        let router = create_ingest_router(IngestState { db });
        let body = serde_json::json!({ "crt": "x", "key": "y" }).to_string();

        let missing = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/ingest/v1/certs")
                    .header("content-type", "application/json")
                    .body(Body::from(body.clone()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(missing.status(), StatusCode::UNAUTHORIZED);

        let unknown = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/ingest/v1/certs")
                    .header("content-type", "application/json")
                    .header(TOKEN_HEADER, "0".repeat(64))
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unknown.status(), StatusCode::UNAUTHORIZED);
    }
}
