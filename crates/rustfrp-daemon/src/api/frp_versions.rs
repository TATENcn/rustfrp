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
    pub tag: String,
    pub published_at: Option<String>,
    /// Archive size for this daemon's platform, when the release ships one.
    pub asset_size: Option<u64>,
    pub download_count: Option<u64>,
    /// Archive filename, so the UI can show the exact URL per channel.
    pub asset_name: String,
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
    let platform = rustfrp_bin::platform::Platform::detect()
        .ok_or_else(|| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "FRP_PLATFORM",
                "Unsupported platform",
            )
        })?
        .slug;
    let versions = state
        .frp_versions
        .list_available()
        .await
        .map_err(binary_error)?
        .into_iter()
        .map(|version| {
            let asset = version.asset_for(&platform);
            let asset_size = asset.map(|asset| asset.size).filter(|size| *size > 0);
            let download_count = asset.map(|asset| asset.download_count);
            let tag = version.tag().to_owned();
            AvailableVersionResponse {
                version: version.version().to_owned(),
                asset_name: rustfrp_bin::download::asset_filename(
                    &rustfrp_bin::FrpVersion::from_tag(&tag),
                    &platform,
                ),
                tag,
                published_at: version.published_at,
                asset_size,
                download_count,
            }
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
    let requested = request.version.trim().trim_start_matches('v').to_owned();
    if state
        .frp_install
        .read()
        .map_err(|_| registry_error())?
        .get(&requested)
        .is_some_and(|task| task.status == "running")
    {
        return Err(api_error(
            StatusCode::CONFLICT,
            "FRP_INSTALL_BUSY",
            "This version is already being installed",
        ));
    }
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
    let running: Vec<_> = state
        .process_manager
        .list_running()
        .await
        .into_iter()
        .filter(|process| process.running)
        .collect();
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
    let version = version.trim_start_matches('v').to_owned();
    if state
        .frp_install
        .read()
        .map_err(|_| registry_error())?
        .get(&version)
        .is_some_and(|task| task.status == "running")
    {
        return Err(api_error(
            StatusCode::CONFLICT,
            "FRP_INSTALL_BUSY",
            "Version is being installed; wait for the download to finish",
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
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub finished_at: Option<chrono::DateTime<chrono::Utc>>,
}

fn registry_error() -> (StatusCode, Json<ApiResponse<()>>) {
    api_error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "INSTALL_STATUS",
        "Install status unavailable",
    )
}

/// Mutate the task for `version` if it is still the same task instance.
fn update_task(
    state: &ApiState,
    version: &str,
    task_id: &str,
    apply: impl FnOnce(&mut InstallTask),
) {
    if let Ok(mut tasks) = state.frp_install.write() {
        if let Some(task) = tasks.get_mut(version).filter(|task| task.id == task_id) {
            apply(task);
        }
    }
}

/// Every install task known to this daemon, newest first.
///
/// Downloads are independent, so more than one can be running. The UI pairs a
/// task with an installed version by name instead of assuming a single global
/// task.
pub async fn installation(State(state): State<ApiState>) -> ApiResult<Vec<InstallTask>> {
    let mut tasks = state
        .frp_install
        .read()
        .map_err(|_| registry_error())?
        .values()
        .cloned()
        .collect::<Vec<_>>();
    tasks.sort_by_key(|task| std::cmp::Reverse(task.started_at));
    let count = tasks.len();
    Ok(Json(ApiResponse::ok_list(tasks, count)))
}

pub async fn begin_install(
    State(state): State<ApiState>,
    Json(request): Json<InstallRequest>,
) -> ApiResult<InstallTask> {
    let version = request.version.trim().trim_start_matches('v').to_owned();
    if version.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "FRP_VERSION_REQUIRED",
            "version is required",
        ));
    }
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
        started_at: chrono::Utc::now(),
        finished_at: None,
    };
    {
        let mut tasks = state.frp_install.write().map_err(|_| registry_error())?;
        // Both the staging directory and the integrity marker belong to one
        // version, so a second concurrent install of it would corrupt the
        // first. Other versions proceed independently.
        if tasks
            .get(&version)
            .is_some_and(|existing| existing.status == "running")
        {
            return Err(api_error(
                StatusCode::CONFLICT,
                "FRP_INSTALL_BUSY",
                "This version is already being installed",
            ));
        }
        tasks.insert(version.clone(), task.clone());
    }
    let task_id = task.id.clone();
    tokio::spawn(async move {
        let progress_state = state.frp_install.clone();
        let progress_version = version.clone();
        let progress_task = task_id.clone();
        let progress: rustfrp_bin::download::ProgressCallback =
            std::sync::Arc::new(move |update| {
                if let Ok(mut tasks) = progress_state.write() {
                    if let Some(task) = tasks
                        .get_mut(&progress_version)
                        .filter(|task| task.id == progress_task)
                    {
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
        update_task(&state, &version, &task_id, |task| {
            task.finished_at = Some(chrono::Utc::now());
            match result {
                Ok(_) => {
                    task.status = "completed";
                    task.stage = "completed";
                }
                Err(error) => {
                    task.status = "failed";
                    task.error = Some(error.to_string());
                }
            }
        });
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
