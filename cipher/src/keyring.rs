//! Keyring management

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{info, debug};

use crate::crypto::{EncryptionKey, Secret, generate_salt, hash_password, verify_password};

/// A keyring collection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Collection {
    pub name: String,
    pub label: String,
    pub locked: bool,
    pub created: chrono::DateTime<chrono::Utc>,
    pub modified: chrono::DateTime<chrono::Utc>,
    #[serde(skip)]
    items: HashMap<String, Item>,
}

/// A secret item in the keyring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Item {
    pub id: String,
    pub label: String,
    pub attributes: HashMap<String, String>,
    pub created: chrono::DateTime<chrono::Utc>,
    pub modified: chrono::DateTime<chrono::Utc>,
    encrypted_secret: Vec<u8>,
}

/// Item search attributes
#[derive(Debug, Clone)]
pub struct SearchAttributes {
    pub attributes: HashMap<String, String>,
}

/// Keyring manager
pub struct Keyring {
    data_dir: PathBuf,
    collections: HashMap<String, Collection>,
    master_key: Option<EncryptionKey>,
    master_salt: [u8; 16],
    master_hash: Option<String>,
}

impl Keyring {
    /// Load keyring from disk
    pub fn load(data_dir: &str) -> Result<Self> {
        let data_dir = PathBuf::from(data_dir);
        std::fs::create_dir_all(&data_dir)?;

        let mut keyring = Self {
            data_dir: data_dir.clone(),
            collections: HashMap::new(),
            master_key: None,
            master_salt: [0u8; 16],
            master_hash: None,
        };

        // Load master key salt and hash
        let master_file = data_dir.join("master.json");
        if master_file.exists() {
            let content = std::fs::read_to_string(&master_file)?;
            let master: MasterKeyData = serde_json::from_str(&content)?;
            keyring.master_salt = master.salt;
            keyring.master_hash = Some(master.password_hash);
        } else {
            // Generate new salt
            keyring.master_salt = generate_salt();
        }

        // Load collections
        let collections_dir = data_dir.join("collections");
        if collections_dir.exists() {
            for entry in std::fs::read_dir(&collections_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("json") {
                    match Self::load_collection(&path) {
                        Ok(collection) => {
                            keyring.collections.insert(collection.name.clone(), collection);
                        }
                        Err(e) => {
                            tracing::warn!("Failed to load collection {:?}: {}", path, e);
                        }
                    }
                }
            }
        }

        // Create default collection if none exists
        if keyring.collections.is_empty() {
            let default = Collection {
                name: "default".to_string(),
                label: "Default Keyring".to_string(),
                locked: true,
                created: chrono::Utc::now(),
                modified: chrono::Utc::now(),
                items: HashMap::new(),
            };
            keyring.collections.insert("default".to_string(), default);
        }

        Ok(keyring)
    }

