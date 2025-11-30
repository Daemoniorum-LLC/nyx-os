//! Service unit definitions and registry

use anyhow::{Result, Context};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{info, warn};

/// Service unit definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Unit {
    /// Unique service name
    pub name: String,
    /// Human-readable description
    #[serde(default)]
    pub description: String,
    /// Documentation URLs
    #[serde(default)]
    pub documentation: Vec<String>,

    /// Service configuration
    #[serde(default)]
    pub service: ServiceConfig,
    /// Install configuration
    #[serde(default)]
    pub install: InstallConfig,
    /// Resource limits
    #[serde(default)]
    pub resources: ResourceConfig,
    /// Socket activation
    #[serde(default)]
    pub socket: Option<SocketConfig>,
}

/// Service execution configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServiceConfig {
    /// Type of service (simple, forking, oneshot, notify)
    #[serde(default = "default_service_type")]
    pub service_type: ServiceType,
    /// Command to execute
    pub exec_start: Option<String>,
    /// Pre-start commands
    #[serde(default)]
    pub exec_start_pre: Vec<String>,
    /// Post-start commands
    #[serde(default)]
    pub exec_start_post: Vec<String>,
    /// Stop command
    pub exec_stop: Option<String>,
    /// Reload command
    pub exec_reload: Option<String>,
    /// Working directory
    pub working_directory: Option<PathBuf>,
    /// User to run as
    pub user: Option<String>,
    /// Group to run as
    pub group: Option<String>,
    /// Environment variables
    #[serde(default)]
    pub environment: HashMap<String, String>,
    /// Environment file paths
    #[serde(default)]
    pub environment_file: Vec<PathBuf>,
    /// Restart policy
    #[serde(default)]
    pub restart: RestartPolicy,
    /// Seconds to wait before restart
    #[serde(default = "default_restart_sec")]
    pub restart_sec: u64,
    /// Maximum restart attempts (0 = unlimited)
    #[serde(default)]
    pub restart_max: u32,
    /// Seconds between restart count resets
    #[serde(default = "default_restart_burst")]
    pub restart_burst_sec: u64,
    /// Timeout for start operation
    #[serde(default = "default_timeout")]
    pub timeout_start_sec: u64,
    /// Timeout for stop operation
    #[serde(default = "default_timeout")]
    pub timeout_stop_sec: u64,
    /// Watchdog interval (0 = disabled)
    #[serde(default)]
    pub watchdog_sec: u64,
    /// Kill signal
    #[serde(default = "default_kill_signal")]
    pub kill_signal: String,
    /// Standard output handling
    #[serde(default)]
    pub standard_output: OutputType,
    /// Standard error handling
    #[serde(default)]
    pub standard_error: OutputType,
    /// PID file path (for forking services)
    pub pid_file: Option<PathBuf>,
}

/// Service type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServiceType {
    /// Simple process, ready immediately
    #[default]
    Simple,
    /// Forks and parent exits
    Forking,
    /// Runs once and exits
    Oneshot,
    /// Notifies when ready via socket
    Notify,
    /// DBus service
    Dbus,
    /// Idle until all jobs dispatched
    Idle,
}

fn default_service_type() -> ServiceType {
    ServiceType::Simple
}

/// Restart policy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RestartPolicy {
    /// Never restart
    #[default]
    No,
    /// Always restart
    Always,
    /// Restart on failure (non-zero exit)
    OnFailure,
    /// Restart on abnormal exit (signal)
    OnAbnormal,
    /// Restart on abort (core dump)
    OnAbort,
    /// Restart on watchdog timeout
    OnWatchdog,
    /// Restart unless stopped cleanly
    UnlessStopped,
}

fn default_restart_sec() -> u64 { 1 }
fn default_restart_burst() -> u64 { 60 }
fn default_timeout() -> u64 { 90 }
fn default_kill_signal() -> String { "SIGTERM".to_string() }

/// Output handling type
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputType {
    /// Inherit from parent
    #[default]
    Inherit,
    /// Write to journal/log
    Journal,
    /// Write to file
    File(PathBuf),
    /// Discard
    Null,
}

