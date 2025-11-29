//! # Archon
//!
//! Process orchestration agent for DaemonOS.
//!
//! ## Philosophy
//!
//! Archon manages the lifecycle of all processes in the system, from spawning
//! to termination. It coordinates with Guardian for security decisions and
//! enforces resource limits through cgroups.
//!
//! ## Features
//!
//! - **Process Lifecycle**: Spawn, monitor, and terminate processes
//! - **Resource Management**: CPU, memory, IO quotas via cgroups
//! - **Process Groups**: Manage related processes together
//! - **Guardian Integration**: Capability checks before process actions
//! - **Statistics**: Real-time process monitoring and metrics
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                         ARCHON                               │
//! ├─────────────────────────────────────────────────────────────┤
//! │                                                              │
//! │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐         │
//! │  │   Process   │  │   Resource  │  │   Stats     │         │
//! │  │   Manager   │  │   Manager   │  │   Collector │         │
//! │  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘         │
//! │         │                │                │                 │
//! │  ┌──────┴────────────────┴────────────────┴──────┐         │
//! │  │              Process Orchestrator              │         │
//! │  └──────────────────────┬────────────────────────┘         │
//! │                         │                                   │
//! │                    ┌────┴────┐                              │
//! │                    │ Guardian │                             │
//! │                    │  Client  │                             │
//! │                    └─────────┘                              │
//! └─────────────────────────────────────────────────────────────┘
//!                              │
//!                              ▼
//!               ┌──────────────────────────┐
//!               │     IPC Socket           │
//!               │  /run/archon/archon.sock │
//!               └──────────────────────────┘
//! ```

mod config;
mod process;
mod resource;
mod cgroup;
mod stats;
mod orchestrator;
mod ipc;

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, error, warn};

/// Archon - Process orchestration agent
#[derive(Parser, Debug)]
#[command(name = "archon", version, about)]
struct Args {
    /// Configuration file
    #[arg(short, long, default_value = "/grimoire/system/archon.yaml")]
    config: PathBuf,

    /// Socket path
    #[arg(short, long, default_value = "/run/archon/archon.sock")]
    socket: PathBuf,

    /// Enable debug logging
    #[arg(short, long)]
    debug: bool,

    /// Guardian socket path
    #[arg(long, default_value = "/run/guardian/guardian.sock")]
    guardian_socket: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize logging
    let log_level = if args.debug { "debug" } else { "info" };
    tracing_subscriber::fmt()
        .with_env_filter(log_level)
        .init();

    info!("Archon v{} starting", env!("CARGO_PKG_VERSION"));

    // Load configuration
    let config = config::load_config(&args.config).await?;

    // Initialize cgroup controller
    let cgroup_manager = Arc::new(cgroup::CgroupManager::new(&config.cgroups)?);

    // Initialize resource manager
    let resource_manager = Arc::new(RwLock::new(
        resource::ResourceManager::new(&config.resources, cgroup_manager.clone())?
    ));

    // Initialize process manager
    let process_manager = Arc::new(RwLock::new(
        process::ProcessManager::new(&config.process, resource_manager.clone())?
    ));

    // Initialize stats collector
    let stats_collector = Arc::new(
        stats::StatsCollector::new(&config.stats, process_manager.clone())?
    );

    // Create orchestrator
    let orchestrator = Arc::new(orchestrator::Orchestrator::new(
        process_manager.clone(),
        resource_manager.clone(),
        stats_collector.clone(),
        args.guardian_socket.clone(),
    ).await?);

    // Start background tasks
    let orchestrator_clone = orchestrator.clone();
    tokio::spawn(async move {
        if let Err(e) = orchestrator_clone.run_background_tasks().await {
            error!("Background task error: {}", e);
        }
    });

    // Start IPC server
    let server = ipc::ArchonServer::new(
        args.socket,
        orchestrator.clone(),
    );

    info!("Archon ready");

    // Run server
    server.run().await
}
