//! Cipher - Nyx Secrets and Keyring Daemon
//!
//! Secure storage for passwords, keys, and secrets with:
//! - Memory-safe secret handling (zeroize)
//! - Strong encryption (ChaCha20-Poly1305)
//! - Key derivation (Argon2id)
//! - Session-based unlocking
//! - D-Bus compatible interface

use anyhow::Result;
use clap::Parser;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use nyx_cipher::keyring::Keyring;
use nyx_cipher::session::SessionManager;
use nyx_cipher::ipc::CipherServer;
use nyx_cipher::state::CipherState;

#[derive(Parser)]
#[command(name = "cipherd")]
#[command(about = "Nyx Secrets Daemon")]
struct Args {
    /// Data directory
    #[arg(long, default_value = "/var/lib/cipher")]
    data_dir: String,

    /// Socket path
    #[arg(long, default_value = "/run/cipher/cipher.sock")]
    socket: String,

    /// User socket path (for per-user access)
    #[arg(long)]
    user_socket: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize logging
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::new("info"))
        .init();

    info!("Starting Cipher secrets daemon");

    // Initialize keyring
    let keyring = Keyring::load(&args.data_dir)?;
    let sessions = SessionManager::new();

    let state = Arc::new(RwLock::new(CipherState {
        keyring,
        sessions,
        data_dir: args.data_dir.clone(),
    }));

    // Start IPC server
    let server = CipherServer::new(&args.socket, state.clone());

    info!("Cipher daemon listening on {}", args.socket);

    server.run().await?;

    Ok(())
}
