use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tokio::process::{Child, Command};
use tokio::time::{sleep, Duration};

#[derive(Debug)]
pub struct FrpsSupervisor {
    binary: PathBuf,
    runtime_dir: PathBuf,
    child: Option<Child>,
    pid: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnsureOutcome {
    pub pid: u32,
    pub adopted: bool,
}

impl FrpsSupervisor {
    pub fn new(binary: PathBuf, runtime_dir: PathBuf) -> Self {
        Self {
            binary,
            runtime_dir,
            child: None,
            pid: None,
        }
    }

    pub async fn verify_config(&self, config: &Path) -> Result<()> {
        let output = Command::new(&self.binary)
            .arg("verify")
            .arg("-c")
            .arg(config)
            .output()
            .await
            .context("run frps configuration verifier")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("frps rejected configuration: {}", stderr.trim());
        }
        Ok(())
    }

    pub async fn ensure_running(&mut self, config: &Path) -> Result<EnsureOutcome> {
        if let Some(pid) = self.read_pid().await? {
            if process_matches(pid, &self.binary).await {
                self.pid = Some(pid);
                tracing::info!(pid, "Adopted existing frps process");
                return Ok(EnsureOutcome { pid, adopted: true });
            }
            self.remove_pid().await;
        }
        self.spawn(config).await.map(|pid| EnsureOutcome {
            pid,
            adopted: false,
        })
    }

    pub async fn restart(&mut self, config: &Path) -> Result<u32> {
        self.stop().await?;
        self.spawn(config).await
    }

    /// Return an exit code when the managed or adopted process has stopped.
    pub async fn poll_exit(&mut self) -> Result<Option<i32>> {
        if let Some(child) = self.child.as_mut() {
            if let Some(status) = child.try_wait()? {
                let code = status.code().unwrap_or(-1);
                self.child = None;
                self.pid = None;
                self.remove_pid().await;
                return Ok(Some(code));
            }
            return Ok(None);
        }
        if let Some(pid) = self.pid {
            if !process_matches(pid, &self.binary).await {
                self.pid = None;
                self.remove_pid().await;
                return Ok(Some(-1));
            }
        }
        Ok(None)
    }

    pub fn pid(&self) -> Option<u32> {
        self.pid
    }

    async fn spawn(&mut self, config: &Path) -> Result<u32> {
        tokio::fs::create_dir_all(&self.runtime_dir).await?;
        let stdout = append_log(self.runtime_dir.join("frps.log"))?;
        let stderr = append_log(self.runtime_dir.join("frps-error.log"))?;
        // Deliberately do not set kill_on_drop: ADR-004 requires frps to survive
        // an agent crash and continue with the last valid configuration.
        let child = Command::new(&self.binary)
            .arg("-c")
            .arg(config)
            .stdout(stdout)
            .stderr(stderr)
            .spawn()
            .context("start frps")?;
        let pid = child.id().context("frps did not expose a process ID")?;
        self.write_pid(pid).await?;
        self.pid = Some(pid);
        self.child = Some(child);
        tracing::info!(pid, config = %config.display(), "frps started");
        Ok(pid)
    }

    async fn stop(&mut self) -> Result<()> {
        let Some(pid) = self.pid.or_else(|| self.child.as_ref().and_then(Child::id)) else {
            return Ok(());
        };
        if self.child.is_none() && !process_matches(pid, &self.binary).await {
            tracing::warn!(pid, "Refusing to signal PID file target that is not frps");
            self.pid = None;
            self.remove_pid().await;
            return Ok(());
        }
        terminate_process(pid).await?;
        for _ in 0..30 {
            if !process_exists(pid).await {
                break;
            }
            sleep(Duration::from_millis(100)).await;
        }
        if process_exists(pid).await {
            kill_process(pid).await?;
        }
        if let Some(child) = self.child.as_mut() {
            let _ = child.wait().await;
        }
        self.child = None;
        self.pid = None;
        self.remove_pid().await;
        Ok(())
    }

    fn pid_path(&self) -> PathBuf {
        self.runtime_dir.join("frps.pid")
    }

