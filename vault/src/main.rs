//! Vault - Secrets management daemon for DaemonOS
//!
//! Provides:
//! - Encrypted secret storage
//! - Password generation
//! - Secure credential management
//! - Automatic backups

mod config;
mod crypto;
mod ipc;
mod store;

use crate::config::VaultConfig;
use crate::crypto::CryptoEngine;
use crate::ipc::{DaemonStatus, IpcHandler, IpcServer};
use crate::store::{SecretMetadata, SecretStore, SecretType, VaultStats};
use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tracing::info;

/// Vault - Secrets management daemon
#[derive(Parser, Debug)]
#[command(name = "vaultd", version, about)]
struct Args {
    /// Configuration file
    #[arg(short, long, default_value = "/grimoire/system/vault.yaml")]
    config: PathBuf,

    /// Socket path
    #[arg(short, long, default_value = "/run/vault/vault.sock")]
    socket: PathBuf,

    /// Debug mode
    #[arg(short, long)]
    debug: bool,
}

/// Daemon state
struct VaultState {
    config: VaultConfig,
    store: RwLock<SecretStore>,
    crypto: CryptoEngine,
}

impl VaultState {
    fn new(config: VaultConfig) -> Self {
        let crypto = CryptoEngine::new(config.encryption.clone());
        let store = SecretStore::new(config.storage.clone(), CryptoEngine::new(config.encryption.clone()));

        Self {
            config,
            store: RwLock::new(store),
            crypto,
        }
    }
}

impl IpcHandler for VaultState {
    fn exists(&self) -> bool {
        self.store.read().unwrap().exists()
    }

    fn initialize(&self, password: &str) -> Result<()> {
        self.store.write().unwrap().initialize(password.as_bytes())
    }

    fn unlock(&self, password: &str) -> Result<()> {
        self.store.write().unwrap().unlock(password.as_bytes())
    }

    fn lock(&self) {
        self.store.write().unwrap().lock()
    }

    fn is_unlocked(&self) -> bool {
        self.store.read().unwrap().is_unlocked()
    }

    fn set(&self, name: &str, value: &str, secret_type: SecretType) -> Result<()> {
        self.store.write().unwrap().set(name, value, secret_type)
    }

    fn get(&self, name: &str) -> Result<String> {
        self.store.write().unwrap().get(name)
    }

    fn delete(&self, name: &str) -> Result<()> {
        self.store.write().unwrap().delete(name)
    }

    fn list(&self) -> Result<Vec<SecretMetadata>> {
        self.store.read().unwrap().list()
    }

    fn search_by_tag(&self, tag: &str) -> Result<Vec<SecretMetadata>> {
        self.store.read().unwrap().search_by_tag(tag)
    }

    fn add_tag(&self, name: &str, tag: &str) -> Result<()> {
        self.store.write().unwrap().add_tag(name, tag)
    }

    fn set_notes(&self, name: &str, notes: Option<String>) -> Result<()> {
        self.store.write().unwrap().set_notes(name, notes)
    }

    fn change_password(&self, old: &str, new: &str) -> Result<()> {
        self.store
            .write()
            .unwrap()
            .change_password(old.as_bytes(), new.as_bytes())
    }

    fn backup(&self) -> Result<String> {
        self.store.read().unwrap().backup()
    }

    fn generate_password(&self, length: usize) -> Result<String> {
        self.crypto.generate_password(length)
    }

    fn stats(&self) -> Result<VaultStats> {
        self.store.read().unwrap().stats()
    }

    fn get_status(&self) -> DaemonStatus {
        let store = self.store.read().unwrap();
        let stats = if store.is_unlocked() {
            store.stats().ok()
        } else {
            None
        };

        DaemonStatus {
            version: env!("CARGO_PKG_VERSION").to_string(),
            vault_exists: store.exists(),
            unlocked: store.is_unlocked(),
            stats,
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let log_level = if args.debug { "debug" } else { "info" };
    tracing_subscriber::fmt()
        .with_env_filter(log_level)
        .init();

    info!("Vault v{} starting", env!("CARGO_PKG_VERSION"));

    let config = VaultConfig::load(&args.config)?;
    let state = Arc::new(VaultState::new(config));

    // Start IPC server
    let socket_path = args.socket.to_string_lossy().to_string();
    let server = IpcServer::new(socket_path, Arc::try_unwrap(state).unwrap_or_else(|arc| (*arc).clone()));

    info!("Vault ready");
    server.run().await
}

impl Clone for VaultState {
    fn clone(&self) -> Self {
        Self::new(self.config.clone())
    }
}
