//! # Grimoire Settings
//!
//! System settings daemon for DaemonOS.
//!
//! ## Features
//!
//! - **Hierarchical Config**: System -> User -> App settings
//! - **Live Reload**: Watch for changes and notify subscribers
//! - **Schema Validation**: Validate settings against schemas
//! - **Migration**: Automatic settings migration on upgrade

mod config;
mod store;
mod schema;
mod watcher;
mod migration;
mod ipc;

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, error};

/// Grimoire - Settings daemon
#[derive(Parser, Debug)]
#[command(name = "grimoire-settings", version, about)]
struct Args {
    /// System config directory
    #[arg(long, default_value = "/grimoire")]
    system_dir: PathBuf,

    /// User config directory
    #[arg(long)]
    user_dir: Option<PathBuf>,

    /// Socket path
    #[arg(short, long, default_value = "/run/grimoire/settings.sock")]
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

    info!("Grimoire Settings v{} starting", env!("CARGO_PKG_VERSION"));

    // Determine user directory
    let user_dir = args.user_dir.unwrap_or_else(|| {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("~/.config"))
            .join("grimoire")
    });

    // Initialize settings store
    let store = Arc::new(RwLock::new(
        store::SettingsStore::new(&args.system_dir, &user_dir)?
    ));

    // Load schemas
    let schemas = Arc::new(schema::SchemaRegistry::new(&args.system_dir)?);

    // Start file watcher
    let store_clone = store.clone();
    let watcher = watcher::SettingsWatcher::new(
        vec![args.system_dir.clone(), user_dir.clone()],
        store_clone,
    )?;

    tokio::spawn(async move {
        watcher.run().await;
    });

    // Start IPC server
    let server = ipc::GrimoireServer::new(args.socket, store, schemas);

    info!("Grimoire Settings ready");
    server.run().await
}
