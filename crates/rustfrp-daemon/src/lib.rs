//! RustFRP daemon — HTTP API server for frpc configuration management
//!
//! This crate depends on `rustfrp-client` (the core library) and adds an
//! HTTP API server layer. When built without the `http-api` feature, the
//! daemon falls back to pure signal-listening mode.

#[cfg(feature = "http-api")]
pub mod api;
#[cfg(feature = "http-api")]
pub mod desktop;
pub mod instance;
pub mod metrics;
pub mod traffic;

#[cfg(feature = "http-api")]
pub mod web;

#[cfg(feature = "http-api")]
pub use api::{bind_api_listener, serve, serve_with_auth, serve_with_listener, serve_with_policy};
#[cfg(feature = "http-api")]
pub use desktop::{DesktopError, DesktopOptions, DesktopSession};
pub use instance::{InstanceLock, InstanceLockError};
