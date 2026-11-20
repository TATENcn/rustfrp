//! HTTP API module — router assembly and serve entry point
//!
//! Provides:
//! - `AuthMiddleware` trait with `NoAuth` (MVP) and `BearerToken` implementations
//! - `create_router()` to build the axum Router with all endpoints, static assets, and SPA fallback
//! - `serve()` to start the HTTP API server with graceful shutdown

pub mod response;
pub mod state;

// Sub-modules for each resource type
pub mod auth;
pub mod bindings;
pub mod cert_ingest;
pub mod cert_ingest_settings;
pub mod certificates;
pub mod config_transfer;
pub mod environments;
pub mod frp_versions;
pub mod logs;
pub mod metrics;
pub mod profiles;
pub mod proxies;
pub mod proxy_status;
pub mod system;
pub mod visitors;

use axum::extract::{Extension, Request};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use axum::Router;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;

use rustfrp_client::core::ClientCore;

use self::auth::{AuthIdentity, AuthPolicy};
use self::response::{ApiError, ApiResponse};
use self::state::ApiState;

/// Authentication middleware trait.
///
/// MVP uses `NoAuth`. `BearerToken` checks `Authorization: Bearer <token>`.
#[allow(clippy::result_large_err)]
pub trait AuthMiddleware: Send + Sync + 'static {
    /// Authenticate the request. Returns `Ok(())` if allowed,
    /// or `Err(response)` with a 401/403 response body.
    fn authenticate(&self, request: &mut Request) -> Result<(), Response>;
}

/// MVP implementation: allow all requests through.
pub struct NoAuth;

impl AuthMiddleware for NoAuth {
    fn authenticate(&self, request: &mut Request) -> Result<(), Response> {
        request.extensions_mut().insert(AuthIdentity {
            name: "local".into(),
            tenant: "default".into(),
            scopes: vec!["*".into()],
        });
        Ok(())
    }
}

/// Bearer token authentication.
///
/// Enabled when the daemon is started with `--api-token "xxx"`.
/// All API routes (except `/api/v1/health`) require a valid token.
/// Static assets and the SPA index are not protected.
pub struct BearerToken {
    token: String,
}

impl BearerToken {
    pub fn new(token: String) -> Self {
        Self { token }
    }
}

impl AuthMiddleware for BearerToken {
    fn authenticate(&self, request: &mut Request) -> Result<(), Response> {
        // Health check endpoint is always public
        if request.uri().path() == "/api/v1/health" {
            return Ok(());
        }

        // Static assets and SPA are public
        let path = request.uri().path();
        if path.starts_with("/assets/") || (!path.starts_with("/api/")) {
            return Ok(());
        }

        let auth_header = request
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "));

        match auth_header {
            Some(t) if t == self.token => {
                request.extensions_mut().insert(AuthIdentity {
                    name: "legacy".into(),
                    tenant: "default".into(),
                    scopes: vec!["*".into()],
                });
                Ok(())
            }
            _ => {
                let body = ApiResponse::<()> {
                    success: false,
                    data: None,
                    count: None,
                    error: Some(ApiError::generic(
                        "AUTH_001",
                        "Invalid or missing API token".into(),
                    )),
                };
                Err((StatusCode::UNAUTHORIZED, Json(body)).into_response())
            }
        }
    }
}

#[derive(Clone)]
pub struct ScopedBearerToken {
    policy: AuthPolicy,
}

impl ScopedBearerToken {
    pub fn new(policy: AuthPolicy) -> Self {
        Self { policy }
    }
}

impl AuthMiddleware for ScopedBearerToken {
    fn authenticate(&self, request: &mut Request) -> Result<(), Response> {
        if request.uri().path() == "/api/v1/health" {
            return Ok(());
        }
        let path = request.uri().path();
        if path.starts_with("/assets/") || !path.starts_with("/api/") {
            return Ok(());
        }
        let bearer = request
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "));
        match bearer.and_then(|token| self.policy.authenticate(token, request)) {
            Some(identity) => {
                request.extensions_mut().insert(identity);
                Ok(())
            }
            None => Err(auth_error(
                "Token is invalid, outside its tenant, or lacks the required scope",
            )),
        }
    }
}

fn auth_error(message: &str) -> Response {
    let body = ApiResponse::<()> {
        success: false,
        data: None,
        count: None,
        error: Some(ApiError::generic("AUTH_001", message.into())),
    };
    (StatusCode::UNAUTHORIZED, Json(body)).into_response()
}

async fn whoami(Extension(identity): Extension<AuthIdentity>) -> Json<ApiResponse<AuthIdentity>> {
    Json(ApiResponse::ok(identity))
}

