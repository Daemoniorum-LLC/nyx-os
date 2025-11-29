//! Application index management

use crate::desktop::DesktopEntry;
use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Application index for fast lookup
pub struct AppIndex {
    entries: Arc<RwLock<HashMap<String, IndexedApp>>>,
    by_category: Arc<RwLock<HashMap<String, Vec<String>>>>,
    keywords: Arc<RwLock<HashMap<String, Vec<String>>>>,
}

#[derive(Debug, Clone)]
pub struct IndexedApp {
    pub id: String,
    pub entry: DesktopEntry,
    pub path: PathBuf,
    pub score: f64,
    pub last_used: Option<std::time::SystemTime>,
    pub use_count: u64,
}

impl AppIndex {
    pub fn new() -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
            by_category: Arc::new(RwLock::new(HashMap::new())),
            keywords: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add an application to the index
    pub async fn add(&self, entry: DesktopEntry, path: PathBuf) {
        let id = entry.id.clone();

        // Index by category
        {
            let mut by_cat = self.by_category.write().await;
            for category in &entry.categories {
                by_cat
                    .entry(category.clone())
                    .or_default()
                    .push(id.clone());
            }
        }

        // Index keywords
        {
            let mut kw_index = self.keywords.write().await;

            // Add name words
            for word in entry.name.split_whitespace() {
                let word_lower = word.to_lowercase();
                kw_index
                    .entry(word_lower)
                    .or_default()
                    .push(id.clone());
            }

            // Add explicit keywords
            for kw in &entry.keywords {
                let kw_lower = kw.to_lowercase();
                kw_index
                    .entry(kw_lower)
                    .or_default()
                    .push(id.clone());
            }

            // Add generic name words
            if let Some(ref generic) = entry.generic_name {
                for word in generic.split_whitespace() {
                    let word_lower = word.to_lowercase();
                    kw_index
                        .entry(word_lower)
                        .or_default()
                        .push(id.clone());
                }
            }
        }

        // Add main entry
        let indexed = IndexedApp {
            id: id.clone(),
            entry,
            path,
            score: 1.0,
            last_used: None,
            use_count: 0,
        };

        self.entries.write().await.insert(id, indexed);
    }

    /// Remove an application from the index
    pub async fn remove(&self, id: &str) {
        // Remove from main entries
        let entry = self.entries.write().await.remove(id);

        if let Some(app) = entry {
            // Remove from categories
            let mut by_cat = self.by_category.write().await;
            for category in &app.entry.categories {
                if let Some(ids) = by_cat.get_mut(category) {
                    ids.retain(|i| i != id);
                }
            }

            // Remove from keywords
            let mut kw_index = self.keywords.write().await;
            for (_, ids) in kw_index.iter_mut() {
                ids.retain(|i| i != id);
            }
        }
    }

    /// Get an application by ID
    pub async fn get(&self, id: &str) -> Option<IndexedApp> {
        self.entries.read().await.get(id).cloned()
    }

    /// Get all applications
    pub async fn all(&self) -> Vec<IndexedApp> {
        self.entries.read().await.values().cloned().collect()
    }

    /// Get applications by category
    pub async fn by_category(&self, category: &str) -> Vec<IndexedApp> {
        let by_cat = self.by_category.read().await;
        let entries = self.entries.read().await;

        by_cat
            .get(category)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| entries.get(id).cloned())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get all categories
    pub async fn categories(&self) -> Vec<String> {
        self.by_category.read().await.keys().cloned().collect()
    }

    /// Search by keyword prefix
    pub async fn search_prefix(&self, prefix: &str) -> Vec<IndexedApp> {
        let prefix_lower = prefix.to_lowercase();
        let kw_index = self.keywords.read().await;
        let entries = self.entries.read().await;

        let mut result_ids: Vec<String> = kw_index
            .iter()
            .filter(|(kw, _)| kw.starts_with(&prefix_lower))
            .flat_map(|(_, ids)| ids.clone())
            .collect();

        result_ids.sort();
        result_ids.dedup();

        result_ids
            .iter()
            .filter_map(|id| entries.get(id).cloned())
            .collect()
    }

    /// Update usage statistics
    pub async fn record_use(&self, id: &str) {
        if let Some(app) = self.entries.write().await.get_mut(id) {
            app.use_count += 1;
            app.last_used = Some(std::time::SystemTime::now());
            // Boost score based on usage
            app.score = 1.0 + (app.use_count as f64).log2() * 0.1;
        }
    }

    /// Get entry count
    pub async fn len(&self) -> usize {
        self.entries.read().await.len()
    }

    /// Check if index is empty
    pub async fn is_empty(&self) -> bool {
        self.entries.read().await.is_empty()
    }

    /// Clear the index
    pub async fn clear(&self) {
        self.entries.write().await.clear();
        self.by_category.write().await.clear();
        self.keywords.write().await.clear();
    }

    /// Get most used applications
    pub async fn most_used(&self, limit: usize) -> Vec<IndexedApp> {
        let entries = self.entries.read().await;
        let mut apps: Vec<_> = entries.values().cloned().collect();
        apps.sort_by(|a, b| b.use_count.cmp(&a.use_count));
        apps.truncate(limit);
        apps
    }

    /// Get recently used applications
    pub async fn recently_used(&self, limit: usize) -> Vec<IndexedApp> {
        let entries = self.entries.read().await;
        let mut apps: Vec<_> = entries
            .values()
            .filter(|a| a.last_used.is_some())
            .cloned()
            .collect();

        apps.sort_by(|a, b| b.last_used.cmp(&a.last_used));
        apps.truncate(limit);
        apps
    }

    /// Serialize index for persistence
    pub async fn serialize(&self) -> Result<String> {
        let entries = self.entries.read().await;
        let data: Vec<_> = entries
            .values()
            .map(|app| {
                serde_json::json!({
                    "id": app.id,
                    "path": app.path,
                    "use_count": app.use_count,
                    "last_used": app.last_used
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs()),
                })
            })
            .collect();

        Ok(serde_json::to_string_pretty(&data)?)
    }

    /// Load usage data from persistence
    pub async fn load_usage(&self, data: &str) -> Result<()> {
        let usage: Vec<serde_json::Value> = serde_json::from_str(data)?;

        let mut entries = self.entries.write().await;

        for item in usage {
            if let Some(id) = item.get("id").and_then(|v| v.as_str()) {
                if let Some(app) = entries.get_mut(id) {
                    if let Some(count) = item.get("use_count").and_then(|v| v.as_u64()) {
                        app.use_count = count;
                    }
                    if let Some(secs) = item.get("last_used").and_then(|v| v.as_u64()) {
                        app.last_used = Some(
                            std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs)
                        );
                    }
                    // Recalculate score
                    app.score = 1.0 + (app.use_count as f64).log2() * 0.1;
                }
            }
        }

        Ok(())
    }
}

impl Default for AppIndex {
    fn default() -> Self {
        Self::new()
    }
}

/// Index builder for batch operations
pub struct IndexBuilder {
    entries: Vec<(DesktopEntry, PathBuf)>,
}

impl IndexBuilder {
    pub fn new() -> Self {
        Self { entries: Vec::new() }
    }

    pub fn add(mut self, entry: DesktopEntry, path: PathBuf) -> Self {
        self.entries.push((entry, path));
        self
    }

    pub async fn build(self) -> AppIndex {
        let index = AppIndex::new();

        for (entry, path) in self.entries {
            index.add(entry, path).await;
        }

        index
    }
}

impl Default for IndexBuilder {
    fn default() -> Self {
        Self::new()
    }
}
