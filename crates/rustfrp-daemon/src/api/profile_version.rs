use super::{
    auth::AuthIdentity,
    response::{ApiError, ApiResponse},
    state::ApiState,
};
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
type Result<T> = std::result::Result<Json<ApiResponse<T>>, (StatusCode, Json<ApiResponse<()>>)>;
#[derive(Serialize)]
pub struct RuntimeVersion {
    pub profile_id: i64,
    pub version: Option<String>,
    pub effective_version: Option<String>,
    pub running_version: Option<String>,
    pub running: bool,
    pub restart_required: bool,
}
#[derive(Deserialize)]
pub struct Selection {
    pub version: Option<String>,
}
fn client_error(e: rustfrp_client::ClientError) -> (StatusCode, Json<ApiResponse<()>>) {
    (
        super::response::status_code(&e),
        Json(ApiResponse {
            success: false,
            data: None,
            count: None,
            error: Some(ApiError::from_client_error(&e)),
        }),
    )
}
pub async fn get(
    State(state): State<ApiState>,
    Extension(identity): Extension<AuthIdentity>,
    Path(id): Path<i64>,
) -> Result<RuntimeVersion> {
    super::profiles::ensure_profile_tenant(&state, id, &identity.tenant).await?;
    let version = state
        .db
        .profile_frpc_version(id)
        .await
        .map_err(client_error)?;
    let path = match &version {
        Some(v) => state
            .frp_versions
            .version_path(v)
            .map_err(super::frp_versions::binary_error)?,
        None => state.process_manager.frpc_path().await,
    };
    let running_path = state.process_manager.running_binary(id).await;
    Ok(Json(ApiResponse::ok(RuntimeVersion {
        profile_id: id,
        effective_version: state.frp_versions.version_for_path(&path),
        version,
        running: running_path.is_some(),
        restart_required: running_path.as_ref().is_some_and(|p| p != &path),
        running_version: running_path
            .as_ref()
            .and_then(|p| state.frp_versions.version_for_path(p)),
    })))
}
pub async fn set(
    State(state): State<ApiState>,
    Extension(identity): Extension<AuthIdentity>,
    Path(id): Path<i64>,
    Json(selection): Json<Selection>,
) -> Result<RuntimeVersion> {
    let _lock = state.frp_version_operation.lock().await;
    super::profiles::ensure_profile_tenant(&state, id, &identity.tenant).await?;
    let version = selection
        .version
        .map(|v| v.trim().trim_start_matches('v').to_string());
    if let Some(version) = &version {
        state
            .frp_versions
            .installed_path(version)
            .await
            .map_err(super::frp_versions::binary_error)?;
    }
    state
        .db
        .set_profile_frpc_version(id, version.as_deref())
        .await
        .map_err(client_error)?;
    get(State(state.clone()), Extension(identity), Path(id)).await
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use rustfrp_bin::manager::VersionManager;
    use rustfrp_client::{
        config::model::FrpsProfile, db::Database, process::manager::ProcessManager, ClientState,
    };
    use rustfrp_common::signal::SignalHandler;
    use std::{os::unix::fs::PermissionsExt, sync::Arc};
    use tokio::sync::RwLock;

    #[tokio::test]
    async fn pinned_servers_survive_default_switch_and_running_versions_cannot_be_deleted() {
        let root = tempfile::tempdir().unwrap();
        let db = Database::open(root.path().join("config.db").to_str().unwrap())
            .await
            .unwrap();
        rustfrp_client::db::migrate::run(&*db.lock().await).unwrap();
        let versions = VersionManager::new(root.path().join("versions-root"));
        let platform = rustfrp_bin::platform::Platform::detect().unwrap();
        for version in ["0.69.0", "0.70.1"] {
            let path = versions.version_path(version).unwrap();
            tokio::fs::create_dir_all(path.parent().unwrap())
                .await
                .unwrap();
            tokio::fs::write(&path, format!("#!/bin/sh\n# {version}\nexec sleep 60\n"))
                .await
                .unwrap();
            tokio::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
                .await
                .unwrap();
            let hash = rustfrp_bin::verify::sha256_file(&path).await.unwrap();
            tokio::fs::write(
                path.with_extension("sha256"),
                format!("{version} {} {hash}\n", platform.slug),
            )
            .await
            .unwrap();
        }
        let default = versions.activate("0.69.0").await.unwrap();
        let manager = ProcessManager::new(root.path().into(), default, SignalHandler::new())
            .with_database(db.clone())
            .with_version_manager(versions.clone());
        let mut state = ApiState::new(
            db.clone(),
            manager.clone(),
            root.path().into(),
            Arc::new(RwLock::new(ClientState::Ready)),
        );
        state.frp_versions = versions;
        let identity = AuthIdentity {
            name: "test".into(),
            tenant: "default".into(),
            scopes: vec!["*".into()],
        };
        let mut ids = Vec::new();
        for name in ["default-server", "pinned-server"] {
            let id = db
                .insert_profile(&FrpsProfile {
                    name: name.into(),
                    ..Default::default()
                })
                .await
                .unwrap();
            tokio::fs::write(root.path().join(format!("{name}.toml")), "")
                .await
                .unwrap();
            ids.push(id);
        }
        let _ = set(
            State(state.clone()),
            Extension(identity.clone()),
            Path(ids[1]),
            Json(Selection {
                version: Some("0.69.0".into()),
            }),
        )
        .await
        .unwrap();
        manager.start(ids[0], "default-server").await.unwrap();
        manager.start(ids[1], "pinned-server").await.unwrap();
        let old = manager.list_running().await;
        let _ = super::super::frp_versions::activate(State(state.clone()), Path("0.70.1".into()))
            .await
            .unwrap();
        let new = manager.list_running().await;
        assert_ne!(
            old.iter().find(|p| p.profile_id == ids[0]).unwrap().pid,
            new.iter().find(|p| p.profile_id == ids[0]).unwrap().pid
        );
        assert_eq!(
            old.iter().find(|p| p.profile_id == ids[1]).unwrap().pid,
            new.iter().find(|p| p.profile_id == ids[1]).unwrap().pid
        );
        let Json(result) = set(
            State(state.clone()),
            Extension(identity.clone()),
            Path(ids[1]),
            Json(Selection {
                version: Some("0.70.1".into()),
            }),
        )
        .await
        .unwrap();
        assert!(result.data.unwrap().restart_required);
        assert!(
            super::super::frp_versions::remove(State(state.clone()), Path("0.69.0".into()))
                .await
                .is_err()
        );
        assert!(set(
            State(state.clone()),
            Extension(identity.clone()),
            Path(ids[1]),
            Json(Selection {
                version: Some("9.99.0".into())
            })
        )
        .await
        .is_err());
        assert_eq!(
            db.profile_frpc_version(ids[1]).await.unwrap().as_deref(),
            Some("0.70.1")
        );
        let other = AuthIdentity {
            tenant: "other".into(),
            ..identity.clone()
        };
        assert!(get(State(state.clone()), Extension(other), Path(ids[1]))
            .await
            .is_err());
        manager
            .ensure_running(ids[1], "pinned-server")
            .await
            .unwrap();
        let Json(result) = get(State(state.clone()), Extension(identity), Path(ids[1]))
            .await
            .unwrap();
        let result = result.data.unwrap();
        assert!(!result.restart_required);
        assert_eq!(result.running_version.as_deref(), Some("0.70.1"));
        let _ = super::super::frp_versions::remove(State(state.clone()), Path("0.69.0".into()))
            .await
            .unwrap();
        manager.shutdown_all().await.unwrap();
        let request = super::super::frp_versions::InstallRequest {
            version: "0.70.1".into(),
            channel: Some("ghfast".into()),
            ..Default::default()
        };
        let Json(task) =
            super::super::frp_versions::begin_install(State(state.clone()), Json(request))
                .await
                .unwrap();
        assert_eq!(task.data.unwrap().status, "running");
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            loop {
                let Json(result) = super::super::frp_versions::installation(State(state.clone()))
                    .await
                    .unwrap();
                if let Some(task) = result.data.flatten() {
                    if task.status != "running" {
                        assert_eq!(task.status, "completed");
                        break;
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
    }
}
