//! Deployment environment CRUD and profile assignment.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use rustfrp_client::db::environment::Environment;
use serde::{Deserialize, Serialize};

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

pub async fn list(State(state): State<ApiState>) -> ApiResult<Vec<EnvironmentView>> {
    let environments = state
        .db
        .list_environments()
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
    Json(mut environment): Json<Environment>,
) -> Result<(StatusCode, Json<ApiResponse<EnvironmentView>>), (StatusCode, Json<ApiResponse<()>>)> {
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
    Path(id): Path<i64>,
    Json(mut environment): Json<Environment>,
) -> ApiResult<EnvironmentView> {
    environment.id = Some(id);
    state
        .db
        .update_environment(&environment)
        .await
        .map_err(|e| api_error(&e))?;
    Ok(Json(ApiResponse::ok(
        view(&state, environment).await.map_err(|e| api_error(&e))?,
    )))
}

pub async fn delete(State(state): State<ApiState>, Path(id): Path<i64>) -> ApiResult<()> {
    state
        .db
        .delete_environment(id)
        .await
        .map_err(|e| api_error(&e))?;
    Ok(Json(ApiResponse::ok(())))
}

pub async fn assign_profile(
    State(state): State<ApiState>,
    Path(profile_id): Path<i64>,
    Json(assignment): Json<Assignment>,
) -> ApiResult<()> {
    state
        .db
        .get_profile(profile_id)
        .await
        .map_err(|e| api_error(&e))?;
    state
        .db
        .set_profile_environment(profile_id, assignment.environment_id)
        .await
        .map_err(|e| api_error(&e))?;
    Ok(Json(ApiResponse::ok(())))
}
