//! System handlers — status, reload, health
//!
//! Endpoints:
//! - GET  /api/v1/status           — daemon status + frpc process info
//! - POST /api/v1/reload           — async reload (returns 202 + task_id)
//! - GET  /api/v1/reload/{task_id} — check reload task status
//! - GET  /api/v1/health           — health check

use std::collections::HashSet;

use axum::extract::{Extension, Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use rustfrp_client::process::diagnostic::ProcessFailure;
use serde::{Deserialize, Serialize};

use super::auth::AuthIdentity;
use super::response::ApiResponse;
use super::state::{ApiState, ReloadPhase, ReloadTaskStatus};

/// Query parameters for the status endpoint.
#[derive(Debug, Default, Deserialize)]
pub struct StatusQuery {
    /// Restrict profile-derived counts and process info to one environment.
    /// Proxies are reusable across profiles, so `total_proxies` stays tenant-wide.
    pub environment_id: Option<i64>,
}

/// Status response payload
#[derive(Debug, Serialize)]
pub struct StatusResponse {
    pub state: String,
    pub uptime_secs: u64,
    pub active_frpc_instances: usize,
    pub total_profiles: usize,
    pub total_proxies: usize,
    pub total_bindings: usize,
    pub total_visitors: usize,
    pub processes: Vec<ProcessInfoResponse>,
    /// Directory generated frpc TOMLs are written to. The UI uses it to suggest
    /// default certificate paths that the daemon can actually create.
    pub config_dir: String,
}

/// Process info in status response (avoids circular dependency by re-defining)
#[derive(Debug, Serialize)]
pub struct ProcessInfoResponse {
    pub profile_id: i64,
    pub profile_name: String,
    pub pid: Option<u32>,
    pub running: bool,
    pub restart_count: u32,
    pub last_failure: Option<ProcessFailure>,
    pub config_path: String,
}

/// GET /api/v1/status — daemon and frpc process status
///
/// Counts and process info are tenant-scoped, and profile-derived values
/// (`total_profiles`, `total_bindings`, `total_visitors`, `active_frpc_instances`,
/// `processes`) are additionally narrowed to `environment_id` when supplied.
/// `total_proxies` stays tenant-wide: a proxy is reusable across profiles and so
/// has no single owning environment.
pub async fn status(
    State(state): State<ApiState>,
    Extension(identity): Extension<AuthIdentity>,
    Query(query): Query<StatusQuery>,
) -> Result<Json<ApiResponse<StatusResponse>>, (StatusCode, Json<ApiResponse<()>>)> {
    let app_state = state.app_state.read().await;
    let state_str = app_state.to_string();

    let fail = |e: rustfrp_client::error::ClientError| {
        (
            super::response::status_code(&e),
            Json(ApiResponse {
                success: false,
                data: None,
                count: None,
                error: Some(super::response::ApiError::from_client_error(&e)),
            }),
        )
    };

    // An unknown or foreign environment is reported like any other foreign id:
    // hidden rather than confirmed absent.
    let scoped: Option<HashSet<i64>> = match query.environment_id {
        Some(id) => {
            super::environments::ensure_environment_tenant(&state, id, &identity.tenant).await?;
            Some(
                state
                    .db
                    .environment_profile_ids(id)
                    .await
                    .map_err(fail)?
                    .into_iter()
                    .collect(),
            )
        }
        None => None,
    };
    let in_scope = |profile_id: i64| scoped.as_ref().is_none_or(|ids| ids.contains(&profile_id));

    let profiles = state
        .db
        .list_profiles_for_tenant(&identity.tenant)
        .await
        .map_err(fail)?;
    let profiles = profiles
        .into_iter()
        .filter(|p| p.id.is_none_or(in_scope))
        .count();

    let proxies = state
        .db
        .list_proxies_for_tenant(&identity.tenant)
        .await
        .map_err(fail)?;

    let bindings = state
        .db
        .list_bindings_for_tenant(&identity.tenant)
        .await
        .map_err(fail)?;
    let bindings = bindings.iter().filter(|b| in_scope(b.profile_id)).count();

    let visitors = state
        .db
        .list_visitors_for_tenant(&identity.tenant)
        .await
        .map_err(fail)?;
    let visitors = visitors.iter().filter(|v| in_scope(v.profile_id)).count();

    let mut process_infos = state.process_manager.list_running().await;
    process_infos.retain(|info| in_scope(info.profile_id));

    // Fill in profile names
    for info in &mut process_infos {
        if let Ok(profile) = state.db.get_profile(info.profile_id).await {
            info.profile_name = profile.name;
        }
    }

    let active_count = process_infos.iter().filter(|p| p.running).count();

    let processes: Vec<ProcessInfoResponse> = process_infos
        .into_iter()
        .map(|p| ProcessInfoResponse {
            profile_id: p.profile_id,
            profile_name: p.profile_name,
            pid: p.pid,
            running: p.running,
            restart_count: p.restart_count,
            last_failure: p.last_failure,
            config_path: p.config_path,
        })
        .collect();

    let uptime_secs = state.start_time.elapsed().as_secs();

    Ok(Json(ApiResponse::ok(StatusResponse {
        state: state_str,
        uptime_secs,
        active_frpc_instances: active_count,
        total_profiles: profiles,
        total_proxies: proxies.len(),
        total_bindings: bindings,
        total_visitors: visitors,
        processes,
        config_dir: state.config_dir.to_string_lossy().to_string(),
    })))
}

/// POST /api/v1/reload — async reload (202 Accepted + task_id)
pub async fn reload(
    State(state): State<ApiState>,
) -> (StatusCode, Json<ApiResponse<ReloadResponse>>) {
    let task_id = uuid::Uuid::new_v4().to_string();

    // Store initial task status
    {
        let mut tasks = state.reload_tasks.write().await;
        tasks.insert(
            task_id.clone(),
            ReloadTaskStatus {
                status: ReloadPhase::Running,
                profiles_affected: 0,
                errors: Vec::new(),
                started_at: chrono::Utc::now(),
                completed_at: None,
            },
        );
    }

    // Spawn async reload task
    let db = state.db.clone();
    let process_manager = state.process_manager.clone();
    let config_dir = state.config_dir.clone();
    let reload_tasks = state.reload_tasks.clone();
    let tid = task_id.clone();

    tokio::spawn(async move {
        let result = async {
            // Regenerate TOML configs
            let toml_files = rustfrp_client::config::generator::generate_all_frpc_tomls(&db, &config_dir).await?;

            // Hot-reload all running frpc instances. A rejected configuration keeps
            // the previous one running, so failures are reported per profile rather
            // than aborting the whole batch.
            let running = process_manager.list_running().await;
            let active_count = running.iter().filter(|p| p.running).count();
            let mut errors = Vec::new();

            for proc in &running {
                if proc.running {
                    if let Err(e) = process_manager.reload(proc.profile_id).await {
                        tracing::warn!(profile_id = proc.profile_id, error = %e, "reload failed for profile");
                        errors.push(format!("profile {}: {e}", proc.profile_id));
                    }
                }
            }

            Ok::<_, rustfrp_client::error::ClientError>((
                toml_files.len() as u32,
                active_count,
                errors,
            ))
        }.await;

        let mut tasks = reload_tasks.write().await;
        if let Some(status) = tasks.get_mut(&tid) {
            match result {
                Ok((toml_count, profiles_affected, errors)) => {
                    // Partial failure is a failed reload: the affected profiles keep
                    // running with their previous configuration.
                    status.status = if errors.is_empty() {
                        ReloadPhase::Completed
                    } else {
                        ReloadPhase::Failed
                    };
                    status.profiles_affected = profiles_affected as u32;
                    status.errors = errors;
                    status.completed_at = Some(chrono::Utc::now());
                    tracing::info!(task_id = %tid, toml_count, profiles_affected, errors = status.errors.len(), "reload completed");
                }
                Err(e) => {
                    status.status = ReloadPhase::Failed;
                    status.errors.push(e.to_string());
                    status.completed_at = Some(chrono::Utc::now());
                    tracing::error!(task_id = %tid, error = %e, "reload failed");
                }
            }
        }
    });

    (
        StatusCode::ACCEPTED,
        Json(ApiResponse::accepted(ReloadResponse { task_id })),
    )
}

#[derive(Debug, Serialize)]
pub struct ReloadResponse {
    pub task_id: String,
}

/// GET /api/v1/reload/{task_id} — check reload task status
pub async fn reload_status(
    State(state): State<ApiState>,
    Path(task_id): Path<String>,
) -> Result<Json<ApiResponse<ReloadTaskStatus>>, (StatusCode, Json<ApiResponse<()>>)> {
    let tasks = state.reload_tasks.read().await;
    let status = tasks.get(&task_id).cloned();

    match status {
        Some(s) => Ok(Json(ApiResponse::ok(s))),
        None => {
            let err = super::response::ApiError::generic(
                "SYS_001",
                format!("Reload task not found: {task_id}"),
            );
            Err((
                StatusCode::NOT_FOUND,
                Json(ApiResponse {
                    success: false,
                    data: None,
                    count: None,
                    error: Some(err),
                }),
            ))
        }
    }
}

/// GET /api/v1/health — health check
pub async fn health() -> Json<ApiResponse<HealthResponse>> {
    Json(ApiResponse::ok(HealthResponse {
        status: "ok".into(),
        version: env!("CARGO_PKG_VERSION").into(),
    }))
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::auth::AuthIdentity;
    use rustfrp_client::config::model::{BindingRule, FrpsProfile, LocalProxy, LocalVisitor};
    use rustfrp_client::db::{environment::Environment, migrate, Database};
    use rustfrp_client::process::manager::ProcessManager;
    use rustfrp_client::ClientState;
    use rustfrp_common::signal::SignalHandler;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    /// Counts and process rows follow `environment_id`, while proxies stay
    /// tenant-wide because a proxy can be bound by profiles in any environment.
    #[tokio::test]
    async fn status_counts_and_processes_follow_the_selected_environment() {
        let root = tempfile::tempdir().unwrap();
        let db = Database::open(root.path().join("test.db").to_str().unwrap())
            .await
            .unwrap();
        migrate::run(&*db.lock().await).unwrap();

        let first = db
            .insert_profile(&FrpsProfile {
                name: "first".into(),
                server_addr: "127.0.0.1".into(),
                ..Default::default()
            })
            .await
            .unwrap();
        let second = db
            .insert_profile(&FrpsProfile {
                name: "second".into(),
                server_addr: "127.0.0.1".into(),
                ..Default::default()
            })
            .await
            .unwrap();
        let other = db
            .insert_environment(&Environment {
                name: "other".into(),
                ..Default::default()
            })
            .await
            .unwrap();
        db.set_profile_environment(second, other).await.unwrap();

        let proxy = db
            .insert_proxy(&LocalProxy {
                name: "shared".into(),
                local_port: 8080,
                ..Default::default()
            })
            .await
            .unwrap();
        for profile_id in [first, second] {
            db.insert_binding(&BindingRule {
                profile_id,
                proxy_id: proxy,
                ..Default::default()
            })
            .await
            .unwrap();
            db.insert_visitor(&LocalVisitor {
                name: format!("visitor-{profile_id}"),
                server_name: "server".into(),
                profile_id,
                ..Default::default()
            })
            .await
            .unwrap();
        }

        let state = ApiState::new(
            db,
            ProcessManager::new(
                root.path().into(),
                "/unused/frpc".into(),
                SignalHandler::new(),
            ),
            root.path().into(),
            Arc::new(RwLock::new(ClientState::Ready)),
        );
        let identity = AuthIdentity {
            name: "test".into(),
            tenant: "default".into(),
            scopes: vec!["*".into()],
        };

        let all = status(
            State(state.clone()),
            Extension(identity.clone()),
            Query(StatusQuery::default()),
        )
        .await
        .unwrap()
        .0
        .data
        .unwrap();
        assert_eq!(all.total_profiles, 2);
        assert_eq!(all.total_proxies, 1);
        assert_eq!(all.total_bindings, 2);
        assert_eq!(all.total_visitors, 2);

        let scoped = status(
            State(state.clone()),
            Extension(identity.clone()),
            Query(StatusQuery {
                environment_id: Some(other),
            }),
        )
        .await
        .unwrap()
        .0
        .data
        .unwrap();
        assert_eq!(scoped.total_profiles, 1);
        assert_eq!(scoped.total_bindings, 1);
        assert_eq!(scoped.total_visitors, 1);
        assert_eq!(
            scoped.total_proxies, 1,
            "proxies are reusable across environments"
        );
        assert!(scoped.processes.iter().all(|p| p.profile_id == second));

        // A foreign environment id behaves like any other hidden resource.
        let foreign = status(
            State(state),
            Extension(AuthIdentity {
                tenant: "elsewhere".into(),
                ..identity
            }),
            Query(StatusQuery {
                environment_id: Some(other),
            }),
        )
        .await;
        assert_eq!(foreign.unwrap_err().0, StatusCode::NOT_FOUND);
    }
}
