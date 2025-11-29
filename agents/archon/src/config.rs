//! Archon configuration

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::info;

/// Archon configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchonConfig {
    /// Process management configuration
    #[serde(default)]
    pub process: ProcessConfig,

    /// Resource management configuration
    #[serde(default)]
    pub resources: ResourceConfig,

    /// Cgroup configuration
    #[serde(default)]
    pub cgroups: CgroupConfig,

    /// Statistics collection configuration
    #[serde(default)]
    pub stats: StatsConfig,
}

impl Default for ArchonConfig {
    fn default() -> Self {
        Self {
            process: ProcessConfig::default(),
            resources: ResourceConfig::default(),
            cgroups: CgroupConfig::default(),
            stats: StatsConfig::default(),
        }
    }
}

/// Process management configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessConfig {
    /// Maximum concurrent processes
    #[serde(default = "default_max_processes")]
    pub max_processes: u32,

    /// Default process priority (nice value)
    #[serde(default)]
    pub default_priority: i32,

    /// Process cleanup interval (seconds)
    #[serde(default = "default_cleanup_interval")]
    pub cleanup_interval_secs: u64,

    /// Zombie reap interval (seconds)
    #[serde(default = "default_reap_interval")]
    pub reap_interval_secs: u64,

    /// Default environment variables
    #[serde(default)]
    pub default_env: Vec<EnvVar>,

    /// Process templates
    #[serde(default)]
    pub templates: Vec<ProcessTemplate>,
}

impl Default for ProcessConfig {
    fn default() -> Self {
        Self {
            max_processes: default_max_processes(),
            default_priority: 0,
            cleanup_interval_secs: default_cleanup_interval(),
            reap_interval_secs: default_reap_interval(),
            default_env: vec![
                EnvVar { key: "PATH".into(), value: "/usr/local/bin:/usr/bin:/bin".into() },
                EnvVar { key: "LANG".into(), value: "en_US.UTF-8".into() },
            ],
            templates: Vec::new(),
        }
    }
}

fn default_max_processes() -> u32 {
    4096
}

fn default_cleanup_interval() -> u64 {
    60
}

fn default_reap_interval() -> u64 {
    1
}

/// Environment variable
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvVar {
    pub key: String,
    pub value: String,
}

/// Process template for common process types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessTemplate {
    /// Template name
    pub name: String,
    /// Description
    #[serde(default)]
    pub description: String,
    /// Resource profile to apply
    pub resource_profile: String,
    /// Default capabilities needed
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// Sandbox profile (if any)
    pub sandbox: Option<String>,
    /// Environment variables
    #[serde(default)]
    pub env: Vec<EnvVar>,
}

/// Resource management configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceConfig {
    /// Enable resource limiting
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Default CPU quota (percentage, 0 = unlimited)
    #[serde(default = "default_cpu_quota")]
    pub default_cpu_percent: u32,

    /// Default memory limit (bytes, 0 = unlimited)
    #[serde(default = "default_memory_limit")]
    pub default_memory_bytes: u64,

    /// Default IO bandwidth (bytes/sec, 0 = unlimited)
    #[serde(default)]
    pub default_io_bandwidth: u64,

    /// Resource profiles
    #[serde(default = "default_resource_profiles")]
    pub profiles: Vec<ResourceProfile>,

    /// OOM killer configuration
    #[serde(default)]
    pub oom: OomConfig,
}

impl Default for ResourceConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            default_cpu_percent: default_cpu_quota(),
            default_memory_bytes: default_memory_limit(),
            default_io_bandwidth: 0,
            profiles: default_resource_profiles(),
            oom: OomConfig::default(),
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_cpu_quota() -> u32 {
    100 // 100% = no limit
}

fn default_memory_limit() -> u64 {
    0 // No limit
}

/// Resource profile
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceProfile {
    /// Profile name
    pub name: String,
    /// Description
    #[serde(default)]
    pub description: String,
    /// CPU quota (percentage)
    #[serde(default = "default_cpu_quota")]
    pub cpu_percent: u32,
    /// Memory limit (bytes)
    #[serde(default)]
    pub memory_bytes: u64,
    /// IO bandwidth limit (bytes/sec)
    #[serde(default)]
    pub io_bandwidth: u64,
    /// Maximum processes
    #[serde(default)]
    pub max_processes: u32,
    /// Maximum open files
    #[serde(default = "default_max_files")]
    pub max_files: u32,
    /// CPU shares (relative weight)
    #[serde(default = "default_cpu_shares")]
    pub cpu_shares: u32,
    /// OOM score adjustment (-1000 to 1000)
    #[serde(default)]
    pub oom_score_adj: i32,
}

fn default_max_files() -> u32 {
    1024
}

fn default_cpu_shares() -> u32 {
    1024 // Default weight
}

