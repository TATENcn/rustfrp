//! 进程管理器
//!
//! 管理多个 frpc ProcessGuard 实例，每个 FrpsProfile 对应一个 frpc 子进程。
//! 负责进程的启动、停止、热重载和生命周期编排（ARCH-009）。

use crate::error::{ClientError, Result};
use crate::process::diagnostic::ProcessFailure;
use crate::process::guard::ProcessGuard;
use rustfrp_common::signal::SignalHandler;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 进程操作结果（start/stop/reload 等动作的结果）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessAction {
    /// 启动了新的 frpc 进程
    Started,
    /// 对已运行的 frpc 发送了 SIGHUP 热重载
    Reloaded,
    /// frpc 进程已停止
    Stopped,
    /// 幂等：已经在运行
    AlreadyRunning,
    /// 幂等：本来就没在运行
    NotRunning,
}

impl ProcessAction {
    /// 返回 API 响应中 `process_status` 字段的字符串值
    pub fn as_str(&self) -> &'static str {
        match self {
            ProcessAction::Started => "started",
            ProcessAction::Reloaded => "reloaded",
            ProcessAction::Stopped => "stopped",
            ProcessAction::AlreadyRunning => "already_running",
            ProcessAction::NotRunning => "not_running",
        }
    }
}

/// 进程运行信息（外部查询用）
#[derive(Debug, Clone)]
pub struct ProcessInfo {
    pub profile_id: i64,
    pub profile_name: String,
    pub pid: Option<u32>,
    pub running: bool,
    pub restart_count: u32,
    pub last_failure: Option<ProcessFailure>,
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
    /// rustfrp 托管的 frpc 二进制绝对路径
    frpc_path: Arc<RwLock<PathBuf>>,
    /// 共享的信号处理器
    signal_handler: SignalHandler,
}

impl ProcessManager {
    /// 创建新的进程管理器
    pub fn new(config_dir: PathBuf, frpc_path: PathBuf, signal_handler: SignalHandler) -> Self {
        Self {
            guards: Arc::new(RwLock::new(HashMap::new())),
            config_dir,
            frpc_path: Arc::new(RwLock::new(frpc_path)),
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

        let guard = ProcessGuard::new(
            toml_path.clone(),
            self.frpc_path.read().await.clone(),
            self.signal_handler.clone(),
            profile_name.to_string(),
        );
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

    /// 确保指定 Profile 的 frpc 在运行。
    ///
    /// - 未运行 → 启动新进程，返回 `ProcessAction::Started`
    /// - 已运行 → 热重载（SIGHUP），返回 `ProcessAction::Reloaded`
    pub async fn ensure_running(
        &self,
        profile_id: i64,
        profile_name: &str,
    ) -> Result<ProcessAction> {
        if self.is_running(profile_id).await {
            self.reload(profile_id).await?;
            Ok(ProcessAction::Reloaded)
        } else {
            self.start(profile_id, profile_name).await?;
            Ok(ProcessAction::Started)
        }
    }

    /// 根据是否有其他运行中的 binding 来决定停止还是热重载。
    ///
    /// - `has_other_running`: 该 profile 下是否还有其它 `running=true` 的 binding
    ///   - `true`  → 热重载（TOML 排除已停止的 proxy），返回 `ProcessAction::Reloaded`
    ///   - `false` → 停止 frpc 进程，返回 `ProcessAction::Stopped`
    pub async fn stop_if_idle(
        &self,
        profile_id: i64,
        has_other_running: bool,
    ) -> Result<ProcessAction> {
        if has_other_running {
            self.reload(profile_id).await?;
            Ok(ProcessAction::Reloaded)
        } else {
            self.stop(profile_id).await?;
            Ok(ProcessAction::Stopped)
        }
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
                last_failure: guard.last_failure(),
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

    /// Update the binary used for subsequently started processes.
    /// Callers must coordinate stopping/restarting existing guards.
    pub async fn set_frpc_path(&self, path: PathBuf) {
        *self.frpc_path.write().await = path;
    }

    pub async fn frpc_path(&self) -> PathBuf {
        self.frpc_path.read().await.clone()
    }

    /// 获取指定 Profile 的 stdout 日志文件路径
    pub async fn get_stdout_log_path(&self, profile_id: i64) -> Option<PathBuf> {
        self.guards
            .read()
            .await
            .get(&profile_id)
            .map(|g| g.stdout_log_path())
    }

    /// 获取指定 Profile 的 stderr 日志文件路径
    pub async fn get_stderr_log_path(&self, profile_id: i64) -> Option<PathBuf> {
        self.guards
            .read()
            .await
            .get(&profile_id)
            .map(|g| g.stderr_log_path())
    }

    /// 获取日志目录路径
    pub fn log_dir(&self) -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".rustfrp")
            .join("logs")
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
        let manager = ProcessManager::new(
            tmp.path().to_path_buf(),
            std::path::PathBuf::from("/nonexistent/frpc"),
            handler,
        );
        assert_eq!(manager.active_count().await, 0);
    }
}
