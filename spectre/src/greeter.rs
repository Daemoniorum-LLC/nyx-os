//! Login greeter interface

use crate::auth::{Authenticator, Credentials};
use crate::pam_auth::PamAuthenticator;
use crate::seat::SeatManager;
use crate::session::{SessionClass, SessionManager};
use crate::user::{self, UserDisplay};
use crate::Config;
use anyhow::Result;
use std::io::{self, Write};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn, error, debug};

/// Text-based greeter
pub struct Greeter {
    vt: u32,
    authenticator: Arc<PamAuthenticator>,
    sessions: Arc<RwLock<SessionManager>>,
    seats: Arc<RwLock<SeatManager>>,
    config: Config,
    current_user: RwLock<Option<String>>,
}

impl Greeter {
    pub fn new(
        vt: u32,
        authenticator: Arc<PamAuthenticator>,
        sessions: Arc<RwLock<SessionManager>>,
        seats: Arc<RwLock<SeatManager>>,
        config: Config,
    ) -> Self {
        Self {
            vt,
            authenticator,
            sessions,
            seats,
            config,
            current_user: RwLock::new(None),
        }
    }

    /// Run the greeter loop
    pub async fn run(&self) -> Result<()> {
        info!("Greeter starting on VT {}", self.vt);

        loop {
            // Clear screen and show welcome
            self.show_welcome();

            // Get available users
            let users = user::list_login_users(
                self.config.min_uid,
                self.config.max_uid,
                &self.config.hidden_users,
            );

            // Show user selection or prompt
            if users.len() > 1 {
                self.show_user_list(&users);
            }

            // Get username
            let username = match self.prompt_username().await {
                Some(u) => u,
                None => continue,
            };

            // Get password
            let password = match self.prompt_password().await {
                Some(p) => p,
                None => continue,
            };

            // Authenticate
            match self.authenticate(&username, &password).await {
                Ok(()) => {
                    // Start session
                    if let Err(e) = self.start_session(&username).await {
                        error!("Failed to start session: {}", e);
                        println!("\nFailed to start session: {}", e);
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    }
                }
                Err(e) => {
                    error!("Authentication failed for {}: {}", username, e);
                    println!("\nLogin incorrect");
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                }
            }
        }
    }

