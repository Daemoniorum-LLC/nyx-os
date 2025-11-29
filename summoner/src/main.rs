//! # Summoner
//!
//! Application launcher for DaemonOS.
//!
//! ## Features
//!
//! - **Desktop Entry Parsing**: Freedesktop .desktop files
//! - **Fuzzy Search**: Fast fuzzy matching
//! - **AI Search**: Natural language app finding (optional)
//! - **Recent Apps**: Track and prioritize frequently used
//! - **Custom Actions**: App-specific quick actions

mod config;
mod index;
mod search;
mod desktop;
mod recent;
mod actions;
mod ipc;

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, error};

/// Summoner - Application launcher
#[derive(Parser, Debug)]
#[command(name = "summoner", version, about)]
struct Args {
    /// Configuration file
    #[arg(short, long, default_value = "/grimoire/system/summoner.yaml")]
    config: PathBuf,

    /// Socket path
    #[arg(short, long, default_value = "/run/summoner/summoner.sock")]
    socket: PathBuf,

    /// Search query (CLI mode)
    #[arg(short, long)]
    query: Option<String>,

    /// Launch app by name (CLI mode)
    #[arg(short, long)]
    launch: Option<String>,

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

    let config = config::load_config(&args.config)?;

    // Build application index
    let index = Arc::new(RwLock::new(index::AppIndex::new(&config)?));

    // Scan for applications
    {
        let mut idx = index.write().await;
        idx.scan().await?;
        info!("Indexed {} applications", idx.count());
    }

    // Handle CLI mode
    if let Some(query) = args.query {
        let idx = index.read().await;
        let results = idx.search(&query, 10);
        for app in results {
            println!("{}: {}", app.name, app.exec);
        }
        return Ok(());
    }

    if let Some(name) = args.launch {
        let idx = index.read().await;
        if let Some(app) = idx.find_by_name(&name) {
            desktop::launch(&app).await?;
        } else {
            eprintln!("Application not found: {}", name);
        }
        return Ok(());
    }

    // Daemon mode
    info!("Summoner v{} starting", env!("CARGO_PKG_VERSION"));

    // Start recent apps tracker
    let recent = Arc::new(RwLock::new(recent::RecentApps::new(&config)?));

    // Start IPC server
    let server = ipc::SummonerServer::new(args.socket, index, recent);

    info!("Summoner ready");
    server.run().await
}
