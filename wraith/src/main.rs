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

use anyhow::Result;
use clap::Parser;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, error, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::interface::InterfaceManager;
use crate::config::NetworkConfig;
use crate::dns::DnsManager;
use crate::profile::ProfileManager;
use crate::ipc::WraithServer;

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

/// Network manager state
pub struct WraithState {
    interfaces: InterfaceManager,
    dns: DnsManager,
    profiles: ProfileManager,
    config: NetworkConfig,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize logging
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::new("info"))
        .init();

    info!("Starting Wraith network manager");

    // Load configuration
    let config = NetworkConfig::load(&args.config_dir)?;

    // Initialize components
    let interfaces = InterfaceManager::new().await?;
    let dns = DnsManager::new(&config)?;
    let profiles = ProfileManager::load(&format!("{}/profiles", args.config_dir))?;

    let state = Arc::new(RwLock::new(WraithState {
        interfaces,
        dns,
        profiles,
        config,
    }));

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
    use rtnetlink::new_connection;

    let (connection, handle, mut messages) = new_connection()?;
    tokio::spawn(connection);

    // Subscribe to link and address events
    let mut link_handle = handle.link().get().execute();

    info!("Monitoring network interfaces");

    loop {
        tokio::select! {
            msg = messages.recv() => {
                if let Some(msg) = msg {
                    let mut state = state.write().await;
                    if let Err(e) = state.interfaces.handle_netlink_event(&msg).await {
                        warn!("Error handling netlink event: {}", e);
                    }
                }
            }
        }
    }
}

impl WraithState {
    async fn apply_saved_profiles(&mut self) -> Result<()> {
        for iface in self.interfaces.list()? {
            if let Some(profile) = self.profiles.get_for_interface(&iface.name) {
                info!("Applying profile {} to {}", profile.name, iface.name);
                self.apply_profile(&iface.name, profile).await?;
            }
        }
        Ok(())
    }

    async fn apply_profile(&mut self, iface: &str, profile: &profile::NetworkProfile) -> Result<()> {
        match &profile.config {
            profile::IpConfig::Dhcp => {
                self.start_dhcp(iface).await?;
            }
            profile::IpConfig::Static { address, gateway, dns } => {
                self.interfaces.set_address(iface, address).await?;
                if let Some(gw) = gateway {
                    self.interfaces.set_gateway(iface, gw).await?;
                }
                if !dns.is_empty() {
                    self.dns.set_servers(dns)?;
                }
            }
        }

        // Bring interface up
        self.interfaces.set_up(iface, true).await?;

        Ok(())
    }

    async fn start_dhcp(&mut self, iface: &str) -> Result<()> {
        let client = dhcp::DhcpClient::new(iface)?;
        let lease = client.request().await?;

        self.interfaces.set_address(iface, &lease.address.to_string()).await?;
        if let Some(gw) = lease.gateway {
            self.interfaces.set_gateway(iface, &gw.to_string()).await?;
        }
        if !lease.dns_servers.is_empty() {
            let servers: Vec<String> = lease.dns_servers.iter()
                .map(|ip| ip.to_string())
                .collect();
            self.dns.set_servers(&servers)?;
        }

        Ok(())
    }
}