/// Build the axum Router with all API endpoints, static assets, and SPA fallback.
///
/// Route priority order:
/// 1. API routes (`/api/v1/*`)
/// 2. Static assets (`/assets/*`)
/// 3. SPA fallback (everything else → `index.html`)
pub fn create_router(state: ApiState, auth: impl AuthMiddleware) -> Router {
    use crate::web;

    // The push endpoint is also served by the control API, so a certificate
    // manager can reach it at whatever address the operator already uses for the
    // UI (a frpc `https` proxy, a reverse proxy, a LAN address). The dedicated
    // listener on 7901 stays for deployments that publish that port instead.
    //
    // It keeps its own `X-Cert-Ingest-Token` auth and sits outside the API
    // prefix, so the bearer middleware ignores it (see `required_scope` callers
    // in `api::auth`, which pass through anything not under `/api/`).
    let ingest_routes = crate::cert_ingest::create_ingest_router(crate::cert_ingest::IngestState {
        db: state.db.clone(),
    });

    // Group 1: API routes (matched first)
    let api_routes = Router::new()
        // Profile CRUD
        .route(
            "/api/v1/profiles",
            axum::routing::get(profiles::list).post(profiles::create),
        )
        .route(
            "/api/v1/profiles/:id",
            axum::routing::get(profiles::get)
                .put(profiles::update)
                .delete(profiles::delete),
        )
        .route(
            "/api/v1/profiles/:id/environment",
            axum::routing::put(environments::assign_profile),
        )
        .route(
            "/api/v1/profiles/:id/proxies",
            axum::routing::put(profiles::replace_proxies),
        )
        .route(
            "/api/v1/profiles/:id/runtime",
            axum::routing::get(profiles::runtime),
        )
        .route(
            "/api/v1/profiles/:id/proxy-status",
            axum::routing::get(proxy_status::profile_proxy_status),
        )
        .route(
            "/api/v1/profiles/:id/start",
            axum::routing::post(profiles::start),
        )
        .route(
            "/api/v1/profiles/:id/stop",
            axum::routing::post(profiles::stop),
        )
        .route(
            "/api/v1/environments",
            axum::routing::get(environments::list).post(environments::create),
        )
        .route(
            "/api/v1/environments/:id",
            axum::routing::put(environments::update).delete(environments::delete),
        )
        // Proxy CRUD
        .route(
            "/api/v1/proxies",
            axum::routing::get(proxies::list).post(proxies::create),
        )
        .route(
            "/api/v1/proxies/:id",
            axum::routing::get(proxies::get)
                .put(proxies::update)
                .delete(proxies::delete),
        )
        // Binding CRUD
        .route(
            "/api/v1/bindings",
            axum::routing::get(bindings::list).post(bindings::create),
        )
        .route(
            "/api/v1/bindings/:id",
            axum::routing::get(bindings::get)
                .put(bindings::update)
                .delete(bindings::delete),
        )
        .route(
            "/api/v1/bindings/:id/toggle",
            axum::routing::patch(bindings::toggle),
        )
        // Visitor CRUD
        .route(
            "/api/v1/visitors",
            axum::routing::get(visitors::list).post(visitors::create),
        )
        .route(
            "/api/v1/visitors/:id",
            axum::routing::get(visitors::get)
                .put(visitors::update)
                .delete(visitors::delete),
        )
        // Logs endpoints
        .route(
            "/api/v1/logs/:profile_id",
            axum::routing::get(logs::get_logs),
        )
        // System endpoints
        .route("/api/v1/status", axum::routing::get(system::status))
        .route("/api/v1/reload", axum::routing::post(system::reload))
        .route(
            "/api/v1/reload/:task_id",
            axum::routing::get(system::reload_status),
        )
        .route("/api/v1/health", axum::routing::get(system::health))
        .route("/api/v1/auth/whoami", axum::routing::get(whoami))
        .route(
            "/api/v1/metrics/history",
            axum::routing::get(metrics::history),
        )
        .route(
            "/api/v1/metrics/traffic",
            axum::routing::post(metrics::ingest_traffic),
        )
        .route(
            "/api/v1/metrics/prometheus",
            axum::routing::get(metrics::prometheus),
        )
        // Explicit migration import and consistent SQLite backup
        // Certificate management: metadata only, no download/export route by design
        .route(
            "/api/v1/certificates",
            axum::routing::get(certificates::list),
        )
        .route(
            "/api/v1/certificates/:id",
            axum::routing::get(certificates::get).delete(certificates::delete),
        )
        .route(
            "/api/v1/certificates/pem",
            axum::routing::post(certificates::import_pem),
        )
        .route(
            "/api/v1/certificates/upload",
            axum::routing::post(certificates::import_upload),
        )
        .route(
            "/api/v1/certificates/from-path",
            axum::routing::post(certificates::import_from_path),
        )
        .route(
            "/api/v1/certificates/:id/apply",
            axum::routing::post(certificates::apply),
        )
        .route(
            "/api/v1/certificate-bindings",
            axum::routing::post(certificates::bind),
        )
        .route(
            "/api/v1/certificate-bindings/:id",
            axum::routing::delete(certificates::unbind),
        )
        .route(
            "/api/v1/cert-ingest",
            axum::routing::get(cert_ingest_settings::get).put(cert_ingest_settings::update),
        )
        // Channels: one resource for push tokens and pull sources.
        .route(
            "/api/v1/certificate-sources",
            axum::routing::get(cert_ingest::list).post(cert_ingest::create),
        )
        .route(
            "/api/v1/certificate-sources/:id",
            axum::routing::delete(cert_ingest::delete),
        )
        .route(
            "/api/v1/certificate-sources/:id/certificate",
            axum::routing::put(cert_ingest::attach),
        )
        .route(
            "/api/v1/certificate-sources/:id/primary",
            axum::routing::post(cert_ingest::promote),
        )
        .route(
            "/api/v1/certificate-sources/:id/run",
            axum::routing::post(cert_ingest::run),
        )
        .route("/api/v1/certificate-sources/kinds", axum::routing::get(cert_ingest::kinds))
        // Aliases kept so existing automation and Certd instructions still work.
        .route(
            "/api/v1/cert-ingest-tokens",
            axum::routing::get(cert_ingest::list).post(cert_ingest::create),
        )
        .route(
            "/api/v1/cert-ingest-tokens/:id",
            axum::routing::delete(cert_ingest::delete),
        )
        .route(
            "/api/v1/cert-pull-sources",
            axum::routing::get(cert_ingest::list).post(cert_ingest::create),
        )
        .route(
            "/api/v1/cert-pull-sources/:id",
            axum::routing::delete(cert_ingest::delete),
        )
        .route(
            "/api/v1/cert-pull-sources/:id/run",
            axum::routing::post(cert_ingest::run),
        )
        .route(
            "/api/v1/config/import",
            axum::routing::post(config_transfer::import),
        )
        .route(
            "/api/v1/config/export",
            axum::routing::get(config_transfer::export),
        )
        // FRP binary multi-version management
        .route(
            "/api/v1/frp/versions",
            axum::routing::get(frp_versions::list).post(frp_versions::install),
        )
        .route(
            "/api/v1/frp/installations",
            axum::routing::get(frp_versions::installation).post(frp_versions::begin_install),
        )
        .route(
            "/api/v1/frp/releases",
            axum::routing::get(frp_versions::available),
        )
        .route(
            "/api/v1/frp/versions/:version/activate",
            axum::routing::post(frp_versions::activate),
        )
        .route(
            "/api/v1/frp/versions/:version",
            axum::routing::delete(frp_versions::remove),
        )
        // Shared state
        .with_state(state);

    // Group 2: Static assets (`/assets/*`)
    let static_routes = Router::new().route("/assets/*path", axum::routing::get(web::serve_asset));

    // Group 3: Combine API + static + SPA fallback
    //
    // Order matters: API first, then static assets, then fallback.
    // axum Router::merge keeps registration order; earlier routes match first.
    let auth = Arc::new(auth);
    Router::new()
        .merge(api_routes)
        .merge(ingest_routes)
        .merge(static_routes)
        .fallback(web::serve_index)
        .layer(
            TraceLayer::new_for_http()
                .on_request(|_request: &axum::http::Request<_>, _span: &tracing::Span| {
                    tracing::debug!("HTTP request");
                })
                .on_response(
                    |response: &axum::http::Response<_>, _latency: _, _span: &tracing::Span| {
                        tracing::debug!(status = %response.status(), "HTTP response");
                    },
                ),
        )
        .layer(axum::middleware::from_fn(
            move |mut request: Request, next: axum::middleware::Next| {
                let auth = auth.clone();
                async move {
                    auth.authenticate(&mut request)?;
                    Ok::<_, Response>(next.run(request).await)
                }
            },
        ))
}

