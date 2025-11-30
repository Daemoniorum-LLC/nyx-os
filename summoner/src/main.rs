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
    let index = Arc::new(RwLock::new(index::AppIndex::new()));

    // Scan for applications
    {
        let mut idx = index.write().await;
        let app_dirs = config.app_directories.iter()
            .map(PathBuf::from)
            .collect::<Vec<_>>();
        let entries = desktop::scan_directories(&app_dirs).await;
        for (entry, path) in entries {
            idx.add(entry, path).await;
        }
        info!("Indexed {} applications", idx.len().await);
    }

    // Handle CLI mode
    if let Some(query) = args.query {
        let idx = index.read().await;
        let results = idx.search_prefix(&query).await;
        for app in results.iter().take(10) {
            println!("{}: {}", app.entry.name, app.entry.exec);
        }
        return Ok(());
    }

    if let Some(name) = args.launch {
        let idx = index.read().await;
        if let Some(app) = idx.get(&name).await {
            let (mut launcher, _rx) = actions::Launcher::new();
            launcher.launch(&app.entry, &[]).await?;
        } else {
            eprintln!("Application not found: {}", name);
        }
        return Ok(());
    }

    // Daemon mode
    info!("Summoner v{} starting", env!("CARGO_PKG_VERSION"));

    // Start recent apps tracker
    let recent_path = dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
        .join("summoner/recent.json");
    let recent = Arc::new(RwLock::new(recent::RecentApps::new(config.recent.max_size, recent_path)));

    // Create search engine and launcher
    let search = Arc::new(search::SearchEngine::new(config.search.clone()));
    let (launcher, mut launch_rx) = actions::Launcher::new();
    let launcher = Arc::new(RwLock::new(launcher));

    // Handle launch events
    tokio::spawn(async move {
        while let Some(event) = launch_rx.recv().await {
            match event {
                actions::LaunchEvent::Started { app_id, pid } => {
                    info!("Launched {} (PID: {})", app_id, pid);
                }
                actions::LaunchEvent::Exited { app_id, pid, code } => {
                    info!("App {} (PID: {}) exited with code {}", app_id, pid, code);
                }
                actions::LaunchEvent::Failed { app_id, error } => {
                    error!("Failed to launch {}: {}", app_id, error);
                }
            }
        }
    });

    // Start IPC server
    let server = ipc::SummonerIpcServer::new(index, search, launcher, recent);

    info!("Summoner ready");
    server.start(&args.socket).await
}
