//! FRP 二进制解压
//!
//! 解压 tar.gz 格式的 FRP 发布包。

use crate::FrpError;
use flate2::read::GzDecoder;
use std::path::Path;
use tar::Archive;

/// 解压 FRP tar.gz 包到目标目录
///
/// # Arguments
///
/// * `archive_path` - tar.gz 文件路径
/// * `output_dir` - 解压目标目录
pub fn extract(archive_path: &Path, output_dir: &Path) -> Result<(), FrpError> {
    tracing::info!(
        archive = %archive_path.display(),
        output = %output_dir.display(),
        "Extracting FRP"
    );

    std::fs::create_dir_all(output_dir)?;

    let file = std::fs::File::open(archive_path)?;
    let decoder = GzDecoder::new(file);
    let mut archive = Archive::new(decoder);

    archive
        .unpack(output_dir)
        .map_err(|e| FrpError::Extract(format!("Extraction failed: {e}")))?;

    tracing::info!(dir = %output_dir.display(), "FRP extraction completed");

    // 标记二进制文件为可执行（Unix）
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for entry in std::fs::read_dir(output_dir)? {
            let entry = entry?;
            let path = entry.path();

            // 解压后的目录里可能还有子目录（如 frp_0.x.0_linux_amd64/）
            if path.is_dir() {
                for inner in std::fs::read_dir(&path)? {
                    let inner = inner?;
                    let name = inner.file_name();
                    let name_str = name.to_string_lossy();
                    if name_str == "frpc"
                        || name_str == "frps"
                        || name_str == "frpc.exe"
                        || name_str == "frps.exe"
                    {
                        let mut perms = inner.metadata()?.permissions();
                        perms.set_mode(0o755);
                        std::fs::set_permissions(inner.path(), perms)?;
                    }
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_extract_nonexistent_file() {
        let tmp = TempDir::new().unwrap();
        let result = extract(Path::new("/nonexistent/file.tar.gz"), tmp.path());
        assert!(result.is_err());
    }
}
