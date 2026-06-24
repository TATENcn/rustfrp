//! Binding CRUD handlers
//!
//! Endpoints:
//! - GET    /api/v1/bindings           — list all bindings (optional ?profile_id= / ?proxy_id=)
//! - POST   /api/v1/bindings           — create a new binding
//! - GET    /api/v1/bindings/{id}       — get a single binding
//! - PUT    /api/v1/bindings/{id}       — update a binding
//! - DELETE /api/v1/bindings/{id}       — delete a binding
//! - PATCH  /api/v1/bindings/{id}/toggle — toggle enabled/disabled

use axum::extract::{Path, Query, State};
use axum::Json;
use chrono::Utc;
use rustfrp_client::config::model::BindingRule;
use serde::Deserialize;

use super::response::ApiResponse;
use super::state::ApiState;

/// Query parameters for listing bindings
#[derive(Debug, Deserialize, Default)]
pub struct ListQuery {
    pub profile_id: Option<i64>,
    pub proxy_id: Option<i64>,
}

/// List all bindings, optionally filtered by profile_id or proxy_id
pub async fn list(
    State(state): State<ApiState>,
    Query(query): Query<ListQuery>,
) -> Result<Json<ApiResponse<Vec<BindingRule>>>, (axum::http::StatusCode, Json<ApiResponse<()>>)> {
    let bindings = if let Some(profile_id) = query.profile_id {
        state.db.list_bindings_for_profile(profile_id).await
    } else if let Some(proxy_id) = query.proxy_id {
        state.db.list_bindings_for_proxy(proxy_id).await
    } else {
        state.db.list_bindings().await
    }
    .map_err(|e| (super::response::status_code(&e), Json(ApiResponse::<()> {
        success: false,
        data: None,
        count: None,
        error: Some(super::response::ApiError::from_client_error(&e)),
    })))?;

    let count = bindings.len();
    Ok(Json(ApiResponse::ok_list(bindings, count)))
}

/// Get a single binding by ID
pub async fn get(
    State(state): State<ApiState>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse<BindingRule>>, (axum::http::StatusCode, Json<ApiResponse<()>>)> {
    let binding = state.db.get_binding(id).await
        .map_err(|e| (super::response::status_code(&e), Json(ApiResponse {
            success: false,
            data: None,
            count: None,
            error: Some(super::response::ApiError::from_client_error(&e)),
        })))?;

    Ok(Json(ApiResponse::ok(binding)))
}

/// Create a new binding rule
pub async fn create(
    State(state): State<ApiState>,
    Json(mut binding): Json<BindingRule>,
) -> Result<(axum::http::StatusCode, Json<ApiResponse<BindingRule>>), (axum::http::StatusCode, Json<ApiResponse<()>>)> {
    let now = Utc::now().to_rfc3339();
    binding.created_at = now.clone();
    binding.updated_at = now;

    let id = state.db.insert_binding(&binding).await
        .map_err(|e| (super::response::status_code(&e), Json(ApiResponse {
            success: false,
            data: None,
            count: None,
            error: Some(super::response::ApiError::from_client_error(&e)),
        })))?;

    binding.id = Some(id);
    Ok((axum::http::StatusCode::CREATED, Json(ApiResponse::ok(binding))))
}

/// Update an existing binding (full replacement)
pub async fn update(
    State(state): State<ApiState>,
    Path(id): Path<i64>,
    Json(mut binding): Json<BindingRule>,
) -> Result<Json<ApiResponse<BindingRule>>, (axum::http::StatusCode, Json<ApiResponse<()>>)> {
    binding.id = Some(id);
    binding.updated_at = Utc::now().to_rfc3339();
    if let Ok(existing) = state.db.get_binding(id).await {
        binding.created_at = existing.created_at;
    }

    state.db.update_binding(&binding).await
        .map_err(|e| (super::response::status_code(&e), Json(ApiResponse {
            success: false,
            data: None,
            count: None,
            error: Some(super::response::ApiError::from_client_error(&e)),
        })))?;

    let updated = state.db.get_binding(id).await
        .map_err(|e| (super::response::status_code(&e), Json(ApiResponse {
            success: false,
            data: None,
            count: None,
            error: Some(super::response::ApiError::from_client_error(&e)),
        })))?;

    Ok(Json(ApiResponse::ok(updated)))
}

/// Delete a binding
pub async fn delete(
    State(state): State<ApiState>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse<()>>, (axum::http::StatusCode, Json<ApiResponse<()>>)> {
    state.db.delete_binding(id).await
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

/// Toggle binding enabled/disabled state
#[derive(Debug, Deserialize)]
pub struct ToggleBody {
    pub enabled: bool,
}

pub async fn toggle(
    State(state): State<ApiState>,
    Path(id): Path<i64>,
    Json(body): Json<ToggleBody>,
) -> Result<Json<ApiResponse<BindingRule>>, (axum::http::StatusCode, Json<ApiResponse<()>>)> {
    state.db.toggle_binding(id, body.enabled).await
        .map_err(|e| (super::response::status_code(&e), Json(ApiResponse {
            success: false,
            data: None,
            count: None,
            error: Some(super::response::ApiError::from_client_error(&e)),
        })))?;

    let binding = state.db.get_binding(id).await
        .map_err(|e| (super::response::status_code(&e), Json(ApiResponse {
            success: false,
            data: None,
            count: None,
            error: Some(super::response::ApiError::from_client_error(&e)),
        })))?;

    Ok(Json(ApiResponse::ok(binding)))
}
