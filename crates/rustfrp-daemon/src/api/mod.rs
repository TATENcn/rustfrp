//! HTTP API module — router assembly and serve entry point
//!
//! Provides:
//! - `AuthMiddleware` trait with `NoAuth` (MVP) and `BearerToken` implementations
//! - `create_router()` to build the axum Router with all endpoints, static assets, and SPA fallback
//! - `serve()` to start the HTTP API server with graceful shutdown

pub mod response;
pub mod state;

// Sub-modules for each resource type
pub mod bindings;
pub mod config_transfer;
pub mod logs;
pub mod profiles;
pub mod proxies;
pub mod system;
pub mod visitors;

use axum::extract::Request;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use axum::Router;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;

use rustfrp_client::core::ClientCore;

use self::response::{ApiError, ApiResponse};
use self::state::ApiState;

/// Authentication middleware trait.
///
/// MVP uses `NoAuth`. `BearerToken` checks `Authorization: Bearer <token>`.
#[allow(clippy::result_large_err)]
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
    fn authenticate(&self, request: &Request) -> Result<(), Response> {
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
            Some(t) if t == self.token => Ok(()),
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

/// Build the axum Router with all API endpoints, static assets, and SPA fallback.
///
/// Route priority order:
/// 1. API routes (`/api/v1/*`)
/// 2. Static assets (`/assets/*`)
/// 3. SPA fallback (everything else → `index.html`)
pub fn create_router(state: ApiState, _auth: impl AuthMiddleware) -> Router {
    use crate::web;

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
        .route(
            "/api/v1/bindings/:id/start",
            axum::routing::post(bindings::start_binding),
        )
        .route(
            "/api/v1/bindings/:id/stop",
            axum::routing::post(bindings::stop_binding),
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
        // Explicit migration import and consistent SQLite backup
        .route(
            "/api/v1/config/import",
            axum::routing::post(config_transfer::import),
        )
        .route(
            "/api/v1/config/export",
            axum::routing::get(config_transfer::export),
        )
        // Shared state
        .with_state(state);

    // Group 2: Static assets (`/assets/*`)
    let static_routes = Router::new().route("/assets/*path", axum::routing::get(web::serve_asset));

    // Group 3: Combine API + static + SPA fallback
    //
    // Order matters: API first, then static assets, then fallback.
    // axum Router::merge keeps registration order; earlier routes match first.
    Router::new()
        .merge(api_routes)
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

/// Start the HTTP API server with bearer token authentication.
#[cfg(feature = "http-api")]
pub async fn serve_with_auth(
    core: ClientCore,
    listen_addr: &str,
    api_token: &str,
) -> anyhow::Result<()> {
    let state = ApiState::new(
        core.db().clone(),
        core.process_manager().clone(),
        core.config_dir().clone(),
        core.state().clone(),
    );

    let router = create_router(state, BearerToken::new(api_token.to_string()));

    let addr: SocketAddr = listen_addr.parse()?;
    tracing::info!(%addr, "HTTP API server starting (auth: BearerToken)");

    let listener = TcpListener::bind(addr).await?;

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
