//! 进程守护
//!
//! 管理 frpc 子进程的完整生命周期：
//! 启动 → 运行（监控 stderr）→ 热重载（SIGHUP）→ 优雅退出（SIGTERM → SIGKILL）
#![allow(clippy::lines_filter_map_ok)]

use crate::error::{ClientError, Result};
use crate::process::diagnostic::{classify_failure, ProcessFailure};
use rustfrp_common::signal::SignalHandler;
use std::fs::OpenOptions;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration, Instant};

/// 最大自动重启次数
const MAX_RESTART_COUNT: u32 = 3;

/// A process surviving this long is considered stable and gets a fresh retry budget.
const STABLE_RUN_SECS: u64 = 60;

/// 优雅退出等待时间
const GRACEFUL_SHUTDOWN_SECS: u64 = 3;

/// 进程守护
///
/// 封装 frpc 子进程，提供启动、热重载、优雅退出能力。
/// 支持崩溃自动重启（最多 3 次，ARCH-006）。
#[derive(Clone)]
pub struct ProcessGuard {
    /// 子进程（Mutex 保护，因为启动/停止都是 &self 的）
    child: Arc<Mutex<Option<Child>>>,
    /// 配置文件路径
    config_path: PathBuf,
    /// frpc 二进制绝对路径（由 rustfrp 管理，非 PATH）
    frpc_path: PathBuf,
    /// 是否正在运行
    running: Arc<AtomicBool>,
    /// 重启次数
    restart_count: Arc<AtomicU32>,
    /// Latest grouped failure, exposed through the status API.
    last_failure: Arc<std::sync::RwLock<Option<ProcessFailure>>>,
    /// 子进程 PID（0 = 未启动），缓存以避免在 async 上下文中调用 blocking_lock()
    pid: Arc<AtomicU32>,
    /// 信号处理器（Phase 2 协调关闭用）
    #[allow(dead_code)]
    signal_handler: SignalHandler,
    /// 日志目录
    log_dir: PathBuf,
    /// Profile 名称（用于生成独立日志文件）
    profile_name: String,
}
impl ProcessGuard {
    /// 创建新的 ProcessGuard
    ///
    /// 注意：此时尚未启动子进程，需调用 `start()`。
    /// `profile_name` 用于生成独立的日志文件名。
    /// `frpc_path` 为 rustfrp 托管的 frpc 二进制绝对路径（非 PATH 查找）。
    pub fn new(
        config_path: PathBuf,
        frpc_path: PathBuf,
        signal_handler: SignalHandler,
        profile_name: String,
    ) -> Self {
        let log_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".rustfrp")
            .join("logs");

        Self {
            child: Arc::new(Mutex::new(None)),
            config_path,
            frpc_path,
            running: Arc::new(AtomicBool::new(false)),
            restart_count: Arc::new(AtomicU32::new(0)),
            last_failure: Arc::new(std::sync::RwLock::new(None)),
            pid: Arc::new(AtomicU32::new(0)),
            signal_handler,
            log_dir,
            profile_name,
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
        let safe_name = sanitize_log_filename(&self.profile_name);

        // 确保配置文件存在
        if !config_path.exists() {
            return Err(ClientError::ProcessStart(format!(
                "Config file does not exist: {}",
                config_path.display()
            )));
        }

        let stdout_log = format!("frpc_{}.log", safe_name);
        let stderr_log = format!("frpc_{}_err.log", safe_name);

        let stdout_file = OpenOptions::new()
            .append(true)
            .create(true)
            .open(log_dir.join(&stdout_log))
            .map_err(|e| {
                ClientError::ProcessStart(format!("Failed to open {}: {e}", stdout_log))
            })?;

        let stderr_file = OpenOptions::new()
            .append(true)
            .create(true)
            .open(log_dir.join(&stderr_log))
            .map_err(|e| {
                ClientError::ProcessStart(format!("Failed to open {}: {e}", stderr_log))
            })?;

        let child = Command::new(&self.frpc_path)
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
            "frpc has started"
        );

        self.pid.store(pid, Ordering::SeqCst);
        *self.child.lock().await = Some(child);
        self.running.store(true, Ordering::SeqCst);
        self.restart_count.store(0, Ordering::SeqCst);
        *self.last_failure.write().expect("failure lock poisoned") = None;

