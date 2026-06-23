//! 监控 Web API
//!
//! 提供 HTTP API 供 Grafana / 前端消费。
//! 绝对只读——不提供任何配置下发接口（ARCH-007）。

use crate::health::TargetRegistry;
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::get,
    Router,
};
use serde_json::{json, Value};
use std::sync::Arc;

/// 应用状态
#[derive(Debug, Clone)]
pub struct AppState {
    pub targets: Arc<TargetRegistry>,
}

/// 创建路由
pub fn create_router(targets: Arc<TargetRegistry>) -> Router {
    let state = AppState { targets };

    Router::new()
        .route("/", get(index))
        .route("/health", get(health))
        .route("/api/v1/nodes", get(list_nodes))
        .route("/api/v1/status", get(status))
        .with_state(state)
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
        .filter(|n| {
            matches!(
                n.state,
                crate::health::NodeState::Unhealthy { .. }
            )
        })
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
        let targets = crate::health::TargetRegistry::from_file("nonexistent.json")
            .unwrap_or_else(|_| {
                // 空注册表
                TargetRegistry::from_json("[]").unwrap()
            });
        let app = create_router(targets);

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
}
