//! FRP 二进制下载
//!
//! 从 GitHub Releases 下载指定版本的 FRP 二进制。

use crate::{FrpError, FrpVersion};
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;

pub const OFFICIAL_RELEASE_BASE: &str = "https://github.com/fatedier/frp/releases/download";

#[derive(Debug, Clone, serde::Serialize)]
pub struct InstallProgress {
    pub stage: &'static str,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
}
pub type ProgressCallback = std::sync::Arc<dyn Fn(InstallProgress) + Send + Sync>;
pub fn report(
    callback: &Option<ProgressCallback>,
    stage: &'static str,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
) {
    if let Some(callback) = callback {
        callback(InstallProgress {
            stage,
            downloaded_bytes,
            total_bytes,
        });
    }
}
pub fn release_base(channel: Option<&str>, custom: Option<&str>) -> Result<String, FrpError> {
    let base = match channel.unwrap_or(if custom.is_some() {
        "custom"
    } else {
        "official"
    }) {
        "official" => OFFICIAL_RELEASE_BASE.to_owned(),
        "ghfast" => format!("https://ghfast.top/{OFFICIAL_RELEASE_BASE}"),
        "gh-proxy" => format!("https://gh-proxy.com/{OFFICIAL_RELEASE_BASE}"),
        "custom" => custom
            .ok_or_else(|| FrpError::Download("Custom HTTPS release base is required".into()))?
            .trim()
            .trim_end_matches('/')
            .to_owned(),
        _ => return Err(FrpError::Download("Unknown download channel".into())),
    };
    let parsed = reqwest::Url::parse(&base).map_err(|e| FrpError::Download(e.to_string()))?;
    if parsed.scheme() != "https"
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(FrpError::Download(
            "Release base must be HTTPS without credentials, query, or fragment".into(),
        ));
    }
    Ok(base)
}

pub fn asset_filename(version: &FrpVersion, platform: &str) -> String {
    let extension = if platform.starts_with("windows_") {
        "zip"
    } else {
        "tar.gz"
    };
    format!("frp_{}_{}.{}", version.version, platform, extension)
}

pub async fn official_checksum(version: &FrpVersion, asset: &str) -> Result<String, FrpError> {
    // GitHub API asset digests provide an independent official path when release downloads are blocked.
    #[derive(serde::Deserialize)]
    struct Asset {
        name: String,
        digest: Option<String>,
    }
    #[derive(serde::Deserialize)]
    struct Release {
        assets: Vec<Asset>,
    }
    let api = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()?;
    if let Ok(response) = api
        .get(format!(
            "https://api.github.com/repos/fatedier/frp/releases/tags/{}",
            version.tag_name
        ))
        .header(reqwest::header::USER_AGENT, "rustfrp-version-manager")
        .send()
        .await
    {
        if response.status().is_success() {
            if let Ok(release) = response.json::<Release>().await {
                if let Some(hash) = release
                    .assets
                    .into_iter()
                    .find(|a| a.name == asset)
                    .and_then(|a| a.digest)
                    .and_then(|d| d.strip_prefix("sha256:").map(str::to_owned))
                    .filter(|h| h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit()))
                {
                    return Ok(hash);
                }
            }
        }
    }
    let url = format!(
        "{OFFICIAL_RELEASE_BASE}/{}/frp_sha256_checksums.txt",
        version.tag_name
    );
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;
    let response = client.get(&url).send().await?;
    if !response.status().is_success() {
        return Err(FrpError::Download(format!(
            "Checksum manifest returned HTTP {}: {url}",
            response.status()
        )));
    }
    let manifest = response.text().await?;
    parse_checksum_manifest(&manifest, asset).ok_or_else(|| {
        FrpError::Verify(format!(
            "Official checksum manifest does not contain asset '{asset}'"
        ))
    })
}

pub fn parse_checksum_manifest(manifest: &str, asset: &str) -> Option<String> {
    manifest.lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        let hash = fields.next()?;
        let filename = fields.next()?.trim_start_matches('*');
        if filename == asset && hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            Some(hash.to_ascii_lowercase())
        } else {
            None
        }
    })
}

/// 从 GitHub Releases 下载 FRP
///
/// # Arguments
///
/// * `version` - FRP 版本
/// * `download_dir` - 下载目录
/// * `platform` - 目标平台（linux_amd64, linux_arm64, windows_amd64 等）
pub async fn download(
    version: &FrpVersion,
    download_dir: &Path,
    platform: &str,
) -> Result<PathBuf, FrpError> {
    download_from(version, download_dir, platform, OFFICIAL_RELEASE_BASE).await
}

