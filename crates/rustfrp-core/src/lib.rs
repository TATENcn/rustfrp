//! RustFRP 微内核
//!
//! 提供配置管理、TOML 生成、进程守护、插件管理的核心能力。
//! 不包含任何 UI、业务逻辑或网络 I/O。
//!
//! # 架构
//!
//! ```text
//! ┌──────────┐   ┌──────────┐   ┌─────────────┐
//! │  SQLite  │──▶│ TOML 生成 │──▶│ 原生 frpc    │
//! │(真理来源) │   │(运行时产物)│   │(子进程)      │
//! └──────────┘   └──────────┘   └─────────────┘
//! ```

pub mod config;
pub mod db;
pub mod error;
pub mod panic_hook;
pub mod plugin;
pub mod process;

pub use error::{CoreError, Result};

/// 核心门面 — 对外暴露的统一 API
///
/// 所有核心操作都通过此 trait 暴露，方便 mock 和测试。
#[async_trait::async_trait]
pub trait CoreFacade: Send + Sync {
    /// 获取核心版本
    fn version(&self) -> &str;

    /// 初始化核心（打开/创建数据库，运行迁移）
    async fn init(&self) -> Result<()>;

    /// 生成所有 frpc TOML 配置（按 Profile 分组）并原子写入
    async fn generate_all_configs(&self, output_dir: &str) -> Result<Vec<String>>;

    /// 启动指定 Profile 的 frpc 子进程
    async fn start_frpc(&self, profile_id: i64) -> Result<()>;

    /// 热重载指定 Profile 的 frpc（发送 SIGHUP）
    async fn reload_frpc(&self, profile_id: i64) -> Result<()>;

    /// 停止指定 Profile 的 frpc 子进程
    async fn stop_frpc(&self, profile_id: i64) -> Result<()>;

    /// 启动所有已配置 Profile 的 frpc 实例
    async fn start_all_frpc(&self) -> Result<()>;

    /// 停止所有 frpc 实例
    async fn stop_all_frpc(&self) -> Result<()>;

    /// 关闭核心，释放资源
    async fn shutdown(&self) -> Result<()>;
}

/// 核心状态
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreState {
    /// 未初始化
    Uninitialized,
    /// 已初始化，数据库就绪
    Ready,
    /// frpc 运行中
    Running,
    /// 正在停止
    Stopping,
    /// 已停止
    Stopped,
    /// 错误状态
    Error(String),
}

impl std::fmt::Display for CoreState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CoreState::Uninitialized => write!(f, "uninitialized"),
            CoreState::Ready => write!(f, "ready"),
            CoreState::Running => write!(f, "running"),
            CoreState::Stopping => write!(f, "stopping"),
            CoreState::Stopped => write!(f, "stopped"),
            CoreState::Error(e) => write!(f, "error: {e}"),
        }
    }
}