        // Wait briefly and check if frpc exited immediately (e.g., bad config).
        // This catches startup failures that would otherwise go unnoticed because
        // stderr is redirected to a log file.
        sleep(Duration::from_millis(800)).await;

        {
            let mut guard = self.child.lock().await;
            if let Some(ref mut child) = *guard {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        // frpc exited immediately — read stderr for diagnostics
                        let code = status.code().unwrap_or(-1);
                        self.running.store(false, Ordering::SeqCst);
                        self.pid.store(0, Ordering::SeqCst);

                        let stderr_content = read_log_tail(&log_dir.join(&stderr_log), 80);
                        let stdout_content = read_log_tail(&log_dir.join(&stdout_log), 40);

                        *guard = None; // clear the dead child

                        return Err(ClientError::ProcessStart(format!(
                            "frpc exited immediately with exit code {code}.\n\
                             Config: {config_path}\n\
                             --- stderr (last 80 lines) ---\n\
                             {stderr_content}\n\
                             --- stdout (last 40 lines) ---\n\
                             {stdout_content}",
                            config_path = config_path.display(),
                            stderr_content = stderr_content,
                            stdout_content = stdout_content,
                        )));
                    }
                    Ok(None) => {
                        // Still running — good, proceed
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to check frpc startup status");
                    }
                }
            }
        }

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
        let pid = self.pid.clone();
        let config_path = self.config_path.clone();
        let frpc_path = self.frpc_path.clone();
        let log_dir = self.log_dir.clone();
        let profile_name = self.profile_name.clone();
        let last_failure = self.last_failure.clone();
        tokio::spawn(async move {
            // 等待启动确认（消除 spawn_child 和 wait() 之间的竞态窗口）
            let _ = ready_rx.await;

            let safe_name = sanitize_log_filename(&profile_name);
            let stdout_log = format!("frpc_{safe_name}.log");
            let stderr_log = format!("frpc_{safe_name}_err.log");
            let stderr_path = log_dir.join(&stderr_log);
            let mut started_at = Instant::now();

            loop {
                // Poll without holding the child mutex across an await. This lets
                // shutdown take ownership and signal the process immediately.
                let exit_status = loop {
                    let status = {
                        let mut guard = child_arc.lock().await;
                        let Some(child) = guard.as_mut() else {
                            return;
                        };
                        match child.try_wait() {
                            Ok(Some(status)) => {
                                guard.take();
                                Some(status)
                            }
                            Ok(None) => None,
                            Err(error) => {
                                tracing::error!(%error, "Failed to poll frpc process");
                                running.store(false, Ordering::SeqCst);
                                return;
                            }
                        }
                    };
                    if let Some(status) = status {
                        break status;
                    }
                    sleep(Duration::from_millis(250)).await;
                };

                if !running.load(Ordering::SeqCst) {
                    return;
                }

                pid.store(0, Ordering::SeqCst);
                if started_at.elapsed() >= Duration::from_secs(STABLE_RUN_SECS) {
                    restart_count.store(0, Ordering::SeqCst);
                    *last_failure.write().expect("failure lock poisoned") = None;
                }
                let code = exit_status.code().unwrap_or(-1);
                let stderr = read_log_tail(&stderr_path, 80);
                let failure = classify_failure(&stderr, code);
                let occurrences = record_failure(&last_failure, failure.clone());
                if occurrences == 1 {
                    tracing::warn!(
                        exit_code = code,
                        reason = ?failure.reason,
                        summary = %failure.summary,
                        "frpc exited unexpectedly"
                    );
                } else {
                    tracing::debug!(reason = ?failure.reason, occurrences, "Grouped repeated frpc failure");
                }

                let completed_restarts = restart_count.load(Ordering::SeqCst);
                if completed_restarts >= MAX_RESTART_COUNT {
                    tracing::error!(
                        attempts = MAX_RESTART_COUNT,
                        reason = ?failure.reason,
                        occurrences,
                        "frpc restart budget exhausted"
                    );
                    running.store(false, Ordering::SeqCst);
                    return;
                }
                let attempt = completed_restarts + 1;
                restart_count.store(attempt, Ordering::SeqCst);

                let delay_secs = 1_u64 << (attempt - 1);
                tracing::info!(
                    attempt,
                    max = MAX_RESTART_COUNT,
                    delay_secs,
                    "Scheduling frpc restart"
                );
                sleep(Duration::from_secs(delay_secs)).await;
                if !running.load(Ordering::SeqCst) {
                    return;
                }

                let stdout_file = match open_log_file(&log_dir.join(&stdout_log)) {
                    Ok(file) => file,
                    Err(error) => {
                        tracing::error!(%error, "Failed to open frpc stdout log");
                        running.store(false, Ordering::SeqCst);
                        return;
                    }
                };
                let stderr_file = match open_log_file(&stderr_path) {
                    Ok(file) => file,
                    Err(error) => {
                        tracing::error!(%error, "Failed to open frpc stderr log");
                        running.store(false, Ordering::SeqCst);
                        return;
                    }
                };
                match Command::new(&frpc_path)
                    .arg("-c")
                    .arg(&config_path)
                    .stdout(std::process::Stdio::from(stdout_file))
                    .stderr(std::process::Stdio::from(stderr_file))
                    .kill_on_drop(true)
                    .spawn()
                {
                    Ok(new_child) => {
                        let new_pid = new_child.id().unwrap_or(0);
                        pid.store(new_pid, Ordering::SeqCst);
                        *child_arc.lock().await = Some(new_child);
                        started_at = Instant::now();
                        tracing::info!(pid = new_pid, attempt, "frpc restarted");
                    }
                    Err(error) => {
                        tracing::error!(%error, "frpc restart failed");
                        running.store(false, Ordering::SeqCst);
                        return;
                    }
                }
            }
        });
    }

    /// 热重载：发送 SIGHUP（Unix）或 restart（Windows）
    ///
    /// Unix 上通过 SIGHUP 通知 frpc 重读配置，不中断已建立的连接。
    /// Windows 没有 SIGHUP，改为 stop + start 实现等效效果（会短暂中断连接）。
    pub async fn reload(&self) -> Result<()> {
        let pid = {
            let guard = self.child.lock().await;
            guard
                .as_ref()
                .and_then(|c| c.id())
                .ok_or_else(|| ClientError::ProcessCommunication("frpc is not running".into()))?
        };

        #[cfg(unix)]
        {
            use nix::sys::signal::{kill, Signal};
            use nix::unistd::Pid;

            kill(Pid::from_raw(pid as i32), Signal::SIGHUP)
                .map_err(|e| ClientError::SignalError(format!("SIGHUP send failed: {e}")))?;

            tracing::info!(pid, "SIGHUP sent (hot reload)");
            Ok(())
        }

        #[cfg(not(unix))]
        {
            tracing::info!(
                pid,
                "SIGHUP not available on this platform, restarting frpc instead"
            );

            // Windows: stop + start the process to pick up config changes.
            // The Mutex lock is released before calling shutdown() to avoid deadlock,
            // since shutdown() also acquires the same lock.
            self.shutdown().await?;
            self.spawn_child().await?;

            tracing::info!("frpc restarted successfully");
            Ok(())
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
        self.pid.store(0, Ordering::SeqCst);

        let mut guard = self.child.lock().await;
        let mut child = match guard.take() {
            Some(c) => c,
            None => {
                tracing::info!("frpc is not running, skipping exit");
                return Ok(());
            }
        };

        let pid = child.id().unwrap_or(0);
        tracing::info!(pid, "Stopping frpc");

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
                tracing::info!(pid, exit_code = code, "frpc exited normally");
            }
            Err(_) => {
                // 3. SIGKILL（超时未退出）
                tracing::warn!(pid, "frpc timeout, sending SIGKILL");
                child.kill().await.ok();
                child.wait().await.ok();
            }
        }

        Ok(())
    }

    /// 获取子进程 PID（仅当进程存在时返回非零值）
    pub fn pid(&self) -> Option<u32> {
        let p = self.pid.load(Ordering::SeqCst);
        if p == 0 {
            None
        } else {
            Some(p)
        }
    }

    /// 子进程是否在运行
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// 获取重启次数
    pub fn restart_count(&self) -> u32 {
        self.restart_count.load(Ordering::SeqCst)
    }

    pub fn last_failure(&self) -> Option<ProcessFailure> {
        self.last_failure
            .read()
            .expect("failure lock poisoned")
            .clone()
    }

    /// 获取配置文件路径
    pub fn config_path(&self) -> &PathBuf {
        &self.config_path
    }

    /// 获取 stdout 日志文件路径
    pub fn stdout_log_path(&self) -> PathBuf {
        let safe_name = sanitize_log_filename(&self.profile_name);
        self.log_dir.join(format!("frpc_{}.log", safe_name))
    }

    /// 获取 stderr 日志文件路径
    pub fn stderr_log_path(&self) -> PathBuf {
        let safe_name = sanitize_log_filename(&self.profile_name);
        self.log_dir.join(format!("frpc_{}_err.log", safe_name))
    }
}

