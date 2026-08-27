//! Resource/traffic history and Prometheus exposition.

use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{header, Response, StatusCode};
use axum::Json;
use serde::{Deserialize, Serialize};

use super::response::{ApiError, ApiResponse};
use super::state::ApiState;
use crate::metrics::{ResourceSample, TrafficSample};

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
}

pub async fn history(
    State(state): State<ApiState>,
    Query(query): Query<HistoryQuery>,
) -> Json<ApiResponse<HistoryResponse>> {
    let limit = query.limit.unwrap_or(360).clamp(1, 360);
    let resources = state.metrics.resources(limit).await;
    let traffic = state
        .metrics
        .traffic(query.environment_id, query.profile_id, limit)
        .await;
    Json(ApiResponse::ok(HistoryResponse { resources, traffic }))
}

pub async fn ingest_traffic(
    State(state): State<ApiState>,
    Json(mut sample): Json<TrafficSample>,
) -> Result<(StatusCode, Json<ApiResponse<()>>), (StatusCode, Json<ApiResponse<()>>)> {
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
