//! Visitor CRUD handlers
//!
//! Endpoints:
//! - GET    /api/v1/visitors       — list all visitors
//! - POST   /api/v1/visitors       — create a new visitor
//! - GET    /api/v1/visitors/{id}  — get a single visitor
//! - PUT    /api/v1/visitors/{id}  — update a visitor
//! - DELETE /api/v1/visitors/{id}  — delete a visitor

use axum::extract::{Path, State};
use axum::Json;
use chrono::Utc;
use rustfrp_client::config::model::LocalVisitor;

use super::response::ApiResponse;
use super::state::ApiState;

/// List all visitors
pub async fn list(State(state): State<ApiState>) -> Result<Json<ApiResponse<Vec<LocalVisitor>>>, (axum::http::StatusCode, Json<ApiResponse<()>>)> {
    let visitors = state.db.list_visitors().await
        .map_err(|e| (super::response::status_code(&e), Json(ApiResponse::<()> {
            success: false,
            data: None,
            count: None,
            error: Some(super::response::ApiError::from_client_error(&e)),
        })))?;

    let count = visitors.len();
    Ok(Json(ApiResponse::ok_list(visitors, count)))
}

/// Get a single visitor by ID
pub async fn get(
    State(state): State<ApiState>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse<LocalVisitor>>, (axum::http::StatusCode, Json<ApiResponse<()>>)> {
    let visitor = state.db.get_visitor(id).await
        .map_err(|e| (super::response::status_code(&e), Json(ApiResponse {
            success: false,
            data: None,
            count: None,
            error: Some(super::response::ApiError::from_client_error(&e)),
        })))?;

    Ok(Json(ApiResponse::ok(visitor)))
}

/// Create a new visitor
pub async fn create(
    State(state): State<ApiState>,
    Json(mut visitor): Json<LocalVisitor>,
) -> Result<(axum::http::StatusCode, Json<ApiResponse<LocalVisitor>>), (axum::http::StatusCode, Json<ApiResponse<()>>)> {
    let now = Utc::now().to_rfc3339();
    visitor.created_at = now.clone();
    visitor.updated_at = now;

    let id = state.db.insert_visitor(&visitor).await
        .map_err(|e| (super::response::status_code(&e), Json(ApiResponse {
            success: false,
            data: None,
            count: None,
            error: Some(super::response::ApiError::from_client_error(&e)),
        })))?;

    visitor.id = Some(id);
    Ok((axum::http::StatusCode::CREATED, Json(ApiResponse::ok(visitor))))
}

/// Update an existing visitor (full replacement)
pub async fn update(
    State(state): State<ApiState>,
    Path(id): Path<i64>,
    Json(mut visitor): Json<LocalVisitor>,
) -> Result<Json<ApiResponse<LocalVisitor>>, (axum::http::StatusCode, Json<ApiResponse<()>>)> {
    visitor.id = Some(id);
    visitor.updated_at = Utc::now().to_rfc3339();
    if let Ok(existing) = state.db.get_visitor(id).await {
        visitor.created_at = existing.created_at;
    }

    state.db.update_visitor(&visitor).await
        .map_err(|e| (super::response::status_code(&e), Json(ApiResponse {
            success: false,
            data: None,
            count: None,
            error: Some(super::response::ApiError::from_client_error(&e)),
        })))?;

    let updated = state.db.get_visitor(id).await
        .map_err(|e| (super::response::status_code(&e), Json(ApiResponse {
            success: false,
            data: None,
            count: None,
            error: Some(super::response::ApiError::from_client_error(&e)),
        })))?;

    Ok(Json(ApiResponse::ok(updated)))
}

/// Delete a visitor
pub async fn delete(
    State(state): State<ApiState>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse<()>>, (axum::http::StatusCode, Json<ApiResponse<()>>)> {
    state.db.delete_visitor(id).await
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