/// Bind the HTTP API listener without accepting connections yet.
///
/// Splitting the bind from the serve loop lets an embedding host (the desktop
/// shell) learn the actual port before a webview navigates to it. Passing
/// `127.0.0.1:0` asks the OS for a free port, which keeps the desktop API
/// unreachable from well-known addresses.
pub async fn bind_api_listener(listen_addr: &str) -> anyhow::Result<TcpListener> {
    let addr: SocketAddr = listen_addr.parse()?;
    Ok(TcpListener::bind(addr).await?)
}

/// Start the HTTP API server on an already bound listener.
///
/// Runs the axum server and the client core lifecycle concurrently. Either
/// side exiting triggers graceful shutdown of the other via `tokio::select!`.
///
/// `auth` selects the credential check; every other detail is identical, so the
/// entry points below differ only in this argument.
pub async fn serve_with_listener(
    core: ClientCore,
    listener: TcpListener,
    auth: impl AuthMiddleware,
    auth_description: &str,
) -> anyhow::Result<()> {
    let state = ApiState::new(
        core.db().clone(),
        core.process_manager().clone(),
        core.config_dir().clone(),
        core.state().clone(),
    );
    state.metrics.spawn_sampler(core.process_manager().clone());
    crate::traffic::spawn(
        state.metrics.clone(),
        state.db.clone(),
        state.process_manager.clone(),
    );
    // Certificate channels fetch on their own schedule; nothing about frpc
    // supervision depends on it, so it is fire-and-forget.
    crate::cert_pull::spawn_scheduler(state.db.clone(), state.process_manager.clone());

    // Start the push endpoint from the stored settings. A bind failure is
    // reported and surfaced through the settings API, never fatal: frpc
    // supervision must not depend on a port being free.
    match rustfrp_client::db::settings::IngestConfig::load(&state.db).await {
        Ok(config) => {
            if let Err(error) = state.ingest.apply(&config).await {
                tracing::warn!(%error, "certificate ingest listener not started; fix it under Settings");
            }
        }
        Err(error) => tracing::warn!(%error, "could not read ingest settings"),
    }

    let router = create_router(state, auth);

    let addr = listener.local_addr()?;
    tracing::info!(%addr, auth = auth_description, "HTTP API server starting");

    let api_server = axum::serve(listener, router.into_make_service());

    tokio::select! {
        result = api_server => {
            if let Err(e) = result {
                tracing::error!(error = %e, "HTTP API server error");
            }
        }
        result = core.run() => {
            if let Err(e) = result {
                tracing::error!(error = %e, "Client core error");
            }
        }
    }

    tracing::info!("Daemon shut down");
    Ok(())
}

