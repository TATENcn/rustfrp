//! RustFRP 插件 SDK
//!
//! 提供给插件开发者使用。包含：
//! - PluginContext — 插件与核心交互的上下文
//! - Permission — 权限枚举
//! - WIT 接口定义（未来）
//!
//! # 使用示例
//!
//! ```rust,no_run
//! use rustfrp_sdk::{PluginContext, PluginResult};
//!
//! pub struct MyPlugin {
//!     ctx: PluginContext,
//! }
//!
//! impl MyPlugin {
//!     pub fn new(ctx: PluginContext) -> Self {
//!         Self { ctx }
//!     }
//! }
//! ```

pub mod context;
pub mod permissions;

pub use context::PluginContext;
pub use permissions::Permission;

/// 插件错误类型
#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Plugin not initialized")]
    NotInitialized,

    #[error("Plugin already stopped")]
    AlreadyStopped,

    #[error("Invalid argument: {0}")]
    InvalidArgument(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

/// 插件 Result 类型
pub type PluginResult<T> = std::result::Result<T, PluginError>;