/// Install configuration (when/how to start)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InstallConfig {
    /// Target to install as part of
    #[serde(default)]
    pub wanted_by: Vec<String>,
    /// Required by these units
    #[serde(default)]
    pub required_by: Vec<String>,
    /// Start before these units
    #[serde(default)]
    pub before: Vec<String>,
    /// Start after these units
    #[serde(default)]
    pub after: Vec<String>,
    /// Requires these units (fails if they fail)
    #[serde(default)]
    pub requires: Vec<String>,
    /// Wants these units (doesn't fail if they fail)
    #[serde(default)]
    pub wants: Vec<String>,
    /// Conflicts with these units (stops them)
    #[serde(default)]
    pub conflicts: Vec<String>,
    /// Alias names for this unit
    #[serde(default)]
    pub alias: Vec<String>,
    /// Whether enabled by default
    #[serde(default)]
    pub enabled: bool,
}

/// Resource limits configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResourceConfig {
    /// Memory limit in bytes (0 = unlimited)
    #[serde(default)]
    pub memory_max: u64,
    /// Memory high watermark
    #[serde(default)]
    pub memory_high: u64,
    /// CPU weight (1-10000, default 100)
    #[serde(default = "default_cpu_weight")]
    pub cpu_weight: u32,
    /// CPU quota as percentage (e.g., 50 = 50%)
    #[serde(default)]
    pub cpu_quota: u32,
    /// Maximum number of tasks/threads
    #[serde(default)]
    pub tasks_max: u32,
    /// IO weight (1-10000, default 100)
    #[serde(default = "default_io_weight")]
    pub io_weight: u32,
    /// Maximum open files
    #[serde(default)]
    pub limit_nofile: u64,
    /// Maximum processes
    #[serde(default)]
    pub limit_nproc: u64,
    /// OOM score adjustment (-1000 to 1000)
    #[serde(default)]
    pub oom_score_adjust: i32,
}

fn default_cpu_weight() -> u32 { 100 }
fn default_io_weight() -> u32 { 100 }

/// Socket activation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocketConfig {
    /// Socket type
    pub socket_type: SocketType,
    /// Listen address
    pub listen: String,
    /// Accept connections (creates instance per connection)
    #[serde(default)]
    pub accept: bool,
    /// Maximum connections
    #[serde(default)]
    pub max_connections: u32,
    /// Socket permissions (octal)
    #[serde(default = "default_socket_mode")]
    pub mode: u32,
    /// Socket owner
    pub user: Option<String>,
    /// Socket group
    pub group: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SocketType {
    Stream,
    Datagram,
    Sequential,
}

fn default_socket_mode() -> u32 { 0o660 }

impl Unit {
    /// Load a unit from a file
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read unit file: {:?}", path))?;

        let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");

        let mut unit: Unit = match extension {
            "yaml" | "yml" => serde_yaml::from_str(&content)?,
            "toml" => toml::from_str(&content)?,
            "json" => serde_json::from_str(&content)?,
            _ => {
                // Try YAML first, then TOML
                serde_yaml::from_str(&content)
                    .or_else(|_| toml::from_str(&content).map_err(anyhow::Error::from))?
            }
        };

        // Derive name from filename if not set
        if unit.name.is_empty() {
            unit.name = path.file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string())
                .unwrap_or_default();
        }

        Ok(unit)
    }

    /// Check if this unit should start before another
    pub fn starts_before(&self, other: &str) -> bool {
        self.install.before.iter().any(|b| b == other)
    }

    /// Check if this unit should start after another
    pub fn starts_after(&self, other: &str) -> bool {
        self.install.after.iter().any(|a| a == other)
    }

    /// Check if this unit requires another
    pub fn requires(&self, other: &str) -> bool {
        self.install.requires.iter().any(|r| r == other)
    }

    /// Get all dependencies (requires + wants + after)
    pub fn dependencies(&self) -> Vec<&str> {
        let mut deps: Vec<&str> = Vec::new();
        deps.extend(self.install.requires.iter().map(|s| s.as_str()));
        deps.extend(self.install.wants.iter().map(|s| s.as_str()));
        deps.extend(self.install.after.iter().map(|s| s.as_str()));
        deps.sort();
        deps.dedup();
        deps
    }

    /// Check if the unit has socket activation
    pub fn is_socket_activated(&self) -> bool {
        self.socket.is_some()
    }

    /// Get the effective command to start the service
    pub fn start_command(&self) -> Option<&str> {
        self.service.exec_start.as_deref()
    }
}

