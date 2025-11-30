//! Scribe - Nyx System Logging Daemon
//!
//! A structured logging daemon featuring:
//! - Binary journal format for efficiency
//! - Structured logging with JSON
//! - Log rotation and compression
//! - Kernel message collection
//! - Remote logging support

mod journal;
mod collector;
mod storage;
mod query;
mod ipc;

use anyhow::Result;
use clap::Parser;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, error};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::journal::Journal;
use crate::collector::{SyslogCollector, KernelCollector, StdoutCollector};
use crate::ipc::ScribeServer;

#[derive(Parser)]
#[command(name = "scribed")]
#[command(about = "Nyx System Logging Daemon")]
struct Args {
    /// Journal directory
    #[arg(long, default_value = "/var/log/scribe")]
    journal_dir: String,

    /// Socket path
    #[arg(long, default_value = "/run/scribe/scribe.sock")]
    socket: String,

    /// Syslog socket path
    #[arg(long, default_value = "/dev/log")]
    syslog_socket: String,

    /// Max journal file size (MB)
    #[arg(long, default_value = "50")]
    max_size_mb: u64,

    /// Retention days
    #[arg(long, default_value = "30")]
    retention_days: u32,
}

/// Daemon state
pub struct ScribeState {
    journal: Journal,
    config: ScribeConfig,
}

#[derive(Clone)]
pub struct ScribeConfig {
    pub journal_dir: String,
    pub max_file_size: u64,
    pub retention_days: u32,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize basic logging (to stderr until journal is ready)
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::new("info"))
        .init();

    info!("Starting Scribe logging daemon");

    let config = ScribeConfig {
        journal_dir: args.journal_dir.clone(),
        max_file_size: args.max_size_mb * 1024 * 1024,
        retention_days: args.retention_days,
    };

    // Initialize journal
    let journal = Journal::open(&args.journal_dir)?;

    let state = Arc::new(RwLock::new(ScribeState {
        journal,
        config: config.clone(),
    }));

    // Start kernel log collector
    let state_clone = state.clone();
    tokio::spawn(async move {
        let collector = KernelCollector::new();
        if let Err(e) = collector.run(state_clone).await {
            error!("Kernel collector error: {}", e);
        }
    });

    // Start syslog collector
    let state_clone = state.clone();
    let syslog_socket = args.syslog_socket.clone();
    tokio::spawn(async move {
        let collector = SyslogCollector::new(&syslog_socket);
        if let Err(e) = collector.run(state_clone).await {
            error!("Syslog collector error: {}", e);
        }
    });

    // Start rotation task
    let state_clone = state.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
            let mut state = state_clone.write().await;
            if let Err(e) = state.journal.rotate() {
                error!("Rotation error: {}", e);
            }
        }
    });

    // Start IPC server
    let server = ScribeServer::new(&args.socket, state.clone());

    info!("Scribe daemon listening on {}", args.socket);

    server.run().await?;

    Ok(())
}
