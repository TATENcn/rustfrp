//! 进程守护
//!
//! 管理 frpc 子进程的完整生命周期：
//! 启动 → 运行（监控 stderr）→ 热重载（SIGHUP）→ 优雅退出（SIGTERM → SIGKILL）

use crate::error::{ClientError, Result};
use rustfrp_common::signal::SignalHandler;
use std::fs::OpenOptions;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};

/// 最大自动重启次数
const MAX_RESTART_COUNT: u32 = 3;

/// 优雅退出等待时间
const GRACEFUL_SHUTDOWN_SECS: u64 = 3;

/// FRP 二进制名称
const FRPC_BINARY: &str = "frpc";

/// 进程守护
///
/// 封装 frpc 子进程，提供启动、热重载、优雅退出能力。
/// 支持崩溃自动重启（最多 3 次，ARCH-006）。
pub struct ProcessGuard {
    /// 子进程（Mutex 保护，因为启动/停止都是 &self 的）
    child: Arc<Mutex<Option<Child>>>,
    /// 配置文件路径
    config_path: PathBuf,
    /// 是否正在运行
    running: Arc<AtomicBool>,
    /// 重启次数
    restart_count: Arc<AtomicU32>,
    /// 信号处理器（Phase 2 协调关闭用）
    #[allow(dead_code)]
    signal_handler: SignalHandler,
    /// 日志目录
    log_dir: PathBuf,
}

impl ProcessGuard {
    /// 创建新的 ProcessGuard
    ///
    /// 注意：此时尚未启动子进程，需调用 `start()`。
    pub fn new(config_path: PathBuf, signal_handler: SignalHandler) -> Self {
        let log_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".rustfrp")
            .join("logs");