fn open_log_file(path: &PathBuf) -> std::io::Result<std::fs::File> {
    OpenOptions::new().append(true).create(true).open(path)
}

fn record_failure(
    state: &std::sync::RwLock<Option<ProcessFailure>>,
    failure: ProcessFailure,
) -> u32 {
    let mut current = state.write().expect("failure lock poisoned");
    if let Some(previous) = current.as_mut() {
        if previous.reason == failure.reason && previous.summary == failure.summary {
            previous.exit_code = failure.exit_code;
            previous.occurrences = previous.occurrences.saturating_add(1);
            return previous.occurrences;
        }
    }
    *current = Some(failure);
    1
}

impl Drop for ProcessGuard {
    fn drop(&mut self) {
        // 确保子进程被清理
        if self.running.load(Ordering::SeqCst) {
            self.running.store(false, Ordering::SeqCst);
            // 注意：Drop 中不能调用 async，但 `kill_on_drop(true)` 会处理
            tracing::info!("ProcessGuard dropped, child process will be automatically cleaned up");
        }
    }
}

impl std::fmt::Debug for ProcessGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessGuard")
            .field("config_path", &self.config_path)
            .field("running", &self.running)
            .field("pid", &self.pid)
            .field("restart_count", &self.restart_count)
            .finish_non_exhaustive()
    }
}

