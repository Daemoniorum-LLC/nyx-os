//! Configuration for Slumber power daemon

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Main configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlumberConfig {
    /// Power profile settings
    #[serde(default)]
    pub profiles: ProfilesConfig,

    /// Battery settings
    #[serde(default)]
    pub battery: BatteryConfig,

    /// Suspend/hibernate settings
    #[serde(default)]
    pub sleep: SleepConfig,

    /// Idle timeout settings
    #[serde(default)]
    pub idle: IdleConfig,

    /// Daemon settings
    #[serde(default)]
    pub daemon: DaemonConfig,
}

impl Default for SlumberConfig {
    fn default() -> Self {
        Self {
            profiles: ProfilesConfig::default(),
            battery: BatteryConfig::default(),
            sleep: SleepConfig::default(),
            idle: IdleConfig::default(),
            daemon: DaemonConfig::default(),
        }
    }
}

/// Power profile configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfilesConfig {
    /// Default power profile
    #[serde(default = "default_profile")]
    pub default_profile: String,

    /// Available profiles
    #[serde(default = "default_profiles")]
    pub profiles: Vec<PowerProfile>,
}

impl Default for ProfilesConfig {
    fn default() -> Self {
        Self {
            default_profile: default_profile(),
            profiles: default_profiles(),
        }
    }
}

/// Power profile definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerProfile {
    /// Profile name
    pub name: String,

    /// CPU governor (e.g., "performance", "powersave", "schedutil")
    #[serde(default = "default_governor")]
    pub cpu_governor: String,

    /// CPU frequency scaling (percentage 0-100)
    #[serde(default = "default_cpu_max")]
    pub cpu_max_freq_percent: u8,

    /// Screen brightness (percentage 0-100)
    #[serde(default = "default_brightness")]
    pub screen_brightness: u8,

    /// Enable turbo boost
    #[serde(default = "default_true")]
    pub turbo_boost: bool,

    /// Disk APM level (1-255, lower = more aggressive power saving)
    #[serde(default = "default_disk_apm")]
    pub disk_apm: u8,

    /// PCI power management
    #[serde(default = "default_true")]
    pub pci_pm: bool,

    /// USB autosuspend timeout (seconds, 0 = disabled)
    #[serde(default = "default_usb_timeout")]
    pub usb_autosuspend: u32,
}

/// Battery configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatteryConfig {
    /// Enable battery monitoring
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Poll interval in seconds
    #[serde(default = "default_battery_interval")]
    pub poll_interval_secs: u32,

    /// Low battery threshold (percentage)
    #[serde(default = "default_low_threshold")]
    pub low_threshold: u8,

    /// Critical battery threshold (percentage)
    #[serde(default = "default_critical_threshold")]
    pub critical_threshold: u8,

    /// Action on low battery
    #[serde(default = "default_low_action")]
    pub low_action: BatteryAction,

    /// Action on critical battery
    #[serde(default = "default_critical_action")]
    pub critical_action: BatteryAction,

    /// Auto-switch to power-saver on battery
    #[serde(default = "default_true")]
    pub auto_powersave: bool,
}

impl Default for BatteryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            poll_interval_secs: default_battery_interval(),
            low_threshold: default_low_threshold(),
            critical_threshold: default_critical_threshold(),
            low_action: default_low_action(),
            critical_action: default_critical_action(),
            auto_powersave: true,
        }
    }
}

/// Battery action
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BatteryAction {
    /// Do nothing
    None,
    /// Send notification
    Notify,
    /// Suspend to RAM
    Suspend,
    /// Hibernate to disk
    Hibernate,
    /// Hybrid sleep
    HybridSleep,
    /// Power off
    Poweroff,
}

/// Sleep configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SleepConfig {
    /// Enable suspend
    #[serde(default = "default_true")]
    pub suspend_enabled: bool,

    /// Enable hibernate
    #[serde(default = "default_true")]
    pub hibernate_enabled: bool,

    /// Enable hybrid sleep
    #[serde(default)]
    pub hybrid_sleep_enabled: bool,

    /// Hibernate after suspend timeout (seconds, 0 = disabled)
    #[serde(default)]
    pub hibernate_delay_secs: u32,

    /// Lock screen before sleep
    #[serde(default = "default_true")]
    pub lock_before_sleep: bool,

    /// Suspend method
    #[serde(default)]
    pub suspend_method: SuspendMethod,
}

