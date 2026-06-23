//! Client error types
//!
//! Contains database (DB), configuration (CFG), and process management (PROC) errors.
//! Shared errors (plugin, system) are wrapped from `rustfrp_common`.

use rustfrp_common::SharedError;
use thiserror::Error;

/// Client-layer error types
#[derive(Error, Debug)]
pub enum ClientError {
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

    // === Shared errors ===
    #[error(transparent)]
    Shared(#[from] SharedError),
}

impl ClientError {
    /// Return unique error code
    pub fn code(&self) -> &'static str {
        match self {
            // DB
            ClientError::DatabaseConnection(_) => "DB_001",
            ClientError::DatabaseMigration(_) => "DB_002",
            ClientError::DatabaseQuery(_) => "DB_003",
            ClientError::RecordNotFound { .. } => "DB_004",
            ClientError::RecordAlreadyExists(_) => "DB_005",
            // CFG
            ClientError::ConfigValidation(_) => "CFG_001",
            ClientError::InvalidIpAddress(_) => "CFG_002",
            ClientError::InvalidPort(_) => "CFG_003",
            ClientError::MissingRequiredField(_) => "CFG_004",
            ClientError::TomlGeneration(_) => "CFG_010",
            ClientError::TomlSerialization(_) => "CFG_011",
            ClientError::TomlWrite(_) => "CFG_012",
            // PROC
            ClientError::ProcessStart(_) => "PROC_001",
            ClientError::ProcessCommunication(_) => "PROC_002",
            ClientError::ProcessExited { .. } => "PROC_003",
            ClientError::ProcessTimeout(_) => "PROC_004",
            ClientError::SignalError(_) => "PROC_005",
            // Shared (delegate)
            ClientError::Shared(e) => e.code(),
        }
    }

    /// Return i18n translation key
    pub fn user_message_key(&self) -> &'static str {
        match self {
            ClientError::DatabaseConnection(_) => "error.db.connection",
            ClientError::DatabaseMigration(_) => "error.db.migration",
            ClientError::DatabaseQuery(_) => "error.db.query",
            ClientError::RecordNotFound { .. } => "error.db.record_not_found",
            ClientError::RecordAlreadyExists(_) => "error.db.record_exists",
            ClientError::ConfigValidation(_) => "error.config.validation",
            ClientError::InvalidIpAddress(_) => "error.config.invalid_ip",
            ClientError::InvalidPort(_) => "error.config.invalid_port",
            ClientError::MissingRequiredField(_) => "error.config.missing_field",
            ClientError::TomlGeneration(_) => "error.config.toml_generation",
            ClientError::TomlSerialization(_) => "error.config.toml_serialization",
            ClientError::TomlWrite(_) => "error.config.toml_write",
            ClientError::ProcessStart(_) => "error.process.start",
            ClientError::ProcessCommunication(_) => "error.process.communication",
            ClientError::ProcessExited { .. } => "error.process.exited",
            ClientError::ProcessTimeout(_) => "error.process.timeout",
            ClientError::SignalError(_) => "error.process.signal",
            ClientError::Shared(e) => e.user_message_key(),
        }
    }
}

/// Client Result type
pub type Result<T> = std::result::Result<T, ClientError>;