/// 文件名安全处理：替换空格、去除非安全字符，用于日志文件名
fn sanitize_log_filename(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => c,
            ' ' => '_',
            _ => '_',
        })
        .collect()
}

/// Read the last `max_lines` lines of a log file for error diagnostics.
///
/// Returns the content as a string, or a descriptive message if the file
/// cannot be read. This is used to surface frpc startup errors to the user.
fn read_log_tail(path: &std::path::Path, max_lines: usize) -> String {
    match std::fs::File::open(path) {
        Ok(file) => {
            use std::io::BufRead;
            let reader = std::io::BufReader::new(file);
            let lines: Vec<String> = reader
                .lines()
                // io::Lines is unbounded on Err; map_while(Result::ok) triggers
                // inference issues with collect, so keep filter_map + allow.
                .filter_map(|l| l.ok())
                .collect();
            let start = if lines.len() > max_lines {
                lines.len() - max_lines
            } else {
                0
            };
            let tail: Vec<&str> = lines[start..].iter().map(|s| s.as_str()).collect();
            if tail.is_empty() {
                "(empty)".to_string()
            } else {
                tail.join("\n")
            }
        }
        Err(e) => format!("(unable to read log: {e})"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::diagnostic::{FailureReason, ProcessFailure};

    #[tokio::test]
    async fn test_process_guard_debug() {
        let handler = SignalHandler::new();
        let guard = ProcessGuard::new(
            PathBuf::from("/nonexistent/frpc.toml"),
            PathBuf::from("/nonexistent/frpc"),
            handler,
            "test_profile".to_string(),
        );
        assert!(!guard.is_running());
        assert_eq!(guard.restart_count(), 0);
        assert_eq!(guard.pid(), None);
    }

    #[test]
    fn repeated_failures_are_grouped_by_reason() {
        let state = std::sync::RwLock::new(None);
        let failure = ProcessFailure {
            reason: FailureReason::NetworkUnreachable,
            summary: "FRP server is unreachable".into(),
            exit_code: 1,
            occurrences: 1,
        };
        assert_eq!(record_failure(&state, failure.clone()), 1);
        assert_eq!(record_failure(&state, failure), 2);
        assert_eq!(state.read().unwrap().as_ref().unwrap().occurrences, 2);

        assert_eq!(
            record_failure(
                &state,
                ProcessFailure {
                    reason: FailureReason::AuthenticationFailed,
                    summary: "FRP authentication failed".into(),
                    exit_code: 1,
                    occurrences: 1,
                },
            ),
            1
        );
    }
}
