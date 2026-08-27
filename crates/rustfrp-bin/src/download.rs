//! FRP 二进制下载
//!
//! 从 GitHub Releases 下载指定版本的 FRP 二进制。

use crate::{FrpError, FrpVersion};
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;

const RELEASE_BASE: &str = "https://github.com/fatedier/frp/releases/download";

pub fn asset_filename(version: &FrpVersion, platform: &str) -> String {
    let extension = if platform.starts_with("windows_") {
        "zip"
    } else {
        "tar.gz"
    };
    format!("frp_{}_{}.{}", version.version, platform, extension)
}

pub async fn official_checksum(version: &FrpVersion, asset: &str) -> Result<String, FrpError> {
    let url = format!(
        "{RELEASE_BASE}/{}/frp_sha256_checksums.txt",
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
    let filename = asset_filename(version, platform);
    let url = format!("{RELEASE_BASE}/{}/{}", version.tag_name, filename);

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

    let bytes = response.bytes().await?;

    // 写入临时文件，成功后重命名
    let tmp_path = output_path.with_file_name(format!(
        "{}.tmp",
        output_path
            .file_name()
            .ok_or_else(|| FrpError::Download("Invalid filename".into()))?
            .to_string_lossy()
    ));
    let mut file = tokio::fs::File::create(&tmp_path).await?;
    file.write_all(&bytes).await?;
    file.flush().await?;
    drop(file);

    tokio::fs::rename(&tmp_path, &output_path).await?;

    tracing::info!(
        path = %output_path.display(),
        size_mb = bytes.len() as f64 / 1_048_576.0,
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
