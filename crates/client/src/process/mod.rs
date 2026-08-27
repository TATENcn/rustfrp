//! 进程管理模块
//!
//! # 职责
//!
//! - `guard.rs` — ProcessGuard：启动/热重载/优雅退出 frpc 子进程
//! - `signal.rs` — 跨平台信号处理

pub mod diagnostic;
pub mod guard;
pub mod manager;
