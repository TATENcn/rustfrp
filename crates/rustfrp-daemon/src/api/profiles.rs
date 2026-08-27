//! Profile CRUD handlers
//!
//! Endpoints:
//! - GET    /api/v1/profiles       — list all profiles
//! - POST   /api/v1/profiles       — create a new profile
//! - GET    /api/v1/profiles/{id}  — get a single profile
//! - PUT    /api/v1/profiles/{id}  — update a profile
//! - DELETE /api/v1/profiles/{id}  — delete a profile (stops frpc first)

use axum::extract::{Extension, Path, State};
use axum::Json;
use chrono::Utc;
use rustfrp_client::config::model::FrpsProfile;
use serde::{Deserialize, Serialize};

use super::auth::AuthIdentity;
use super::response::ApiResponse;
use super::state::ApiState;

/// List all profiles
pub async fn list(
    State(state): State<ApiState>,
    Extension(identity): Extension<AuthIdentity>,
) -> Result<Json<ApiResponse<Vec<FrpsProfile>>>, (axum::http::StatusCode, Json<ApiResponse<()>>)> {
    let all_profiles = state.db.list_profiles().await.map_err(|e| {
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
    let mut profiles = Vec::new();
    for profile in all_profiles {
        if state
            .db
            .profile_belongs_to_tenant(profile.id.unwrap_or_default(), &identity.tenant)
            .await
            .map_err(api_error)?
        {
            profiles.push(profile);
        }
    }

    let count = profiles.len();
    Ok(Json(ApiResponse::ok_list(profiles, count)))
}

/// Get a single profile by ID
pub async fn get(
    State(state): State<ApiState>,
    Extension(identity): Extension<AuthIdentity>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse<FrpsProfile>>, (axum::http::StatusCode, Json<ApiResponse<()>>)> {
    ensure_profile_tenant(&state, id, &identity.tenant).await?;
    let profile = state.db.get_profile(id).await.map_err(|e| {
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

    Ok(Json(ApiResponse::ok(profile)))
}

/// Create a new profile
pub async fn create(
    State(state): State<ApiState>,
    Extension(identity): Extension<AuthIdentity>,
    Json(mut profile): Json<FrpsProfile>,
) -> Result<
    (axum::http::StatusCode, Json<ApiResponse<FrpsProfile>>),
    (axum::http::StatusCode, Json<ApiResponse<()>>),
> {
    // Server-managed timestamps
    let now = Utc::now().to_rfc3339();
    profile.created_at = now.clone();
    profile.updated_at = now;

    let environment_id = state
        .db
        .default_environment_for_tenant(&identity.tenant)
        .await
        .map_err(api_error)?;
    let id = state.db.insert_profile(&profile).await.map_err(|e| {
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

    if let Err(error) = state.db.set_profile_environment(id, environment_id).await {
        let _ = state.db.delete_profile(id).await;
        return Err(api_error(error));
    }
    profile.id = Some(id);
    Ok((
        axum::http::StatusCode::CREATED,
        Json(ApiResponse::ok(profile)),
    ))
}

/// Update an existing profile (full replacement)
pub async fn update(
    State(state): State<ApiState>,
    Extension(identity): Extension<AuthIdentity>,
    Path(id): Path<i64>,
    Json(mut profile): Json<FrpsProfile>,
) -> Result<Json<ApiResponse<FrpsProfile>>, (axum::http::StatusCode, Json<ApiResponse<()>>)> {
    ensure_profile_tenant(&state, id, &identity.tenant).await?;
    // Ensure we update the correct record
    profile.id = Some(id);
    // Server manages timestamps
    profile.updated_at = Utc::now().to_rfc3339();
    // Preserve original created_at from DB
    if let Ok(existing) = state.db.get_profile(id).await {
        profile.created_at = existing.created_at;
    }

    state.db.update_profile(&profile).await.map_err(|e| {
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

    let updated = state.db.get_profile(id).await.map_err(|e| {
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

/// Delete a profile (stops associated frpc process first)
pub async fn delete(
    State(state): State<ApiState>,
    Extension(identity): Extension<AuthIdentity>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse<()>>, (axum::http::StatusCode, Json<ApiResponse<()>>)> {
    ensure_profile_tenant(&state, id, &identity.tenant).await?;

    // Stop frpc for this profile first (best-effort, don't fail if not running)
    let _ = state.process_manager.stop(id).await;

    // Delete from database
    state.db.delete_profile(id).await.map_err(|e| {
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

#[derive(Debug, Serialize)]
pub struct ProfileRuntimeResponse {
    pub profile_id: i64,
    pub desired_running: bool,
    pub running: bool,
    pub process_status: String,
    pub enabled_proxy_count: usize,
}

#[derive(Debug, Deserialize)]
pub struct ReplaceProfileProxiesBody {
    pub proxy_ids: Vec<i64>,
}

/// Replace the proxy memberships of a Profile. BindingRule remains the
/// persistence model; the UI exposes it as a Profile-owned multi-select.
pub async fn replace_proxies(
    State(state): State<ApiState>,
    Extension(identity): Extension<AuthIdentity>,
    Path(id): Path<i64>,
    Json(body): Json<ReplaceProfileProxiesBody>,
) -> Result<
    Json<ApiResponse<Vec<rustfrp_client::config::model::BindingRule>>>,
    (axum::http::StatusCode, Json<ApiResponse<()>>),
> {
    ensure_profile_tenant(&state, id, &identity.tenant).await?;
    let mut requested = body.proxy_ids;
    requested.sort_unstable();
    requested.dedup();
    for proxy_id in &requested {
        state.db.get_proxy(*proxy_id).await.map_err(api_error)?;
    }

    let existing = state
        .db
        .list_bindings_for_profile(id)
        .await
        .map_err(api_error)?;
    for binding in &existing {
        if !requested.contains(&binding.proxy_id) {
            state
                .db
                .delete_binding(binding.id.unwrap_or_default())
                .await
                .map_err(api_error)?;
        }
    }
    for (priority, proxy_id) in requested.iter().enumerate() {
        if let Some(binding) = existing
            .iter()
            .find(|binding| binding.proxy_id == *proxy_id)
        {
            if !binding.enabled {
                state
                    .db
                    .toggle_binding(binding.id.unwrap_or_default(), true)
                    .await
                    .map_err(api_error)?;
            }
        } else {
            let binding = rustfrp_client::config::model::BindingRule {
                profile_id: id,
                proxy_id: *proxy_id,
                enabled: true,
                priority: priority as i32,
                ..Default::default()
            };
            state.db.insert_binding(&binding).await.map_err(api_error)?;
        }
    }

    if state.process_manager.is_running(id).await {
        let profile = state.db.get_profile(id).await.map_err(api_error)?;
        let generated = rustfrp_client::config::generator::generate_frpc_toml_for_profile(
            &state.db,
            id,
            &state.config_dir,
        )
        .await
        .map_err(api_error)?;
        if generated.is_some() {
            state
                .process_manager
                .ensure_running(id, &profile.name)
                .await
                .map_err(api_error)?;
        } else {
            state.process_manager.stop(id).await.map_err(api_error)?;
            state
                .db
                .set_profile_desired_running(id, false)
                .await
                .map_err(api_error)?;
        }
    }
    let bindings = state
        .db
        .list_bindings_for_profile(id)
        .await
        .map_err(api_error)?;
    Ok(Json(ApiResponse::ok(bindings)))
}

pub async fn runtime(
    State(state): State<ApiState>,
    Extension(identity): Extension<AuthIdentity>,
    Path(id): Path<i64>,
) -> Result<
    Json<ApiResponse<ProfileRuntimeResponse>>,
    (axum::http::StatusCode, Json<ApiResponse<()>>),
> {
    ensure_profile_tenant(&state, id, &identity.tenant).await?;
    let desired = state
        .db
        .profile_desired_running(id)
        .await
        .map_err(api_error)?;
    let running = state.process_manager.is_running(id).await;
    let count = state
        .db
        .list_enabled_bindings_for_profile(id)
        .await
        .map_err(api_error)?
        .len();
    Ok(Json(ApiResponse::ok(ProfileRuntimeResponse {
        profile_id: id,
        desired_running: desired,
        running,
        process_status: if running { "running" } else { "stopped" }.into(),
        enabled_proxy_count: count,
    })))
}

pub async fn start(
    State(state): State<ApiState>,
    Extension(identity): Extension<AuthIdentity>,
    Path(id): Path<i64>,
) -> Result<
    Json<ApiResponse<ProfileRuntimeResponse>>,
    (axum::http::StatusCode, Json<ApiResponse<()>>),
> {
    ensure_profile_tenant(&state, id, &identity.tenant).await?;
    let profile = state.db.get_profile(id).await.map_err(api_error)?;
    let enabled = state
        .db
        .list_enabled_bindings_for_profile(id)
        .await
        .map_err(api_error)?;
    if enabled.is_empty() {
        return Err((
            axum::http::StatusCode::BAD_REQUEST,
            Json(ApiResponse {
                success: false,
                data: None,
                count: None,
                error: Some(super::response::ApiError::generic(
                    "PROFILE_HAS_NO_PROXIES",
                    "Select at least one enabled proxy before starting the profile".into(),
                )),
            }),
        ));
    }
    rustfrp_client::config::generator::generate_frpc_toml_for_profile(
        &state.db,
        id,
        &state.config_dir,
    )
    .await
    .map_err(api_error)?;
    let action = state
        .process_manager
        .ensure_running(id, &profile.name)
        .await
        .map_err(api_error)?;
    state
        .db
        .set_profile_desired_running(id, true)
        .await
        .map_err(api_error)?;
    Ok(Json(ApiResponse::ok(ProfileRuntimeResponse {
        profile_id: id,
        desired_running: true,
        running: true,
        process_status: action.as_str().into(),
        enabled_proxy_count: enabled.len(),
    })))
}

pub async fn stop(
    State(state): State<ApiState>,
    Extension(identity): Extension<AuthIdentity>,
    Path(id): Path<i64>,
) -> Result<
    Json<ApiResponse<ProfileRuntimeResponse>>,
    (axum::http::StatusCode, Json<ApiResponse<()>>),
> {
    ensure_profile_tenant(&state, id, &identity.tenant).await?;
    state.process_manager.stop(id).await.map_err(api_error)?;
    state
        .db
        .set_profile_desired_running(id, false)
        .await
        .map_err(api_error)?;
    let count = state
        .db
        .list_enabled_bindings_for_profile(id)
        .await
        .map_err(api_error)?
        .len();
    Ok(Json(ApiResponse::ok(ProfileRuntimeResponse {
        profile_id: id,
        desired_running: false,
        running: false,
        process_status: "stopped".into(),
        enabled_proxy_count: count,
    })))
}

fn api_error(
    error: rustfrp_client::error::ClientError,
) -> (axum::http::StatusCode, Json<ApiResponse<()>>) {
    (
        super::response::status_code(&error),
        Json(ApiResponse {
            success: false,
            data: None,
            count: None,
            error: Some(super::response::ApiError::from_client_error(&error)),
        }),
    )
}

async fn ensure_profile_tenant(
    state: &ApiState,
    id: i64,
    tenant: &str,
) -> Result<(), (axum::http::StatusCode, Json<ApiResponse<()>>)> {
    let belongs = state
        .db
        .profile_belongs_to_tenant(id, tenant)
        .await
        .map_err(api_error)?;
    if belongs {
        Ok(())
    } else {
        Err((
            axum::http::StatusCode::NOT_FOUND,
            Json(ApiResponse {
                success: false,
                data: None,
                count: None,
                error: Some(super::response::ApiError::generic(
                    "TENANT_NOT_FOUND",
                    "profile was not found in the active tenant".into(),
                )),
            }),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::state::ApiState;
    use axum::extract::{Path, State};
    use rustfrp_client::ClientState;
    use rustfrp_common::signal::SignalHandler;
    use std::sync::Arc;
    use tempfile::TempDir;
    use tokio::sync::RwLock;

    async fn setup_test_state() -> ApiState {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let db = rustfrp_client::db::Database::open(tmp.path().to_str().unwrap())
            .await
            .unwrap();
        rustfrp_client::db::migrate::run(&*db.lock().await).unwrap();

        let tmp_dir = TempDir::new().unwrap();
        let config_dir = tmp_dir.path().to_path_buf();
        let signal = SignalHandler::new();
        let process_manager = rustfrp_client::process::manager::ProcessManager::new(
            config_dir.clone(),
            std::path::PathBuf::from("/nonexistent/frpc"),
            signal,
        );
        let app_state = Arc::new(RwLock::new(ClientState::Ready));

        ApiState::new(db, process_manager, config_dir, app_state)
    }

    fn identity() -> Extension<AuthIdentity> {
        Extension(AuthIdentity {
            name: "test".into(),
            tenant: "default".into(),
            scopes: vec!["*".into()],
        })
    }

    // ── Profile CRUD integration tests ──

    #[tokio::test]
    async fn test_list_profiles_empty() {
        let state = setup_test_state().await;
        let result = list(State(state), identity()).await;
        assert!(result.is_ok());
        let Json(resp) = result.unwrap();
        assert!(resp.success);
        assert_eq!(resp.count, Some(0));
    }

    #[tokio::test]
    async fn test_create_and_get_profile() {
        let state = setup_test_state().await;

        let profile = FrpsProfile {
            name: "Test Server".into(),
            server_addr: "frp.example.com".into(),
            server_port: 7000,
            ..Default::default()
        };

        let result = create(State(state.clone()), identity(), Json(profile)).await;
        assert!(result.is_ok());
        let (status, Json(resp)) = result.unwrap();
        assert_eq!(status, axum::http::StatusCode::CREATED);
        assert!(resp.success);

        let created = resp.data.unwrap();
        assert!(created.id.is_some());
        let id = created.id.unwrap();
        assert_eq!(created.name, "Test Server");
        // token field should NOT appear in API response (but oidc_token_endpoint_url is fine)
        let json = serde_json::to_string(&created).unwrap();
        assert!(
            !json.contains("\"token\":"),
            "token field should be redacted, but found in: {json}"
        );

        // Now GET the profile
        let result = get(State(state), identity(), Path(id)).await;
        assert!(result.is_ok());
        let Json(resp) = result.unwrap();
        assert_eq!(resp.data.unwrap().server_addr, "frp.example.com");
    }

    #[tokio::test]
    async fn test_get_profile_not_found() {
        let state = setup_test_state().await;
        let result = get(State(state), identity(), Path(99999)).await;
        assert!(result.is_err());
        let (status, _) = result.unwrap_err();
        assert_eq!(status, axum::http::StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_update_profile() {
        let state = setup_test_state().await;

        // Create first
        let profile = FrpsProfile {
            name: "Original".into(),
            server_addr: "a.example.com".into(),
            server_port: 7000,
            ..Default::default()
        };
        let created = create(State(state.clone()), identity(), Json(profile))
            .await
            .unwrap();
        let id = created.1.data.as_ref().unwrap().id.unwrap();

        // Update
        let mut updated = FrpsProfile {
            name: "Updated".into(),
            server_addr: "b.example.com".into(),
            server_port: 7001,
            ..Default::default()
        };
        updated.id = Some(id);

        let result = update(State(state.clone()), identity(), Path(id), Json(updated)).await;
        assert!(result.is_ok());
        let Json(resp) = result.unwrap();
        let data = resp.data.unwrap();
        assert_eq!(data.name, "Updated");
        assert_eq!(data.server_port, 7001);
    }

    #[tokio::test]
    async fn test_delete_profile() {
        let state = setup_test_state().await;

        // Create first
        let profile = FrpsProfile {
            name: "ToDelete".into(),
            server_addr: "del.example.com".into(),
            server_port: 7000,
            ..Default::default()
        };
        let created = create(State(state.clone()), identity(), Json(profile))
            .await
            .unwrap();
        let id = created.1.data.as_ref().unwrap().id.unwrap();

        // Delete
        let result = delete(State(state.clone()), identity(), Path(id)).await;
        assert!(result.is_ok());
        assert!(result.unwrap().data.is_none()); // no body on success

        // Verify gone
        let result = get(State(state), identity(), Path(id)).await;
        assert!(result.is_err());
    }
}
