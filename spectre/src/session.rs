//! Session management

use crate::user::UserInfo;
use crate::Config;
use anyhow::{Result, anyhow};
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::process::Child;
use uuid::Uuid;
use tracing::{info, warn, error, debug};

/// Session state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionState {
    /// Session is starting
    Starting,
    /// Session is active
    Active,
    /// Session is locked
    Locked,
    /// Session is closing
    Closing,
    /// Session has ended
    Ended,
}

impl SessionState {
    pub fn as_str(&self) -> &'static str {
        match self {
            SessionState::Starting => "starting",
            SessionState::Active => "active",
            SessionState::Locked => "locked",
            SessionState::Closing => "closing",
            SessionState::Ended => "ended",
        }
    }
}

/// Session class
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionClass {
    /// User session
    User,
    /// Greeter session
    Greeter,
    /// Background session
    Background,
    /// Lock screen session
    LockScreen,
}

/// User session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Unique session ID
    pub id: String,
    /// Username
    pub username: String,
    /// User ID
    pub uid: u32,
    /// Group ID
    pub gid: u32,
    /// Session state
    pub state: SessionState,
    /// Session class
    pub class: SessionClass,
    /// Session type (e.g., "wayland", "x11", "tty")
    pub session_type: String,
    /// Seat this session is on
    pub seat: String,
    /// VT number (if applicable)
    pub vt: Option<u32>,
    /// TTY device
    pub tty: Option<String>,
    /// Display (e.g., ":0" for X11, "wayland-0" for Wayland)
    pub display: Option<String>,
    /// Remote host (if remote session)
    pub remote_host: Option<String>,
    /// Leader PID
    pub leader_pid: Option<u32>,
    /// When session was created
    pub created_at: DateTime<Local>,
    /// When session became active
    pub active_since: Option<DateTime<Local>>,
    /// Session environment
    pub environment: HashMap<String, String>,
}

impl Session {
    pub fn new(
        user: &UserInfo,
        seat: &str,
        session_type: &str,
        class: SessionClass,
    ) -> Self {
        let id = Uuid::new_v4().to_string();

        Self {
            id,
            username: user.username.clone(),
            uid: user.uid,
            gid: user.gid,
            state: SessionState::Starting,
            class,
            session_type: session_type.to_string(),
            seat: seat.to_string(),
            vt: None,
            tty: None,
            display: None,
            remote_host: None,
            leader_pid: None,
            created_at: Local::now(),
            active_since: None,
            environment: HashMap::new(),
        }
    }

    /// Mark session as active
    pub fn activate(&mut self) {
        self.state = SessionState::Active;
        self.active_since = Some(Local::now());
    }

    /// Lock the session
    pub fn lock(&mut self) {
        if self.state == SessionState::Active {
            self.state = SessionState::Locked;
        }
    }

    /// Unlock the session
    pub fn unlock(&mut self) {
        if self.state == SessionState::Locked {
            self.state = SessionState::Active;
        }
    }

    /// Check if session is active
    pub fn is_active(&self) -> bool {
        self.state == SessionState::Active
    }

    /// Check if session is graphical
    pub fn is_graphical(&self) -> bool {
        matches!(self.session_type.as_str(), "wayland" | "x11" | "mir")
    }

    /// Get session duration
    pub fn duration(&self) -> Option<chrono::Duration> {
        self.active_since.map(|since| Local::now() - since)
    }

    /// Set up environment for session
    pub fn setup_environment(&mut self, xdg_runtime: &str) {
        // Basic environment
        self.environment.insert("USER".into(), self.username.clone());
        self.environment.insert("LOGNAME".into(), self.username.clone());
        self.environment.insert("HOME".into(), format!("/home/{}", self.username));
        self.environment.insert("XDG_RUNTIME_DIR".into(), xdg_runtime.to_string());
        self.environment.insert("XDG_SESSION_ID".into(), self.id.clone());
        self.environment.insert("XDG_SESSION_TYPE".into(), self.session_type.clone());
        self.environment.insert("XDG_SESSION_CLASS".into(), format!("{:?}", self.class).to_lowercase());
        self.environment.insert("XDG_SEAT".into(), self.seat.clone());

        if let Some(vt) = self.vt {
            self.environment.insert("XDG_VTNR".into(), vt.to_string());
        }

        if let Some(display) = &self.display {
            if self.session_type == "wayland" {
                self.environment.insert("WAYLAND_DISPLAY".into(), display.clone());
            } else if self.session_type == "x11" {
                self.environment.insert("DISPLAY".into(), display.clone());
            }
        }
    }
}

/// Session manager
pub struct SessionManager {
    sessions: HashMap<String, Session>,
    user_sessions: HashMap<String, Vec<String>>, // username -> session IDs
    config: Config,
    processes: HashMap<String, Child>,
}