fn default_resource_profiles() -> Vec<ResourceProfile> {
    vec![
        ResourceProfile {
            name: "minimal".into(),
            description: "Minimal resources for background tasks".into(),
            cpu_percent: 10,
            memory_bytes: 128 * 1024 * 1024, // 128 MB
            io_bandwidth: 1024 * 1024,       // 1 MB/s
            max_processes: 4,
            max_files: 256,
            cpu_shares: 256,
            oom_score_adj: 500,
        },
        ResourceProfile {
            name: "standard".into(),
            description: "Standard resources for typical applications".into(),
            cpu_percent: 50,
            memory_bytes: 1024 * 1024 * 1024, // 1 GB
            io_bandwidth: 0,                   // No limit
            max_processes: 64,
            max_files: 1024,
            cpu_shares: 1024,
            oom_score_adj: 0,
        },
        ResourceProfile {
            name: "performance".into(),
            description: "High resources for demanding applications".into(),
            cpu_percent: 100,
            memory_bytes: 4 * 1024 * 1024 * 1024, // 4 GB
            io_bandwidth: 0,
            max_processes: 256,
            max_files: 4096,
            cpu_shares: 2048,
            oom_score_adj: -100,
        },
        ResourceProfile {
            name: "realtime".into(),
            description: "Priority resources for latency-sensitive tasks".into(),
            cpu_percent: 100,
            memory_bytes: 2 * 1024 * 1024 * 1024, // 2 GB
            io_bandwidth: 0,
            max_processes: 32,
            max_files: 2048,
            cpu_shares: 4096,
            oom_score_adj: -500,
        },
        ResourceProfile {
            name: "ai_inference".into(),
            description: "Resources for AI/ML inference workloads".into(),
            cpu_percent: 100,
            memory_bytes: 8 * 1024 * 1024 * 1024, // 8 GB
            io_bandwidth: 0,
            max_processes: 16,
            max_files: 2048,
            cpu_shares: 2048,
            oom_score_adj: -200,
        },
    ]
}

/// OOM killer configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OomConfig {
    /// Enable OOM handling
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Log OOM events
    #[serde(default = "default_true")]
    pub log_events: bool,

    /// Memory pressure threshold (percentage)
    #[serde(default = "default_pressure_threshold")]
    pub pressure_threshold: u32,

    /// Actions to take under memory pressure
    #[serde(default)]
    pub pressure_actions: Vec<PressureAction>,
}

fn default_pressure_threshold() -> u32 {
    80
}

/// Memory pressure action
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PressureAction {
    /// Log a warning
    Warn,
    /// Notify affected processes
    Notify,
    /// Kill lowest priority processes
    KillLowPriority,
    /// Trigger garbage collection
    TriggerGc,
}

/// Cgroup configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CgroupConfig {
    /// Enable cgroup management
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Cgroup version (1 or 2)
    #[serde(default = "default_cgroup_version")]
    pub version: u8,

    /// Mount point
    #[serde(default = "default_cgroup_mount")]
    pub mount_point: String,

    /// Root cgroup name
    #[serde(default = "default_cgroup_root")]
    pub root_name: String,

    /// Controllers to enable
    #[serde(default = "default_controllers")]
    pub controllers: Vec<String>,
}

impl Default for CgroupConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            version: default_cgroup_version(),
            mount_point: default_cgroup_mount(),
            root_name: default_cgroup_root(),
            controllers: default_controllers(),
        }
    }
}

fn default_cgroup_version() -> u8 {
    2
}

fn default_cgroup_mount() -> String {
    "/sys/fs/cgroup".into()
}

fn default_cgroup_root() -> String {
    "nyx".into()
}

fn default_controllers() -> Vec<String> {
    vec![
        "cpu".into(),
        "memory".into(),
        "io".into(),
        "pids".into(),
    ]
}

/// Statistics collection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatsConfig {
    /// Enable statistics collection
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Collection interval (seconds)
    #[serde(default = "default_stats_interval")]
    pub interval_secs: u64,

    /// History retention (entries)
    #[serde(default = "default_history_size")]
    pub history_size: usize,

    /// Export metrics to file
    #[serde(default)]
    pub export_path: Option<String>,

    /// Metrics to collect
    #[serde(default = "default_metrics")]
    pub metrics: Vec<MetricType>,
}

impl Default for StatsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_secs: default_stats_interval(),
            history_size: default_history_size(),
            export_path: None,
            metrics: default_metrics(),
        }
    }
}

fn default_stats_interval() -> u64 {
    5
}

fn default_history_size() -> usize {
    1000
}

fn default_metrics() -> Vec<MetricType> {
    vec![
        MetricType::CpuUsage,
        MetricType::MemoryUsage,
        MetricType::IoUsage,
        MetricType::ProcessCount,
    ]
}

/// Metric types
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MetricType {
    CpuUsage,
    MemoryUsage,
    IoUsage,
    ProcessCount,
    ThreadCount,
    OpenFiles,
    NetworkUsage,
    GpuUsage,
}

/// Load configuration from file
pub async fn load_config(path: &Path) -> Result<ArchonConfig> {
    if path.exists() {
        let contents = tokio::fs::read_to_string(path).await?;
        let config: ArchonConfig = serde_yaml::from_str(&contents)?;
        info!("Loaded configuration from {}", path.display());
        Ok(config)
    } else {
        info!("No configuration file found, using defaults");
        Ok(ArchonConfig::default())
    }
}
