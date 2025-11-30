//! Configuration for Iris display daemon

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Main configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrisConfig {
    /// Display settings
    #[serde(default)]
    pub displays: DisplaysConfig,

    /// Backlight settings
    #[serde(default)]
    pub backlight: BacklightConfig,

    /// Color settings
    #[serde(default)]
    pub color: ColorConfig,

    /// Daemon settings
    #[serde(default)]
    pub daemon: DaemonConfig,
}

impl Default for IrisConfig {
    fn default() -> Self {
        Self {
            displays: DisplaysConfig::default(),
            backlight: BacklightConfig::default(),
            color: ColorConfig::default(),
            daemon: DaemonConfig::default(),
        }
    }
}

/// Display configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplaysConfig {
    /// Auto-detect displays
    #[serde(default = "default_true")]
    pub auto_detect: bool,

    /// Primary display name
    #[serde(default)]
    pub primary: Option<String>,

    /// Display arrangements
    #[serde(default)]
    pub arrangements: Vec<DisplayArrangement>,
}

impl Default for DisplaysConfig {
    fn default() -> Self {
        Self {
            auto_detect: true,
            primary: None,
            arrangements: Vec::new(),
        }
    }
}

/// Display arrangement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayArrangement {
    /// Display name/ID
    pub display: String,
    /// Position (x, y)
    pub position: (i32, i32),
    /// Rotation (0, 90, 180, 270)
    #[serde(default)]
    pub rotation: u16,
    /// Scale factor
    #[serde(default = "default_scale")]
    pub scale: f32,
}

/// Backlight configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacklightConfig {
    /// Enable backlight control
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Backlight device path
    #[serde(default = "default_backlight_path")]
    pub device: String,

    /// Minimum brightness (percentage)
    #[serde(default = "default_min_brightness")]
    pub min_brightness: u8,

    /// Enable smooth transitions
    #[serde(default = "default_true")]
    pub smooth_transitions: bool,

    /// Transition duration in ms
    #[serde(default = "default_transition_ms")]
    pub transition_ms: u32,
}

impl Default for BacklightConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            device: default_backlight_path(),
            min_brightness: default_min_brightness(),
            smooth_transitions: true,
            transition_ms: default_transition_ms(),
        }
    }
}

/// Color configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorConfig {
    /// Enable night light
    #[serde(default)]
    pub night_light: bool,

    /// Night light color temperature (Kelvin)
    #[serde(default = "default_night_temp")]
    pub night_temperature: u32,

    /// Day color temperature (Kelvin)
    #[serde(default = "default_day_temp")]
    pub day_temperature: u32,

    /// Sunrise time (HH:MM)
    #[serde(default = "default_sunrise")]
    pub sunrise: String,

    /// Sunset time (HH:MM)
    #[serde(default = "default_sunset")]
    pub sunset: String,

    /// Transition duration in minutes
    #[serde(default = "default_color_transition")]
    pub transition_minutes: u32,
}

impl Default for ColorConfig {
    fn default() -> Self {
        Self {
            night_light: false,
            night_temperature: default_night_temp(),
            day_temperature: default_day_temp(),
            sunrise: default_sunrise(),
            sunset: default_sunset(),
            transition_minutes: default_color_transition(),
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

fn default_scale() -> f32 {
    1.0
}

fn default_backlight_path() -> String {
    "/sys/class/backlight/intel_backlight".to_string()
}

fn default_min_brightness() -> u8 {
    5
}

fn default_transition_ms() -> u32 {
    200
}

fn default_night_temp() -> u32 {
    3500
}

fn default_day_temp() -> u32 {
    6500
}

fn default_sunrise() -> String {
    "06:30".to_string()
}

fn default_sunset() -> String {
    "19:30".to_string()
}

fn default_color_transition() -> u32 {
    30
}

fn default_socket_path() -> String {
    "/run/iris/iris.sock".to_string()
}

fn default_log_level() -> String {
    "info".to_string()
}

impl IrisConfig {
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
