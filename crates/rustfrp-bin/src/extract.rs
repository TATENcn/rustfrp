//! FRP 二进制解压
//!
//! Extract official FRP tar.gz and Windows zip release archives.

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

    if archive_path
        .extension()
        .is_some_and(|extension| extension == "zip")
    {
        extract_zip(archive_path, output_dir)?;
    } else {
        let file = std::fs::File::open(archive_path)?;
        let decoder = GzDecoder::new(file);
        let mut archive = Archive::new(decoder);
        archive
            .unpack(output_dir)
            .map_err(|e| FrpError::Extract(format!("Extraction failed: {e}")))?;
    }

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

fn extract_zip(archive_path: &Path, output_dir: &Path) -> Result<(), FrpError> {
    let file = std::fs::File::open(archive_path)?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|error| FrpError::Extract(format!("Invalid zip archive: {error}")))?;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| FrpError::Extract(format!("Invalid zip entry: {error}")))?;
        let relative = entry.enclosed_name().ok_or_else(|| {
            FrpError::Extract(format!("Unsafe path in zip archive: {}", entry.name()))
        })?;
        let output = output_dir.join(relative);
        if entry.is_dir() {
            std::fs::create_dir_all(&output)?;
            continue;
        }
        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut destination = std::fs::File::create(output)?;
        std::io::copy(&mut entry, &mut destination)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;
    use zip::write::SimpleFileOptions;

    #[test]
    fn test_extract_nonexistent_file() {
        let tmp = TempDir::new().unwrap();
        let result = extract(Path::new("/nonexistent/file.tar.gz"), tmp.path());
        assert!(result.is_err());
    }

    #[test]
    fn extracts_windows_zip_archive() {
        let tmp = TempDir::new().unwrap();
        let archive_path = tmp.path().join("frp_windows_amd64.zip");
        let file = std::fs::File::create(&archive_path).unwrap();
        let mut archive = zip::ZipWriter::new(file);
        archive
            .start_file(
                "frp_0.70.1_windows_amd64/frpc.exe",
                SimpleFileOptions::default(),
            )
            .unwrap();
        archive.write_all(b"frpc-binary").unwrap();
        archive.finish().unwrap();

        let output = tmp.path().join("out");
        extract(&archive_path, &output).unwrap();
        assert_eq!(
            std::fs::read(output.join("frp_0.70.1_windows_amd64/frpc.exe")).unwrap(),
            b"frpc-binary"
        );
    }
}
