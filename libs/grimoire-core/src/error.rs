//! Error types for Grimoire operations

use thiserror::Error;

/// Grimoire error types
#[derive(Debug, Error)]
pub enum GrimoireError {
    /// Persona not found
    #[error("Persona not found: {0}")]
    PersonaNotFound(String),

    /// Ritual not found
    #[error("Ritual not found: {0}")]
    RitualNotFound(String),

    /// Model not loaded
    #[error("Model not loaded: {0}")]
    ModelNotLoaded(String),

    /// Inference failed
    #[error("Inference failed: {0}")]
    InferenceFailed(String),

    /// Parse error (TOML, JSON, etc.)
    #[error("Parse error: {0}")]
    ParseError(String),

    /// IO error
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    /// IPC error
    #[error("IPC error: {0}")]
    IpcError(String),

    /// Memory error
    #[error("Memory error: {0}")]
    MemoryError(String),

    /// Validation error
    #[error("Validation error: {0}")]
    ValidationError(String),

    /// Encryption error (Cipher integration)
    #[error("Encryption error: {0}")]
    EncryptionError(String),

    /// Ritual execution error
    #[error("Ritual execution error: {0}")]
    RitualExecutionError(String),

    /// Permission denied
    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    /// Resource already exists
    #[error("Resource already exists: {0}")]
    AlreadyExists(String),

    /// Timeout
    #[error("Operation timed out: {0}")]
    Timeout(String),

    /// Service unavailable
    #[error("Service unavailable: {0}")]
    Unavailable(String),
}

/// Result type for Grimoire operations
pub type Result<T> = std::result::Result<T, GrimoireError>;

impl GrimoireError {
    /// Check if this error is recoverable
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            Self::Timeout(_) | Self::Unavailable(_) | Self::IpcError(_)
        )
    }

    /// Check if this error indicates a missing resource
    pub fn is_not_found(&self) -> bool {
        matches!(
            self,
            Self::PersonaNotFound(_) | Self::RitualNotFound(_) | Self::ModelNotLoaded(_)
        )
    }

    /// Convert to IPC error code
    pub fn to_error_code(&self) -> crate::ipc::ErrorCode {
        match self {
            Self::PersonaNotFound(_)
            | Self::RitualNotFound(_)
            | Self::ModelNotLoaded(_) => crate::ipc::ErrorCode::NotFound,

            Self::ValidationError(_)
            | Self::ParseError(_) => crate::ipc::ErrorCode::ValidationError,

            Self::PermissionDenied(_) => crate::ipc::ErrorCode::PermissionDenied,

            Self::AlreadyExists(_) => crate::ipc::ErrorCode::AlreadyExists,

            Self::Timeout(_) => crate::ipc::ErrorCode::Timeout,

            Self::Unavailable(_) => crate::ipc::ErrorCode::Unavailable,

            _ => crate::ipc::ErrorCode::InternalError,
        }
    }
}
