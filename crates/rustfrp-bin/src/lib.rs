//! FRP 二进制管理
//!
//! 负责 FRP 二进制文件的下载、SHA256 校验、解压。
//! 从核心层拆出，无头模式（路由器）不编译此 crate。

pub mod download;
pub mod extract;
pub mod verify;

/// FRP 二进制管理错误
///
/// Each variant carries a unique error code and i18n key (CODE-003).
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

impl FrpError {
    /// Return unique error code, format `FRP_{sequence}`
    pub fn code(&self) -> &'static str {
        match self {
            FrpError::Download(_) => "FRP_001",
            FrpError::Verify(_) => "FRP_002",
            FrpError::Extract(_) => "FRP_003",
            FrpError::Io(_) => "FRP_004",
            FrpError::Network(_) => "FRP_005",
        }
    }

    /// Return i18n translation key for frontend
    pub fn user_message_key(&self) -> &'static str {
        match self {
            FrpError::Download(_) => "error.frp.download",
            FrpError::Verify(_) => "error.frp.verify",
            FrpError::Extract(_) => "error.frp.extract",
            FrpError::Io(_) => "error.frp.io",
            FrpError::Network(_) => "error.frp.network",
        }
    }
}

/// FRP 二进制版本信息
#[derive(Debug, Clone)]
pub struct FrpVersion {
    pub version: String,
    pub tag_name: String,
}

impl FrpVersion {
    /// 从版本字符串解析（如 "0.70.1" 或 "v0.70.1"）。
    ///
    /// 内部统一规整：`version` 为去 `v` 的纯版本号（用于文件名），
    /// `tag_name` 为 GitHub release tag（始终带 `v` 前缀，用于下载 URL）。
    pub fn from_tag(tag: &str) -> Self {
        let version = tag.trim_start_matches('v').to_string();
        let tag_name = format!("v{version}");
        Self { version, tag_name }
    }
}

/// 获取默认 FRP 安装目录
pub fn default_frp_dir() -> std::path::PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".rustfrp")
        .join("frp")
}

pub mod ensure;
pub mod platform;
