//! 监控 Web API
//!
//! 提供 HTTP API 供 Grafana / 前端消费。
//! 绝对只读——不提供任何配置下发接口（ARCH-007）。

use crate::health::TargetRegistry;
use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, HeaderMap, Response, StatusCode},
    response::{IntoResponse, Json},
    routing::get,
    Router,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::Arc;

/// 应用状态
#[derive(Debug, Clone)]
pub struct AppState {
    pub targets: Arc<TargetRegistry>,
    pub templates_dir: Option<PathBuf>,
    pub agent_token: Option<String>,
}

/// 创建路由
pub fn create_router(
    targets: Arc<TargetRegistry>,
    templates_dir: Option<PathBuf>,
    agent_token: Option<String>,
) -> Router {
    let state = AppState {
        targets,
        templates_dir,
        agent_token,
    };

    Router::new()
        .route("/", get(index))
        .route("/health", get(health))
        .route("/api/v1/nodes", get(list_nodes))
        .route("/api/v1/status", get(status))
        .route("/api/v1/agent/config/:node_id", get(agent_config))
        .with_state(state)
}

/// Read-only Pull endpoint from ADR-005. Templates are never mutated here.
async fn agent_config(
    State(state): State<AppState>,
    Path(node_id): Path<String>,
    headers: HeaderMap,
) -> Response<Body> {
    let (Some(directory), Some(expected_token)) = (&state.templates_dir, &state.agent_token) else {
        return response(StatusCode::NOT_FOUND, "agent templates are disabled");
    };
    if !valid_node_id(&node_id) {
        return response(StatusCode::BAD_REQUEST, "invalid node ID");
    }
    let authorized = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .is_some_and(|value| value == expected_token);
    if !authorized {
        return response(StatusCode::UNAUTHORIZED, "unauthorized");
    }
    let path = directory.join(format!("{node_id}.toml"));
    let contents = match tokio::fs::read(&path).await {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return response(StatusCode::NOT_FOUND, "template not found")
        }
        Err(error) => {
            tracing::error!(%error, path = %path.display(), "Failed to read agent template");
            return response(StatusCode::INTERNAL_SERVER_ERROR, "template read failed");
        }
    };
    let etag = format!("\"{:x}\"", Sha256::digest(&contents));
    if headers
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == etag)
    {
        return Response::builder()
            .status(StatusCode::NOT_MODIFIED)
            .header(header::ETAG, etag)
            .body(Body::empty())
            .expect("static response is valid");
    }
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/toml; charset=utf-8")
        .header(header::ETAG, etag)
        .header(header::CACHE_CONTROL, "no-cache")
        .body(Body::from(contents))
        .expect("static response is valid")
}

fn valid_node_id(node_id: &str) -> bool {
    !node_id.is_empty()
        && node_id.len() <= 64
        && node_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn response(status: StatusCode, message: &'static str) -> Response<Body> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(Body::from(message))
        .expect("static response is valid")
}

/// 首页
async fn index() -> impl IntoResponse {
    Json(json!({
        "service": "rustfrp-monitor",
        "version": env!("CARGO_PKG_VERSION"),
        "endpoints": {
            "/health": "GET - 服务健康检查",
            "/api/v1/nodes": "GET - 列出所有节点及状态",
            "/api/v1/status": "GET - 全局状态摘要"
        }
    }))
}

/// 健康检查
async fn health() -> StatusCode {
    StatusCode::OK
}

/// 列出所有节点及其当前状态
async fn list_nodes(State(state): State<AppState>) -> Json<Value> {
    let nodes = state.targets.list_nodes().await;

    let node_list: Vec<Value> = nodes
        .iter()
        .map(|info| {
            let status = match &info.state {
                crate::health::NodeState::Healthy { last_scrap, .. } => {
                    json!({
                        "status": "healthy",
                        "last_scrap": last_scrap.to_rfc3339()
                    })
                }
                crate::health::NodeState::Unhealthy {
                    last_error,
                    consecutive_failures,
                } => {
                    json!({
                        "status": "unhealthy",
                        "error": last_error,
                        "consecutive_failures": consecutive_failures
                    })
                }
                crate::health::NodeState::Offline => {
                    json!({
                        "status": "offline"
                    })
                }
            };

            json!({
                "id": info.node.id,
                "name": info.node.name,
                "metrics_url": info.node.metrics_url,
                "state": status
            })
        })
        .collect();

    Json(json!({ "nodes": node_list }))
}

/// 全局状态摘要
async fn status(State(state): State<AppState>) -> Json<Value> {
    let nodes = state.targets.list_nodes().await;

    let total = nodes.len();
    let healthy = nodes
        .iter()
        .filter(|n| matches!(n.state, crate::health::NodeState::Healthy { .. }))
        .count();
    let unhealthy = nodes
        .iter()
        .filter(|n| matches!(n.state, crate::health::NodeState::Unhealthy { .. }))
        .count();
    let offline = total - healthy - unhealthy;

    Json(json!({
        "total_nodes": total,
        "healthy": healthy,
        "unhealthy": unhealthy,
        "offline": offline,
        "health_percentage": if total > 0 {
            (healthy as f64 / total as f64 * 100.0).round()
        } else {
            100.0
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_health_endpoint() {
        let targets =
            crate::health::TargetRegistry::from_file("nonexistent.json").unwrap_or_else(|_| {
                // 空注册表
                TargetRegistry::from_json("[]").unwrap()
            });
        let app = create_router(targets, None, None);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn serves_authenticated_template_with_etag() {
        let temp = tempfile::tempdir().unwrap();
        tokio::fs::write(temp.path().join("edge-1.toml"), "bindPort = 7000\n")
            .await
            .unwrap();
        let targets = TargetRegistry::from_json("[]").unwrap();
        let app = create_router(targets, Some(temp.path().into()), Some("secret".into()));

        let unauthorized = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/agent/config/edge-1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

        let first = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/agent/config/edge-1")
                    .header(header::AUTHORIZATION, "Bearer secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(first.status(), StatusCode::OK);
        let etag = first.headers()[header::ETAG].clone();

        let cached = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/agent/config/edge-1")
                    .header(header::AUTHORIZATION, "Bearer secret")
                    .header(header::IF_NONE_MATCH, etag)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(cached.status(), StatusCode::NOT_MODIFIED);
    }
}
