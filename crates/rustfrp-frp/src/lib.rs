//! FRP 二进制管理
//!
//! 负责 FRP 二进制文件的下载、SHA256 校验、解压。
//! 从核心层拆出，无头模式（路由器）不编译此 crate。

pub mod download;
pub mod extract;
pub mod verify;

/// FRP 二进制管理错误
#[derive(Debug, thiserror::Error)]
pub enum FrpError {
    #[error("Download failed: {0}")]
    Download(String),

    #[error("Verification failed: {0}")]
    Verify(String),

    #[error("Extraction failed: {0}")]
    Extract(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),
}

/// FRP 二进制版本信息
#[derive(Debug, Clone)]
pub struct FrpVersion {
    pub version: String,
    pub tag_name: String,
}

impl FrpVersion {
    /// 从 GitHub release tag 解析版本
    pub fn from_tag(tag: &str) -> Self {
        let version = tag.trim_start_matches('v').to_string();
        Self {
            version,
            tag_name: tag.to_string(),
        }
    }
}

/// 获取默认 FRP 安装目录
pub fn default_frp_dir() -> std::path::PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".rustfrp")
        .join("frp")
}
