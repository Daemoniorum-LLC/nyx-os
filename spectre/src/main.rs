//! # Spectre
//!
//! Session and login manager for DaemonOS.
//!
//! ## Features
//!
//! - **PAM Authentication**: Pluggable authentication modules
//! - **Session Management**: User sessions with environment setup
//! - **Multi-seat Support**: Multiple simultaneous login sessions
//! - **Auto-login**: Configurable automatic login
//! - **Session Lock**: Screen locking and unlock
//! - **XDG Compliance**: Proper XDG runtime directory setup

mod auth;
mod session;
mod seat;
mod user;
mod greeter;
mod pam_auth;
mod ipc;

use anyhow::Result;
use clap::{Parser, Subcommand};
use libnyx_platform::Platform;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn, error};

/// Spectre - Session Manager
#[derive(Parser, Debug)]
#[command(name = "spectre", version, about)]
struct Args {
    /// Configuration file
    #[arg(short, long, default_value = "/grimoire/system/spectre.yaml")]
    config: PathBuf,

    /// Socket path
    #[arg(short, long, default_value = "/run/spectre/spectre.sock")]
    socket: PathBuf,

    /// VT to use for greeter
    #[arg(long, default_value = "7")]
    vt: u32,

    /// Enable debug logging
    #[arg(short, long)]
    debug: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Start a session for a user
    Login {
        #[arg(short, long)]
        username: String,
        #[arg(short, long)]
        session: Option<String>,
    },
    /// End current session
    Logout,
    /// Lock current session
    Lock,
    /// Unlock session
    Unlock,
    /// List active sessions
    Sessions,
    /// List available seats
    Seats,
    /// Switch to another session
    Switch { session_id: String },
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let log_level = if args.debug { "debug" } else { "info" };
    tracing_subscriber::fmt()
        .with_env_filter(log_level)
        .init();

    let platform = Platform::detect();

    info!(
        "Spectre v{} starting on {}",
        env!("CARGO_PKG_VERSION"),
        platform.name()
    );

    // Handle CLI commands
    if let Some(cmd) = args.command {
        return handle_client_command(&args.socket, cmd).await;
    }

    // Daemon mode
    run_daemon(args).await
}

async fn handle_client_command(socket: &PathBuf, cmd: Commands) -> Result<()> {
    let client = ipc::SpectreClient::new(socket.clone());

    match cmd {
        Commands::Login { username, session } => {
            println!("Use the greeter interface to login");
            // Interactive login would go through greeter
        }
        Commands::Logout => {
            client.logout_current().await?;
            println!("Session ended");
        }
        Commands::Lock => {
            client.lock_current().await?;
            println!("Session locked");
        }
        Commands::Unlock => {
            println!("Use the greeter to unlock");
        }
        Commands::Sessions => {
            let sessions = client.list_sessions().await?;
            println!("{:<36} {:<12} {:<8} {:<10}", "SESSION ID", "USER", "SEAT", "STATE");
            println!("{}", "-".repeat(70));
            for s in sessions {
                println!(
                    "{:<36} {:<12} {:<8} {:<10}",
                    s.id, s.username, s.seat, s.state
                );
            }
        }
        Commands::Seats => {
            let seats = client.list_seats().await?;
            println!("{:<12} {:<20} {:<10}", "SEAT", "ACTIVE SESSION", "CAN TTY");
            println!("{}", "-".repeat(45));
            for s in seats {
                println!(
                    "{:<12} {:<20} {:<10}",
                    s.id,
                    s.active_session.as_deref().unwrap_or("-"),
                    if s.can_tty { "yes" } else { "no" }
                );
            }
        }
        Commands::Switch { session_id } => {
            client.switch_session(&session_id).await?;
            println!("Switched to session {}", session_id);
        }
    }

    Ok(())
}

