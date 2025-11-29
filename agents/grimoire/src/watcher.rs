//! File watching for live config reload

use anyhow::Result;
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock};

/// File watcher for configuration changes
pub struct ConfigWatcher {
    watcher: Option<RecommendedWatcher>,
    watched_paths: Arc<RwLock<HashMap<PathBuf, WatchEntry>>>,
    debounce_ms: u64,
    event_tx: mpsc::Sender<ConfigEvent>,
}

struct WatchEntry {
    callback_id: u64,
    last_modified: Instant,
}

#[derive(Debug, Clone)]
pub enum ConfigEvent {
    Modified(PathBuf),
    Created(PathBuf),
    Deleted(PathBuf),
    Error(String),
}

impl ConfigWatcher {
    pub fn new(debounce_ms: u64) -> (Self, mpsc::Receiver<ConfigEvent>) {
        let (event_tx, event_rx) = mpsc::channel(100);

        let watcher = Self {
            watcher: None,
            watched_paths: Arc::new(RwLock::new(HashMap::new())),
            debounce_ms,
            event_tx,
        };

        (watcher, event_rx)
    }

    /// Start watching
    pub fn start(&mut self) -> Result<()> {
        let watched_paths = Arc::clone(&self.watched_paths);
        let event_tx = self.event_tx.clone();
        let debounce = Duration::from_millis(self.debounce_ms);

        let watcher = RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| {
                let paths = Arc::clone(&watched_paths);
                let tx = event_tx.clone();

                tokio::spawn(async move {
                    match res {
                        Ok(event) => {
                            for path in event.paths {
                                let mut watched = paths.write().await;

                                // Debounce check
                                if let Some(entry) = watched.get_mut(&path) {
                                    if entry.last_modified.elapsed() < debounce {
                                        continue;
                                    }
                                    entry.last_modified = Instant::now();
                                }

                                let config_event = match event.kind {
                                    notify::EventKind::Modify(_) => ConfigEvent::Modified(path),
                                    notify::EventKind::Create(_) => ConfigEvent::Created(path),
                                    notify::EventKind::Remove(_) => ConfigEvent::Deleted(path),
                                    _ => continue,
                                };

                                let _ = tx.send(config_event).await;
                            }
                        }
                        Err(e) => {
                            let _ = tx.send(ConfigEvent::Error(e.to_string())).await;
                        }
                    }
                });
            },
            Config::default(),
        )?;

        self.watcher = Some(watcher);
        Ok(())
    }

    /// Watch a file or directory
    pub async fn watch(&mut self, path: impl AsRef<Path>) -> Result<u64> {
        let path = path.as_ref().to_path_buf();
        let callback_id = rand::random::<u64>();

        if let Some(ref mut watcher) = self.watcher {
            watcher.watch(&path, RecursiveMode::NonRecursive)?;

            self.watched_paths.write().await.insert(
                path.clone(),
                WatchEntry {
                    callback_id,
                    last_modified: Instant::now(),
                },
            );

            tracing::debug!("Watching: {:?}", path);
        }

        Ok(callback_id)
    }

    /// Stop watching a path
    pub async fn unwatch(&mut self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();

        if let Some(ref mut watcher) = self.watcher {
            watcher.unwatch(path)?;
            self.watched_paths.write().await.remove(path);
        }

        Ok(())
    }

    /// Check if a path is being watched
    pub async fn is_watched(&self, path: impl AsRef<Path>) -> bool {
        self.watched_paths.read().await.contains_key(path.as_ref())
    }

    /// Get all watched paths
    pub async fn watched_paths(&self) -> Vec<PathBuf> {
        self.watched_paths.read().await.keys().cloned().collect()
    }
}

/// Config reload handler
pub struct ReloadHandler {
    callbacks: HashMap<u64, Box<dyn Fn(&PathBuf) + Send + Sync>>,
    path_callbacks: HashMap<PathBuf, Vec<u64>>,
}

impl ReloadHandler {
    pub fn new() -> Self {
        Self {
            callbacks: HashMap::new(),
            path_callbacks: HashMap::new(),
        }
    }