impl Default for SleepConfig {
    fn default() -> Self {
        Self {
            suspend_enabled: true,
            hibernate_enabled: true,
            hybrid_sleep_enabled: false,
            hibernate_delay_secs: 0,
            lock_before_sleep: true,
            suspend_method: SuspendMethod::default(),
        }
    }
}

/// Suspend method
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuspendMethod {
    /// Platform suspend (ACPI S3)
    #[default]
    Platform,
    /// Freeze (suspend-to-idle)
    Freeze,
    /// Standby (power-on suspend)
    Standby,
}

/// Idle timeout configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdleConfig {
    /// Enable idle detection
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Dim screen after (seconds, 0 = disabled)
    #[serde(default = "default_dim_timeout")]
    pub dim_timeout_secs: u32,

    /// Turn off screen after (seconds, 0 = disabled)
    #[serde(default = "default_screen_off_timeout")]
    pub screen_off_timeout_secs: u32,

    /// Suspend after (seconds, 0 = disabled)
    #[serde(default)]
    pub suspend_timeout_secs: u32,

    /// Different timeouts on battery
    #[serde(default = "default_true")]
    pub battery_aware: bool,

    /// Battery timeout multiplier (0.5 = half the timeout on battery)
    #[serde(default = "default_battery_multiplier")]
    pub battery_multiplier: f32,
}

impl Default for IdleConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            dim_timeout_secs: default_dim_timeout(),
            screen_off_timeout_secs: default_screen_off_timeout(),
            suspend_timeout_secs: 0,
            battery_aware: true,
            battery_multiplier: default_battery_multiplier(),
        }
    }
}

/// Daemon configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    /// Socket path
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

fn default_profile() -> String {
    "balanced".to_string()
}

fn default_profiles() -> Vec<PowerProfile> {
    vec![
        PowerProfile {
            name: "performance".to_string(),
            cpu_governor: "performance".to_string(),
            cpu_max_freq_percent: 100,
            screen_brightness: 100,
            turbo_boost: true,
            disk_apm: 254,
            pci_pm: false,
            usb_autosuspend: 0,
        },
        PowerProfile {
            name: "balanced".to_string(),
            cpu_governor: "schedutil".to_string(),
            cpu_max_freq_percent: 100,
            screen_brightness: 80,
            turbo_boost: true,
            disk_apm: 128,
            pci_pm: true,
            usb_autosuspend: 2,
        },
        PowerProfile {
            name: "powersave".to_string(),
            cpu_governor: "powersave".to_string(),
            cpu_max_freq_percent: 60,
            screen_brightness: 50,
            turbo_boost: false,
            disk_apm: 64,
            pci_pm: true,
            usb_autosuspend: 1,
        },
    ]
}

fn default_governor() -> String {
    "schedutil".to_string()
}

fn default_cpu_max() -> u8 {
    100
}

fn default_brightness() -> u8 {
    80
}

fn default_disk_apm() -> u8 {
    128
}

fn default_usb_timeout() -> u32 {
    2
}

fn default_battery_interval() -> u32 {
    30
}

fn default_low_threshold() -> u8 {
    20
}

fn default_critical_threshold() -> u8 {
    5
}

fn default_low_action() -> BatteryAction {
    BatteryAction::Notify
}

fn default_critical_action() -> BatteryAction {
    BatteryAction::Hibernate
}

fn default_dim_timeout() -> u32 {
    120
}

fn default_screen_off_timeout() -> u32 {
    300
}

fn default_battery_multiplier() -> f32 {
    0.5
}

fn default_socket_path() -> String {
    "/run/slumber/slumber.sock".to_string()
}

fn default_state_path() -> String {
    "/var/lib/slumber/state.json".to_string()
}

fn default_log_level() -> String {
    "info".to_string()
}

impl SlumberConfig {
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
