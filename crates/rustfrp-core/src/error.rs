//! Error type definitions
//!
//! All core-layer public APIs return `Result<T>`, i.e. `std::result::Result<T, CoreError>`.
//!
//! # Error code specification (CODE-003)
//!
//! - Format: `{module}_{sequence}`
//! - Each variant MUST implement `code()` and `user_message_key()`
//! - `Display` is developer-facing (English), `user_message_key()` is for frontend i18n

use thiserror::Error;

/// Core-layer error types
///
/// Grouped by module. Each error has a unique error code.
#[derive(Error, Debug)]
pub enum CoreError {
    // === Database errors (DB) ===
    #[error("Database connection failed: {0}")]
    DatabaseConnection(String),

    #[error("Database migration failed: {0}")]
    DatabaseMigration(String),

    #[error("Database query failed: {0}")]
    DatabaseQuery(#[from] rusqlite::Error),

    #[error("Record not found: table={table}, id={id}")]
    RecordNotFound { table: String, id: i64 },

    #[error("Record already exists: {0}")]
    RecordAlreadyExists(String),

    // === Config validation errors (CFG) ===
    #[error("Configuration validation failed: {0}")]
    ConfigValidation(String),

    #[error("Invalid IP address: {0}")]
    InvalidIpAddress(String),

    #[error("Invalid port: {0}")]
    InvalidPort(String),

    #[error("Required field missing: {0}")]
    MissingRequiredField(String),

    // === TOML generation errors (CFG) ===
    #[error("TOML generation failed: {0}")]
    TomlGeneration(String),

    #[error("TOML serialization failed: {0}")]
    TomlSerialization(String),

    #[error("TOML write failed: {0}")]
    TomlWrite(String),

    // === Process management errors (PROC) ===
    #[error("Process start failed: {0}")]
    ProcessStart(String),

    #[error("Process communication failed: {0}")]
    ProcessCommunication(String),

    #[error("Process exited: exit_code={exit_code}")]
    ProcessExited { exit_code: i32 },

    #[error("Process timeout: {0}")]
    ProcessTimeout(String),

    #[error("Signal send failed: {0}")]
    SignalError(String),

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

    // === General ===
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl CoreError {
    /// Return unique error code, format `{module}_{sequence}`
    ///
    /// Used for log indexing, alerting rules, and frontend error branching.
    pub fn code(&self) -> &'static str {
        match self {
            // DB
            CoreError::DatabaseConnection(_) => "DB_001",
            CoreError::DatabaseMigration(_) => "DB_002",
            CoreError::DatabaseQuery(_) => "DB_003",
            CoreError::RecordNotFound { .. } => "DB_004",
            CoreError::RecordAlreadyExists(_) => "DB_005",
            // CFG
            CoreError::ConfigValidation(_) => "CFG_001",
            CoreError::InvalidIpAddress(_) => "CFG_002",
            CoreError::InvalidPort(_) => "CFG_003",
            CoreError::MissingRequiredField(_) => "CFG_004",
            CoreError::TomlGeneration(_) => "CFG_010",
            CoreError::TomlSerialization(_) => "CFG_011",
            CoreError::TomlWrite(_) => "CFG_012",
            // PROC
            CoreError::ProcessStart(_) => "PROC_001",
            CoreError::ProcessCommunication(_) => "PROC_002",
            CoreError::ProcessExited { .. } => "PROC_003",
            CoreError::ProcessTimeout(_) => "PROC_004",
            CoreError::SignalError(_) => "PROC_005",
            // PLG
            CoreError::PluginLoad(_) => "PLG_001",
            CoreError::PluginUnload(_) => "PLG_002",
            CoreError::PluginValidation(_) => "PLG_003",
            CoreError::PluginPermissionDenied { .. } => "PLG_004",
            CoreError::PluginLifecycleViolation(_) => "PLG_005",
            CoreError::PluginPanic(_) => "PLG_006",
            // General
            CoreError::Io(_) => "SYS_001",
            CoreError::Internal(_) => "SYS_002",
        }
    }

    /// Return i18n translation key
    ///
    /// The frontend uses this key to look up translations in the i18n dictionary.
    /// `Display` is developer-facing (English); this method is for frontend i18n.
    pub fn user_message_key(&self) -> &'static str {
        match self {
            CoreError::DatabaseConnection(_) => "error.db.connection",
            CoreError::DatabaseMigration(_) => "error.db.migration",
            CoreError::DatabaseQuery(_) => "error.db.query",
            CoreError::RecordNotFound { .. } => "error.db.record_not_found",
            CoreError::RecordAlreadyExists(_) => "error.db.record_exists",
            CoreError::ConfigValidation(_) => "error.config.validation",
            CoreError::InvalidIpAddress(_) => "error.config.invalid_ip",
            CoreError::InvalidPort(_) => "error.config.invalid_port",
            CoreError::MissingRequiredField(_) => "error.config.missing_field",
            CoreError::TomlGeneration(_) => "error.config.toml_generation",
            CoreError::TomlSerialization(_) => "error.config.toml_serialization",
            CoreError::TomlWrite(_) => "error.config.toml_write",
            CoreError::ProcessStart(_) => "error.process.start",
            CoreError::ProcessCommunication(_) => "error.process.communication",
            CoreError::ProcessExited { .. } => "error.process.exited",
            CoreError::ProcessTimeout(_) => "error.process.timeout",
            CoreError::SignalError(_) => "error.process.signal",
            CoreError::PluginLoad(_) => "error.plugin.load",
            CoreError::PluginUnload(_) => "error.plugin.unload",
            CoreError::PluginValidation(_) => "error.plugin.validation",
            CoreError::PluginPermissionDenied { .. } => "error.plugin.permission_denied",
            CoreError::PluginLifecycleViolation(_) => "error.plugin.lifecycle",
            CoreError::PluginPanic(_) => "error.plugin.panic",
            CoreError::Io(_) => "error.system.io",
            CoreError::Internal(_) => "error.system.internal",
        }
    }
}

/// Core-layer unified Result type
pub type Result<T> = std::result::Result<T, CoreError>;
