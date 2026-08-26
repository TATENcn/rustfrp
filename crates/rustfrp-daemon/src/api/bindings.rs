//! Binding CRUD handlers
//!
//! Endpoints:
//! - GET    /api/v1/bindings           — list all bindings (optional ?profile_id= / ?proxy_id=)
//! - POST   /api/v1/bindings           — create a new binding
//! - GET    /api/v1/bindings/{id}       — get a single binding
//! - PUT    /api/v1/bindings/{id}       — update a binding
//! - DELETE /api/v1/bindings/{id}       — delete a binding
//! - PATCH  /api/v1/bindings/{id}/toggle — toggle enabled/disabled
//! - POST   /api/v1/bindings/{id}/start  — start a binding (launch/reload frpc)
//! - POST   /api/v1/bindings/{id}/stop   — stop a binding (stop/reload frpc)

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::Utc;
use rustfrp_client::config::model::BindingRule;
use serde::{Deserialize, Serialize};

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
    .map_err(|e| {
        (
            super::response::status_code(&e),
            Json(ApiResponse::<()> {
                success: false,
                data: None,
                count: None,
                error: Some(super::response::ApiError::from_client_error(&e)),
            }),
        )
    })?;

    let count = bindings.len();
    Ok(Json(ApiResponse::ok_list(bindings, count)))
}

