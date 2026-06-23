//! Shared error types
//!
//! Contains plugin errors (PLG) and system errors (SYS) that are
//! used by both client and server via the common crate.

use thiserror::Error;

/// Shared error types used across all crates
///
/// Each variant has a unique error code via `code()` and an i18n key via `user_message_key()`.
#[derive(Error, Debug)]
pub enum SharedError {
    // === Plugin errors (PLG) ===
    #[error("Plugin load failed: {0}")]
    PluginLoad(String),

    #[error("Plugin unload failed: {0}")]
    PluginUnload(String),

    #[error("Plugin validation failed: {0}")]
    PluginValidation(String),

    #[error("Plugin permission denied: required={required:?}, granted={granted:?}")]
    PluginPermissionDenied {
        required: Vec<String>,
        granted: Vec<String>,
    },

    #[error("Plugin lifecycle violation: {0}")]
    PluginLifecycleViolation(String),

    #[error("Plugin crashed: {0}")]
    PluginPanic(String),

    // === System errors (SYS) ===
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl SharedError {
    /// Return unique error code, format `{module}_{sequence}`
    pub fn code(&self) -> &'static str {
        match self {
            // PLG
            SharedError::PluginLoad(_) => "PLG_001",
            SharedError::PluginUnload(_) => "PLG_002",
            SharedError::PluginValidation(_) => "PLG_003",
            SharedError::PluginPermissionDenied { .. } => "PLG_004",
            SharedError::PluginLifecycleViolation(_) => "PLG_005",
            SharedError::PluginPanic(_) => "PLG_006",
            // SYS
            SharedError::Io(_) => "SYS_001",
            SharedError::Internal(_) => "SYS_002",
        }
    }

    /// Return i18n translation key for frontend
    pub fn user_message_key(&self) -> &'static str {
        match self {
            SharedError::PluginLoad(_) => "error.plugin.load",
            SharedError::PluginUnload(_) => "error.plugin.unload",
            SharedError::PluginValidation(_) => "error.plugin.validation",
            SharedError::PluginPermissionDenied { .. } => "error.plugin.permission_denied",
            SharedError::PluginLifecycleViolation(_) => "error.plugin.lifecycle",
            SharedError::PluginPanic(_) => "error.plugin.panic",
            SharedError::Io(_) => "error.system.io",
            SharedError::Internal(_) => "error.system.internal",
        }
    }
}

/// Common Result type alias
pub type Result<T> = std::result::Result<T, SharedError>;
