//! FRP 二进制下载
//!
//! 从 GitHub Releases 下载指定版本的 FRP 二进制。

use crate::{FrpError, FrpVersion};
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;

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
    let filename = format!("frp_{}_{}.tar.gz", version.version, platform);
    let url = format!(
        "https://github.com/fatedier/frp/releases/download/{}/{}",
        version.tag_name, filename
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