/// Get a single binding by ID
pub async fn get(
    State(state): State<ApiState>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse<BindingRule>>, (axum::http::StatusCode, Json<ApiResponse<()>>)> {
    let binding = state.db.get_binding(id).await.map_err(|e| {
        (
            super::response::status_code(&e),
            Json(ApiResponse {
                success: false,
                data: None,
                count: None,
                error: Some(super::response::ApiError::from_client_error(&e)),
            }),
        )
    })?;

    Ok(Json(ApiResponse::ok(binding)))
}

/// Create a new binding rule
pub async fn create(
    State(state): State<ApiState>,
    Json(mut binding): Json<BindingRule>,
) -> Result<
    (axum::http::StatusCode, Json<ApiResponse<BindingRule>>),
    (axum::http::StatusCode, Json<ApiResponse<()>>),
> {
    let now = Utc::now().to_rfc3339();
    binding.created_at = now.clone();
    binding.updated_at = now;

    let id = state.db.insert_binding(&binding).await.map_err(|e| {
        (
            super::response::status_code(&e),
            Json(ApiResponse {
                success: false,
                data: None,
                count: None,
                error: Some(super::response::ApiError::from_client_error(&e)),
            }),
        )
    })?;

    binding.id = Some(id);
    Ok((
        axum::http::StatusCode::CREATED,
        Json(ApiResponse::ok(binding)),
    ))
}

/// Update an existing binding (full replacement)
pub async fn update(
    State(state): State<ApiState>,
    Path(id): Path<i64>,
    Json(mut binding): Json<BindingRule>,
) -> Result<Json<ApiResponse<BindingRule>>, (axum::http::StatusCode, Json<ApiResponse<()>>)> {
    binding.id = Some(id);
    binding.updated_at = Utc::now().to_rfc3339();
    // Preserve running state from existing record — start/stop endpoints
    // are the only way to change running state.
    if let Ok(existing) = state.db.get_binding(id).await {
        binding.created_at = existing.created_at;
        binding.running = existing.running;
    }

    state.db.update_binding(&binding).await.map_err(|e| {
        (
            super::response::status_code(&e),
            Json(ApiResponse {
                success: false,
                data: None,
                count: None,
                error: Some(super::response::ApiError::from_client_error(&e)),
            }),
        )
    })?;

    let updated = state.db.get_binding(id).await.map_err(|e| {
        (
            super::response::status_code(&e),
            Json(ApiResponse {
                success: false,
                data: None,
                count: None,
                error: Some(super::response::ApiError::from_client_error(&e)),
            }),
        )
    })?;

    Ok(Json(ApiResponse::ok(updated)))
}

/// Delete a binding
pub async fn delete(
    State(state): State<ApiState>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse<()>>, (axum::http::StatusCode, Json<ApiResponse<()>>)> {
    state.db.delete_binding(id).await.map_err(|e| {
        (
            super::response::status_code(&e),
            Json(ApiResponse {
                success: false,
                data: None,
                count: None,
                error: Some(super::response::ApiError::from_client_error(&e)),
            }),
        )
    })?;

    Ok(Json(ApiResponse {
        success: true,
        data: None,
        count: None,
        error: None,
    }))
}

/// Toggle binding enabled/disabled state
///
/// When disabling a running binding, auto-stops it first (maintains
/// the invariant `running=true ⇒ enabled=true`).
#[derive(Debug, Deserialize)]
pub struct ToggleBody {
    pub enabled: bool,
}

pub async fn toggle(
    State(state): State<ApiState>,
    Path(id): Path<i64>,
    Json(body): Json<ToggleBody>,
) -> Result<Json<ApiResponse<BindingRule>>, (axum::http::StatusCode, Json<ApiResponse<()>>)> {
    // If disabling a running binding, auto-stop first
    if !body.enabled {
        if let Ok(existing) = state.db.get_binding(id).await {
            if existing.running {
                // Execute stop flow (inline, no API round-trip)
                let _ = stop_binding_inner(&state, id, &existing).await;
            }
        }
    }

    state
        .db
        .toggle_binding(id, body.enabled)
        .await
        .map_err(|e| {
            (
                super::response::status_code(&e),
                Json(ApiResponse {
                    success: false,
                    data: None,
                    count: None,
                    error: Some(super::response::ApiError::from_client_error(&e)),
                }),
            )
        })?;

    let binding = state.db.get_binding(id).await.map_err(|e| {
        (
            super::response::status_code(&e),
            Json(ApiResponse {
                success: false,
                data: None,
                count: None,
                error: Some(super::response::ApiError::from_client_error(&e)),
            }),
        )
    })?;

    Ok(Json(ApiResponse::ok(binding)))
}

// ── Binding start/stop response ──

#[derive(Debug, Serialize)]
pub struct BindingControlResponse {
    pub binding_id: i64,
    pub running: bool,
    pub process_status: String,
    pub profile_id: i64,
    pub profile_name: String,
}

// ── POST /api/v1/bindings/:id/start ──

pub async fn start_binding(
    State(state): State<ApiState>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse<BindingControlResponse>>, (StatusCode, Json<ApiResponse<()>>)> {
    let binding = state.db.get_binding(id).await.map_err(|e| {
        (
            super::response::status_code(&e),
            Json(ApiResponse {
                success: false,
                data: None,
                count: None,
                error: Some(super::response::ApiError::from_client_error(&e)),
            }),
        )
    })?;

    // Precondition: enabled must be true
    if !binding.enabled {
        let err = super::response::ApiError::generic(
            "BINDING_DISABLED",
            "Binding is disabled. Enable it first before starting.".into(),
        );
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiResponse {
                success: false,
                data: None,
                count: None,
                error: Some(err),
            }),
        ));
    }

    // Idempotent: already running
    if binding.running {
        let profile = state
            .db
            .get_profile(binding.profile_id)
            .await
            .map_err(|e| {
                (
                    super::response::status_code(&e),
                    Json(ApiResponse {
                        success: false,
                        data: None,
                        count: None,
                        error: Some(super::response::ApiError::from_client_error(&e)),
                    }),
                )
            })?;

        return Ok(Json(ApiResponse::ok(BindingControlResponse {
            binding_id: id,
            running: true,
            process_status: "already_running".into(),
            profile_id: binding.profile_id,
            profile_name: profile.name,
        })));
    }

    // Set running = true
    state.db.set_running(id, true).await.map_err(|e| {
        (
            super::response::status_code(&e),
            Json(ApiResponse {
                success: false,
                data: None,
                count: None,
                error: Some(super::response::ApiError::from_client_error(&e)),
            }),
        )
    })?;

    // Regenerate TOML for this profile
    let toml_result = rustfrp_client::config::generator::generate_frpc_toml_for_profile(
        &state.db,
        binding.profile_id,
        &state.config_dir,
    )
    .await;

    let profile = state
        .db
        .get_profile(binding.profile_id)
        .await
        .map_err(|e| {
            (
                super::response::status_code(&e),
                Json(ApiResponse {
                    success: false,
                    data: None,
                    count: None,
                    error: Some(super::response::ApiError::from_client_error(&e)),
                }),
            )
        })?;

    // If TOML generation failed, roll back running state
    if let Err(e) = &toml_result {
        let _ = state.db.set_running(id, false).await;
        let err = super::response::ApiError::generic(
            "TOML_GENERATION_FAILED",
            format!("Failed to generate TOML: {e}"),
        );
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse {
                success: false,
                data: None,
                count: None,
                error: Some(err),
            }),
        ));
    }

    // Ensure frpc is running (start or reload)
    let action = match state
        .process_manager
        .ensure_running(binding.profile_id, &profile.name)
        .await
    {
        Ok(action) => action,
        Err(e) => {
            // Roll back running state on process error
            let _ = state.db.set_running(id, false).await;
            return Err((
                super::response::status_code(&e),
                Json(ApiResponse {
                    success: false,
                    data: None,
                    count: None,
                    error: Some(super::response::ApiError::from_client_error(&e)),
                }),
            ));
        }
    };

    Ok(Json(ApiResponse::ok(BindingControlResponse {
        binding_id: id,
        running: true,
        process_status: action.as_str().into(),
        profile_id: binding.profile_id,
        profile_name: profile.name,
    })))
}

