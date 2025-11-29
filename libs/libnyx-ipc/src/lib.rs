//! # libnyx-ipc
//!
//! IPC client library for Nyx agents to communicate with system services.
//!
//! ## Usage
//!
//! ```rust
//! use libnyx_ipc::guardian::GuardianClient;
//! use libnyx_ipc::init::InitClient;
//!
//! // Connect to Guardian
//! let guardian = GuardianClient::connect().await?;
//! let response = guardian.check_capability("filesystem:read", Some("/etc/passwd")).await?;
//!
//! // Connect to Init
//! let init = InitClient::connect().await?;
//! init.register_service("my-agent", pid).await?;
//! ```

pub mod guardian;
pub mod init;
pub mod protocol;

pub use guardian::GuardianClient;
pub use init::InitClient;
pub use protocol::{Message, Response};

/// Default socket paths
pub mod paths {
    /// Guardian socket path
    pub const GUARDIAN_SOCKET: &str = "/run/guardian/guardian.sock";
    /// Init control socket path
    pub const INIT_SOCKET: &str = "/run/nyx/init.sock";
}

/// Common errors
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    #[error("Request failed: {0}")]
    RequestFailed(String),
    #[error("Timeout")]
    Timeout,
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
    #[error("Service unavailable")]
    ServiceUnavailable,
    #[error("Protocol error: {0}")]
    ProtocolError(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
