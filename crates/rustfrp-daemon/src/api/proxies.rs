//! Proxy CRUD handlers
//!
//! Endpoints:
//! - GET    /api/v1/proxies       — list all proxies
//! - POST   /api/v1/proxies       — create a new proxy
//! - GET    /api/v1/proxies/{id}  — get a single proxy
//! - PUT    /api/v1/proxies/{id}  — update a proxy
//! - DELETE /api/v1/proxies/{id}  — delete a proxy

use axum::extract::{Path, State};
use axum::Json;
use chrono::Utc;
use rustfrp_client::config::model::LocalProxy;

use super::response::ApiResponse;
use super::state::ApiState;

/// List all proxies
pub async fn list(State(state): State<ApiState>) -> Result<Json<ApiResponse<Vec<LocalProxy>>>, (axum::http::StatusCode, Json<ApiResponse<()>>)> {
    let proxies = state.db.list_proxies().await
        .map_err(|e| (super::response::status_code(&e), Json(ApiResponse::<()> {
            success: false,
            data: None,
            count: None,
            error: Some(super::response::ApiError::from_client_error(&e)),
        })))?;

    let count = proxies.len();
    Ok(Json(ApiResponse::ok_list(proxies, count)))
}

/// Get a single proxy by ID
pub async fn get(
    State(state): State<ApiState>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse<LocalProxy>>, (axum::http::StatusCode, Json<ApiResponse<()>>)> {
    let proxy = state.db.get_proxy(id).await
        .map_err(|e| (super::response::status_code(&e), Json(ApiResponse {
            success: false,
            data: None,
            count: None,
            error: Some(super::response::ApiError::from_client_error(&e)),
        })))?;

    Ok(Json(ApiResponse::ok(proxy)))
}

/// Create a new proxy
pub async fn create(
    State(state): State<ApiState>,
    Json(mut proxy): Json<LocalProxy>,
) -> Result<(axum::http::StatusCode, Json<ApiResponse<LocalProxy>>), (axum::http::StatusCode, Json<ApiResponse<()>>)> {
    let now = Utc::now().to_rfc3339();
    proxy.created_at = now.clone();
    proxy.updated_at = now;

    let id = state.db.insert_proxy(&proxy).await
        .map_err(|e| (super::response::status_code(&e), Json(ApiResponse {
            success: false,
            data: None,
            count: None,
            error: Some(super::response::ApiError::from_client_error(&e)),
        })))?;

    proxy.id = Some(id);
    Ok((axum::http::StatusCode::CREATED, Json(ApiResponse::ok(proxy))))
}

/// Update an existing proxy (full replacement)
pub async fn update(
    State(state): State<ApiState>,
    Path(id): Path<i64>,
    Json(mut proxy): Json<LocalProxy>,
) -> Result<Json<ApiResponse<LocalProxy>>, (axum::http::StatusCode, Json<ApiResponse<()>>)> {
    proxy.id = Some(id);
    proxy.updated_at = Utc::now().to_rfc3339();
    if let Ok(existing) = state.db.get_proxy(id).await {
        proxy.created_at = existing.created_at;
    }

    state.db.update_proxy(&proxy).await
        .map_err(|e| (super::response::status_code(&e), Json(ApiResponse {
            success: false,
            data: None,
            count: None,
            error: Some(super::response::ApiError::from_client_error(&e)),
        })))?;

    let updated = state.db.get_proxy(id).await
        .map_err(|e| (super::response::status_code(&e), Json(ApiResponse {
            success: false,
            data: None,
            count: None,
            error: Some(super::response::ApiError::from_client_error(&e)),
        })))?;

    Ok(Json(ApiResponse::ok(updated)))
}

/// Delete a proxy
pub async fn delete(
    State(state): State<ApiState>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse<()>>, (axum::http::StatusCode, Json<ApiResponse<()>>)> {
    state.db.delete_proxy(id).await
        .map_err(|e| (super::response::status_code(&e), Json(ApiResponse {
            success: false,
            data: None,
            count: None,
            error: Some(super::response::ApiError::from_client_error(&e)),
        })))?;

    Ok(Json(ApiResponse {
        success: true,
        data: None,
        count: None,
        error: None,
    }))
}
