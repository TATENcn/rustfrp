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
    pub in_use: bool,
    pub source: Option<String>,
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
    pub channel: Option<String>,
}

type ApiResult<T> = Result<Json<ApiResponse<T>>, (StatusCode, Json<ApiResponse<()>>)>;

pub async fn list(State(state): State<ApiState>) -> ApiResult<VersionListResponse> {
    let mut installed: Vec<InstalledVersionResponse> = state
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
            in_use: false,
            source: None,
        })
        .collect();
    for item in &mut installed {
        item.source = tokio::fs::read_to_string(
            std::path::Path::new(&item.frpc_path).with_extension("source"),
        )
        .await
        .ok();
        item.in_use =
            state.process_manager.frpc_path().await == std::path::Path::new(&item.frpc_path);
        item.in_use |= !state
            .db
            .profiles_using_version(&item.version)
            .await
            .map_err(client_error)?
            .is_empty();
        for process in state
            .process_manager
            .list_running()
            .await
            .into_iter()
            .filter(|p| p.running)
        {
            if let Some(path) = state
                .process_manager
                .running_binary(process.profile_id)
                .await
            {
                item.in_use |=
                    state.frp_versions.version_for_path(&path).as_deref() == Some(&item.version);
            }
        }
    }
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
    let base = rustfrp_bin::download::release_base(
        request.channel.as_deref(),
        request.mirror_base.as_deref(),
    )
    .map_err(binary_error)?;
    state
        .frp_versions
        .install(&request.version, Some(&base))
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
        in_use: false,
        source: None,
    })))
}

pub async fn activate(
    State(state): State<ApiState>,
    Path(version): Path<String>,
) -> ApiResult<VersionListResponse> {
    let _operation = state.frp_version_operation.lock().await;
    let previous_version = state.frp_versions.active_version().await;
    let previous_path = state.process_manager.frpc_path().await;
    // Validate before stopping anything, and never restart pinned profiles.
    state
        .frp_versions
        .installed_path(&version)
        .await
        .map_err(binary_error)?;
    let mut running = Vec::new();
    for process in state.process_manager.list_running().await {
        if process.running
            && state
                .db
                .profile_frpc_version(process.profile_id)
                .await
                .map_err(client_error)?
                .is_none()
        {
            running.push(process);
        }
    }
    for process in &running {
        if let Err(error) = state.process_manager.stop(process.profile_id).await {
            restart_processes(&state, &running).await;
            return Err(client_error(error));
        }
    }

    let new_path = match state.frp_versions.activate(&version).await {
        Ok(path) => path,
        Err(error) => {
            restart_processes(&state, &running).await;
            return Err(binary_error(error));
        }
    };
    state.process_manager.set_frpc_path(new_path).await;
    if let Err(error) = restart_processes_checked(&state, &running).await {
        for process in &running {
            let _ = state.process_manager.stop(process.profile_id).await;
        }
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
    if !state
        .db
        .profiles_using_version(&version)
        .await
        .map_err(client_error)?
        .is_empty()
    {
        return Err(api_error(
            StatusCode::CONFLICT,
            "FRP_VERSION_IN_USE",
            "Version is referenced by server configurations",
        ));
    }
    let target = state
        .frp_versions
        .version_path(&version)
        .map_err(binary_error)?;
    if state.process_manager.frpc_path().await == target {
        return Err(api_error(
            StatusCode::CONFLICT,
            "FRP_VERSION_IN_USE",
            "Version is the current default binary",
        ));
    }
    for process in state.process_manager.list_running().await {
        if state
            .process_manager
            .running_binary(process.profile_id)
            .await
            .as_ref()
            == Some(&target)
        {
            return Err(api_error(
                StatusCode::CONFLICT,
                "FRP_VERSION_IN_USE",
                "Version is still running; restart its server before removal",
            ));
        }
    }
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

pub(super) fn binary_error(error: rustfrp_bin::FrpError) -> (StatusCode, Json<ApiResponse<()>>) {
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

#[derive(Debug, Clone, Serialize)]
pub struct InstallTask {
    pub id: String,
    pub version: String,
    pub source: String,
    pub status: &'static str,
    pub stage: &'static str,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub error: Option<String>,
}

pub async fn installation(State(state): State<ApiState>) -> ApiResult<Option<InstallTask>> {
    let task = state
        .frp_install
        .read()
        .map_err(|_| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "INSTALL_STATUS",
                "Install status unavailable",
            )
        })?
        .clone();
    Ok(Json(ApiResponse::ok(task)))
}

pub async fn begin_install(
    State(state): State<ApiState>,
    Json(request): Json<InstallRequest>,
) -> ApiResult<InstallTask> {
    let operation = state
        .frp_version_operation
        .clone()
        .try_lock_owned()
        .map_err(|_| {
            api_error(
                StatusCode::CONFLICT,
                "FRP_OPERATION_BUSY",
                "Another version operation is running",
            )
        })?;
    let version = request.version.trim().trim_start_matches('v').to_owned();
    state
        .frp_versions
        .version_path(&version)
        .map_err(binary_error)?;
    let source = rustfrp_bin::download::release_base(
        request.channel.as_deref(),
        request.mirror_base.as_deref(),
    )
    .map_err(binary_error)?;
    let task = InstallTask {
        id: uuid::Uuid::new_v4().to_string(),
        version: version.clone(),
        source: source.clone(),
        status: "running",
        stage: "checking",
        downloaded_bytes: 0,
        total_bytes: None,
        error: None,
    };
    *state.frp_install.write().map_err(|_| {
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "INSTALL_STATUS",
            "Install status unavailable",
        )
    })? = Some(task.clone());
    tokio::spawn(async move {
        let _operation = operation;
        let progress_state = state.frp_install.clone();
        let progress: rustfrp_bin::download::ProgressCallback =
            std::sync::Arc::new(move |update| {
                if let Ok(mut status) = progress_state.write() {
                    if let Some(task) = status.as_mut() {
                        task.stage = update.stage;
                        if update.stage == "downloading" {
                            task.downloaded_bytes = update.downloaded_bytes;
                            task.total_bytes = update.total_bytes;
                        }
                    }
                }
            });
        let result = state
            .frp_versions
            .install_with_progress(&version, &source, progress)
            .await;
        if let Ok(mut status) = state.frp_install.write() {
            if let Some(task) = status.as_mut() {
                match result {
                    Ok(_) => {
                        task.status = "completed";
                        task.stage = "completed";
                    }
                    Err(e) => {
                        task.status = "failed";
                        task.error = Some(e.to_string());
                    }
                }
            }
        };
    });
    Ok(Json(ApiResponse::ok(task)))
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
