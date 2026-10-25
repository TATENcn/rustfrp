//! Resource/traffic history and Prometheus exposition.

use axum::body::Body;
use axum::extract::{Extension, Query, State};
use axum::http::{header, Response, StatusCode};
use axum::Json;
use serde::{Deserialize, Serialize};

use super::auth::AuthIdentity;
use super::response::{ApiError, ApiResponse};
use super::state::ApiState;
use crate::metrics::{ResourceSample, TrafficSample, METRICS_HISTORY_CAPACITY};

#[derive(Debug, Default, Deserialize)]
pub struct HistoryQuery {
    pub environment_id: Option<i64>,
    pub profile_id: Option<i64>,
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct HistoryResponse {
    pub resources: Vec<ResourceSample>,
    pub traffic: Vec<TrafficSample>,
    pub proxy_traffic: Vec<TrafficSample>,
    pub host_traffic: Vec<TrafficSample>,
    pub traffic_collection: crate::traffic::CollectionStatus,
}

pub async fn history(
    State(state): State<ApiState>,
    Query(query): Query<HistoryQuery>,
    Extension(identity): Extension<AuthIdentity>,
) -> Json<ApiResponse<HistoryResponse>> {
    let limit = query
        .limit
        .unwrap_or(METRICS_HISTORY_CAPACITY)
        .clamp(1, METRICS_HISTORY_CAPACITY);
    let mut resources = state.metrics.resources(limit).await;
    let mut traffic = state
        .metrics
        .traffic(
            query.environment_id,
            query.profile_id,
            query.limit.unwrap_or(METRICS_HISTORY_CAPACITY * 10),
        )
        .await;
    let mut traffic_collection = state.metrics.collection().await;
    let mut allowed = std::collections::HashSet::new();
    if let Ok(profiles) = state.db.list_profiles().await {
        for profile in profiles {
            if let Some(id) = profile.id {
                if state
                    .db
                    .profile_belongs_to_tenant(id, &identity.tenant)
                    .await
                    .unwrap_or(false)
                {
                    allowed.insert(id);
                }
            }
        }
    }
    traffic.retain(|sample| allowed.contains(&sample.profile_id));
    traffic_collection.profiles.retain(|item| {
        allowed.contains(&item.profile_id)
            && query
                .environment_id
                .is_none_or(|id| id == item.environment_id)
            && query.profile_id.is_none_or(|id| id == item.profile_id)
    });
    for sample in &mut resources {
        sample.processes.retain(|p| allowed.contains(&p.profile_id));
    }
    let proxy_traffic = state
        .metrics
        .proxy_traffic()
        .await
        .into_iter()
        .filter(|s| {
            allowed.contains(&s.profile_id)
                && query.environment_id.is_none_or(|id| id == s.environment_id)
                && query.profile_id.is_none_or(|id| id == s.profile_id)
        })
        .collect();
    traffic_collection.proxies.retain(|s| {
        allowed.contains(&s.profile_id)
            && query.environment_id.is_none_or(|id| id == s.environment_id)
            && query.profile_id.is_none_or(|id| id == s.profile_id)
    });
    let host_traffic = state.metrics.host_traffic().await;
    Json(ApiResponse::ok(HistoryResponse {
        resources,
        traffic,
        proxy_traffic,
        host_traffic,
        traffic_collection,
    }))
}

pub async fn ingest_traffic(
    State(state): State<ApiState>,
    Extension(identity): Extension<AuthIdentity>,
    Json(mut sample): Json<TrafficSample>,
) -> Result<(StatusCode, Json<ApiResponse<()>>), (StatusCode, Json<ApiResponse<()>>)> {
    let belongs = state
        .db
        .profile_belongs_to_tenant(sample.profile_id, &identity.tenant)
        .await
        .map_err(|error| {
            (
                super::response::status_code(&error),
                Json(ApiResponse {
                    success: false,
                    data: None,
                    count: None,
                    error: Some(ApiError::from_client_error(&error)),
                }),
            )
        })?;
    if !belongs {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ApiResponse {
                success: false,
                data: None,
                count: None,
                error: Some(ApiError::generic(
                    "TENANT_NOT_FOUND",
                    "profile was not found in the active tenant".into(),
                )),
            }),
        ));
    }
    let expected = state
        .db
        .profile_environment_id(sample.profile_id)
        .await
        .map_err(|error| {
            (
                super::response::status_code(&error),
                Json(ApiResponse {
                    success: false,
                    data: None,
                    count: None,
                    error: Some(ApiError::from_client_error(&error)),
                }),
            )
        })?;
    if sample.environment_id != expected {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ApiResponse {
                success: false,
                data: None,
                count: None,
                error: Some(ApiError::generic(
                    "METRIC_ENV_MISMATCH",
                    "profile does not belong to the supplied environment".into(),
                )),
            }),
        ));
    }
    sample.timestamp = chrono::Utc::now();
    sample.source = "external".into();
    sample.proxy_id = None;
    sample.interface = None;
    state.metrics.push_traffic(sample).await;
    Ok((StatusCode::ACCEPTED, Json(ApiResponse::ok(()))))
}

