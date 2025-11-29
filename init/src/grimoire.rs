//! Grimoire integration for service configuration
//!
//! Services can be configured via YAML files in the Grimoire directory structure.

use anyhow::Result;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use tokio::sync::broadcast;
use tracing::{debug, info, warn};

/// Grimoire configuration paths
pub struct GrimoirePaths {
    /// Root grimoire directory
    pub root: PathBuf,
    /// System services directory
    pub system_services: PathBuf,
    /// User services directory (per-user)
    pub user_services: PathBuf,
    /// Personas directory
    pub personas: PathBuf,
}

impl GrimoirePaths {
    /// Create from root path
    pub fn from_root(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Self {
            system_services: root.join("system/services"),
            user_services: root.join("user/services"),
            personas: root.join("personas"),
            root,
        }
    }

    /// Default paths
    pub fn default_system() -> Self {
        Self::from_root("/grimoire")
    }

    /// User paths
    pub fn default_user() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
        Self::from_root(format!("{}/.grimoire", home))
    }
}

/// Events from grimoire file watcher
#[derive(Debug, Clone)]
pub enum GrimoireEvent {
    /// Service configuration changed
    ServiceChanged { name: String },
    /// Service added
    ServiceAdded { name: String, path: PathBuf },
    /// Service removed
    ServiceRemoved { name: String },
    /// Persona changed
    PersonaChanged { name: String },
}

/// Watch grimoire directory for changes
pub struct GrimoireWatcher {
    paths: GrimoirePaths,
    events: broadcast::Sender<GrimoireEvent>,
    _watcher: RecommendedWatcher,
}

impl GrimoireWatcher {
    /// Create a new watcher
    pub fn new(paths: GrimoirePaths) -> Result<Self> {
        let (tx, _rx) = broadcast::channel(256);
        let events_tx = tx.clone();

        // Create file watcher
        let (notify_tx, notify_rx) = mpsc::channel();

        let mut watcher = notify::recommended_watcher(move |res| {
            let _ = notify_tx.send(res);
        })?;

        // Watch directories
        if paths.system_services.exists() {
            watcher.watch(&paths.system_services, RecursiveMode::Recursive)?;
        }

        if paths.user_services.exists() {
            watcher.watch(&paths.user_services, RecursiveMode::Recursive)?;
        }

        if paths.personas.exists() {
            watcher.watch(&paths.personas, RecursiveMode::Recursive)?;
        }

        // Spawn handler thread
        let services_dir = paths.system_services.clone();
        let personas_dir = paths.personas.clone();

        std::thread::spawn(move || {
            while let Ok(Ok(event)) = notify_rx.recv() {
                for path in event.paths {
                    if let Some(event) = classify_change(&path, &services_dir, &personas_dir) {
                        let _ = events_tx.send(event);
                    }
                }
            }
        });

        info!("Grimoire watcher started for {}", paths.root.display());

        Ok(Self {
            paths,
            events: tx,
            _watcher: watcher,
        })
    }

    /// Subscribe to grimoire events
    pub fn subscribe(&self) -> broadcast::Receiver<GrimoireEvent> {
        self.events.subscribe()
    }
}

fn classify_change(
    path: &Path,
    services_dir: &Path,
    personas_dir: &Path,
) -> Option<GrimoireEvent> {
    let file_name = path.file_stem()?.to_str()?;

    if path.starts_with(services_dir) {
        if path.exists() {
            Some(GrimoireEvent::ServiceChanged {
                name: file_name.to_string(),
            })
        } else {
            Some(GrimoireEvent::ServiceRemoved {
                name: file_name.to_string(),
            })
        }
    } else if path.starts_with(personas_dir) {
        Some(GrimoireEvent::PersonaChanged {
            name: file_name.to_string(),
        })
    } else {
        None
    }
}

/// Load a persona by name
pub async fn load_persona(paths: &GrimoirePaths, name: &str) -> Result<PersonaConfig> {
    let path = paths.personas.join(format!("{}.yaml", name));

    if !path.exists() {
        anyhow::bail!("Persona not found: {}", name);
    }

    let contents = tokio::fs::read_to_string(&path).await?;
    let config: PersonaConfig = serde_yaml::from_str(&contents)?;

    Ok(config)
}

/// Persona configuration
#[derive(Debug, Clone, serde::Deserialize)]
pub struct PersonaConfig {
    /// Persona code/identifier
    pub code: String,
    /// Display name
    pub name: String,
    /// Description
    #[serde(default)]
    pub description: String,
    /// Default model
    #[serde(default)]
    pub default_model: Option<String>,
    /// Temperature
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    /// Tags
    #[serde(default)]
    pub tags: Vec<String>,
    /// System prompt
    #[serde(default)]
    pub system_prompt: Option<String>,
}

fn default_temperature() -> f32 {
    0.7
}
