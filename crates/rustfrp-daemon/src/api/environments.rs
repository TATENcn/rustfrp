//! Deployment environment CRUD and profile assignment.

use axum::extract::{Extension, Path, State};
use axum::http::StatusCode;
use axum::Json;
use rustfrp_client::db::environment::Environment;
use serde::{Deserialize, Serialize};

use super::auth::AuthIdentity;
use super::response::{ApiError, ApiResponse};
use super::state::ApiState;

type ApiResult<T> = Result<Json<ApiResponse<T>>, (StatusCode, Json<ApiResponse<()>>)>;

#[derive(Debug, Serialize)]
pub struct EnvironmentView {
    #[serde(flatten)]
    pub environment: Environment,
    pub profile_ids: Vec<i64>,
}

#[derive(Debug, Deserialize)]
pub struct Assignment {
    pub environment_id: i64,
}

fn api_error(error: &rustfrp_client::error::ClientError) -> (StatusCode, Json<ApiResponse<()>>) {
    (
        super::response::status_code(error),
        Json(ApiResponse {
            success: false,
            data: None,
            count: None,
            error: Some(ApiError::from_client_error(error)),
        }),
    )
}

async fn view(
    state: &ApiState,
    environment: Environment,
) -> Result<EnvironmentView, rustfrp_client::error::ClientError> {
    let profile_ids = state
        .db
        .environment_profile_ids(environment.id.expect("persisted environment"))
        .await?;
    Ok(EnvironmentView {
        environment,
        profile_ids,
    })
}

pub async fn list(
    State(state): State<ApiState>,
    Extension(identity): Extension<AuthIdentity>,
) -> ApiResult<Vec<EnvironmentView>> {
    let environments = state
        .db
        .list_environments_for_tenant(&identity.tenant)
        .await
        .map_err(|e| api_error(&e))?;
    let mut views = Vec::with_capacity(environments.len());
    for environment in environments {
        views.push(view(&state, environment).await.map_err(|e| api_error(&e))?);
    }
    let count = views.len();
    Ok(Json(ApiResponse::ok_list(views, count)))
}

pub async fn create(
    State(state): State<ApiState>,
    Extension(identity): Extension<AuthIdentity>,
    Json(mut environment): Json<Environment>,
) -> Result<(StatusCode, Json<ApiResponse<EnvironmentView>>), (StatusCode, Json<ApiResponse<()>>)> {
    environment.tenant_id = identity.tenant;
    let id = state
        .db
        .insert_environment(&environment)
        .await
        .map_err(|e| api_error(&e))?;
    environment.id = Some(id);
    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::ok(
            view(&state, environment).await.map_err(|e| api_error(&e))?,
        )),
    ))
}

pub async fn update(
    State(state): State<ApiState>,
    Extension(identity): Extension<AuthIdentity>,
    Path(id): Path<i64>,
    Json(mut environment): Json<Environment>,
) -> ApiResult<EnvironmentView> {
    ensure_environment_tenant(&state, id, &identity.tenant).await?;
    environment.id = Some(id);
    environment.tenant_id = identity.tenant;
    state
        .db
        .update_environment(&environment)
        .await
        .map_err(|e| api_error(&e))?;
    Ok(Json(ApiResponse::ok(
        view(&state, environment).await.map_err(|e| api_error(&e))?,
    )))
}

pub async fn delete(
    State(state): State<ApiState>,
    Extension(identity): Extension<AuthIdentity>,
    Path(id): Path<i64>,
) -> ApiResult<()> {
    ensure_environment_tenant(&state, id, &identity.tenant).await?;
    state
        .db
        .delete_environment(id)
        .await
        .map_err(|e| api_error(&e))?;
    Ok(Json(ApiResponse::ok(())))
}

pub async fn assign_profile(
    State(state): State<ApiState>,
    Extension(identity): Extension<AuthIdentity>,
    Path(profile_id): Path<i64>,
    Json(assignment): Json<Assignment>,
) -> ApiResult<()> {
    ensure_profile_tenant(&state, profile_id, &identity.tenant).await?;
    ensure_environment_tenant(&state, assignment.environment_id, &identity.tenant).await?;
    state
        .db
        .set_profile_environment(profile_id, assignment.environment_id)
        .await
        .map_err(|e| api_error(&e))?;
    Ok(Json(ApiResponse::ok(())))
}

fn hidden_resource() -> (StatusCode, Json<ApiResponse<()>>) {
    (
        StatusCode::NOT_FOUND,
        Json(ApiResponse {
            success: false,
            data: None,
            count: None,
            error: Some(ApiError::generic(
                "TENANT_NOT_FOUND",
                "resource was not found in the active tenant".into(),
            )),
        }),
    )
}

async fn ensure_environment_tenant(
    state: &ApiState,
    id: i64,
    tenant: &str,
) -> Result<(), (StatusCode, Json<ApiResponse<()>>)> {
    state
        .db
        .environment_belongs_to_tenant(id, tenant)
        .await
        .map_err(|error| api_error(&error))?
        .then_some(())
        .ok_or_else(hidden_resource)
}

async fn ensure_profile_tenant(
    state: &ApiState,
    id: i64,
    tenant: &str,
) -> Result<(), (StatusCode, Json<ApiResponse<()>>)> {
    state
        .db
        .profile_belongs_to_tenant(id, tenant)
        .await
        .map_err(|error| api_error(&error))?
        .then_some(())
        .ok_or_else(hidden_resource)
}
