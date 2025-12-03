//! Configuration for Nyx Shell

use nyx_theme::{AccentColor, ThemeMode};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Shell configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellConfig {
    /// Theme configuration
    pub theme: ThemeConfig,
    /// Panel configuration
    pub panel: PanelConfig,
    /// Dock configuration
    pub dock: DockConfig,
    /// Workspace configuration
    pub workspaces: WorkspaceConfig,
}

impl Default for ShellConfig {
    fn default() -> Self {
        Self {
            theme: ThemeConfig::default(),
            panel: PanelConfig::default(),
            dock: DockConfig::default(),
            workspaces: WorkspaceConfig::default(),
        }
    }
}

impl ShellConfig {
    /// Load configuration from file or create default
    pub fn load() -> Self {
        let config_path = Self::config_path();

        if config_path.exists() {
            match std::fs::read_to_string(&config_path) {
                Ok(content) => match toml::from_str(&content) {
                    Ok(config) => return config,
                    Err(e) => {
                        tracing::warn!("Failed to parse config: {}", e);
                    }
                },
                Err(e) => {
                    tracing::warn!("Failed to read config: {}", e);
                }
            }
        }

        Self::default()
    }

    /// Save configuration to file
    pub fn save(&self) -> anyhow::Result<()> {
        let config_path = Self::config_path();

        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(self)?;
        std::fs::write(config_path, content)?;

        Ok(())
    }

    /// Get the configuration file path
    fn config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("nyx")
            .join("shell.toml")
    }
}

/// Theme configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeConfig {
    /// Theme mode
    pub mode: ThemeMode,
    /// Accent color
    pub accent: AccentColor,
    /// Custom accent hex
    pub custom_accent: Option<String>,
    /// Enable blur effects
    pub blur: bool,
    /// Enable animations
    pub animations: bool,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            mode: ThemeMode::Dark,
            accent: AccentColor::Aurora,
            custom_accent: None,
            blur: true,
            animations: true,
        }
    }
}

/// Panel configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PanelConfig {
    /// Panel height
    pub height: u32,
    /// Show activities button
    pub show_activities: bool,
    /// Show workspace switcher
    pub show_workspaces: bool,
    /// Show clock
    pub show_clock: bool,
    /// Clock format (12h or 24h)
    pub clock_24h: bool,
    /// Show date in clock
    pub show_date: bool,
    /// Show system tray
    pub show_tray: bool,
}

impl Default for PanelConfig {
    fn default() -> Self {
        Self {
            height: 32,
            show_activities: true,
            show_workspaces: true,
            show_clock: true,
            clock_24h: false,
            show_date: true,
            show_tray: true,
        }
    }
}

/// Dock configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DockConfig {
    /// Dock position
    pub position: DockPosition,
    /// Dock icon size
    pub icon_size: u32,
    /// Show running indicators
    pub show_running: bool,
    /// Auto-hide dock
    pub auto_hide: bool,
    /// Magnification on hover
    pub magnification: bool,
    /// Pinned applications
    pub pinned_apps: Vec<String>,
}

impl Default for DockConfig {
    fn default() -> Self {
        Self {
            position: DockPosition::Bottom,
            icon_size: 48,
            show_running: true,
            auto_hide: false,
            magnification: true,
            pinned_apps: vec![
                "nyx-assistant".to_string(),
                "umbra".to_string(),
                "nyx-settings".to_string(),
            ],
        }
    }
}

/// Dock position
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DockPosition {
    /// Bottom of screen (default)
    #[default]
    Bottom,
    /// Left side of screen
    Left,
    /// Right side of screen
    Right,
}

/// Workspace configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    /// Number of workspaces
    pub count: u32,
    /// Dynamic workspaces (auto-create/remove)
    pub dynamic: bool,
    /// Workspace names
    pub names: Vec<String>,
    /// Wrap around when switching
    pub wrap_around: bool,
}

impl Default for WorkspaceConfig {
    fn default() -> Self {
        Self {
            count: 4,
            dynamic: true,
            names: vec![
                "Main".to_string(),
                "Work".to_string(),
                "Development".to_string(),
                "Media".to_string(),
            ],
            wrap_around: true,
        }
    }
}
