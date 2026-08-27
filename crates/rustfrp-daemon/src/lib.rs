//! RustFRP daemon — HTTP API server for frpc configuration management
//!
//! This crate depends on `rustfrp-client` (the core library) and adds an
//! HTTP API server layer. When built without the `http-api` feature, the
//! daemon falls back to pure signal-listening mode.

#[cfg(feature = "http-api")]
pub mod api;
pub mod metrics;

#[cfg(feature = "http-api")]
pub mod web;

#[cfg(feature = "http-api")]
pub use api::{serve, serve_with_auth};
