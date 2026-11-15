//! Proxy CRUD handlers
//!
//! Endpoints:
//! - GET    /api/v1/proxies       — list all proxies
//! - POST   /api/v1/proxies       — create a new proxy
//! - GET    /api/v1/proxies/{id}  — get a single proxy
//! - PUT    /api/v1/proxies/{id}  — update a proxy
//! - DELETE /api/v1/proxies/{id}  — delete a proxy

use axum::extract::{Extension, Path, State};
use axum::Json;
use chrono::Utc;
use rustfrp_client::config::model::LocalProxy;

use super::auth::AuthIdentity;
use super::response::{api_error, ApiResponse};
use super::state::ApiState;

pub async fn list(
    State(state): State<ApiState>,
    Extension(identity): Extension<AuthIdentity>,
) -> Result<Json<ApiResponse<Vec<LocalProxy>>>, (axum::http::StatusCode, Json<ApiResponse<()>>)> {
    let proxies = state
        .db
        .list_proxies_for_tenant(&identity.tenant)
        .await
        .map_err(api_error)?;
    let count = proxies.len();
    Ok(Json(ApiResponse::ok_list(proxies, count)))
}

pub async fn get(
    State(state): State<ApiState>,
    Extension(identity): Extension<AuthIdentity>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse<LocalProxy>>, (axum::http::StatusCode, Json<ApiResponse<()>>)> {
    let proxy = state
        .db
        .get_proxy_in_tenant(id, &identity.tenant)
        .await
        .map_err(api_error)?;
    Ok(Json(ApiResponse::ok(proxy)))
}

/// Create a new proxy
pub async fn create(
    State(state): State<ApiState>,
    Extension(identity): Extension<AuthIdentity>,
    Json(mut proxy): Json<LocalProxy>,
) -> Result<
    (axum::http::StatusCode, Json<ApiResponse<LocalProxy>>),
    (axum::http::StatusCode, Json<ApiResponse<()>>),
> {
    // Reject structurally invalid proxies before they can break the whole
    // profile's generated configuration (e.g. tcpmux without a multiplexer).
    proxy.validate().map_err(api_error)?;

    let now = Utc::now().to_rfc3339();
    proxy.created_at = now.clone();
    proxy.updated_at = now;

    let id = state
        .db
        .insert_proxy_for_tenant(&proxy, &identity.tenant)
        .await
        .map_err(api_error)?;

    proxy.id = Some(id);
    Ok((
        axum::http::StatusCode::CREATED,
        Json(ApiResponse::ok(proxy)),
    ))
}

/// Update an existing proxy (full replacement)
pub async fn update(
    State(state): State<ApiState>,
    Extension(identity): Extension<AuthIdentity>,
    Path(id): Path<i64>,
    Json(mut proxy): Json<LocalProxy>,
) -> Result<Json<ApiResponse<LocalProxy>>, (axum::http::StatusCode, Json<ApiResponse<()>>)> {
    let existing = state
        .db
        .get_proxy_in_tenant(id, &identity.tenant)
        .await
        .map_err(api_error)?;
    proxy.id = Some(id);
    proxy.created_at = existing.created_at;
    proxy.updated_at = Utc::now().to_rfc3339();
    proxy.validate().map_err(api_error)?;
    state.db.update_proxy(&proxy).await.map_err(api_error)?;
    Ok(Json(ApiResponse::ok(proxy)))
}

/// Delete a proxy
pub async fn delete(
    State(state): State<ApiState>,
    Extension(identity): Extension<AuthIdentity>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse<()>>, (axum::http::StatusCode, Json<ApiResponse<()>>)> {
    // Verify ownership first so a foreign id is indistinguishable from a
    // missing one.
    state
        .db
        .get_proxy_in_tenant(id, &identity.tenant)
        .await
        .map_err(api_error)?;
    state.db.delete_proxy(id).await.map_err(api_error)?;
    Ok(Json(ApiResponse::ok(())))
}