pub async fn prometheus(State(state): State<ApiState>) -> Response<Body> {
    let resources = state.metrics.resources(1).await;
    let traffic = state.metrics.traffic(None, None, 360).await;
    let mut output = String::from(
        "# HELP rustfrp_daemon_cpu_percent Daemon CPU usage percent.\n\
         # TYPE rustfrp_daemon_cpu_percent gauge\n\
         # HELP rustfrp_daemon_memory_bytes Daemon resident memory.\n\
         # TYPE rustfrp_daemon_memory_bytes gauge\n",
    );
    if let Some(sample) = resources.last() {
        output.push_str(&format!(
            "rustfrp_daemon_cpu_percent {}\nrustfrp_daemon_memory_bytes {}\n",
            sample.daemon_cpu_percent, sample.daemon_memory_bytes
        ));
        output.push_str(&format!("rustfrp_system_cpu_percent {}\nrustfrp_system_memory_used_bytes {}\nrustfrp_system_memory_total_bytes {}\n", sample.system_cpu_percent, sample.system_memory_used_bytes, sample.system_memory_total_bytes));
        output.push_str(&format!(
            "rustfrp_ebpf_available {}\nrustfrp_ebpf_pinned_objects {}\n",
            u8::from(sample.ebpf.available),
            sample.ebpf.pinned_objects
        ));
        if let Some(value) = sample.ebpf.unprivileged_disabled {
            output.push_str(&format!("rustfrp_ebpf_unprivileged_disabled {value}\n"));
        }
        for process in &sample.processes {
            output.push_str(&format!(
                "rustfrp_frpc_cpu_percent{{profile_id=\"{}\",pid=\"{}\"}} {}\n",
                process.profile_id, process.pid, process.cpu_percent
            ));
            output.push_str(&format!(
                "rustfrp_frpc_memory_bytes{{profile_id=\"{}\",pid=\"{}\"}} {}\n",
                process.profile_id, process.pid, process.memory_bytes
            ));
        }
    }
    for sample in latest_traffic_by_profile(traffic) {
        output.push_str(&format!(
            "rustfrp_traffic_received_bytes{{environment_id=\"{}\",profile_id=\"{}\"}} {}\n",
            sample.environment_id, sample.profile_id, sample.received_bytes
        ));
        output.push_str(&format!(
            "rustfrp_traffic_sent_bytes{{environment_id=\"{}\",profile_id=\"{}\"}} {}\n",
            sample.environment_id, sample.profile_id, sample.sent_bytes
        ));
        output.push_str(&format!(
            "rustfrp_active_connections{{environment_id=\"{}\",profile_id=\"{}\"}} {}\n",
            sample.environment_id, sample.profile_id, sample.active_connections
        ));
    }
    Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )
        .body(Body::from(output))
        .expect("static metrics headers")
}