    fn load_collection(path: &Path) -> Result<Collection> {
        let content = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&content)?)
    }

    /// Initialize with master password
    pub fn initialize(&mut self, password: &str) -> Result<()> {
        if self.master_hash.is_some() {
            return Err(anyhow!("Keyring already initialized"));
        }

        let hash = hash_password(password)?;
        self.master_hash = Some(hash.clone());
        self.master_key = Some(EncryptionKey::derive_from_password(
            password,
            &self.master_salt,
        )?);

        // Save master data
        self.save_master()?;

        info!("Keyring initialized");
        Ok(())
    }

    /// Unlock keyring with password
    pub fn unlock(&mut self, password: &str) -> Result<()> {
        let hash = self.master_hash.as_ref()
            .ok_or_else(|| anyhow!("Keyring not initialized"))?;

        if !verify_password(password, hash)? {
            return Err(anyhow!("Invalid password"));
        }

        self.master_key = Some(EncryptionKey::derive_from_password(
            password,
            &self.master_salt,
        )?);

        // Unlock all collections
        for collection in self.collections.values_mut() {
            collection.locked = false;
        }

        info!("Keyring unlocked");
        Ok(())
    }

    /// Lock keyring
    pub fn lock(&mut self) {
        self.master_key = None;

        for collection in self.collections.values_mut() {
            collection.locked = true;
        }

        info!("Keyring locked");
    }

    /// Check if keyring is unlocked
    pub fn is_unlocked(&self) -> bool {
        self.master_key.is_some()
    }

    /// Create a new collection
    pub fn create_collection(&mut self, name: &str, label: &str) -> Result<()> {
        if self.collections.contains_key(name) {
            return Err(anyhow!("Collection already exists: {}", name));
        }

        let collection = Collection {
            name: name.to_string(),
            label: label.to_string(),
            locked: !self.is_unlocked(),
            created: chrono::Utc::now(),
            modified: chrono::Utc::now(),
            items: HashMap::new(),
        };

        self.collections.insert(name.to_string(), collection);
        self.save_collection(name)?;

        Ok(())
    }

    /// Store a secret
    pub fn store_secret(
        &mut self,
        collection: &str,
        id: &str,
        label: &str,
        secret: &Secret,
        attributes: HashMap<String, String>,
    ) -> Result<()> {
        let key = self.master_key.as_ref()
            .ok_or_else(|| anyhow!("Keyring is locked"))?;

        let encrypted = key.encrypt(secret.as_bytes())?;

        let item = Item {
            id: id.to_string(),
            label: label.to_string(),
            attributes,
            created: chrono::Utc::now(),
            modified: chrono::Utc::now(),
            encrypted_secret: encrypted,
        };

        let coll = self.collections.get_mut(collection)
            .ok_or_else(|| anyhow!("Collection not found: {}", collection))?;

        coll.items.insert(id.to_string(), item);
        coll.modified = chrono::Utc::now();

        self.save_collection(collection)?;

        debug!("Stored secret {} in {}", id, collection);
        Ok(())
    }

    /// Retrieve a secret
    pub fn get_secret(&self, collection: &str, id: &str) -> Result<Secret> {
        let key = self.master_key.as_ref()
            .ok_or_else(|| anyhow!("Keyring is locked"))?;

        let coll = self.collections.get(collection)
            .ok_or_else(|| anyhow!("Collection not found: {}", collection))?;

        let item = coll.items.get(id)
            .ok_or_else(|| anyhow!("Item not found: {}", id))?;

        let decrypted = key.decrypt(&item.encrypted_secret)?;

        Ok(Secret::new(decrypted))
    }

    /// Search for items
    pub fn search(&self, collection: &str, attrs: &SearchAttributes) -> Result<Vec<&Item>> {
        let coll = self.collections.get(collection)
            .ok_or_else(|| anyhow!("Collection not found: {}", collection))?;

        let results: Vec<&Item> = coll.items.values()
            .filter(|item| {
                attrs.attributes.iter().all(|(k, v)| {
                    item.attributes.get(k).map(|iv| iv == v).unwrap_or(false)
                })
            })
            .collect();

        Ok(results)
    }

    /// Delete a secret
    pub fn delete_secret(&mut self, collection: &str, id: &str) -> Result<()> {
        let coll = self.collections.get_mut(collection)
            .ok_or_else(|| anyhow!("Collection not found: {}", collection))?;

        coll.items.remove(id)
            .ok_or_else(|| anyhow!("Item not found: {}", id))?;

        coll.modified = chrono::Utc::now();
        self.save_collection(collection)?;

        Ok(())
    }

    /// List collections
    pub fn list_collections(&self) -> Vec<&Collection> {
        self.collections.values().collect()
    }

    /// List items in collection
    pub fn list_items(&self, collection: &str) -> Result<Vec<&Item>> {
        let coll = self.collections.get(collection)
            .ok_or_else(|| anyhow!("Collection not found: {}", collection))?;

        Ok(coll.items.values().collect())
    }

    fn save_master(&self) -> Result<()> {
        let master = MasterKeyData {
            salt: self.master_salt,
            password_hash: self.master_hash.clone().unwrap_or_default(),
        };

        let content = serde_json::to_string_pretty(&master)?;
        std::fs::write(self.data_dir.join("master.json"), &content)?;

        Ok(())
    }

    fn save_collection(&self, name: &str) -> Result<()> {
        let coll = self.collections.get(name)
            .ok_or_else(|| anyhow!("Collection not found: {}", name))?;

        let collections_dir = self.data_dir.join("collections");
        std::fs::create_dir_all(&collections_dir)?;

        let path = collections_dir.join(format!("{}.json", name));
        let content = serde_json::to_string_pretty(coll)?;
        std::fs::write(&path, &content)?;

        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
struct MasterKeyData {
    salt: [u8; 16],
    password_hash: String,
}