    fn show_welcome(&self) {
        // ANSI escape to clear screen
        print!("\x1b[2J\x1b[H");

        println!("╔══════════════════════════════════════════════════════════╗");
        println!("║                                                          ║");
        println!("║                      D A E M O N O S                     ║");
        println!("║                                                          ║");
        println!("║                    Nyx Session Manager                   ║");
        println!("║                                                          ║");
        println!("╚══════════════════════════════════════════════════════════╝");
        println!();

        // Show hostname
        if let Ok(hostname) = std::fs::read_to_string("/etc/hostname") {
            println!("  Host: {}", hostname.trim());
        }

        // Show date/time
        println!("  Time: {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"));
        println!();
    }

    fn show_user_list(&self, users: &[user::UserInfo]) {
        println!("Available users:");
        println!();

        for (i, user) in users.iter().enumerate() {
            let display: UserDisplay = user.clone().into();
            println!("  {}. {} ({})", i + 1, display.display_name, display.username);
        }

        println!();
    }

    async fn prompt_username(&self) -> Option<String> {
        print!("Username: ");
        io::stdout().flush().ok()?;

        let mut input = String::new();
        if io::stdin().read_line(&mut input).is_err() {
            return None;
        }

        let username = input.trim().to_string();
        if username.is_empty() {
            return None;
        }

        *self.current_user.write().await = Some(username.clone());
        Some(username)
    }

    async fn prompt_password(&self) -> Option<String> {
        print!("Password: ");
        io::stdout().flush().ok()?;

        // Disable echo for password input
        let password = read_password()?;
        println!(); // Newline after password

        if password.is_empty() {
            return None;
        }

        Some(password)
    }

    async fn authenticate(&self, username: &str, password: &str) -> Result<()> {
        let credentials = Credentials::new(username, password);
        let result = self.authenticator.authenticate(&credentials).await?;

        match result {
            crate::auth::AuthResult::Success(_) => Ok(()),
            crate::auth::AuthResult::Failure(msg) => Err(anyhow::anyhow!(msg)),
            crate::auth::AuthResult::Locked(msg) => Err(anyhow::anyhow!("Account locked: {}", msg)),
            crate::auth::AuthResult::PasswordExpired => Err(anyhow::anyhow!("Password expired")),
            crate::auth::AuthResult::Continue(_) => Err(anyhow::anyhow!("Additional auth required")),
        }
    }

    async fn start_session(&self, username: &str) -> Result<()> {
        // Get user info
        let user_info = user::get_user_info(username)?;

        // Get seat
        let seat = {
            let seats = self.seats.read().await;
            seats.get_default_seat()
                .ok_or_else(|| anyhow::anyhow!("No seat available"))?
        };

        // Get default session type
        let session_type = self.config.default_session.clone()
            .unwrap_or_else(|| "umbra".to_string());

        // Create session
        let session = {
            let mut sessions = self.sessions.write().await;
            sessions.create_session(
                &user_info,
                &seat,
                &session_type,
                SessionClass::User,
            )?
        };

        info!("Created session {} for {}", session.id, username);

        // Get session command
        let command = {
            let sessions = self.sessions.read().await;
            sessions.get_session_command(&session_type)
                .map(String::from)
        };

        // Start session process
        if let Some(cmd) = command {
            let mut sessions = self.sessions.write().await;
            sessions.start_session_process(&session.id, &cmd)?;
            sessions.activate(&session.id)?;
        } else {
            // Default to user's shell
            let mut sessions = self.sessions.write().await;
            sessions.start_session_process(&session.id, &user_info.shell)?;
            sessions.activate(&session.id)?;
        }

        // Add to seat
        {
            let mut seats = self.seats.write().await;
            seats.add_session(&seat, &session.id)?;
            seats.switch_session(&seat, &session.id)?;
        }

        Ok(())
    }

    /// Lock current session
    pub async fn lock(&self) -> Result<()> {
        if let Some(username) = self.current_user.read().await.as_ref() {
            let sessions = self.sessions.read().await;
            for session in sessions.user_sessions(username) {
                if session.is_active() {
                    drop(sessions);
                    let mut sessions = self.sessions.write().await;
                    sessions.lock(&session.id)?;
                    return Ok(());
                }
            }
        }
        Err(anyhow::anyhow!("No active session"))
    }

    /// Get current user
    pub async fn current_user(&self) -> Option<String> {
        self.current_user.read().await.clone()
    }
}

/// Read password with echo disabled
fn read_password() -> Option<String> {
    use nix::sys::termios::{self, LocalFlags, Termios};
    use std::os::unix::io::AsRawFd;

    let stdin = io::stdin();
    let fd = stdin.as_raw_fd();

    // Save current terminal settings
    let original = termios::tcgetattr(fd).ok()?;

    // Disable echo
    let mut new_settings = original.clone();
    new_settings.local_flags.remove(LocalFlags::ECHO);
    termios::tcsetattr(fd, termios::SetArg::TCSANOW, &new_settings).ok()?;

    // Read password
    let mut password = String::new();
    let result = io::stdin().read_line(&mut password);

    // Restore terminal settings
    termios::tcsetattr(fd, termios::SetArg::TCSANOW, &original).ok()?;

    result.ok()?;
    Some(password.trim().to_string())
}

/// Graphical greeter events
#[derive(Debug, Clone)]
pub enum GreeterEvent {
    UserSelected(String),
    PasswordEntered(String),
    SessionSelected(String),
    PowerAction(PowerAction),
    SwitchVt(u32),
}

#[derive(Debug, Clone, Copy)]
pub enum PowerAction {
    Shutdown,
    Reboot,
    Suspend,
    Hibernate,
}
