//! 进程管理模块
//!
//! # 职责
//!
//! - `guard.rs` — ProcessGuard：启动/热重载/优雅退出 frpc 子进程
//! - `command.rs` — 子进程构造（Windows 下禁止弹出控制台窗口）
//! - `signal.rs` — 跨平台信号处理

pub mod command;
pub mod diagnostic;
pub mod guard;
pub mod manager;