/// Start the HTTP API server on the given listen address.
///
/// Binds the address and delegates to [`serve_with_listener`].
pub async fn serve_with(
    core: ClientCore,
    listen_addr: &str,
    auth: impl AuthMiddleware,
    auth_description: &str,
) -> anyhow::Result<()> {
    let listener = bind_api_listener(listen_addr).await?;
    serve_with_listener(core, listener, auth, auth_description).await
}

/// Serve without authentication.
pub async fn serve(core: ClientCore, listen_addr: &str) -> anyhow::Result<()> {
    serve_with(core, listen_addr, NoAuth, "none").await
}

/// Serve with a single bearer token.
#[cfg(feature = "http-api")]
pub async fn serve_with_auth(
    core: ClientCore,
    listen_addr: &str,
    api_token: &str,
) -> anyhow::Result<()> {
    serve_with(
        core,
        listen_addr,
        BearerToken::new(api_token.to_string()),
        "bearer token",
    )
    .await
}

/// Serve with a scoped, tenant-aware policy.
pub async fn serve_with_policy(
    core: ClientCore,
    listen_addr: &str,
    policy: AuthPolicy,
) -> anyhow::Result<()> {
    serve_with(
        core,
        listen_addr,
        ScopedBearerToken::new(policy),
        "scoped bearer policy",
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request as HttpRequest;
    use rustfrp_client::db::{migrate, Database};
    use rustfrp_client::process::manager::ProcessManager;
    use rustfrp_client::ClientState;
    use rustfrp_common::signal::SignalHandler;
    use tokio::sync::RwLock;
    use tower::ServiceExt;

    #[tokio::test]
    async fn bearer_auth_is_applied_to_api_routes_but_not_health() {
        let database_file = tempfile::NamedTempFile::new().unwrap();
        let db = Database::open(database_file.path().to_str().unwrap())
            .await
            .unwrap();
        {
            let connection = db.lock().await;
            migrate::run(&connection).unwrap();
        }
        let runtime = tempfile::tempdir().unwrap();
        let manager = ProcessManager::new(
            runtime.path().into(),
            "/nonexistent/frpc".into(),
            SignalHandler::new(),
        );
        let state = ApiState::new(
            db,
            manager,
            runtime.path().into(),
            Arc::new(RwLock::new(ClientState::Ready)),
        );
        let app = create_router(state, BearerToken::new("secret".into()));

        let unauthorized = app
            .clone()
            .oneshot(
                HttpRequest::builder()
                    .uri("/api/v1/status")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

        let health = app
            .oneshot(
                HttpRequest::builder()
                    .uri("/api/v1/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(health.status(), StatusCode::OK);
    }
}