/// Download from an official-compatible mirror base. Integrity remains anchored
/// to the separately fetched official checksum manifest in `ensure`.
pub async fn download_from(
    version: &FrpVersion,
    download_dir: &Path,
    platform: &str,
    release_base: &str,
) -> Result<PathBuf, FrpError> {
    download_with_progress(version, download_dir, platform, release_base, None).await
}
pub async fn download_with_progress(
    version: &FrpVersion,
    download_dir: &Path,
    platform: &str,
    release_base: &str,
    progress: Option<ProgressCallback>,
) -> Result<PathBuf, FrpError> {
    let filename = asset_filename(version, platform);
    let url = format!(
        "{}/{}/{}",
        release_base.trim_end_matches('/'),
        version.tag_name,
        filename
    );

    let output_path = download_dir.join(&filename);

    // 如果已存在，跳过下载
    if output_path.exists() {
        tracing::info!(path = %output_path.display(), "FRP already exists, skipping download");
        return Ok(output_path);
    }

    tracing::info!(url = %url, "Downloading FRP from {url}");

    std::fs::create_dir_all(download_dir)?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()?;

    let response = client.get(&url).send().await?;

    if !response.status().is_success() {
        return Err(FrpError::Download(format!(
            "HTTP {}: {url}",
            response.status()
        )));
    }

    let mut response = response;
    let total = response.content_length();
    const MAX_ARCHIVE: u64 = 256 * 1024 * 1024;
    if total.is_some_and(|n| n > MAX_ARCHIVE) {
        return Err(FrpError::Download("Archive exceeds 256 MiB limit".into()));
    }
    let tmp_path = output_path.with_extension("download.tmp");
    let mut file = tokio::fs::File::create(&tmp_path).await?;
    let mut downloaded = 0u64;
    let transfer = async {
        while let Some(chunk) = response.chunk().await? {
            downloaded = downloaded.saturating_add(chunk.len() as u64);
            if downloaded > MAX_ARCHIVE {
                return Err(FrpError::Download("Archive exceeds 256 MiB limit".into()));
            }
            file.write_all(&chunk).await?;
            report(&progress, "downloading", downloaded, total);
        }
        file.flush().await?;
        Ok::<_, FrpError>(())
    }
    .await;
    drop(file);
    if let Err(e) = transfer {
        let _ = tokio::fs::remove_file(&tmp_path).await;
        return Err(e);
    }
    tokio::fs::rename(&tmp_path, &output_path).await?;

    tracing::info!(
        path = %output_path.display(),
        size_mb = downloaded as f64 / 1_048_576.0,
        "FRP download completed"
    );

    Ok(output_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_checksum_manifest_formats() {
        let manifest = concat!(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa  frp_0.70.1_linux_amd64.tar.gz\n",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb *frp_0.70.1_windows_amd64.zip\n",
        );
        assert_eq!(
            parse_checksum_manifest(manifest, "frp_0.70.1_linux_amd64.tar.gz").unwrap(),
            "a".repeat(64)
        );
        assert_eq!(
            parse_checksum_manifest(manifest, "frp_0.70.1_windows_amd64.zip").unwrap(),
            "b".repeat(64)
        );
        assert!(parse_checksum_manifest(manifest, "missing").is_none());
    }

    #[test]
    fn selects_platform_archive_extension() {
        let version = FrpVersion::from_tag("0.70.1");
        assert!(asset_filename(&version, "linux_amd64").ends_with(".tar.gz"));
        assert!(asset_filename(&version, "windows_amd64").ends_with(".zip"));
    }
}

#[cfg(test)]
mod channel_tests {
    use super::*;
    #[test]
    fn channel_urls_preserve_official_asset_paths_and_reject_unsafe_custom_bases() {
        assert_eq!(
            release_base(Some("official"), None).unwrap(),
            OFFICIAL_RELEASE_BASE
        );
        assert_eq!(
            release_base(Some("ghfast"), None).unwrap(),
            format!("https://ghfast.top/{OFFICIAL_RELEASE_BASE}")
        );
        assert_eq!(
            release_base(Some("gh-proxy"), None).unwrap(),
            format!("https://gh-proxy.com/{OFFICIAL_RELEASE_BASE}")
        );
        for base in [
            "http://example.com",
            "https://user:pass@example.com",
            "https://example.com?q=1",
            "https://example.com/#fragment",
        ] {
            assert!(release_base(Some("custom"), Some(base)).is_err());
        }
        assert!(release_base(Some("unknown"), None).is_err());
        assert!(release_base(Some("custom"), None).is_err());
    }
}