impl SessionManager {
    pub fn new(config: Config) -> Result<Self> {
        Ok(Self {
            sessions: HashMap::new(),
            user_sessions: HashMap::new(),
            config,
            processes: HashMap::new(),
        })
    }

    /// Create a new session
    pub fn create_session(
        &mut self,
        user: &UserInfo,
        seat: &str,
        session_type: &str,
        class: SessionClass,
    ) -> Result<Session> {
        let mut session = Session::new(user, seat, session_type, class);

        // Set up XDG runtime directory
        let xdg_runtime = crate::pam_auth::setup_xdg_runtime(user.uid, user.gid)?;
        session.setup_environment(&xdg_runtime);

        // Register session
        let session_id = session.id.clone();
        self.sessions.insert(session_id.clone(), session.clone());
        self.user_sessions
            .entry(user.username.clone())
            .or_default()
            .push(session_id);

        info!("Created session {} for {}", session.id, user.username);

        Ok(session)
    }

    /// Get a session by ID
    pub fn get(&self, id: &str) -> Option<&Session> {
        self.sessions.get(id)
    }

    /// Get mutable session by ID
    pub fn get_mut(&mut self, id: &str) -> Option<&mut Session> {
        self.sessions.get_mut(id)
    }

    /// Get all sessions for a user
    pub fn user_sessions(&self, username: &str) -> Vec<&Session> {
        self.user_sessions.get(username)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.sessions.get(id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get all active sessions
    pub fn active_sessions(&self) -> impl Iterator<Item = &Session> {
        self.sessions.values().filter(|s| s.is_active())
    }

    /// Get all sessions
    pub fn all(&self) -> impl Iterator<Item = &Session> {
        self.sessions.values()
    }

    /// Activate a session
    pub fn activate(&mut self, id: &str) -> Result<()> {
        let session = self.sessions.get_mut(id)
            .ok_or_else(|| anyhow!("Session not found: {}", id))?;

        session.activate();
        info!("Activated session {}", id);

        Ok(())
    }

    /// Lock a session
    pub fn lock(&mut self, id: &str) -> Result<()> {
        let session = self.sessions.get_mut(id)
            .ok_or_else(|| anyhow!("Session not found: {}", id))?;

        session.lock();
        info!("Locked session {}", id);

        Ok(())
    }

    /// Unlock a session
    pub fn unlock(&mut self, id: &str) -> Result<()> {
        let session = self.sessions.get_mut(id)
            .ok_or_else(|| anyhow!("Session not found: {}", id))?;

        session.unlock();
        info!("Unlocked session {}", id);

        Ok(())
    }

    /// End a session
    pub fn end(&mut self, id: &str) -> Result<()> {
        // Kill session process if any
        if let Some(mut child) = self.processes.remove(id) {
            let _ = child.kill();
            let _ = child.wait();
        }

        // Update session state
        if let Some(session) = self.sessions.get_mut(id) {
            session.state = SessionState::Ended;

            // Remove from user sessions
            if let Some(user_ids) = self.user_sessions.get_mut(&session.username) {
                user_ids.retain(|i| i != id);
            }

            info!("Ended session {} for {}", id, session.username);
        }

        Ok(())
    }

    /// Start a session process
    pub fn start_session_process(&mut self, id: &str, command: &str) -> Result<()> {
        let session = self.sessions.get_mut(id)
            .ok_or_else(|| anyhow!("Session not found: {}", id))?;

        let parts: Vec<&str> = command.split_whitespace().collect();
        if parts.is_empty() {
            return Err(anyhow!("Empty command"));
        }

        let mut cmd = std::process::Command::new(parts[0]);

        if parts.len() > 1 {
            cmd.args(&parts[1..]);
        }

        // Set up environment
        cmd.env_clear();
        for (key, value) in &session.environment {
            cmd.env(key, value);
        }

        // Set working directory
        cmd.current_dir(&session.environment.get("HOME").cloned().unwrap_or_else(|| "/".into()));

        // Start process
        let child = cmd.spawn()?;
        session.leader_pid = child.id();

        self.processes.insert(id.to_string(), child);

        debug!("Started session process for {} with PID {:?}", id, session.leader_pid);

        Ok(())
    }

    /// Get session command from config
    pub fn get_session_command(&self, session_type: &str) -> Option<&str> {
        self.config.sessions.iter()
            .find(|s| s.session_type == session_type)
            .map(|s| s.command.as_str())
    }

    /// Clean up ended sessions
    pub fn cleanup(&mut self) {
        let ended: Vec<String> = self.sessions.iter()
            .filter(|(_, s)| s.state == SessionState::Ended)
            .map(|(id, _)| id.clone())
            .collect();

        for id in ended {
            self.sessions.remove(&id);
        }
    }
}

/// Session scope for cgroups
pub fn session_scope(session_id: &str) -> String {
    format!("session-{}.scope", session_id)
}
