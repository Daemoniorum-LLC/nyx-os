//! # nyx-init
//!
//! The DaemonOS init system - where agents become daemons.
//!
//! ## Philosophy
//!
//! Traditional init systems (systemd, launchd, runit) manage processes.
//! nyx-init manages **agents** - intelligent services that can:
//!
//! - Communicate via async IPC
//! - Request capabilities dynamically
//! - Self-heal and adapt
//! - Be configured via natural language (Grimoire)
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                        nyx-init                              │
//! ├─────────────────────────────────────────────────────────────┤
//! │                                                              │
//! │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐         │
//! │  │   Service   │  │ Dependency  │  │   Health    │         │
//! │  │   Manager   │  │   Graph     │  │   Monitor   │         │
//! │  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘         │
//! │         │                │                │                 │
//! │  ┌──────┴────────────────┴────────────────┴──────┐         │
//! │  │              Event Bus (IPC Ring)              │         │
//! │  └──────────────────────┬────────────────────────┘         │
//! │                         │                                   │
//! │  ┌──────────────────────┴────────────────────────┐         │
//! │  │            Grimoire Config Loader              │         │
//! │  │     /grimoire/system/services/*.yaml          │         │
//! │  └───────────────────────────────────────────────┘         │
//! │                                                              │
//! └─────────────────────────────────────────────────────────────┘
//!                              │
//!          ┌───────────────────┼───────────────────┐
//!          ▼                   ▼                   ▼
//!    ┌──────────┐       ┌──────────┐       ┌──────────┐
//!    │ Guardian │       │ Malphas  │       │ Archon   │
//!    │(Security)│       │(AI Route)│       │ (Files)  │
//!    └──────────┘       └──────────┘       └──────────┘
//! ```

mod config;
mod service;
mod supervisor;
mod dependency;
mod health;
mod ipc;
mod grimoire;

pub use config::InitConfig;
pub use service::{Service, ServiceState, ServiceSpec};
pub use supervisor::Supervisor;
pub use dependency::DependencyGraph;

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use tracing::{info, error, warn};

/// nyx-init - DaemonOS init system
#[derive(Parser, Debug)]
#[command(name = "nyx-init", version, about)]
struct Args {
    /// Configuration directory (also: NYX_CONFIG_DIR env var)
    #[arg(short, long, env = "NYX_CONFIG_DIR", default_value = "/grimoire/system")]
    config_dir: PathBuf,

    /// Run in user session mode (not PID 1)
    #[arg(long)]
    user_session: bool,

    /// Enable debug logging (also: NYX_DEBUG env var)
    #[arg(short, long, env = "NYX_DEBUG")]
    debug: bool,

    /// Dry run - validate config without starting services
    #[arg(long)]
    dry_run: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize logging
    let log_level = if args.debug { "debug" } else { "info" };
    tracing_subscriber::fmt()
        .with_env_filter(log_level)
        .init();

    info!("nyx-init v{} starting", env!("CARGO_PKG_VERSION"));

    // Load configuration from Grimoire
    let config = config::load_config(&args.config_dir).await?;

    if args.dry_run {
        info!("Dry run mode - validating configuration");
        validate_config(&config).await?;
        info!("Configuration valid!");
        return Ok(());
    }

    // Build dependency graph
    let dep_graph = dependency::build_graph(&config.services)?;
    info!("Built dependency graph with {} services", dep_graph.service_count());

    // Create supervisor
    let mut supervisor = Supervisor::new(config, dep_graph);

    // If we're PID 1, set up signal handlers and mount filesystems
    if !args.user_session && std::process::id() == 1 {
        setup_pid1_environment().await?;
    }

    // Start services in dependency order
    supervisor.start_all().await?;

    // Enter main loop
    supervisor.run().await
}

async fn setup_pid1_environment() -> Result<()> {
    info!("Running as PID 1, setting up system environment");

    // Mount essential filesystems
    // In a real implementation, these would be actual mount calls
    info!("Mounting /proc, /sys, /dev");

    // Set up console
    info!("Setting up console");

    // Set hostname
    info!("Setting hostname to 'daemon'");

    Ok(())
}

async fn validate_config(config: &InitConfig) -> Result<()> {
    // Validate all service specs
    for service in &config.services {
        service.validate()?;
    }

    // Validate dependency graph (check for cycles)
    let graph = dependency::build_graph(&config.services)?;
    graph.validate()?;

    Ok(())
}
