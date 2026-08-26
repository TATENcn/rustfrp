//! 确保 FRP 二进制可用
//!
//! 幂等地保证指定版本的 frpc/frps 二进制已下载、校验、解压到本地，
//! 并返回其绝对路径。调用方（client/agent）据此用绝对路径拉起进程，
//! 不再依赖系统 PATH。
//!
//! # 流程
//!
//! 1. 若目标二进制已存在于 `frp_dir/{bin}` 且可执行 → 直接返回（幂等）。
//! 2. 否则：download（tar.gz）→ verify_sha256（若提供 hash）→ extract → 定位二进制 → chmod。

use crate::download::download;
use crate::extract::extract;
use crate::verify::verify_sha256;
use crate::{default_frp_dir, FrpError, FrpVersion};
use std::path::{Path, PathBuf};

/// 二进制名（含平台无关的后缀处理）
fn binary_name(bin: &str) -> String {
    if cfg!(windows) {
        format!("{bin}.exe")
    } else {
        bin.to_string()
    }
}

/// 默认使用的 FRP 版本（与 fatedier/frp 最新稳定版对齐）。
///
/// 可由环境变量 `RUSTFRP_FRP_VERSION` 或显式 `version` 参数覆盖。
pub const DEFAULT_FRP_VERSION: &str = "0.70.1";

/// 从环境变量读取覆盖版本，否则回退默认。
pub fn resolved_version(explicit: Option<&str>) -> String {
    if let Some(v) = explicit {
        return v.to_string();
    }
    std::env::var("RUSTFRP_FRP_VERSION").unwrap_or_else(|_| DEFAULT_FRP_VERSION.to_string())
}

/// 确保某 FRP 二进制可用，返回其绝对路径。
///
/// # Arguments
///
/// * `bin`        - `"frpc"` 或 `"frps"`
/// * `version`    - 可选版本覆盖（如 `Some("0.70.1")`）；空则取默认/环境变量
/// * `frp_dir`    - 二进制落地目录（解压后的根目录）；默认 `~/.rustfrp/frp`
/// * `sha256`     - 可选校验和；为 `None` 时跳过校验（仍下载）
///
/// # Returns
///
/// 已就绪的二进制绝对路径。
pub async fn ensure_binary(
    bin: &str,
    version: Option<&str>,
    frp_dir: Option<&Path>,
    sha256: Option<&str>,
) -> Result<PathBuf, FrpError> {
    let version_str = resolved_version(version);
    let frp_dir = frp_dir
        .map(Path::to_path_buf)
        .unwrap_or_else(default_frp_dir);
    let target = frp_dir.join(binary_name(bin));

    // 幂等：已存在即可用
    if target.exists()
        && target
            .metadata()
            .map(|m| !m.permissions().readonly())
            .unwrap_or(false)
    {
        tracing::info!(path = %target.display(), "FRP binary already present");
        return Ok(target);
    }

    let platform = crate::platform::Platform::detect()
        .ok_or_else(|| FrpError::Download("Unsupported platform for auto-download".into()))?;
    let ver = FrpVersion::from_tag(&version_str);

    std::fs::create_dir_all(&frp_dir).map_err(FrpError::Io)?;

    // 下载 tar.gz 到临时 staging 目录，避免污染 frp_dir
    let staging = frp_dir.join("downloads");
    let archive = download(&ver, &staging, &platform.slug).await?;

    if let Some(hash) = sha256 {
        verify_sha256(&archive, hash).await?;
    } else {
        tracing::warn!("No SHA256 provided; skipping verification (risky)");
    }

    // 解压到独立解包目录，再定位二进制
    let unpack = frp_dir.join(format!("frp_{}_{}", ver.version, platform.slug));
    extract(&archive, &unpack)?;

    // 在解包目录（含一层子目录）内查找二进制
    let found = find_binary(&unpack, bin).ok_or_else(|| {
        FrpError::Extract(format!("frp binary '{bin}' not found after extraction"))
    })?;

    // 移动到最终位置
    tokio::fs::copy(&found, &target)
        .await
        .map_err(FrpError::Io)?;
    set_executable(&target)?;

    tracing::info!(path = %target.display(), "FRP binary ready");
    Ok(target)
}

/// 在解包目录（含一层子目录）内查找指定二进制。
fn find_binary(root: &Path, bin: &str) -> Option<PathBuf> {
    let want = binary_name(bin);
    let candidate = root.join(&want);
    if candidate.exists() {
        return Some(candidate);
    }
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let p = entry.path().join(&want);
            if p.exists() {
                return Some(p);
            }
        }
    }
    None
}

/// Unix 下设置可执行位。
#[cfg(unix)]
fn set_executable(path: &Path) -> Result<(), FrpError> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path).map_err(FrpError::Io)?.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms).map_err(FrpError::Io)
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> Result<(), FrpError> {
    Ok(())
}