fn latest_traffic_by_profile(samples: Vec<TrafficSample>) -> Vec<TrafficSample> {
    let mut latest = std::collections::BTreeMap::new();
    for sample in samples {
        latest.insert((sample.environment_id, sample.profile_id), sample);
    }
    latest.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustfrp_client::{
        config::model::FrpsProfile,
        db::{environment::Environment, Database},
        process::manager::ProcessManager,
        ClientState,
    };
    use rustfrp_common::signal::SignalHandler;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    #[tokio::test]
    async fn traffic_history_and_collection_status_are_tenant_and_environment_scoped() {
        let root = tempfile::tempdir().unwrap();
        let db = Database::open(root.path().join("test.db").to_str().unwrap())
            .await
            .unwrap();
        rustfrp_client::db::migrate::run(&*db.lock().await).unwrap();
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
        let env = db
            .insert_environment(&Environment {
                name: "other".into(),
                tenant_id: "other".into(),
                is_default: true,
                ..Default::default()
            })
            .await
            .unwrap();
        db.set_profile_environment(second, env).await.unwrap();
        let own_env = db.profile_environment_id(first).await.unwrap();
        let manager = ProcessManager::new(
            root.path().into(),
            "/unused/frpc".into(),
            SignalHandler::new(),
        );
        let state = ApiState::new(
            db,
            manager,
            root.path().into(),
            Arc::new(RwLock::new(ClientState::Ready)),
        );
        for (profile_id, environment_id) in [(first, own_env), (second, env)] {
            state
                .metrics
                .push_traffic(TrafficSample {
                    timestamp: chrono::Utc::now(),
                    profile_id,
                    environment_id,
                    received_bytes: 100,
                    sent_bytes: 200,
                    active_connections: 1,
                    source: "linux_tcp".into(),
                    counter_epoch: "test".into(),
                    proxy_id: None,
                    interface: None,
                })
                .await;
        }
        for (profile_id, environment_id) in [(first, own_env), (second, env)] {
            state
                .metrics
                .push_traffic(TrafficSample {
                    timestamp: chrono::Utc::now(),
                    profile_id,
                    environment_id,
                    received_bytes: 300,
                    sent_bytes: 400,
                    active_connections: 1,
                    source: "linux_local_tcp".into(),
                    counter_epoch: "test".into(),
                    proxy_id: Some(123),
                    interface: None,
                })
                .await;
        }
        state
            .metrics
            .set_collection(crate::traffic::CollectionStatus {
                state: "ready",
                profiles: vec![
                    crate::traffic::ProfileCollection {
                        profile_id: first,
                        environment_id: own_env,
                        state: "collecting",
                    },
                    crate::traffic::ProfileCollection {
                        profile_id: second,
                        environment_id: env,
                        state: "collecting",
                    },
                ],
                proxies: vec![
                    crate::traffic::ProxyCollection {
                        profile_id: first,
                        environment_id: own_env,
                        proxy_id: 123,
                        name: "own".into(),
                        state: "collecting",
                    },
                    crate::traffic::ProxyCollection {
                        profile_id: second,
                        environment_id: env,
                        proxy_id: 123,
                        name: "other".into(),
                        state: "collecting",
                    },
                ],
                ..Default::default()
            })
            .await;
        let identity = AuthIdentity {
            name: "test".into(),
            tenant: "default".into(),
            scopes: vec![],
        };
        let Json(response) = history(
            State(state.clone()),
            Query(HistoryQuery::default()),
            Extension(identity.clone()),
        )
        .await;
        let payload = response.data.unwrap();
        assert_eq!(payload.proxy_traffic.len(), 1);
        assert_eq!(payload.proxy_traffic[0].profile_id, first);
        assert_eq!(payload.traffic_collection.proxies.len(), 1);
        assert_eq!(payload.traffic.len(), 1);
        assert_eq!(payload.traffic[0].profile_id, first);
        assert_eq!(payload.traffic_collection.profiles.len(), 1);
        let Json(response) = history(
            State(state),
            Query(HistoryQuery {
                environment_id: Some(env),
                ..Default::default()
            }),
            Extension(identity),
        )
        .await;
        let payload = response.data.unwrap();
        assert!(payload.traffic.is_empty());
        assert!(payload.proxy_traffic.is_empty());
        assert!(payload.traffic_collection.proxies.is_empty());
        assert!(payload.traffic_collection.profiles.is_empty());
    }
}
