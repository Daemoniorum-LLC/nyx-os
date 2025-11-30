//! Configuration for Vault secrets daemon

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Main configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultConfig {
    /// Storage settings
    #[serde(default)]
    pub storage: StorageConfig,

    /// Encryption settings
    #[serde(default)]
    pub encryption: EncryptionConfig,

    /// Access control settings
    #[serde(default)]
    pub access: AccessConfig,

    /// Daemon settings
    #[serde(default)]
    pub daemon: DaemonConfig,
}

impl Default for VaultConfig {
    fn default() -> Self {
        Self {
            storage: StorageConfig::default(),
            encryption: EncryptionConfig::default(),
            access: AccessConfig::default(),
            daemon: DaemonConfig::default(),
        }
    }
}

/// Storage configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Storage path
    #[serde(default = "default_storage_path")]
    pub path: String,

    /// Backup path
    #[serde(default = "default_backup_path")]
    pub backup_path: String,

    /// Enable automatic backups
    #[serde(default = "default_true")]
    pub auto_backup: bool,

    /// Backup interval in hours
    #[serde(default = "default_backup_interval")]
    pub backup_interval_hours: u32,

    /// Max backup count
    #[serde(default = "default_backup_count")]
    pub max_backups: usize,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            path: default_storage_path(),
            backup_path: default_backup_path(),
            auto_backup: true,
            backup_interval_hours: default_backup_interval(),
            max_backups: default_backup_count(),
        }
    }
}

/// Encryption configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptionConfig {
    /// Key derivation iterations
    #[serde(default = "default_iterations")]
    pub pbkdf2_iterations: u32,

    /// Salt length in bytes
    #[serde(default = "default_salt_len")]
    pub salt_length: usize,

    /// Auto-lock timeout in seconds (0 = disabled)
    #[serde(default = "default_lock_timeout")]
    pub auto_lock_timeout_secs: u32,

    /// Require unlock on startup
    #[serde(default = "default_true")]
    pub require_unlock: bool,
}

impl Default for EncryptionConfig {
    fn default() -> Self {
        Self {
            pbkdf2_iterations: default_iterations(),
            salt_length: default_salt_len(),
            auto_lock_timeout_secs: default_lock_timeout(),
            require_unlock: true,
        }
    }
}

/// Access control configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessConfig {
    /// Require authentication for read operations
    #[serde(default = "default_true")]
    pub auth_for_read: bool,

    /// Allow listing secret names without auth
    #[serde(default)]
    pub allow_list_names: bool,

    /// Trusted process paths that can access secrets
    #[serde(default)]
    pub trusted_processes: Vec<String>,
}

impl Default for AccessConfig {
    fn default() -> Self {
        Self {
            auth_for_read: true,
            allow_list_names: false,
            trusted_processes: Vec::new(),
        }
    }
}

/// Daemon configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    /// Socket path
    #[serde(default = "default_socket_path")]
    pub socket_path: String,

    /// Log level
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            socket_path: default_socket_path(),
            log_level: default_log_level(),
        }
    }
}

// Default value functions
fn default_true() -> bool {
    true
}

fn default_storage_path() -> String {
    "/var/lib/vault/secrets.enc".to_string()
}

fn default_backup_path() -> String {
    "/var/lib/vault/backups".to_string()
}

fn default_backup_interval() -> u32 {
    24
}

fn default_backup_count() -> usize {
    7
}

fn default_iterations() -> u32 {
    100_000
}

fn default_salt_len() -> usize {
    32
}

fn default_lock_timeout() -> u32 {
    300 // 5 minutes
}

fn default_socket_path() -> String {
    "/run/vault/vault.sock".to_string()
}

fn default_log_level() -> String {
    "info".to_string()
}

impl VaultConfig {
    /// Load configuration from file
    pub fn load(path: &Path) -> Result<Self> {
        if path.exists() {
            let content = std::fs::read_to_string(path)?;
            let config: Self = serde_yaml::from_str(&content)?;
            Ok(config)
        } else {
            Ok(Self::default())
        }
    }
}
