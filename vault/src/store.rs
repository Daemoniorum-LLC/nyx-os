//! Secret storage

use crate::config::StorageConfig;
use crate::crypto::{CryptoEngine, EncryptedData};
use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use tracing::{debug, info, warn};
use uuid::Uuid;
use zeroize::Zeroize;

/// Secret metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretMetadata {
    /// Secret ID
    pub id: Uuid,
    /// Secret name/path
    pub name: String,
    /// Creation time
    pub created_at: DateTime<Utc>,
    /// Last modified time
    pub modified_at: DateTime<Utc>,
    /// Last accessed time
    pub accessed_at: Option<DateTime<Utc>>,
    /// Secret type
    pub secret_type: SecretType,
    /// Tags
    pub tags: Vec<String>,
    /// Notes
    pub notes: Option<String>,
}

/// Secret type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecretType {
    /// Generic secret
    Generic,
    /// Password
    Password,
    /// API key
    ApiKey,
    /// SSH key
    SshKey,
    /// Certificate
    Certificate,
    /// Token
    Token,
}

/// Secret entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Secret {
    /// Metadata
    pub metadata: SecretMetadata,
    /// Encrypted value
    #[serde(skip)]
    pub value: Option<String>,
}

/// Vault data structure (serialized to disk)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct VaultData {
    /// Version
    version: u32,
    /// Secrets (name -> encrypted data)
    secrets: HashMap<String, SecretEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SecretEntry {
    metadata: SecretMetadata,
    /// Base64-encoded encrypted value
    encrypted_value: String,
}

/// Secret store
pub struct SecretStore {
    config: StorageConfig,
    crypto: CryptoEngine,
    data: Option<VaultData>,
    master_password: Option<Vec<u8>>,
    unlocked: bool,
}

impl SecretStore {
    /// Create new secret store
    pub fn new(config: StorageConfig, crypto: CryptoEngine) -> Self {
        Self {
            config,
            crypto,
            data: None,
            master_password: None,
            unlocked: false,
        }
    }

    /// Check if vault exists
    pub fn exists(&self) -> bool {
        Path::new(&self.config.path).exists()
    }

    /// Initialize new vault
    pub fn initialize(&mut self, password: &[u8]) -> Result<()> {
        if self.exists() {
            return Err(anyhow!("Vault already exists"));
        }

        let data = VaultData {
            version: 1,
            secrets: HashMap::new(),
        };

        self.data = Some(data);
        self.master_password = Some(password.to_vec());
        self.unlocked = true;

        self.save()?;

        info!("Vault initialized");
        Ok(())
    }

    /// Unlock vault with password
    pub fn unlock(&mut self, password: &[u8]) -> Result<()> {
        if !self.exists() {
            return Err(anyhow!("Vault does not exist. Initialize first."));
        }

        // Read encrypted vault data
        let encrypted_bytes = fs::read(&self.config.path)?;
        let encrypted = EncryptedData::from_bytes(&encrypted_bytes)?;

        // Decrypt
        let plaintext = self.crypto.decrypt(&encrypted, password)?;
        let vault_json = String::from_utf8(plaintext)?;
        let data: VaultData = serde_json::from_str(&vault_json)?;

        self.data = Some(data);
        self.master_password = Some(password.to_vec());
        self.unlocked = true;

        info!("Vault unlocked");
        Ok(())
    }

    /// Lock vault
    pub fn lock(&mut self) {
        if let Some(ref mut password) = self.master_password {
            password.zeroize();
        }
        self.master_password = None;
        self.data = None;
        self.unlocked = false;
        info!("Vault locked");
    }

    /// Check if vault is unlocked
    pub fn is_unlocked(&self) -> bool {
        self.unlocked
    }

    /// Save vault to disk
    fn save(&self) -> Result<()> {
        let data = self
            .data
            .as_ref()
            .ok_or_else(|| anyhow!("Vault not loaded"))?;

        let password = self
            .master_password
            .as_ref()
            .ok_or_else(|| anyhow!("Vault locked"))?;

        // Serialize to JSON
        let json = serde_json::to_string(data)?;

        // Encrypt
        let encrypted = self.crypto.encrypt(json.as_bytes(), password)?;
        let encrypted_bytes = encrypted.to_bytes();

        // Ensure directory exists
        if let Some(parent) = Path::new(&self.config.path).parent() {
            fs::create_dir_all(parent)?;
        }

        // Write to file
        fs::write(&self.config.path, encrypted_bytes)?;

        debug!("Vault saved");
        Ok(())
    }

    /// Set a secret
    pub fn set(&mut self, name: &str, value: &str, secret_type: SecretType) -> Result<()> {
        self.require_unlocked()?;

        let password = self.master_password.as_ref().unwrap();
        let data = self.data.as_mut().unwrap();

        // Encrypt the value
        let encrypted = self.crypto.encrypt(value.as_bytes(), password)?;
        let encrypted_b64 = base64::encode(encrypted.to_bytes());

        let now = Utc::now();
        let existing = data.secrets.get(name);

        let metadata = SecretMetadata {
            id: existing.map(|e| e.metadata.id).unwrap_or_else(Uuid::new_v4),
            name: name.to_string(),
            created_at: existing.map(|e| e.metadata.created_at).unwrap_or(now),
            modified_at: now,
            accessed_at: None,
            secret_type,
            tags: existing
                .map(|e| e.metadata.tags.clone())
                .unwrap_or_default(),
            notes: existing.and_then(|e| e.metadata.notes.clone()),
        };

        let entry = SecretEntry {
            metadata,
            encrypted_value: encrypted_b64,
        };

        data.secrets.insert(name.to_string(), entry);
        self.save()?;

        info!("Secret '{}' saved", name);
        Ok(())
    }

