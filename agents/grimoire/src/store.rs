//! Settings storage backend

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Settings storage with hierarchical keys
pub struct SettingsStore {
    root: Arc<RwLock<SettingsNode>>,
    file_path: PathBuf,
    dirty: Arc<RwLock<bool>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SettingsNode {
    #[serde(default)]
    pub value: Option<Value>,
    #[serde(default)]
    pub children: HashMap<String, SettingsNode>,
    #[serde(skip)]
    pub metadata: Option<NodeMetadata>,
}

#[derive(Debug, Clone)]
pub struct NodeMetadata {
    pub schema: Option<String>,
    pub description: Option<String>,
    pub default: Option<Value>,
    pub modified: std::time::SystemTime,
}

impl SettingsStore {
    /// Create a new settings store
    pub fn new(file_path: PathBuf) -> Self {
        Self {
            root: Arc::new(RwLock::new(SettingsNode::default())),
            file_path,
            dirty: Arc::new(RwLock::new(false)),
        }
    }

    /// Load settings from file
    pub async fn load(&self) -> Result<()> {
        if self.file_path.exists() {
            let content = tokio::fs::read_to_string(&self.file_path).await?;
            let node: SettingsNode = serde_yaml::from_str(&content)?;
            *self.root.write().await = node;
            tracing::info!("Loaded settings from {:?}", self.file_path);
        }
        Ok(())
    }

    /// Save settings to file
    pub async fn save(&self) -> Result<()> {
        let root = self.root.read().await;
        let content = serde_yaml::to_string(&*root)?;

        // Ensure directory exists
        if let Some(parent) = self.file_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        tokio::fs::write(&self.file_path, content).await?;
        *self.dirty.write().await = false;
        tracing::debug!("Saved settings to {:?}", self.file_path);
        Ok(())
    }

    /// Get a value by path (e.g., "display.theme.name")
    pub async fn get(&self, path: &str) -> Option<Value> {
        let root = self.root.read().await;
        let parts: Vec<&str> = path.split('.').collect();
        self.get_recursive(&root, &parts)
    }

    fn get_recursive(&self, node: &SettingsNode, path: &[&str]) -> Option<Value> {
        if path.is_empty() {
            return node.value.clone();
        }

        let key = path[0];
        if let Some(child) = node.children.get(key) {
            if path.len() == 1 {
                child.value.clone()
            } else {
                self.get_recursive(child, &path[1..])
            }
        } else {
            None
        }
    }

    /// Set a value by path
    pub async fn set(&self, path: &str, value: Value) -> Result<()> {
        let mut root = self.root.write().await;
        let parts: Vec<&str> = path.split('.').collect();
        self.set_recursive(&mut root, &parts, value)?;
        *self.dirty.write().await = true;
        Ok(())
    }

    fn set_recursive(&self, node: &mut SettingsNode, path: &[&str], value: Value) -> Result<()> {
        if path.is_empty() {
            node.value = Some(value);
            return Ok(());
        }

        let key = path[0];

        if path.len() == 1 {
            // Set the value at this key
            let child = node.children.entry(key.to_string()).or_default();
            child.value = Some(value);
        } else {
            // Navigate/create children
            let child = node.children.entry(key.to_string()).or_default();
            self.set_recursive(child, &path[1..], value)?;
        }

        Ok(())
    }

    /// Delete a value by path
    pub async fn delete(&self, path: &str) -> Result<bool> {
        let mut root = self.root.write().await;
        let parts: Vec<&str> = path.split('.').collect();
        let deleted = self.delete_recursive(&mut root, &parts);
        if deleted {
            *self.dirty.write().await = true;
        }
        Ok(deleted)
    }

    fn delete_recursive(&self, node: &mut SettingsNode, path: &[&str]) -> bool {
        if path.is_empty() {
            return false;
        }

        let key = path[0];

        if path.len() == 1 {
            // Remove this key
            node.children.remove(key).is_some()
        } else if let Some(child) = node.children.get_mut(key) {
            self.delete_recursive(child, &path[1..])
        } else {
            false
        }
    }

