//! Herald configuration

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HeraldConfig {
    #[serde(default)]
    pub display: DisplayConfig,
    #[serde(default)]
    pub history: HistoryConfig,
    #[serde(default)]
    pub dnd: DndConfig,
    #[serde(default)]
    pub sounds: SoundConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayConfig {
    #[serde(default = "default_timeout")]
    pub default_timeout_ms: u64,
    #[serde(default = "default_max_visible")]
    pub max_visible: usize,
    #[serde(default)]
    pub position: Position,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            default_timeout_ms: default_timeout(),
            max_visible: default_max_visible(),
            position: Position::TopRight,
        }
    }
}

fn default_timeout() -> u64 { 5000 }
fn default_max_visible() -> usize { 5 }

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Position {
    TopLeft, #[default] TopRight, BottomLeft, BottomRight, TopCenter, BottomCenter,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_history_size")]
    pub max_size: usize,
    #[serde(default = "default_retention")]
    pub retention_days: u32,
}

impl Default for HistoryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_size: default_history_size(),
            retention_days: default_retention(),
        }
    }
}

fn default_history_size() -> usize { 1000 }
fn default_retention() -> u32 { 7 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DndConfig {
    #[serde(default)]
    pub schedule: Vec<DndSchedule>,
    #[serde(default)]
    pub allow_critical: bool,
}

impl Default for DndConfig {
    fn default() -> Self {
        Self { schedule: Vec::new(), allow_critical: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DndSchedule {
    pub days: Vec<String>,
    pub start: String,
    pub end: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoundConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub default_sound: Option<String>,
    #[serde(default)]
    pub critical_sound: Option<String>,
}

impl Default for SoundConfig {
    fn default() -> Self {
        Self { enabled: true, default_sound: None, critical_sound: None }
    }
}

fn default_true() -> bool { true }

pub fn load_config(path: &Path) -> Result<HeraldConfig> {
    if path.exists() {
        let contents = std::fs::read_to_string(path)?;
        Ok(serde_yaml::from_str(&contents)?)
    } else {
        Ok(HeraldConfig::default())
    }
}
