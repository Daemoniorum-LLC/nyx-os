//! Session management for keyring access

use anyhow::{Result, anyhow};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use rand::RngCore;

/// Session token
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionToken(String);

impl SessionToken {
    pub fn generate() -> Self {
        let mut bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut bytes);
        Self(base64::encode(&bytes))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for SessionToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Session information
#[derive(Debug)]
pub struct Session {
    pub token: SessionToken,
    pub client_pid: Option<u32>,
    pub client_exe: Option<String>,
    pub created: Instant,
    pub last_access: Instant,
    pub timeout: Duration,
    pub collections: Vec<String>,
}

impl Session {
    pub fn new(timeout: Duration) -> Self {
        let now = Instant::now();
        Self {
            token: SessionToken::generate(),
            client_pid: None,
            client_exe: None,
            created: now,
            last_access: now,
            timeout,
            collections: vec!["default".to_string()],
        }
    }

    pub fn is_expired(&self) -> bool {
        self.last_access.elapsed() > self.timeout
    }

    pub fn touch(&mut self) {
        self.last_access = Instant::now();
    }

    pub fn can_access(&self, collection: &str) -> bool {
        self.collections.contains(&collection.to_string())
    }
}

/// Session manager
pub struct SessionManager {
    sessions: HashMap<String, Session>,
    default_timeout: Duration,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
            default_timeout: Duration::from_secs(300), // 5 minutes
        }
    }

    /// Create a new session
    pub fn create_session(&mut self, pid: Option<u32>, exe: Option<String>) -> SessionToken {
        let mut session = Session::new(self.default_timeout);
        session.client_pid = pid;
        session.client_exe = exe;

        let token = session.token.clone();
        self.sessions.insert(token.as_str().to_string(), session);

        token
    }

    /// Validate and refresh session
    pub fn validate(&mut self, token: &str) -> Result<&mut Session> {
        let session = self.sessions.get_mut(token)
            .ok_or_else(|| anyhow!("Invalid session"))?;

        if session.is_expired() {
            self.sessions.remove(token);
            return Err(anyhow!("Session expired"));
        }

        session.touch();
        Ok(session)
    }

    /// Close a session
    pub fn close_session(&mut self, token: &str) {
        self.sessions.remove(token);
    }

    /// Clean up expired sessions
    pub fn cleanup_expired(&mut self) {
        self.sessions.retain(|_, s| !s.is_expired());
    }

    /// Grant collection access to session
    pub fn grant_access(&mut self, token: &str, collection: &str) -> Result<()> {
        let session = self.sessions.get_mut(token)
            .ok_or_else(|| anyhow!("Invalid session"))?;

        if !session.collections.contains(&collection.to_string()) {
            session.collections.push(collection.to_string());
        }

        Ok(())
    }

    /// Revoke collection access from session
    pub fn revoke_access(&mut self, token: &str, collection: &str) -> Result<()> {
        let session = self.sessions.get_mut(token)
            .ok_or_else(|| anyhow!("Invalid session"))?;

        session.collections.retain(|c| c != collection);
        Ok(())
    }

    /// Set session timeout
    pub fn set_timeout(&mut self, token: &str, timeout: Duration) -> Result<()> {
        let session = self.sessions.get_mut(token)
            .ok_or_else(|| anyhow!("Invalid session"))?;

        session.timeout = timeout;
        Ok(())
    }

    /// Get active session count
    pub fn active_count(&self) -> usize {
        self.sessions.values().filter(|s| !s.is_expired()).count()
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}
