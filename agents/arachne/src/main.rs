//! # Arachne
//!
//! Network agent for DaemonOS.
//!
//! ## Features
//!
//! - **Firewall**: iptables/nftables rule management
//! - **DNS**: Local resolver with caching and filtering
//! - **VPN**: WireGuard integration
//! - **Network Monitoring**: Connection tracking and bandwidth

mod config;
mod firewall;
mod dns;
mod interfaces;
mod routing;
mod monitor;
mod vpn;
mod ipc;

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{info, error};

/// Arachne - Network agent
#[derive(Parser, Debug)]
#[command(name = "arachne", version, about)]
struct Args {
    /// Configuration file
    #[arg(short, long, default_value = "/grimoire/system/arachne.yaml")]
    config: PathBuf,

    /// Socket path
    #[arg(short, long, default_value = "/run/arachne/arachne.sock")]
    socket: PathBuf,

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

    info!("Arachne v{} starting", env!("CARGO_PKG_VERSION"));

    let config = config::load_config(&args.config).await?;

    // Initialize components
    let firewall = Arc::new(firewall::Firewall::new(config.firewall.clone()));
    let dns_resolver = Arc::new(dns::DnsResolver::new(config.dns.clone()));
    let interfaces = Arc::new(tokio::sync::RwLock::new(interfaces::InterfaceManager::new()));
    let routing = Arc::new(tokio::sync::RwLock::new(routing::RoutingTable::new()));
    let monitor = Arc::new(monitor::NetworkMonitor::new(interfaces.clone(), config.monitor.interval_secs));
    let vpn = Arc::new(vpn::VpnManager::new(config.vpn.clone()));

    // Start network monitor
    let monitor_clone = monitor.clone();
    tokio::spawn(async move {
        monitor_clone.start().await;
    });

    // Start IPC server
    let server = ipc::IpcServer::new(
        firewall,
        dns_resolver,
        interfaces,
        routing,
        monitor,
        vpn,
    );

    info!("Arachne ready");
    server.start(&args.socket).await
}
