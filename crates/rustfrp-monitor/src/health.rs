//! 节点健康状态管理
//!
//! 管理被监控 FRPS 节点的健康状态。
//! 健康 → 不健康 → 离线 状态机。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 目标 FRPS 节点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetNode {
    /// 节点唯一 ID
    pub id: String,
    /// 节点名称（用于显示）
    pub name: String,
    /// /metrics 端点 URL
    pub metrics_url: String,
}

/// 节点状态
#[derive(Debug, Clone)]
pub enum NodeState {
    /// 健康：最近拉取成功
    Healthy {
        last_scrap: DateTime<Utc>,
        /// 原始 metrics 文本（Phase 2 Prometheus 集成用）
        #[allow(dead_code)]
        metrics_body: String,
    },
    /// 不健康：拉取失败
    Unhealthy {
        last_error: String,
        consecutive_failures: u32,
    },
    /// 离线：从未成功拉取过
    Offline,
}

/// 节点运行时信息
#[derive(Debug, Clone)]
pub struct NodeInfo {
    pub node: TargetNode,
    pub state: NodeState,
    pub consecutive_failures: u32,
}

/// 目标节点注册表
///
/// 线程安全的共享状态，供 scraper 和 web 模块共用。
#[derive(Debug)]
pub struct TargetRegistry {
    nodes: Arc<RwLock<HashMap<String, TargetNode>>>,
    states: Arc<RwLock<HashMap<String, NodeState>>>,
}

impl TargetRegistry {
    /// 从 JSON 字符串直接构建（测试/简单场景用）
    #[allow(dead_code)]
    pub fn from_json(json: &str) -> Result<Arc<Self>, anyhow::Error> {
        let nodes: Vec<TargetNode> = serde_json::from_str(json)?;
        let nodes_map: HashMap<String, TargetNode> = nodes
            .into_iter()
            .map(|n| (n.id.clone(), n))
            .collect();

        Ok(Arc::new(Self {
            nodes: Arc::new(RwLock::new(nodes_map)),
            states: Arc::new(RwLock::new(HashMap::new())),
        }))
    }

    /// 从 JSON 文件加载目标节点
    pub fn from_file(path: &str) -> Result<Arc<Self>, anyhow::Error> {
        let content = std::fs::read_to_string(path)?;
        let nodes: Vec<TargetNode> = serde_json::from_str(&content)?;

        let nodes_map: HashMap<String, TargetNode> = nodes
            .into_iter()
            .map(|n| (n.id.clone(), n))
            .collect();

        tracing::info!(count = nodes_map.len(), "目标节点加载完成");

        Ok(Arc::new(Self {
            nodes: Arc::new(RwLock::new(nodes_map)),
            states: Arc::new(RwLock::new(HashMap::new())),
        }))
    }

    /// 列出所有节点
    pub async fn list_nodes(&self) -> Vec<NodeInfo> {
        let nodes = self.nodes.read().await;
        let states = self.states.read().await;

        nodes
            .values()
            .map(|node| {
                let state = states
                    .get(&node.id)
                    .cloned()
                    .unwrap_or(NodeState::Offline);
                let failures = match &state {
                    NodeState::Unhealthy {
                        consecutive_failures,
                        ..
                    } => *consecutive_failures,
                    _ => 0,
                };
                NodeInfo {
                    node: node.clone(),
                    state,
                    consecutive_failures: failures,
                }
            })
            .collect()
    }

    /// 更新节点状态
    pub async fn update_state(&self, node_id: &str, state: NodeState) {
        self.states.write().await.insert(node_id.to_string(), state);
    }

    /// 节点数量（异步安全）
    pub async fn len(&self) -> usize {
        self.nodes.read().await.len()
    }

    /// 是否为空
    #[allow(dead_code)]
    pub async fn is_empty(&self) -> bool {
        self.len().await == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_load_from_file() {
        let tmp = NamedTempFile::new().unwrap();
        let content = r#"[
            {"id": "frps-1", "name": "FRPS #1", "metrics_url": "http://1.2.3.4:7400/metrics"},
            {"id": "frps-2", "name": "FRPS #2", "metrics_url": "http://5.6.7.8:7400/metrics"}
        ]"#;
        std::fs::write(tmp.path(), content).unwrap();

        let registry = TargetRegistry::from_file(tmp.path().to_str().unwrap()).unwrap();
        assert_eq!(registry.len().await, 2);
    }
}