    /// Check if a path exists
    pub async fn exists(&self, path: &str) -> bool {
        self.get(path).await.is_some()
    }

    /// List keys under a path
    pub async fn list(&self, path: &str) -> Vec<String> {
        let root = self.root.read().await;

        if path.is_empty() {
            return root.children.keys().cloned().collect();
        }

        let parts: Vec<&str> = path.split('.').collect();
        self.list_recursive(&root, &parts)
    }

    fn list_recursive(&self, node: &SettingsNode, path: &[&str]) -> Vec<String> {
        if path.is_empty() {
            return node.children.keys().cloned().collect();
        }

        let key = path[0];
        if let Some(child) = node.children.get(key) {
            self.list_recursive(child, &path[1..])
        } else {
            Vec::new()
        }
    }

    /// Get all settings as a flat map
    pub async fn flatten(&self) -> HashMap<String, Value> {
        let root = self.root.read().await;
        let mut result = HashMap::new();
        self.flatten_recursive(&root, String::new(), &mut result);
        result
    }

    fn flatten_recursive(&self, node: &SettingsNode, prefix: String, result: &mut HashMap<String, Value>) {
        if let Some(ref value) = node.value {
            if !prefix.is_empty() {
                result.insert(prefix.clone(), value.clone());
            }
        }

        for (key, child) in &node.children {
            let new_prefix = if prefix.is_empty() {
                key.clone()
            } else {
                format!("{}.{}", prefix, key)
            };
            self.flatten_recursive(child, new_prefix, result);
        }
    }

    /// Import settings from another store
    pub async fn import(&self, other: &SettingsStore) -> Result<()> {
        let other_flat = other.flatten().await;
        for (path, value) in other_flat {
            self.set(&path, value).await?;
        }
        Ok(())
    }

    /// Export settings to JSON
    pub async fn export_json(&self) -> Result<String> {
        let root = self.root.read().await;
        Ok(serde_json::to_string_pretty(&*root)?)
    }

    /// Import settings from JSON
    pub async fn import_json(&self, json: &str) -> Result<()> {
        let node: SettingsNode = serde_json::from_str(json)?;
        *self.root.write().await = node;
        *self.dirty.write().await = true;
        Ok(())
    }

    /// Check if there are unsaved changes
    pub async fn is_dirty(&self) -> bool {
        *self.dirty.read().await
    }

    /// Get typed value
    pub async fn get_typed<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Option<T> {
        self.get(path).await
            .and_then(|v| serde_json::from_value(v).ok())
    }

    /// Set typed value
    pub async fn set_typed<T: Serialize>(&self, path: &str, value: T) -> Result<()> {
        let json_value = serde_json::to_value(value)?;
        self.set(path, json_value).await
    }
}

/// Multi-layer settings with overrides
pub struct LayeredStore {
    layers: Vec<(String, Arc<SettingsStore>)>,
}

impl LayeredStore {
    pub fn new() -> Self {
        Self { layers: Vec::new() }
    }

    /// Add a layer (later layers override earlier ones)
    pub fn add_layer(&mut self, name: &str, store: Arc<SettingsStore>) {
        self.layers.push((name.to_string(), store));
    }

    /// Get value from highest-priority layer that has it
    pub async fn get(&self, path: &str) -> Option<Value> {
        for (_, store) in self.layers.iter().rev() {
            if let Some(value) = store.get(path).await {
                return Some(value);
            }
        }
        None
    }

    /// Set value in the top layer
    pub async fn set(&self, path: &str, value: Value) -> Result<()> {
        if let Some((_, store)) = self.layers.last() {
            store.set(path, value).await
        } else {
            Err(anyhow!("No layers configured"))
        }
    }

    /// Get the layer name that provides a value
    pub async fn get_source(&self, path: &str) -> Option<String> {
        for (name, store) in self.layers.iter().rev() {
            if store.exists(path).await {
                return Some(name.clone());
            }
        }
        None
    }
}

impl Default for LayeredStore {
    fn default() -> Self {
        Self::new()
    }
}
