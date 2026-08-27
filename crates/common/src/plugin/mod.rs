//! 插件管理模块
//!
//! # 职责
//!
//! - `manager.rs` — 插件管理器（load / unload / list）
//! - `manifest.rs` — manifest.json 解析与校验
//! - `sandbox.rs` — WASM Host Functions 白名单
//! - `lifecycle.rs` — 插件生命周期状态机
//!
//! # 三种插件形态
//!
//! | 形态 | 运行时 | 场景 | 隔离级别 |
//! |---|---|---|---|
//! | WASM | Wasmtime | 流量统计、配置校验等纯逻辑 | 沙箱 |
//! | Native | libloading | GUI 渲染、硬件交互 | 进程内 |
//! | Sidecar | Stdio/Socket | 消息推送、第三方 API | 进程级 |

pub mod lifecycle;
pub mod manager;
pub mod manifest;
pub mod runtime;
pub mod sandbox;
