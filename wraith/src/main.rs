//! Wraith - Nyx Network Manager
//!
//! A network manager daemon for DaemonOS handling:
//! - Interface management
//! - IP configuration (DHCP/static)
//! - WiFi connections
//! - DNS resolution
//! - Network profiles

mod interface;
mod config;
mod dhcp;
mod dns;
mod wifi;
mod profile;
mod ipc;
mod state;

use anyhow::Result;
use clap::Parser;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, error, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::ipc::WraithServer;
use crate::state::WraithState;

#[derive(Parser)]
#[command(name = "wraithd")]
#[command(about = "Nyx Network Manager Daemon")]
struct Args {
    /// Configuration directory
    #[arg(long, default_value = "/etc/wraith")]
    config_dir: String,

    /// Socket path
    #[arg(long, default_value = "/run/wraith/wraith.sock")]
    socket: String,

    /// Run in foreground
    #[arg(short, long)]
    foreground: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize logging
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(EnvFilter::new("info"))
        .init();

    info!("Starting Wraith network manager");

    // Initialize state
    let state = Arc::new(RwLock::new(WraithState::new(&args.config_dir).await?));

    // Apply saved profiles
    {
        let mut state = state.write().await;
        if let Err(e) = state.apply_saved_profiles().await {
            warn!("Failed to apply saved profiles: {}", e);
        }
    }

    // Start interface monitoring
    let state_clone = state.clone();
    tokio::spawn(async move {
        if let Err(e) = monitor_interfaces(state_clone).await {
            error!("Interface monitor error: {}", e);
        }
    });

    // Start IPC server
    let server = WraithServer::new(&args.socket, state.clone());
    server.run().await?;

    Ok(())
}

async fn monitor_interfaces(state: Arc<RwLock<WraithState>>) -> Result<()> {
    use futures::StreamExt;
    use rtnetlink::new_connection;

    let (connection, _handle, mut messages) = new_connection()?;
    tokio::spawn(connection);

    info!("Monitoring network interfaces");

    while let Some((msg, _addr)) = messages.next().await {
        let mut state = state.write().await;
        if let Err(e) = state.interfaces.handle_netlink_event(&msg).await {
            warn!("Error handling netlink event: {}", e);
        }
    }

    Ok(())
}