    /// Get a secret
    pub fn get(&mut self, name: &str) -> Result<String> {
        self.require_unlocked()?;

        let password = self.master_password.as_ref().unwrap();
        let data = self.data.as_mut().unwrap();

        let entry = data
            .secrets
            .get_mut(name)
            .ok_or_else(|| anyhow!("Secret not found: {}", name))?;

        // Decrypt the value
        let encrypted_bytes = base64::decode(&entry.encrypted_value)?;
        let encrypted = EncryptedData::from_bytes(&encrypted_bytes)?;
        let plaintext = self.crypto.decrypt(&encrypted, password)?;
        let value = String::from_utf8(plaintext)?;

        // Update access time
        entry.metadata.accessed_at = Some(Utc::now());

        debug!("Secret '{}' accessed", name);
        Ok(value)
    }

    /// Delete a secret
    pub fn delete(&mut self, name: &str) -> Result<()> {
        self.require_unlocked()?;

        let data = self.data.as_mut().unwrap();

        if data.secrets.remove(name).is_none() {
            return Err(anyhow!("Secret not found: {}", name));
        }

        self.save()?;

        info!("Secret '{}' deleted", name);
        Ok(())
    }

    /// List all secret names
    pub fn list(&self) -> Result<Vec<SecretMetadata>> {
        self.require_unlocked()?;

        let data = self.data.as_ref().unwrap();
        Ok(data
            .secrets
            .values()
            .map(|e| e.metadata.clone())
            .collect())
    }

    /// Search secrets by tag
    pub fn search_by_tag(&self, tag: &str) -> Result<Vec<SecretMetadata>> {
        self.require_unlocked()?;

        let data = self.data.as_ref().unwrap();
        Ok(data
            .secrets
            .values()
            .filter(|e| e.metadata.tags.contains(&tag.to_string()))
            .map(|e| e.metadata.clone())
            .collect())
    }

    /// Add tag to secret
    pub fn add_tag(&mut self, name: &str, tag: &str) -> Result<()> {
        self.require_unlocked()?;

        let data = self.data.as_mut().unwrap();
        let entry = data
            .secrets
            .get_mut(name)
            .ok_or_else(|| anyhow!("Secret not found: {}", name))?;

        if !entry.metadata.tags.contains(&tag.to_string()) {
            entry.metadata.tags.push(tag.to_string());
            self.save()?;
        }

        Ok(())
    }

    /// Set notes for secret
    pub fn set_notes(&mut self, name: &str, notes: Option<String>) -> Result<()> {
        self.require_unlocked()?;

        let data = self.data.as_mut().unwrap();
        let entry = data
            .secrets
            .get_mut(name)
            .ok_or_else(|| anyhow!("Secret not found: {}", name))?;

        entry.metadata.notes = notes;
        self.save()?;

        Ok(())
    }

    /// Change master password
    pub fn change_password(&mut self, old_password: &[u8], new_password: &[u8]) -> Result<()> {
        // Verify old password
        if !self.unlocked {
            self.unlock(old_password)?;
        }

        // Update password and re-save
        self.master_password = Some(new_password.to_vec());
        self.save()?;

        info!("Master password changed");
        Ok(())
    }

    /// Create backup
    pub fn backup(&self) -> Result<String> {
        self.require_unlocked()?;

        let backup_dir = Path::new(&self.config.backup_path);
        fs::create_dir_all(backup_dir)?;

        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        let backup_file = backup_dir.join(format!("vault_backup_{}.enc", timestamp));

        fs::copy(&self.config.path, &backup_file)?;

        info!("Backup created: {:?}", backup_file);
        Ok(backup_file.to_string_lossy().to_string())
    }

    /// Get vault stats
    pub fn stats(&self) -> Result<VaultStats> {
        self.require_unlocked()?;

        let data = self.data.as_ref().unwrap();

        let by_type: HashMap<SecretType, usize> = data
            .secrets
            .values()
            .fold(HashMap::new(), |mut acc, e| {
                *acc.entry(e.metadata.secret_type).or_insert(0) += 1;
                acc
            });

        Ok(VaultStats {
            total_secrets: data.secrets.len(),
            by_type,
            vault_version: data.version,
        })
    }

    /// Require vault to be unlocked
    fn require_unlocked(&self) -> Result<()> {
        if !self.unlocked {
            Err(anyhow!("Vault is locked"))
        } else {
            Ok(())
        }
    }
}

/// Vault statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultStats {
    pub total_secrets: usize,
    pub by_type: HashMap<SecretType, usize>,
    pub vault_version: u32,
}

impl Drop for SecretStore {
    fn drop(&mut self) {
        self.lock();
    }
}
