//! 配置模块
//!
//! # 职责
//!
//! - `model.rs` — 数据模型，1:1 映射 FRP 官方 TOML 规范
//! - `validate.rs` — Schema 校验（IP 格式、端口范围、必填字段）
//! - `generator.rs` — SQLite → TOML 生成器 + 原子写入（tmp → rename）

pub mod generator;
pub mod import;
pub mod model;
pub mod validate;
