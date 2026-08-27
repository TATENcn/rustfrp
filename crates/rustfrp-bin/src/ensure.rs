//! 确保 FRP 二进制可用
//!
//! 幂等地保证指定版本的 frpc/frps 二进制已下载、校验、解压到本地，
//! 并返回其绝对路径。调用方（client/agent）据此用绝对路径拉起进程，
//! 不再依赖系统 PATH。
//!
//! # 流程
//!
//! 1. 校验目标二进制的版本、平台和本地完整性标记，匹配时直接返回。
//! 2. 否则下载官方发布包，强制校验 SHA256，再解压并原子安装。

use crate::download::{asset_filename, download, official_checksum};
use crate::extract::extract;
use crate::verify::{sha256_file, verify_sha256};
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
/// * `sha256`     - 可选校验和；为 `None` 时从官方发布清单获取，校验不会跳过
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
    let integrity_marker = target.with_extension("sha256");
    let platform = crate::platform::Platform::detect()
        .ok_or_else(|| FrpError::Download("Unsupported platform for auto-download".into()))?;
    let ver = FrpVersion::from_tag(&version_str);

    // Only trust binaries installed by us after archive verification. The
    // marker also detects later local modification or disk corruption.
    if target.exists() && integrity_marker.exists() {
        if let Ok(contents) = tokio::fs::read_to_string(&integrity_marker).await {
            if let Some(marker) = IntegrityMarker::parse(&contents) {
                if marker.version == ver.version
                    && marker.platform == platform.slug
                    && verify_sha256(&target, marker.sha256).await.is_ok()
                {
                    tracing::info!(path = %target.display(), "Verified cached FRP binary");
                    return Ok(target);
                }
            }
        }
        tracing::warn!(path = %target.display(), "Cached FRP binary is stale or failed integrity verification; reinstalling");
    }

    std::fs::create_dir_all(&frp_dir).map_err(FrpError::Io)?;

    // 下载 tar.gz 到临时 staging 目录，避免污染 frp_dir
    let staging = frp_dir.join("downloads");
    let archive = download(&ver, &staging, &platform.slug).await?;

    let asset = asset_filename(&ver, &platform.slug);
    let expected_hash = match sha256 {
        Some(hash) => hash.to_string(),
        None => official_checksum(&ver, &asset).await?,
    };
    if let Err(error) = verify_sha256(&archive, &expected_hash).await {
        let _ = tokio::fs::remove_file(&archive).await;
        return Err(error);
    }

    // 解压到独立解包目录，再定位二进制
    let unpack = frp_dir.join(format!("frp_{}_{}", ver.version, platform.slug));
    extract(&archive, &unpack)?;

    // 在解包目录（含一层子目录）内查找二进制
    let found = find_binary(&unpack, bin).ok_or_else(|| {
        FrpError::Extract(format!("frp binary '{bin}' not found after extraction"))
    })?;

    // Install atomically and store the installed binary's own hash.
    let target_tmp = target.with_extension("installing");
    tokio::fs::copy(&found, &target_tmp)
        .await
        .map_err(FrpError::Io)?;
    set_executable(&target_tmp)?;
    let binary_hash = sha256_file(&target_tmp).await?;
    if target.exists() {
        tokio::fs::remove_file(&target).await?;
    }
    tokio::fs::rename(&target_tmp, &target).await?;
    let marker_tmp = integrity_marker.with_extension("sha256.tmp");
    let marker = IntegrityMarker {
        version: &ver.version,
        platform: &platform.slug,
        sha256: &binary_hash,
    };
    tokio::fs::write(&marker_tmp, marker.serialize()).await?;
    tokio::fs::rename(marker_tmp, &integrity_marker).await?;

    tracing::info!(path = %target.display(), "FRP binary ready");
    Ok(target)
}

struct IntegrityMarker<'a> {
    version: &'a str,
    platform: &'a str,
    sha256: &'a str,
}

impl<'a> IntegrityMarker<'a> {
    fn parse(contents: &'a str) -> Option<Self> {
        let mut fields = contents.split_whitespace();
        let marker = Self {
            version: fields.next()?,
            platform: fields.next()?,
            sha256: fields.next()?,
        };
        if fields.next().is_some()
            || marker.sha256.len() != 64
            || !marker.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return None;
        }
        Some(marker)
    }

    fn serialize(&self) -> String {
        format!("{} {} {}\n", self.version, self.platform, self.sha256)
    }
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

#[cfg(test)]
mod tests {
    use super::IntegrityMarker;

    #[test]
    fn integrity_marker_round_trips_and_rejects_invalid_data() {
        let hash = "a".repeat(64);
        let marker = IntegrityMarker {
            version: "0.70.1",
            platform: "linux_amd64",
            sha256: &hash,
        };
        let encoded = marker.serialize();
        let parsed = IntegrityMarker::parse(&encoded).unwrap();
        assert_eq!(parsed.version, "0.70.1");
        assert_eq!(parsed.platform, "linux_amd64");
        assert_eq!(parsed.sha256, hash);
        assert!(IntegrityMarker::parse("0.70.1 linux_amd64 invalid").is_none());
    }
}