// ── POST /api/v1/bindings/:id/stop ──

pub async fn stop_binding(
    State(state): State<ApiState>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse<BindingControlResponse>>, (StatusCode, Json<ApiResponse<()>>)> {
    let binding = state.db.get_binding(id).await.map_err(|e| {
        (
            super::response::status_code(&e),
            Json(ApiResponse {
                success: false,
                data: None,
                count: None,
                error: Some(super::response::ApiError::from_client_error(&e)),
            }),
        )
    })?;

    // Idempotent: already not running
    if !binding.running {
        let profile = state
            .db
            .get_profile(binding.profile_id)
            .await
            .map_err(|e| {
                (
                    super::response::status_code(&e),
                    Json(ApiResponse {
                        success: false,
                        data: None,
                        count: None,
                        error: Some(super::response::ApiError::from_client_error(&e)),
                    }),
                )
            })?;

        return Ok(Json(ApiResponse::ok(BindingControlResponse {
            binding_id: id,
            running: false,
            process_status: "not_running".into(),
            profile_id: binding.profile_id,
            profile_name: profile.name,
        })));
    }

    let result = stop_binding_inner(&state, id, &binding)
        .await
        .map_err(|e| {
            (
                super::response::status_code(&e),
                Json(ApiResponse {
                    success: false,
                    data: None,
                    count: None,
                    error: Some(super::response::ApiError::from_client_error(&e)),
                }),
            )
        })?;

    Ok(Json(ApiResponse::ok(result)))
}

/// Core stop logic shared by `stop_binding` and `toggle` (disable→auto-stop).
async fn stop_binding_inner(
    state: &ApiState,
    id: i64,
    binding: &BindingRule,
) -> Result<BindingControlResponse, rustfrp_client::error::ClientError> {
    let profile = state.db.get_profile(binding.profile_id).await?;

    // Set running = false
    state.db.set_running(id, false).await?;

    // Check if there are other running bindings for this profile
    let other_running = state
        .db
        .list_running_bindings_for_profile(binding.profile_id)
        .await?;

    let profile_name = profile.name.clone();

    if !other_running.is_empty() {
        // Regenerate TOML (excludes the stopped binding) and reload
        let _ = rustfrp_client::config::generator::generate_frpc_toml_for_profile(
            &state.db,
            binding.profile_id,
            &state.config_dir,
        )
        .await?;

        let action = state
            .process_manager
            .stop_if_idle(binding.profile_id, true)
            .await?;

        Ok(BindingControlResponse {
            binding_id: id,
            running: false,
            process_status: action.as_str().into(),
            profile_id: binding.profile_id,
            profile_name,
        })
    } else {
        // No other running bindings → stop frpc entirely
        let _ = rustfrp_client::config::generator::generate_frpc_toml_for_profile(
            &state.db,
            binding.profile_id,
            &state.config_dir,
        )
        .await?;

        let action = state
            .process_manager
            .stop_if_idle(binding.profile_id, false)
            .await?;

        Ok(BindingControlResponse {
            binding_id: id,
            running: false,
            process_status: action.as_str().into(),
            profile_id: binding.profile_id,
            profile_name,
        })
    }
}
