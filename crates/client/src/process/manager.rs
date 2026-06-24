//! 进程管理器
//!
//! 管理多个 frpc ProcessGuard 实例，每个 FrpsProfile 对应一个 frpc 子进程。
//! 负责进程的启动、停止、热重载和生命周期编排（ARCH-009）。

use crate::error::{ClientError, Result};
use crate::process::guard::ProcessGuard;
use rustfrp_common::signal::SignalHandler;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 进程运行信息（外部查询用）
#[derive(Debug, Clone)]
pub struct ProcessInfo {
    pub profile_id: i64,
    pub profile_name: String,
    pub pid: Option<u32>,
    pub running: bool,
    pub restart_count: u32,
    pub config_path: String,
}

/// 进程管理器
///
/// 持有所有 frpc ProcessGuard 实例的映射表。
/// 一个 FrpsProfile → 一个 ProcessGuard → 一个 frpc 子进程。
#[derive(Debug, Clone)]
pub struct ProcessManager {
    /// Profile ID → ProcessGuard
    guards: Arc<RwLock<HashMap<i64, ProcessGuard>>>,
    /// TOML 配置文件输出目录
    config_dir: PathBuf,
    /// 共享的信号处理器
    signal_handler: SignalHandler,
}

impl ProcessManager {
    /// 创建新的进程管理器
    pub fn new(config_dir: PathBuf, signal_handler: SignalHandler) -> Self {
        Self {
            guards: Arc::new(RwLock::new(HashMap::new())),
            config_dir,
            signal_handler,
        }
    }

    /// 获取配置目录
    pub fn config_dir(&self) -> &PathBuf {
        &self.config_dir
    }

    /// 启动指定 Profile 对应的 frpc 实例
    ///
    /// 若该 profile_id 已有运行中实例，先停止旧的再启动新的。
    pub async fn start(&self, profile_id: i64, profile_name: &str) -> Result<()> {
        let toml_path = self
            .config_dir
            .join(sanitize_filename(profile_name) + ".toml");

        if !toml_path.exists() {
            return Err(ClientError::ProcessStart(format!(
                "TOML config not found: {}",
                toml_path.display()
            )));
        }

        // 若已存在，先停止
        self.stop(profile_id).await?;

        let guard = ProcessGuard::new(toml_path.clone(), self.signal_handler.clone());
        guard.start().await?;

        let pid = guard.pid();
        self.guards.write().await.insert(profile_id, guard);

        tracing::info!(profile_id, profile_name, pid, "frpc instance started");
        Ok(())
    }

    /// 停止指定 Profile 的 frpc 实例
    pub async fn stop(&self, profile_id: i64) -> Result<()> {
        if let Some(guard) = self.guards.write().await.remove(&profile_id) {
            guard.shutdown().await?;
            tracing::info!(profile_id, "frpc instance stopped");
        }
        Ok(())
    }

    /// 热重载指定 Profile 的 frpc 实例
    pub async fn reload(&self, profile_id: i64) -> Result<()> {
        let guards = self.guards.read().await;
        let guard = guards.get(&profile_id).ok_or_else(|| {
            ClientError::ProcessCommunication(format!(
                "frpc instance not found for profile {profile_id}"
            ))
        })?;

        guard.reload().await
    }

    /// 停止所有 frpc 实例
    pub async fn shutdown_all(&self) -> Result<()> {
        let profile_ids: Vec<i64> = self.guards.read().await.keys().copied().collect();

        for id in profile_ids {
            if let Err(e) = self.stop(id).await {
                tracing::warn!(profile_id = id, error = %e, "Error stopping frpc instance");
            }
        }

        tracing::info!("All frpc instances stopped");
        Ok(())
    }

    /// 列出所有及运行中的实例信息
    pub async fn list_running(&self) -> Vec<ProcessInfo> {
        self.guards
            .read()
            .await
            .iter()
            .map(|(&profile_id, guard)| ProcessInfo {
                profile_id,
                profile_name: String::new(), // 由上层调用方补充
                pid: guard.pid(),
                running: guard.is_running(),
                restart_count: guard.restart_count(),
                config_path: guard.config_path().to_string_lossy().to_string(),
            })
            .collect()
    }

    /// 查询指定 Profile 是否正在运行
    pub async fn is_running(&self, profile_id: i64) -> bool {
        self.guards
            .read()
            .await
            .get(&profile_id)
            .map(|g| g.is_running())
            .unwrap_or(false)
    }

    /// 更新指定实例的 profile_name 字段（供 list_running 后的补充展示）
    pub async fn update_process_info(&self, infos: &mut [ProcessInfo], db: &crate::db::Database) {
        for info in infos.iter_mut() {
            if let Ok(profile) = db.get_profile(info.profile_id).await {
                info.profile_name = profile.name;
            }
        }
    }

    /// 活动实例数量
    pub async fn active_count(&self) -> usize {
        self.guards
            .read()
            .await
            .values()
            .filter(|g| g.is_running())
            .count()
    }
}

use crate::config::generator::sanitize_filename;

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_process_manager_new() {
        let tmp = TempDir::new().unwrap();
        let handler = SignalHandler::new();
        let manager = ProcessManager::new(tmp.path().to_path_buf(), handler);
        assert_eq!(manager.active_count().await, 0);
    }
}
