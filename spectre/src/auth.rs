//! Authentication traits and types

use anyhow::Result;
use async_trait::async_trait;
use zeroize::Zeroize;

/// Authentication result
#[derive(Debug, Clone)]
pub enum AuthResult {
    /// Authentication successful
    Success(AuthInfo),
    /// Authentication failed with reason
    Failure(String),
    /// Need additional authentication factor
    Continue(AuthChallenge),
    /// Account locked/disabled
    Locked(String),
    /// Password expired
    PasswordExpired,
}

impl AuthResult {
    pub fn is_success(&self) -> bool {
        matches!(self, AuthResult::Success(_))
    }
}

/// Information about authenticated user
#[derive(Debug, Clone)]
pub struct AuthInfo {
    pub username: String,
    pub uid: u32,
    pub gid: u32,
    pub home: String,
    pub shell: String,
    pub groups: Vec<u32>,
}

/// Challenge for additional authentication
#[derive(Debug, Clone)]
pub struct AuthChallenge {
    pub challenge_type: ChallengeType,
    pub message: String,
}

#[derive(Debug, Clone, Copy)]
pub enum ChallengeType {
    Password,
    Otp,
    Fingerprint,
    SmartCard,
    Custom,
}

/// Credentials for authentication
#[derive(Zeroize)]
#[zeroize(drop)]
pub struct Credentials {
    pub username: String,
    #[zeroize(skip)]
    password: Vec<u8>,
    pub otp: Option<String>,
}

impl Credentials {
    pub fn new(username: &str, password: &str) -> Self {
        Self {
            username: username.to_string(),
            password: password.as_bytes().to_vec(),
            otp: None,
        }
    }

    pub fn with_otp(mut self, otp: &str) -> Self {
        self.otp = Some(otp.to_string());
        self
    }

    pub fn password(&self) -> &[u8] {
        &self.password
    }

    pub fn password_str(&self) -> Option<&str> {
        std::str::from_utf8(&self.password).ok()
    }
}

impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credentials")
            .field("username", &self.username)
            .field("password", &"[REDACTED]")
            .field("otp", &self.otp.as_ref().map(|_| "[REDACTED]"))
            .finish()
    }
}

/// Trait for authentication backends
#[async_trait]
pub trait Authenticator: Send + Sync {
    /// Authenticate user with credentials
    async fn authenticate(&self, credentials: &Credentials) -> Result<AuthResult>;

    /// Validate an existing session token
    async fn validate_session(&self, token: &str) -> Result<bool>;

    /// Start a new authentication conversation
    async fn start_auth(&self, username: &str) -> Result<AuthChallenge>;

    /// Respond to an authentication challenge
    async fn respond(&self, username: &str, response: &str) -> Result<AuthResult>;

    /// Close authentication session
    async fn close(&self, username: &str) -> Result<()>;

    /// Check if user account is valid (not expired, not locked)
    async fn check_account(&self, username: &str) -> Result<AccountStatus>;
}

/// Account status
#[derive(Debug, Clone)]
pub enum AccountStatus {
    Valid,
    Expired,
    Locked,
    PasswordExpired,
    NotFound,
}

/// Simple password verification
pub fn verify_password_hash(_password: &[u8], _hash: &str) -> bool {
    // In real implementation, would use proper password hashing
    // like argon2, bcrypt, or scrypt
    false
}

/// Lock file for failed attempts
pub struct LoginLock {
    username: String,
    attempts: u32,
    locked_until: Option<std::time::SystemTime>,
}

impl LoginLock {
    pub fn new(username: &str) -> Self {
        Self {
            username: username.to_string(),
            attempts: 0,
            locked_until: None,
        }
    }

    pub fn record_failure(&mut self) {
        self.attempts += 1;

        // Lock after 5 failed attempts
        if self.attempts >= 5 {
            self.locked_until = Some(
                std::time::SystemTime::now() +
                std::time::Duration::from_secs(300) // 5 minutes
            );
        }
    }

    pub fn is_locked(&self) -> bool {
        if let Some(until) = self.locked_until {
            std::time::SystemTime::now() < until
        } else {
            false
        }
    }

    pub fn reset(&mut self) {
        self.attempts = 0;
        self.locked_until = None;
    }
}
