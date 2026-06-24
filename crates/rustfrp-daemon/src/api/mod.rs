//! HTTP API module — router assembly and serve entry point
//!
//! Provides:
//! - `AuthMiddleware` trait (MVP: `NoAuth`) for future authentication
//! - `create_router()` to build the axum Router with all endpoints
//! - `serve()` to start the HTTP API server with graceful shutdown

pub mod response;
pub mod state;

// Sub-modules for each resource type
pub mod bindings;
pub mod profiles;
pub mod proxies;
pub mod system;
pub mod visitors;

use axum::extract::Request;
use axum::response::Response;
use axum::Router;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;

use rustfrp_client::core::ClientCore;

use self::state::ApiState;

/// Authentication middleware trait.
///
/// MVP uses `NoAuth`. Future implementations:
/// - `BearerToken` — check `Authorization: Bearer <token>` header
/// - `HmacSign` — HMAC-signed requests
pub trait AuthMiddleware: Send + Sync + 'static {
    /// Authenticate the request. Returns `Ok(())` if allowed,
    /// or `Err(response)` with a 401/403 response body.
    fn authenticate(&self, request: &Request) -> Result<(), Response>;
}

/// MVP implementation: allow all requests through.
pub struct NoAuth;

impl AuthMiddleware for NoAuth {
    fn authenticate(&self, _request: &Request) -> Result<(), Response> {
        Ok(())
    }
}

/// Build the axum Router with all API endpoints.
///
/// Injects shared `ApiState` and authentication middleware.
pub fn create_router(state: ApiState, _auth: impl AuthMiddleware) -> Router {
    // MVP: NoAuth passes all requests through.
    // Future: insert auth as a tower layer that calls auth.authenticate().

    Router::new()
        // Profile CRUD
        .route("/api/v1/profiles", axum::routing::get(profiles::list))
        .route("/api/v1/profiles", axum::routing::post(profiles::create))
        .route("/api/v1/profiles/{id}", axum::routing::get(profiles::get))
        .route("/api/v1/profiles/{id}", axum::routing::put(profiles::update))
        .route(
            "/api/v1/profiles/{id}",
            axum::routing::delete(profiles::delete),
        )
        // Proxy CRUD
        .route("/api/v1/proxies", axum::routing::get(proxies::list))
        .route("/api/v1/proxies", axum::routing::post(proxies::create))
        .route("/api/v1/proxies/{id}", axum::routing::get(proxies::get))
        .route("/api/v1/proxies/{id}", axum::routing::put(proxies::update))
        .route(
            "/api/v1/proxies/{id}",
            axum::routing::delete(proxies::delete),
        )
        // Binding CRUD
        .route("/api/v1/bindings", axum::routing::get(bindings::list))
        .route("/api/v1/bindings", axum::routing::post(bindings::create))
        .route("/api/v1/bindings/{id}", axum::routing::get(bindings::get))
        .route(
            "/api/v1/bindings/{id}",
            axum::routing::put(bindings::update),
        )
        .route(
            "/api/v1/bindings/{id}",
            axum::routing::delete(bindings::delete),
        )
        .route(
            "/api/v1/bindings/{id}/toggle",
            axum::routing::patch(bindings::toggle),
        )
        // Visitor CRUD
        .route("/api/v1/visitors", axum::routing::get(visitors::list))
        .route("/api/v1/visitors", axum::routing::post(visitors::create))
        .route("/api/v1/visitors/{id}", axum::routing::get(visitors::get))
        .route(
            "/api/v1/visitors/{id}",
            axum::routing::put(visitors::update),
        )
        .route(
            "/api/v1/visitors/{id}",
            axum::routing::delete(visitors::delete),
        )
        // System endpoints
        .route("/api/v1/status", axum::routing::get(system::status))
        .route("/api/v1/reload", axum::routing::post(system::reload))
        .route(
            "/api/v1/reload/{task_id}",
            axum::routing::get(system::reload_status),
        )
        .route("/api/v1/health", axum::routing::get(system::health))
        // Shared state
        .with_state(state)
        // Trace layer (disable body logging for security)
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
}

/// Start the HTTP API server.
///
/// Launches the axum server on the given listen address and runs
/// the client core lifecycle concurrently. Either process exiting
/// triggers graceful shutdown of the other via `tokio::select!`.
pub async fn serve(core: ClientCore, listen_addr: &str) -> anyhow::Result<()> {
    let state = ApiState::new(
        core.db().clone(),
        core.process_manager().clone(),
        core.config_dir().clone(),
        core.state().clone(),
    );

    let router = create_router(state, NoAuth);

    let addr: SocketAddr = listen_addr.parse()?;
    tracing::info!(%addr, "HTTP API server starting");

    let listener = TcpListener::bind(addr).await?;

    let api_server = axum::serve(listener, router.into_make_service());

    // Concurrent: API server + client core lifecycle
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
