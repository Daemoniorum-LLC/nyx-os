//! Chronos configuration

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Main configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChronosConfig {
    /// NTP configuration
    #[serde(default)]
    pub ntp: NtpConfig,

    /// Timezone configuration
    #[serde(default)]
    pub timezone: TimezoneConfig,

    /// RTC configuration
    #[serde(default)]
    pub rtc: RtcConfig,

    /// Daemon settings
    #[serde(default)]
    pub daemon: DaemonConfig,
}

impl Default for ChronosConfig {
    fn default() -> Self {
        Self {
            ntp: NtpConfig::default(),
            timezone: TimezoneConfig::default(),
            rtc: RtcConfig::default(),
            daemon: DaemonConfig::default(),
        }
    }
}

/// NTP client configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NtpConfig {
    /// Enable NTP synchronization
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// NTP server pools
    #[serde(default = "default_ntp_servers")]
    pub servers: Vec<String>,

    /// Polling interval in seconds (power of 2, 64-1024)
    #[serde(default = "default_poll_interval")]
    pub poll_interval: u32,

    /// Maximum allowed offset before step correction (seconds)
    #[serde(default = "default_step_threshold")]
    pub step_threshold: f64,

    /// Panic threshold - if offset exceeds this, refuse to set time (seconds)
    #[serde(default = "default_panic_threshold")]
    pub panic_threshold: f64,

    /// Minimum number of servers to query
    #[serde(default = "default_min_servers")]
    pub min_servers: usize,

    /// Enable hardware timestamping if available
    #[serde(default)]
    pub hardware_timestamps: bool,
}

impl Default for NtpConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            servers: default_ntp_servers(),
            poll_interval: default_poll_interval(),
            step_threshold: default_step_threshold(),
            panic_threshold: default_panic_threshold(),
            min_servers: default_min_servers(),
            hardware_timestamps: false,
        }
    }
}

/// Timezone configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimezoneConfig {
    /// Current timezone (IANA format, e.g., "America/New_York")
    #[serde(default = "default_timezone")]
    pub timezone: String,

    /// Path to timezone data
    #[serde(default = "default_tzdata_path")]
    pub tzdata_path: String,

    /// Enable automatic timezone detection
    #[serde(default)]
    pub auto_detect: bool,
}

impl Default for TimezoneConfig {
    fn default() -> Self {
        Self {
            timezone: default_timezone(),
            tzdata_path: default_tzdata_path(),
            auto_detect: false,
        }
    }
}

/// RTC (Real-Time Clock) configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RtcConfig {
    /// Enable RTC synchronization
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// RTC device path
    #[serde(default = "default_rtc_device")]
    pub device: String,

    /// RTC is in UTC (vs local time)
    #[serde(default = "default_true")]
    pub utc: bool,

    /// Sync RTC on shutdown
    #[serde(default = "default_true")]
    pub sync_on_shutdown: bool,

    /// Sync interval in seconds (0 = only on shutdown)
    #[serde(default)]
    pub sync_interval: u64,
}

impl Default for RtcConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            device: default_rtc_device(),
            utc: true,
            sync_on_shutdown: true,
            sync_interval: 0,
        }
    }
}

/// Daemon configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    /// Socket path for IPC
    #[serde(default = "default_socket_path")]
    pub socket_path: String,

    /// State file path
    #[serde(default = "default_state_path")]
    pub state_path: String,

    /// Log level
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            socket_path: default_socket_path(),
            state_path: default_state_path(),
            log_level: default_log_level(),
        }
    }
}

// Default value functions
fn default_true() -> bool {
    true
}

fn default_ntp_servers() -> Vec<String> {
    vec![
        "0.pool.ntp.org".to_string(),
        "1.pool.ntp.org".to_string(),
        "2.pool.ntp.org".to_string(),
        "3.pool.ntp.org".to_string(),
    ]
}

fn default_poll_interval() -> u32 {
    64 // seconds
}

fn default_step_threshold() -> f64 {
    0.128 // 128ms - step if offset > this, otherwise slew
}

fn default_panic_threshold() -> f64 {
    1000.0 // 1000 seconds - refuse to sync if offset > this
}

fn default_min_servers() -> usize {
    1
}

fn default_timezone() -> String {
    "UTC".to_string()
}

fn default_tzdata_path() -> String {
    "/usr/share/zoneinfo".to_string()
}

fn default_rtc_device() -> String {
    "/dev/rtc0".to_string()
}

fn default_socket_path() -> String {
    "/run/chronos/chronos.sock".to_string()
}

fn default_state_path() -> String {
    "/var/lib/chronos/state".to_string()
}

fn default_log_level() -> String {
    "info".to_string()
}

impl ChronosConfig {
    /// Load configuration from file
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        if path.exists() {
            let content = std::fs::read_to_string(path)?;
            let config: Self = toml::from_str(&content)?;
            Ok(config)
        } else {
            Ok(Self::default())
        }
    }

    /// Save configuration to file
    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }
}