    async fn read_pid(&self) -> Result<Option<u32>> {
        match tokio::fs::read_to_string(self.pid_path()).await {
            Ok(value) => Ok(value.trim().parse().ok()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    async fn write_pid(&self, pid: u32) -> Result<()> {
        let temporary = self.runtime_dir.join("frps.pid.pending");
        tokio::fs::write(&temporary, format!("{pid}\n")).await?;
        #[cfg(windows)]
        if self.pid_path().exists() {
            tokio::fs::remove_file(self.pid_path()).await?;
        }
        tokio::fs::rename(temporary, self.pid_path()).await?;
        Ok(())
    }

    async fn remove_pid(&self) {
        let _ = tokio::fs::remove_file(self.pid_path()).await;
    }
}

fn append_log(path: PathBuf) -> Result<std::process::Stdio> {
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    Ok(std::process::Stdio::from(file))
}

#[cfg(unix)]
async fn process_exists(pid: u32) -> bool {
    nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid as i32), None).is_ok()
}

#[cfg(unix)]
async fn process_matches(pid: u32, binary: &Path) -> bool {
    if !process_exists(pid).await {
        return false;
    }
    let expected = binary
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("frps");
    Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "command="])
        .output()
        .await
        .map(|output| {
            output.status.success()
                && String::from_utf8_lossy(&output.stdout)
                    .split_whitespace()
                    .next()
                    .and_then(|command| Path::new(command).file_name())
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name == expected)
        })
        .unwrap_or(false)
}

#[cfg(windows)]
async fn process_exists(pid: u32) -> bool {
    Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH"])
        .output()
        .await
        .map(|output| String::from_utf8_lossy(&output.stdout).contains(&pid.to_string()))
        .unwrap_or(false)
}

#[cfg(windows)]
async fn process_matches(pid: u32, binary: &Path) -> bool {
    let expected = binary
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("frps.exe");
    Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH"])
        .output()
        .await
        .map(|output| {
            output.status.success()
                && String::from_utf8_lossy(&output.stdout)
                    .split_whitespace()
                    .next()
                    .is_some_and(|name| name.eq_ignore_ascii_case(expected))
        })
        .unwrap_or(false)
}

#[cfg(unix)]
async fn terminate_process(pid: u32) -> Result<()> {
    nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(pid as i32),
        nix::sys::signal::Signal::SIGTERM,
    )?;
    Ok(())
}

#[cfg(windows)]
async fn terminate_process(pid: u32) -> Result<()> {
    let status = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T"])
        .status()
        .await?;
    anyhow::ensure!(status.success(), "taskkill failed for PID {pid}");
    Ok(())
}

#[cfg(unix)]
async fn kill_process(pid: u32) -> Result<()> {
    nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(pid as i32),
        nix::sys::signal::Signal::SIGKILL,
    )?;
    Ok(())
}

#[cfg(windows)]
async fn kill_process(pid: u32) -> Result<()> {
    let status = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .status()
        .await?;
    anyhow::ensure!(status.success(), "forced taskkill failed for PID {pid}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn ignores_missing_pid_file() {
        let temp = tempfile::tempdir().unwrap();
        let supervisor = FrpsSupervisor::new("frps".into(), temp.path().into());
        assert_eq!(supervisor.read_pid().await.unwrap(), None);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn frps_survives_supervisor_drop() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let binary = temp.path().join("fake-frps");
        std::fs::write(
            &binary,
            "#!/bin/sh\nif [ \"$1\" = verify ]; then exit 0; fi\nwhile true; do sleep 1; done\n",
        )
        .unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
        let config = temp.path().join("frps.toml");
        std::fs::write(&config, "bindPort = 7000\n").unwrap();

        let mut supervisor = FrpsSupervisor::new(binary, temp.path().into());
        supervisor.verify_config(&config).await.unwrap();
        let pid = supervisor.ensure_running(&config).await.unwrap().pid;
        drop(supervisor);
        assert!(process_exists(pid).await);
        kill_process(pid).await.unwrap();
    }
}
