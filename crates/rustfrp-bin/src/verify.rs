//! SHA256 校验
//!
//! 下载完成后校验文件的 SHA256，确保未被篡改或损坏。

use crate::FrpError;
use sha2::{Digest, Sha256};
use std::path::Path;
use tokio::io::AsyncReadExt;

/// 校验文件的 SHA256
///
/// # Arguments
///
/// * `file_path` - 待校验的文件路径
/// * `expected_hash` - 期望的 SHA256 哈希（hex 字符串）
pub async fn verify_sha256(file_path: &Path, expected_hash: &str) -> Result<(), FrpError> {
    let mut file = tokio::fs::File::open(file_path)
        .await
        .map_err(FrpError::Io)?;

    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];

    loop {
        let n = file.read(&mut buffer).await.map_err(FrpError::Io)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }

    let result = hasher.finalize();
    let actual_hash = format!("{:x}", result);

    if actual_hash.eq_ignore_ascii_case(expected_hash) {
        tracing::info!(
            hash = %&actual_hash[..16],
            "SHA256 verification passed"
        );
        Ok(())
    } else {
        Err(FrpError::Verify(format!(
            "SHA256 mismatch. Expected: {}, Actual: {}",
            expected_hash, actual_hash
        )))
    }
}
