//! FRP multi-version installation and activation API.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use super::response::{ApiError, ApiResponse};
use super::state::ApiState;

#[derive(Debug, Serialize)]
pub struct InstalledVersionResponse {
    pub version: String,
    pub platform: String,
    pub active: bool,
    pub integrity_ok: bool,
    pub frpc_path: String,
    pub has_frps: bool,
}

#[derive(Debug, Serialize)]
pub struct VersionListResponse {
    pub active: Option<String>,
    pub installed: Vec<InstalledVersionResponse>,
}

#[derive(Debug, Serialize)]
pub struct AvailableVersionResponse {
    pub version: String,
    pub published_at: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub struct InstallRequest {
    pub version: String,
    /// Official-compatible release base. Archives may come from a mirror, while
    /// checksums are always fetched from the official release manifest.
    pub mirror_base: Option<String>,
}

type ApiResult<T> = Result<Json<ApiResponse<T>>, (StatusCode, Json<ApiResponse<()>>)>;

pub async fn list(State(state): State<ApiState>) -> ApiResult<VersionListResponse> {
    let installed = state
        .frp_versions
        .list_installed()
        .await
        .map_err(binary_error)?
        .into_iter()
        .map(|version| InstalledVersionResponse {
            version: version.version,
            platform: version.platform,
            active: version.active,
            integrity_ok: version.integrity_ok,
            frpc_path: version.frpc_path.to_string_lossy().into_owned(),
            has_frps: version.frps_path.is_some(),
        })
        .collect();
    Ok(Json(ApiResponse::ok(VersionListResponse {
        active: state.frp_versions.active_version().await,
        installed,
    })))
}

pub async fn available(State(state): State<ApiState>) -> ApiResult<Vec<AvailableVersionResponse>> {
    let versions = state
        .frp_versions
        .list_available()
        .await
        .map_err(binary_error)?
        .into_iter()
        .map(|version| AvailableVersionResponse {
            version: version.version().to_owned(),
            published_at: version.published_at,
        })
        .collect::<Vec<_>>();
    let count = versions.len();
    Ok(Json(ApiResponse::ok_list(versions, count)))
}

pub async fn install(
    State(state): State<ApiState>,
    Json(request): Json<InstallRequest>,
) -> ApiResult<InstalledVersionResponse> {
    let _operation = state.frp_version_operation.lock().await;
    if request.version.trim().is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "FRP_VERSION_REQUIRED",
            "version is required",
        ));
    }
    if request
        .mirror_base
        .as_deref()
        .is_some_and(|mirror| !mirror.starts_with("https://"))
    {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "FRP_MIRROR_INSECURE",
            "mirror_base must use HTTPS",
        ));
    }
    state
        .frp_versions
        .install(&request.version, request.mirror_base.as_deref())
        .await
        .map_err(binary_error)?;
    let installed = state
        .frp_versions
        .list_installed()
        .await
        .map_err(binary_error)?
        .into_iter()
        .find(|candidate| candidate.version == request.version.trim_start_matches('v'))
        .ok_or_else(|| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "FRP_INSTALL_MISSING",
                "installed version was not found",
            )
        })?;
    Ok(Json(ApiResponse::ok(InstalledVersionResponse {
        version: installed.version,
        platform: installed.platform,
        active: installed.active,
        integrity_ok: installed.integrity_ok,
        frpc_path: installed.frpc_path.to_string_lossy().into_owned(),
        has_frps: installed.frps_path.is_some(),
    })))
}

pub async fn activate(
    State(state): State<ApiState>,
    Path(version): Path<String>,
) -> ApiResult<VersionListResponse> {
    let _operation = state.frp_version_operation.lock().await;
    let previous_version = state.frp_versions.active_version().await;
    let previous_path = state.process_manager.frpc_path().await;
    let running = state.process_manager.list_running().await;
    state
        .process_manager
        .shutdown_all()
        .await
        .map_err(client_error)?;

    let new_path = match state.frp_versions.activate(&version).await {
        Ok(path) => path,
        Err(error) => {
            restart_processes(&state, &running).await;
            return Err(binary_error(error));
        }
    };
    state.process_manager.set_frpc_path(new_path).await;
    if let Err(error) = restart_processes_checked(&state, &running).await {
        let _ = state.process_manager.shutdown_all().await;
        match previous_version {
            Some(previous) => {
                let _ = state.frp_versions.activate(&previous).await;
            }
            None => {
                let _ = state.frp_versions.clear_active().await;
            }
        }
        state.process_manager.set_frpc_path(previous_path).await;
        restart_processes(&state, &running).await;
        return Err(client_error(error));
    }
    list(State(state.clone())).await
}

pub async fn remove(State(state): State<ApiState>, Path(version): Path<String>) -> ApiResult<()> {
    let _operation = state.frp_version_operation.lock().await;
    state
        .frp_versions
        .delete(&version)
        .await
        .map_err(binary_error)?;
    Ok(Json(ApiResponse::ok(())))
}

async fn restart_processes_checked(
    state: &ApiState,
    processes: &[rustfrp_client::process::manager::ProcessInfo],
) -> rustfrp_client::Result<()> {
    for process in processes.iter().filter(|process| process.running) {
        let profile = state.db.get_profile(process.profile_id).await?;
        state
            .process_manager
            .start(process.profile_id, &profile.name)
            .await?;
    }
    Ok(())
}

async fn restart_processes(
    state: &ApiState,
    processes: &[rustfrp_client::process::manager::ProcessInfo],
) {
    if let Err(error) = restart_processes_checked(state, processes).await {
        tracing::error!(%error, "Failed to restore frpc processes during version switch rollback");
    }
}

fn binary_error(error: rustfrp_bin::FrpError) -> (StatusCode, Json<ApiResponse<()>>) {
    api_error(StatusCode::BAD_REQUEST, error.code(), &error.to_string())
}

fn client_error(error: rustfrp_client::ClientError) -> (StatusCode, Json<ApiResponse<()>>) {
    (
        super::response::status_code(&error),
        Json(ApiResponse {
            success: false,
            data: None,
            count: None,
            error: Some(ApiError::from_client_error(&error)),
        }),
    )
}

fn api_error(status: StatusCode, code: &str, message: &str) -> (StatusCode, Json<ApiResponse<()>>) {
    (
        status,
        Json(ApiResponse {
            success: false,
            data: None,
            count: None,
            error: Some(ApiError::generic(code, message.to_owned())),
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_request_defaults_optional_mirror() {
        let request: InstallRequest =
            serde_json::from_value(serde_json::json!({ "version": "0.70.1" })).unwrap();
        assert_eq!(request.version, "0.70.1");
        assert!(request.mirror_base.is_none());
    }
}