    /// Register a reload callback
    pub fn on_reload<F>(&mut self, path: PathBuf, callback: F) -> u64
    where
        F: Fn(&PathBuf) + Send + Sync + 'static,
    {
        let id = rand::random::<u64>();
        self.callbacks.insert(id, Box::new(callback));
        self.path_callbacks
            .entry(path)
            .or_default()
            .push(id);
        id
    }

    /// Unregister a callback
    pub fn remove_callback(&mut self, id: u64) {
        self.callbacks.remove(&id);
        for callbacks in self.path_callbacks.values_mut() {
            callbacks.retain(|&cb_id| cb_id != id);
        }
    }

    /// Handle a config event
    pub fn handle_event(&self, event: &ConfigEvent) {
        let path = match event {
            ConfigEvent::Modified(p) | ConfigEvent::Created(p) => p,
            _ => return,
        };

        if let Some(callback_ids) = self.path_callbacks.get(path) {
            for id in callback_ids {
                if let Some(callback) = self.callbacks.get(id) {
                    callback(path);
                }
            }
        }
    }
}

impl Default for ReloadHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Auto-reload wrapper for settings
pub struct AutoReloadSettings<T> {
    inner: Arc<RwLock<T>>,
    path: PathBuf,
    loader: Box<dyn Fn(&Path) -> Result<T> + Send + Sync>,
}

impl<T: Send + Sync + 'static> AutoReloadSettings<T> {
    pub fn new<F>(path: PathBuf, loader: F) -> Result<Self>
    where
        F: Fn(&Path) -> Result<T> + Send + Sync + 'static,
    {
        let initial = loader(&path)?;

        Ok(Self {
            inner: Arc::new(RwLock::new(initial)),
            path,
            loader: Box::new(loader),
        })
    }

    pub async fn get(&self) -> tokio::sync::RwLockReadGuard<'_, T> {
        self.inner.read().await
    }

    pub async fn reload(&self) -> Result<()> {
        let new_value = (self.loader)(&self.path)?;
        *self.inner.write().await = new_value;
        tracing::info!("Reloaded settings from {:?}", self.path);
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Directory watcher for multiple config files
pub struct DirectoryWatcher {
    watcher: Option<RecommendedWatcher>,
    directory: PathBuf,
    pattern: Option<String>,
    event_tx: mpsc::Sender<(PathBuf, ConfigEvent)>,
}

impl DirectoryWatcher {
    pub fn new(
        directory: PathBuf,
        pattern: Option<String>,
    ) -> (Self, mpsc::Receiver<(PathBuf, ConfigEvent)>) {
        let (event_tx, event_rx) = mpsc::channel(100);

        let watcher = Self {
            watcher: None,
            directory,
            pattern,
            event_tx,
        };

        (watcher, event_rx)
    }

    pub fn start(&mut self) -> Result<()> {
        let dir = self.directory.clone();
        let pattern = self.pattern.clone();
        let event_tx = self.event_tx.clone();

        let watcher = RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| {
                let tx = event_tx.clone();
                let pat = pattern.clone();

                tokio::spawn(async move {
                    if let Ok(event) = res {
                        for path in event.paths {
                            // Filter by pattern if specified
                            if let Some(ref pattern) = pat {
                                let file_name = path.file_name()
                                    .and_then(|n| n.to_str())
                                    .unwrap_or("");

                                if let Ok(regex) = regex::Regex::new(pattern) {
                                    if !regex.is_match(file_name) {
                                        continue;
                                    }
                                }
                            }

                            let config_event = match event.kind {
                                notify::EventKind::Modify(_) => ConfigEvent::Modified(path.clone()),
                                notify::EventKind::Create(_) => ConfigEvent::Created(path.clone()),
                                notify::EventKind::Remove(_) => ConfigEvent::Deleted(path.clone()),
                                _ => continue,
                            };

                            let _ = tx.send((path, config_event)).await;
                        }
                    }
                });
            },
            Config::default(),
        )?;

        if let Some(ref mut w) = self.watcher {
            *w = watcher;
        } else {
            self.watcher = Some(watcher);
        }

        if let Some(ref mut watcher) = self.watcher {
            watcher.watch(&self.directory, RecursiveMode::NonRecursive)?;
        }

        Ok(())
    }
}
