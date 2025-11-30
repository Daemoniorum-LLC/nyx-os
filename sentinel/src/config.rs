//! Configuration for Sentinel monitoring daemon

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Main configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SentinelConfig {
    /// Metrics collection settings
    #[serde(default)]
    pub metrics: MetricsConfig,

    /// Alert thresholds
    #[serde(default)]
    pub alerts: AlertConfig,

    /// Process monitoring
    #[serde(default)]
    pub processes: ProcessConfig,

    /// Daemon settings
    #[serde(default)]
    pub daemon: DaemonConfig,
}

impl Default for SentinelConfig {
    fn default() -> Self {
        Self {
            metrics: MetricsConfig::default(),
            alerts: AlertConfig::default(),
            processes: ProcessConfig::default(),
            daemon: DaemonConfig::default(),
        }
    }
}

/// Metrics collection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    /// Collection interval in seconds
    #[serde(default = "default_interval")]
    pub interval_secs: u32,

    /// Enable CPU metrics
    #[serde(default = "default_true")]
    pub cpu: bool,

    /// Enable memory metrics
    #[serde(default = "default_true")]
    pub memory: bool,

    /// Enable disk metrics
    #[serde(default = "default_true")]
    pub disk: bool,

    /// Enable network metrics
    #[serde(default = "default_true")]
    pub network: bool,

    /// Enable process metrics
    #[serde(default = "default_true")]
    pub processes: bool,

    /// Enable temperature sensors
    #[serde(default = "default_true")]
    pub temperature: bool,

    /// History retention in samples
    #[serde(default = "default_history_size")]
    pub history_size: usize,

    /// Track top N processes by CPU
    #[serde(default = "default_top_count")]
    pub top_cpu_count: usize,

    /// Track top N processes by memory
    #[serde(default = "default_top_count")]
    pub top_memory_count: usize,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            interval_secs: default_interval(),
            cpu: true,
            memory: true,
            disk: true,
            network: true,
            processes: true,
            temperature: true,
            history_size: default_history_size(),
            top_cpu_count: default_top_count(),
            top_memory_count: default_top_count(),
        }
    }
}

/// Alert threshold configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertConfig {
    /// Enable alerting
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// CPU usage threshold (percentage)
    #[serde(default = "default_cpu_threshold")]
    pub cpu_threshold: f32,

    /// Memory usage threshold (percentage)
    #[serde(default = "default_memory_threshold")]
    pub memory_threshold: f32,

    /// Disk usage threshold (percentage)
    #[serde(default = "default_disk_threshold")]
    pub disk_threshold: f32,

    /// Temperature threshold (Celsius)
    #[serde(default = "default_temp_threshold")]
    pub temp_threshold: f32,

    /// Load average threshold (per core)
    #[serde(default = "default_load_threshold")]
    pub load_threshold: f32,

    /// Alert cooldown in seconds
    #[serde(default = "default_cooldown")]
    pub cooldown_secs: u32,
}

impl Default for AlertConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            cpu_threshold: default_cpu_threshold(),
            memory_threshold: default_memory_threshold(),
            disk_threshold: default_disk_threshold(),
            temp_threshold: default_temp_threshold(),
            load_threshold: default_load_threshold(),
            cooldown_secs: default_cooldown(),
        }
    }
}

/// Process monitoring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessConfig {
    /// Track top N processes by CPU
    #[serde(default = "default_top_count")]
    pub top_cpu_count: usize,

    /// Track top N processes by memory
    #[serde(default = "default_top_count")]
    pub top_memory_count: usize,

    /// Watch specific processes by name
    #[serde(default)]
    pub watch_list: Vec<String>,
}

impl Default for ProcessConfig {
    fn default() -> Self {
        Self {
            top_cpu_count: default_top_count(),
            top_memory_count: default_top_count(),
            watch_list: Vec::new(),
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

fn default_interval() -> u32 {
    5
}

fn default_history_size() -> usize {
    720 // 1 hour at 5s intervals
}

fn default_cpu_threshold() -> f32 {
    90.0
}

fn default_memory_threshold() -> f32 {
    90.0
}

fn default_disk_threshold() -> f32 {
    90.0
}

fn default_temp_threshold() -> f32 {
    85.0
}

fn default_load_threshold() -> f32 {
    2.0
}

fn default_cooldown() -> u32 {
    300 // 5 minutes
}

fn default_top_count() -> usize {
    10
}

fn default_socket_path() -> String {
    "/run/sentinel/sentinel.sock".to_string()
}

fn default_log_level() -> String {
    "info".to_string()
}

impl SentinelConfig {
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
