//! 数据库模块
//!
//! 管理 SQLite 连接池、增量迁移、三表 CRUD。
//!
//! # 设计
//!
//! - `mod.rs` — 连接池管理 + 模块入口
//! - `migrate.rs` — 增量迁移 + checksum 校验
//! - `profile.rs` — FrpsProfile CRUD
//! - `proxy.rs` — LocalProxy CRUD
//! - `binding.rs` — BindingRule CRUD
//! - `visitor.rs` — LocalVisitor CRUD

pub mod binding;
pub mod environment;
pub mod migrate;
pub mod profile;
pub mod proxy;
pub mod visitor;

use crate::error::{ClientError, Result};
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

/// 数据库连接池（核心层使用单一连接 + Mutex 包装）
///
/// SQLite 在 WAL 模式下支持并发读但写是串行的。
/// 使用 `Arc<Mutex<Connection>>` 确保写操作互斥。
#[derive(Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
    db_path: PathBuf,
}

impl Database {
    /// 打开（或创建）数据库
    ///
    /// 自动启用 WAL 模式和外键约束。
    /// 若数据库文件不存在则自动创建。
    pub async fn open(path: &str) -> Result<Self> {
        let db_path = PathBuf::from(path);

        // 确保父目录存在
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                ClientError::DatabaseConnection(format!("Failed to create database directory: {e}"))
            })?;
        }

        let conn = Connection::open(&db_path).map_err(|e| {
            ClientError::DatabaseConnection(format!("Failed to open database: {e}"))
        })?;

        // 启用 WAL 模式（允许并发读）
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(|e| ClientError::DatabaseConnection(format!("Failed to enable WAL: {e}")))?;

        // 启用外键约束
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(|e| {
                ClientError::DatabaseConnection(format!("Failed to enable foreign keys: {e}"))
            })?;

        tracing::info!(path = %db_path.display(), "Database opened (WAL mode)");

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            db_path,
        })
    }

    /// 获取数据库文件路径
    pub fn path(&self) -> &PathBuf {
        &self.db_path
    }

    /// 获取内部连接（用于需要在锁内执行多个操作的情况）
    ///
    /// 注意：返回的是 MutexGuard，持有期间会阻塞其他操作。
    pub async fn lock(&self) -> tokio::sync::MutexGuard<'_, Connection> {
        self.conn.lock().await
    }
}

impl std::fmt::Debug for Database {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Database")
            .field("path", &self.db_path)
            .finish_non_exhaustive()
    }
}

/// 获取默认数据库路径
///
/// 返回 `~/.rustfrp/config.db`
pub fn default_db_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".rustfrp")
        .join("config.db")
}
