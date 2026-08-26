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
    if expected_hash.len() != 64 || !expected_hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(FrpError::Verify(
            "Invalid SHA256 value; expected 64 hexadecimal characters".into(),
        ));
    }
    let actual_hash = sha256_file(file_path).await?;

    if actual_hash.eq_ignore_ascii_case(expected_hash) {
        tracing::info!(hash = %&actual_hash[..16], "SHA256 verification passed");
        Ok(())
    } else {
        Err(FrpError::Verify(format!(
            "SHA256 mismatch. Expected: {}, Actual: {}",
            expected_hash, actual_hash
        )))
    }
}

pub async fn sha256_file(file_path: &Path) -> Result<String, FrpError> {
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

    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn verifies_known_content_and_rejects_malformed_hash() {
        let file = tempfile::NamedTempFile::new().unwrap();
        tokio::fs::write(file.path(), b"rustfrp").await.unwrap();
        let hash = sha256_file(file.path()).await.unwrap();
        assert_eq!(
            hash,
            "d90fdb6a50bc804bf305b305eb93d7e43aca6c81a9935e998fba29026cb8f842"
        );
        verify_sha256(file.path(), &hash).await.unwrap();
        assert!(verify_sha256(file.path(), "not-a-hash").await.is_err());
    }
}