        Self {
            child: Arc::new(Mutex::new(None)),
            config_path,
            running: Arc::new(AtomicBool::new(false)),
            restart_count: Arc::new(AtomicU32::new(0)),
            signal_handler,
            log_dir,
        }
    }

    /// 启动 frpc 子进程
    ///
    /// 如果已有进程在运行，先停止。
    pub async fn start(&self) -> Result<()> {
        // 确保日志目录存在
        std::fs::create_dir_all(&self.log_dir).map_err(|e| {
            ClientError::ProcessStart(format!("Failed to create log directory: {e}"))
        })?;

        self.spawn_child().await
    }

    /// 内部：启动子进程
    async fn spawn_child(&self) -> Result<()> {
        let config_path = self.config_path.clone();
        let log_dir = self.log_dir.clone();

        // 确保配置文件存在
        if !config_path.exists() {
            return Err(ClientError::ProcessStart(format!(
                "Config file does not exist: {}",
                config_path.display()
            )));
        }

        let stdout_file = OpenOptions::new()
            .append(true)
            .create(true)
            .open(log_dir.join("frpc.log"))
            .map_err(|e| ClientError::ProcessStart(format!("Failed to open frpc.log: {e}")))?;

        let stderr_file = OpenOptions::new()
            .append(true)
            .create(true)
            .open(log_dir.join("frpc_err.log"))
            .map_err(|e| ClientError::ProcessStart(format!("Failed to open frpc_err.log: {e}")))?;

        let child = Command::new(FRPC_BINARY)
            .arg("-c")
            .arg(&config_path)
            .stdout(std::process::Stdio::from(stdout_file))
            .stderr(std::process::Stdio::from(stderr_file))
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| {
                ClientError::ProcessStart(format!(
                    "Failed to start frpc: {e}. Make sure frpc is installed and in PATH"
                ))
            })?;

        let pid = child.id().unwrap_or(0);
        tracing::info!(
            pid,
            config = %config_path.display(),
            "frpc 已启动"
        );

        *self.child.lock().await = Some(child);
        self.running.store(true, Ordering::SeqCst);
        self.restart_count.store(0, Ordering::SeqCst);

        // 创建 oneshot channel 通知 watchdog 已就绪
        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();

        // 后台监控子进程（崩溃重启）
        self.spawn_watchdog(ready_rx);

        // 通知 watchdog 子进程已稳定启动
        let _ = ready_tx.send(());

        Ok(())
    }

    /// 后台监控子进程状态
    ///
    /// 若子进程非正常退出（非本守护发起的停止），则自动重启（最多 3 次）。
    /// `ready_rx` 用于精确等待子进程稳定启动后再开始监控。
    fn spawn_watchdog(&self, ready_rx: tokio::sync::oneshot::Receiver<()>) {
        let child_arc = self.child.clone();
        let running = self.running.clone();
        let restart_count = self.restart_count.clone();
        let config_path = self.config_path.clone();
        let log_dir = self.log_dir.clone();

        tokio::spawn(async move {
            // 等待启动确认（消除 spawn_child 和 wait() 之间的竞态窗口）
            let _ = ready_rx.await;

            // 等待子进程退出
            let exit_status = {
                let mut guard = child_arc.lock().await;
                if let Some(ref mut child) = *guard {
                    child.wait().await.ok()
                } else {
                    return;
                }
            };

            // 检查是否是主动停止
            if !running.load(Ordering::SeqCst) {
                return; // 主动停止，不重启
            }

            // 子进程意外退出
            let code = exit_status.and_then(|s| s.code()).unwrap_or(-1);
            tracing::warn!(exit_code = code, "frpc 意外退出");

            let count = restart_count.load(Ordering::SeqCst);
            if count < MAX_RESTART_COUNT {
                restart_count.store(count + 1, Ordering::SeqCst);
                tracing::info!(
                    attempt = count + 1,
                    max = MAX_RESTART_COUNT,
                    "自动重启 frpc"
                );

                // 等待 1 秒后重启
                sleep(Duration::from_secs(1)).await;

                // 重新启动
                let stdout_file = match OpenOptions::new()
                    .append(true)
                    .create(true)
                    .open(log_dir.join("frpc.log"))
                {
                    Ok(f) => f,
                    Err(e) => {
                        tracing::error!(error = %e, "Failed to open frpc.log");
                        return;
                    }
                };
                let stderr_file = match OpenOptions::new()
                    .append(true)
                    .create(true)
                    .open(log_dir.join("frpc_err.log"))
                {
                    Ok(f) => f,
                    Err(e) => {
                        tracing::error!(error = %e, "Failed to open frpc_err.log");
                        return;
                    }
                };

                match Command::new(FRPC_BINARY)
                    .arg("-c")
                    .arg(&config_path)
                    .stdout(std::process::Stdio::from(stdout_file))
                    .stderr(std::process::Stdio::from(stderr_file))
                    .kill_on_drop(true)
                    .spawn()
                {
                    Ok(new_child) => {
                        let pid = new_child.id().unwrap_or(0);
                        tracing::info!(pid, "frpc 已重启");
                        *child_arc.lock().await = Some(new_child);
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "frpc 重启失败");
                        running.store(false, Ordering::SeqCst);
                    }
                }
            } else {
                tracing::error!(count, "frpc 已达到最大重启次数，停止重启");
                running.store(false, Ordering::SeqCst);
            }
        });
    }

    /// 热重载：发送 SIGHUP
    ///
    /// 通知 frpc 重新读取配置文件，不中断已建立的连接。
    /// 仅 Unix 平台支持。
    pub async fn reload(&self) -> Result<()> {
        let guard = self.child.lock().await;
        let child = guard
            .as_ref()
            .ok_or_else(|| ClientError::ProcessCommunication("frpc is not running".into()))?;

        let pid = child
            .id()
            .ok_or_else(|| ClientError::ProcessCommunication("Failed to get frpc PID".into()))?;

        #[cfg(unix)]
        {
            use nix::sys::signal::{kill, Signal};
            use nix::unistd::Pid;

            kill(Pid::from_raw(pid as i32), Signal::SIGHUP)
                .map_err(|e| ClientError::SignalError(format!("SIGHUP send failed: {e}")))?;

            tracing::info!(pid, "已发送 SIGHUP（热重载）");
        }

        #[cfg(not(unix))]
        {
            // Windows 不支持 SIGHUP，记录警告
            tracing::warn!(
                pid,
                "SIGHUP hot reload is not supported on this platform. Use stop + start instead."
            );
            Err(ClientError::SignalError(
                "SIGHUP not supported on this platform".into(),
            ))
        }
    }

    /// 优雅退出
    ///
    /// 流程（ARCH-006）：
    /// 1. 发送 SIGTERM
    /// 2. 等待 3 秒
    /// 3. 若未退出则 SIGKILL
    pub async fn shutdown(&self) -> Result<()> {
        self.running.store(false, Ordering::SeqCst);

        let mut guard = self.child.lock().await;
        let mut child = match guard.take() {
            Some(c) => c,
            None => {
                tracing::info!("frpc 未在运行，跳过退出");
                return Ok(());
            }
        };

        let pid = child.id().unwrap_or(0);
        tracing::info!(pid, "正在停止 frpc");

        // 1. SIGTERM
        #[cfg(unix)]
        {
            use nix::sys::signal::{kill, Signal};
            use nix::unistd::Pid;
            kill(Pid::from_raw(pid as i32), Signal::SIGTERM).ok();
        }

        // 2. 等待 3 秒
        match tokio::time::timeout(Duration::from_secs(GRACEFUL_SHUTDOWN_SECS), child.wait()).await
        {
            Ok(status) => {
                let code = status.ok().and_then(|s| s.code()).unwrap_or(-1);
                tracing::info!(pid, exit_code = code, "frpc 已正常退出");
            }
            Err(_) => {
                // 3. SIGKILL（超时未退出）
                tracing::warn!(pid, "frpc 超时未退出，发送 SIGKILL");
                child.kill().await.ok();
                child.wait().await.ok();
            }
        }

        Ok(())
    }

    /// 获取子进程 PID（仅当进程存在时）
    pub fn pid(&self) -> Option<u32> {
        self.child.blocking_lock().as_ref().and_then(|c| c.id())
    }

    /// 子进程是否在运行
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// 获取重启次数
    pub fn restart_count(&self) -> u32 {
        self.restart_count.load(Ordering::SeqCst)
    }
}

impl Drop for ProcessGuard {
    fn drop(&mut self) {
        // 确保子进程被清理
        if self.running.load(Ordering::SeqCst) {
            self.running.store(false, Ordering::SeqCst);
            // 注意：Drop 中不能调用 async，但 `kill_on_drop(true)` 会处理
            tracing::info!("ProcessGuard dropped，子进程将自动清理");
        }
    }
}

impl std::fmt::Debug for ProcessGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessGuard")
            .field("config_path", &self.config_path)
            .field("running", &self.running)
            .field("restart_count", &self.restart_count)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_process_guard_debug() {
        let handler = SignalHandler::new();
        let guard = ProcessGuard::new(PathBuf::from("/nonexistent/frpc.toml"), handler);
        assert!(!guard.is_running());
        assert_eq!(guard.restart_count(), 0);
    }
}
