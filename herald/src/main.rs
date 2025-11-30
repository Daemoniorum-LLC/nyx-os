//! # Herald
//!
//! Notification system for DaemonOS with platform-aware display.
//!
//! ## Features
//!
//! - **Freedesktop Notifications**: D-Bus notification spec (Linux/WSLg)
//! - **Windows Toast**: Native Windows notifications (WSL)
//! - **Notification History**: Persistent notification log
//! - **Do Not Disturb**: Scheduling and manual modes
//! - **Priority Levels**: Urgent, normal, low
//! - **Actions**: Interactive notification buttons

mod config;
mod notification;
mod history;
mod dnd;
mod dbus;
mod display;
mod ipc;

use libnyx_platform::{Platform, compat::NotificationBackend};

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
        let client = ipc::HeraldClient::new(&args.socket);
        let id = client.notify("herald-cli", &summary, args.body.as_deref()).await?;
        println!("Notification sent: {}", id);
        return Ok(());
    }

    // Daemon mode
    let platform = Platform::detect();
    let backend = libnyx_platform::compat::notification_backend();

    info!(
        "Herald v{} starting on {} with {:?} backend",
        env!("CARGO_PKG_VERSION"),
        platform.name(),
        backend
    );

    let config = config::load_config(&args.config)?;

    // Initialize components
    let history_path = std::path::PathBuf::from("/var/lib/herald/history.json");
    let history = Arc::new(RwLock::new(history::NotificationHistory::new(
        config.history.max_size,
        config.history.retention_days,
        history_path,
    )));
    let dnd_manager = Arc::new(dnd::DndManager::new(config.dnd.clone()));
    let queue = Arc::new(RwLock::new(notification::NotificationQueue::default()));

    // Create action channel
    let (action_tx, mut action_rx) = tokio::sync::mpsc::channel::<(u32, String)>(100);

    // Handle actions in background
    tokio::spawn(async move {
        while let Some((id, action)) = action_rx.recv().await {
            info!("Action invoked: notification={}, action={}", id, action);
        }
    });

    // Start D-Bus service only on native Linux or WSLg
    if matches!(backend, NotificationBackend::Freedesktop) {
        let (dbus_server, mut event_rx, signal_tx) = dbus::NotificationDbusServer::new();
        let queue_clone = queue.clone();
        let history_clone = history.clone();

        tokio::spawn(async move {
            while let Some(event) = event_rx.recv().await {
                match event {
                    dbus::DbusEvent::Notify(request) => {
                        let mut q = queue_clone.write().await;
                        let notification = dbus::NotificationDbusServer::to_notification(request, 0);
                        let id = q.add(notification.clone());
                        history_clone.write().await.add(notification);
                        info!("D-Bus notification: id={}", id);
                    }
                    dbus::DbusEvent::CloseNotification(id) => {
                        queue_clone.write().await.remove(id);
                    }
                    _ => {}
                }
            }
        });
        info!("D-Bus notification handler started");
    } else {
        info!("D-Bus service skipped (not using Freedesktop backend)");
    }

    // Start IPC server
    let server = ipc::HeraldIpcServer::new(queue, history, dnd_manager, action_tx);

    info!("Herald ready");
    server.start(&args.socket).await
}