async fn run_daemon(args: Args) -> Result<()> {
    // Ensure runtime directory
    std::fs::create_dir_all("/run/spectre")?;

    // Load configuration
    let config = load_config(&args.config)?;

    // Initialize seat manager
    let seat_manager = Arc::new(RwLock::new(seat::SeatManager::new()?));

    // Initialize session manager
    let session_manager = Arc::new(RwLock::new(
        session::SessionManager::new(config.clone())?
    ));

    // Initialize PAM authenticator
    let authenticator = Arc::new(pam_auth::PamAuthenticator::new(
        config.pam_service.clone()
    ));

    // Check for auto-login
    if let Some(auto_user) = &config.auto_login {
        if config.auto_login_delay == 0 || is_first_boot() {
            info!("Auto-login configured for user: {}", auto_user);
            if let Err(e) = auto_login(
                auto_user,
                &session_manager,
                &seat_manager,
                &config,
            ).await {
                warn!("Auto-login failed: {}", e);
            }
        }
    }

    // Start greeter on configured VT
    let greeter = Arc::new(greeter::Greeter::new(
        args.vt,
        authenticator.clone(),
        session_manager.clone(),
        seat_manager.clone(),
        config.clone(),
    ));

    // Spawn greeter
    let greeter_handle = {
        let g = greeter.clone();
        tokio::spawn(async move {
            if let Err(e) = g.run().await {
                error!("Greeter error: {}", e);
            }
        })
    };

    // Start IPC server
    let server = ipc::SpectreServer::new(
        args.socket.clone(),
        session_manager.clone(),
        seat_manager.clone(),
        greeter.clone(),
    );

    info!("Spectre ready");

    // Run IPC server
    tokio::select! {
        result = server.run() => {
            if let Err(e) = result {
                error!("IPC server error: {}", e);
            }
        }
        _ = greeter_handle => {
            info!("Greeter exited");
        }
    }

    Ok(())
}

fn load_config(path: &PathBuf) -> Result<Config> {
    if path.exists() {
        let content = std::fs::read_to_string(path)?;
        Ok(serde_yaml::from_str(&content)?)
    } else {
        Ok(Config::default())
    }
}

fn is_first_boot() -> bool {
    !std::path::Path::new("/var/lib/spectre/.not-first-boot").exists()
}

async fn auto_login(
    username: &str,
    sessions: &Arc<RwLock<session::SessionManager>>,
    seats: &Arc<RwLock<seat::SeatManager>>,
    config: &Config,
) -> Result<()> {
    // Get default seat
    let seat = {
        let sm = seats.read().await;
        sm.get_default_seat().ok_or_else(|| anyhow::anyhow!("No seat available"))?
    };

    // Get user info
    let user_info = user::get_user_info(username)?;

    // Get default session type
    let session_type = config.default_session.clone()
        .unwrap_or_else(|| "umbra".to_string());

    // Create session (without PAM auth for auto-login)
    let session = {
        let mut sm = sessions.write().await;
        sm.create_session(
            &user_info,
            &seat,
            &session_type,
            session::SessionClass::User,
        )?
    };

    info!("Auto-login session created: {}", session.id);

    Ok(())
}

/// Spectre configuration
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(default)]
pub struct Config {
    /// PAM service name
    pub pam_service: String,
    /// Default session type
    pub default_session: Option<String>,
    /// Available session types
    pub sessions: Vec<SessionEntry>,
    /// Auto-login user (if any)
    pub auto_login: Option<String>,
    /// Delay before auto-login (seconds)
    pub auto_login_delay: u32,
    /// Greeter theme
    pub greeter_theme: String,
    /// Allow shutdown from greeter
    pub allow_shutdown: bool,
    /// Allow reboot from greeter
    pub allow_reboot: bool,
    /// Minimum UID for login
    pub min_uid: u32,
    /// Maximum UID for login
    pub max_uid: u32,
    /// Hide users from greeter
    pub hidden_users: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            pam_service: "login".to_string(),
            default_session: Some("umbra".to_string()),
            sessions: vec![
                SessionEntry {
                    name: "Umbra Shell".to_string(),
                    command: "/usr/bin/umbra".to_string(),
                    session_type: "umbra".to_string(),
                    desktop: None,
                },
                SessionEntry {
                    name: "Aether Desktop".to_string(),
                    command: "/usr/bin/aether-session".to_string(),
                    session_type: "wayland".to_string(),
                    desktop: Some("aether".to_string()),
                },
            ],
            auto_login: None,
            auto_login_delay: 3,
            greeter_theme: "default".to_string(),
            allow_shutdown: true,
            allow_reboot: true,
            min_uid: 1000,
            max_uid: 60000,
            hidden_users: vec!["root".to_string(), "nobody".to_string()],
        }
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct SessionEntry {
    pub name: String,
    pub command: String,
    pub session_type: String,
    pub desktop: Option<String>,
}
