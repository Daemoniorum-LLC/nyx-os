//! # Grimoire Daemon
//!
//! Unified persona and settings daemon for DaemonOS.
//!
//! ## Features
//!
//! - **Persona Management**: Register, load, and manage AI personas
//! - **Persona Memory**: Per-persona encrypted memory (via Cipher)
//! - **Ritual Execution**: Automated multi-step workflows
//! - **Hierarchical Config**: System -> User -> App settings
//! - **Live Reload**: Watch for changes and notify subscribers
//! - **Schema Validation**: Validate settings against schemas
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    GRIMOIRE DAEMON                           │
//! │  ┌─────────────────┐  ┌─────────────────┐                   │
//! │  │  PersonaStore   │  │  SettingsStore  │                   │
//! │  │  - Lilith       │  │  - system.toml  │                   │
//! │  │  - Mammon       │  │  - user.toml    │                   │
//! │  │  - Leviathan    │  │  - app/*.toml   │                   │
//! │  │  - Custom...    │  └─────────────────┘                   │
//! │  └────────┬────────┘           │                            │
//! │           │                    │                            │
//! │           └────────────────────┘                            │
//! │                    │                                         │
//! │           ┌────────┴────────┐                               │
//! │           │  MemoryManager  │ ←─── Cipher (encrypted)       │
//! │           └─────────────────┘                               │
//! │                    │                                         │
//! │           ┌────────┴────────┐                               │
//! │           │   IPC Server    │ ←─── Unix Socket              │
//! │           └─────────────────┘                               │
//! └─────────────────────────────────────────────────────────────┘
//! ```

mod config;
mod store;
mod schema;
mod watcher;
mod migration;
mod ipc;
mod persona_store;
mod persona_ipc;
mod ritual_store;

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn, error};

/// Grimoire - Unified persona and settings daemon
#[derive(Parser, Debug)]
#[command(name = "grimoired", version, about = "Grimoire daemon for DaemonOS")]
struct Args {
    /// Base grimoire directory
    #[arg(long, default_value = "/grimoire")]
    base_dir: PathBuf,

    /// User config directory override
    #[arg(long)]
    user_dir: Option<PathBuf>,

    /// Socket path for IPC
    #[arg(short, long, default_value = "/run/grimoire/grimoire.sock")]
    socket: PathBuf,

    /// Enable debug logging
    #[arg(short, long)]
    debug: bool,

    /// Skip loading built-in personas
    #[arg(long)]
    no_builtin: bool,
}

/// Daemon state
pub struct GrimoireDaemon {
    /// Persona store
    pub persona_store: Arc<persona_store::PersonaStore>,
    /// Ritual store
    pub ritual_store: Arc<RwLock<ritual_store::RitualStore>>,
    /// Settings store
    pub settings_store: Arc<RwLock<store::SettingsStore>>,
    /// Schema registry
    pub schemas: Arc<schema::SchemaRegistry>,
    /// Start time
    pub started_at: std::time::Instant,
}

impl GrimoireDaemon {
    /// Get daemon status
    pub async fn status(&self) -> grimoire_core::DaemonStatus {
        grimoire_core::DaemonStatus {
            healthy: true,
            persona_count: self.persona_store.persona_count().await,
            ritual_count: self.ritual_store.read().await.ritual_count(),
            active_executions: self.ritual_store.read().await.active_execution_count(),
            uptime_secs: self.started_at.elapsed().as_secs(),
            memory_bytes: 0, // TODO: Track memory usage
            cipher_available: self.persona_store.cipher_available(),
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize logging
    let log_level = if args.debug { "debug" } else { "info" };
    tracing_subscriber::fmt()
        .with_env_filter(log_level)
        .init();

    info!("Grimoire Daemon v{} starting", env!("CARGO_PKG_VERSION"));
    info!("Base directory: {:?}", args.base_dir);

    // Determine user directory
    let user_dir = args.user_dir.unwrap_or_else(|| {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("~/.config"))
            .join("grimoire")
    });
    info!("User directory: {:?}", user_dir);

    // Create base directories
    tokio::fs::create_dir_all(&args.base_dir).await?;
    tokio::fs::create_dir_all(&user_dir).await?;

    // Ensure socket directory exists
    if let Some(socket_dir) = args.socket.parent() {
        tokio::fs::create_dir_all(socket_dir).await?;
    }

    // Initialize persona store
    let persona_store = Arc::new(persona_store::PersonaStore::new(&args.base_dir));
    persona_store.init().await?;
    info!("Persona store initialized: {} personas", persona_store.persona_count().await);

    // Initialize ritual store
    let ritual_store = Arc::new(RwLock::new(
        ritual_store::RitualStore::new(&args.base_dir.join("rituals"))
    ));
    ritual_store.write().await.init().await?;
    info!("Ritual store initialized: {} rituals", ritual_store.read().await.ritual_count());

    // Initialize settings store
    let settings_store = Arc::new(RwLock::new(
        store::SettingsStore::new(args.base_dir.join("settings.yaml"))
    ));
    settings_store.write().await.load().await?;
    info!("Settings store initialized");

    // Load schemas
    let schemas = Arc::new(
        schema::SchemaRegistry::new(&args.base_dir.join("schemas"))
            .unwrap_or_else(|e| {
                warn!("Failed to load schemas: {}", e);
                schema::SchemaRegistry::empty()
            })
    );

    // Start file watcher for settings
    let settings_clone = settings_store.clone();
    let watcher = watcher::SettingsWatcher::new(
        vec![args.base_dir.join("settings"), user_dir.join("settings")],
        settings_clone,
    );
    if let Ok(w) = watcher {
        tokio::spawn(async move {
            w.run().await;
        });
    }

    // Create daemon state
    let daemon = Arc::new(GrimoireDaemon {
        persona_store,
        ritual_store,
        settings_store,
        schemas,
        started_at: std::time::Instant::now(),
    });

    // Register shutdown handler
    let daemon_shutdown = daemon.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        info!("Shutdown signal received, persisting state...");

        // Persist all memories
        if let Err(e) = daemon_shutdown.persona_store.persist_all_memories().await {
            error!("Failed to persist memories: {}", e);
        }

        info!("Grimoire daemon shutting down");
        std::process::exit(0);
    });

    // Start unified IPC server
    let server = persona_ipc::UnifiedGrimoireServer::new(args.socket, daemon);

    info!("Grimoire daemon ready, listening for connections");
    server.run().await
}
