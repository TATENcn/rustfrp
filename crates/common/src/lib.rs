//! RustFRP shared infrastructure
//!
//! Provides plugin system, signal handling, error types, and panic hook —
//! shared between client (frpc wrapper) and server (control + agent).

pub mod error;
pub mod logging;
pub mod panic_hook;
pub mod plugin;
pub mod signal;

pub use error::{Result, SharedError};