/// Registry of loaded units
pub struct UnitRegistry {
    units: HashMap<String, Unit>,
    aliases: HashMap<String, String>,
    enabled: std::collections::HashSet<String>,
}

impl UnitRegistry {
    pub fn new() -> Self {
        Self {
            units: HashMap::new(),
            aliases: HashMap::new(),
            enabled: std::collections::HashSet::new(),
        }
    }

    /// Load all units from a directory
    pub fn load_directory(&mut self, path: &Path) -> Result<usize> {
        if !path.exists() {
            warn!("Unit directory does not exist: {:?}", path);
            return Ok(0);
        }

        let mut count = 0;

        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();

            if !path.is_file() {
                continue;
            }

            let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if !matches!(extension, "yaml" | "yml" | "toml" | "json" | "service") {
                continue;
            }

            match Unit::load(&path) {
                Ok(unit) => {
                    info!("Loaded unit: {} from {:?}", unit.name, path);

                    // Register aliases
                    for alias in &unit.install.alias {
                        self.aliases.insert(alias.clone(), unit.name.clone());
                    }

                    // Track enabled state
                    if unit.install.enabled {
                        self.enabled.insert(unit.name.clone());
                    }

                    self.units.insert(unit.name.clone(), unit);
                    count += 1;
                }
                Err(e) => {
                    warn!("Failed to load unit from {:?}: {}", path, e);
                }
            }
        }

        Ok(count)
    }

    /// Get a unit by name or alias
    pub fn get(&self, name: &str) -> Option<&Unit> {
        self.units.get(name).or_else(|| {
            self.aliases.get(name).and_then(|real| self.units.get(real))
        })
    }

    /// Get a mutable reference to a unit
    pub fn get_mut(&mut self, name: &str) -> Option<&mut Unit> {
        let real_name = self.aliases.get(name).cloned();
        real_name.as_ref()
            .and_then(|n| self.units.get_mut(n))
            .or_else(|| self.units.get_mut(name))
    }

    /// Register a unit
    pub fn register(&mut self, unit: Unit) {
        for alias in &unit.install.alias {
            self.aliases.insert(alias.clone(), unit.name.clone());
        }
        self.units.insert(unit.name.clone(), unit);
    }

    /// Iterate over all units
    pub fn all(&self) -> impl Iterator<Item = &Unit> {
        self.units.values()
    }

    /// Get all enabled units
    pub fn enabled(&self) -> impl Iterator<Item = &Unit> {
        self.units.values().filter(|u| self.enabled.contains(&u.name))
    }

    /// Enable a unit
    pub fn enable(&mut self, name: &str) -> bool {
        if self.units.contains_key(name) {
            self.enabled.insert(name.to_string());
            true
        } else {
            false
        }
    }

    /// Disable a unit
    pub fn disable(&mut self, name: &str) -> bool {
        self.enabled.remove(name)
    }

    /// Check if a unit is enabled
    pub fn is_enabled(&self, name: &str) -> bool {
        self.enabled.contains(name)
    }

    /// Get unit names
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.units.keys().map(|s| s.as_str())
    }
}

impl Default for UnitRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unit_parse_yaml() {
        let yaml = r#"
name: test-service
description: Test service
service:
  exec_start: /usr/bin/test
  restart: always
install:
  after:
    - network.target
  enabled: true
"#;
        let unit: Unit = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(unit.name, "test-service");
        assert_eq!(unit.service.restart, RestartPolicy::Always);
        assert!(unit.install.enabled);
    }

    #[test]
    fn test_unit_dependencies() {
        let yaml = r#"
name: app
install:
  requires: [database]
  wants: [logging]
  after: [network]
"#;
        let unit: Unit = serde_yaml::from_str(yaml).unwrap();
        let deps = unit.dependencies();
        assert!(deps.contains(&"database"));
        assert!(deps.contains(&"logging"));
        assert!(deps.contains(&"network"));
    }
}
