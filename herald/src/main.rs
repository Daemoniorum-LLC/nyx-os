//! # Herald
//!
//! Notification system for DaemonOS.
//!
//! ## Features
//!
//! - **Freedesktop Notifications**: D-Bus notification spec
//! - **Notification History**: Persistent notification log
//! - **Do Not Disturb**: Scheduling and manual modes
//! - **Priority Levels**: Urgent, normal, low
//! - **Actions**: Interactive notification buttons

mod config;
mod notification;
mod history;
mod dnd;
mod dbus;
mod ipc;

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, error};

/// Herald - Notification system
#[derive(Parser, Debug)]
#[command(name = "herald", version, about)]
struct Args {
    /// Configuration file
    #[arg(short, long, default_value = "/grimoire/system/herald.yaml")]
    config: PathBuf,

    /// Socket path
    #[arg(short, long, default_value = "/run/herald/herald.sock")]
    socket: PathBuf,

    /// Send a notification (CLI mode)
    #[arg(long)]
    notify: Option<String>,

    /// Notification body
    #[arg(long)]
    body: Option<String>,

    /// Urgency level (low, normal, critical)
    #[arg(long, default_value = "normal")]
    urgency: String,

    /// Enable debug logging
    #[arg(short, long)]
    debug: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let log_level = if args.debug { "debug" } else { "info" };
    tracing_subscriber::fmt()
        .with_env_filter(log_level)
        .init();

    // CLI notification mode
    if let Some(summary) = args.notify {
        let client = ipc::HeraldClient::connect(&args.socket).await?;
        let urgency = match args.urgency.as_str() {
            "low" => notification::Urgency::Low,
            "critical" => notification::Urgency::Critical,
            _ => notification::Urgency::Normal,
        };
        let id = client.notify(&summary, args.body.as_deref(), urgency).await?;
        println!("Notification sent: {}", id);
        return Ok(());
    }

    // Daemon mode
    info!("Herald v{} starting", env!("CARGO_PKG_VERSION"));

    let config = config::load_config(&args.config)?;

    // Initialize components
    let history = Arc::new(RwLock::new(history::NotificationHistory::new(&config)?));
    let dnd = Arc::new(RwLock::new(dnd::DoNotDisturb::new(&config)?));
    let notifications = Arc::new(RwLock::new(
        notification::NotificationManager::new(history.clone(), dnd.clone())?
    ));

    // Start D-Bus service
    let notif_clone = notifications.clone();
    tokio::spawn(async move {
        if let Err(e) = dbus::run_dbus_service(notif_clone).await {
            error!("D-Bus service error: {}", e);
        }
    });

    // Start IPC server
    let server = ipc::HeraldServer::new(args.socket, notifications, history, dnd);

    info!("Herald ready");
    server.run().await
}
