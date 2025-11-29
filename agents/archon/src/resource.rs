//! Resource management
//!
//! Manages CPU, memory, and IO resources for processes.

use crate::cgroup::CgroupManager;
use crate::config::{ResourceConfig, ResourceProfile};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Resource limits for a process
#[derive(Debug, Clone)]
pub struct ResourceLimits {
    /// CPU quota (percentage)
    pub cpu_percent: u32,
    /// CPU shares (relative weight)
    pub cpu_shares: u32,
    /// Memory limit (bytes)
    pub memory_bytes: u64,
    /// Memory + swap limit (bytes)
    pub memory_swap_bytes: u64,
    /// IO bandwidth limit (bytes/sec)
    pub io_bandwidth: u64,
    /// Maximum processes
    pub max_processes: u32,
    /// Maximum open files
    pub max_files: u32,
    /// OOM score adjustment
    pub oom_score_adj: i32,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            cpu_percent: 100,
            cpu_shares: 1024,
            memory_bytes: 0,
            memory_swap_bytes: 0,
            io_bandwidth: 0,
            max_processes: 0,
            max_files: 1024,
            oom_score_adj: 0,
        }
    }
}

/// Resource manager
pub struct ResourceManager {
    /// Configuration
    config: ResourceConfig,
    /// Cgroup manager
    cgroup_manager: Arc<CgroupManager>,
    /// Resource profiles
    profiles: HashMap<String, ResourceProfile>,
    /// Per-process resource tracking
    process_resources: HashMap<u32, ResourceLimits>,
}

impl ResourceManager {
    /// Create a new resource manager
    pub fn new(config: &ResourceConfig, cgroup_manager: Arc<CgroupManager>) -> Result<Self> {
        let profiles: HashMap<String, ResourceProfile> = config
            .profiles
            .iter()
            .map(|p| (p.name.clone(), p.clone()))
            .collect();

        info!("Resource manager initialized with {} profiles", profiles.len());

        Ok(Self {
            config: config.clone(),
            cgroup_manager,
            profiles,
            process_resources: HashMap::new(),
        })
    }

    /// Get a resource profile by name
    pub fn get_profile(&self, name: &str) -> Option<&ResourceProfile> {
        self.profiles.get(name)
    }

    /// List all profiles
    pub fn list_profiles(&self) -> Vec<&ResourceProfile> {
        self.profiles.values().collect()
    }

    /// Apply a resource profile to a process
    pub async fn apply_profile(&self, pid: u32, profile_name: &str) -> Result<()> {
        let profile = self.profiles.get(profile_name)
            .ok_or_else(|| anyhow::anyhow!("Unknown resource profile: {}", profile_name))?;

        let limits = ResourceLimits {
            cpu_percent: profile.cpu_percent,
            cpu_shares: profile.cpu_shares,
            memory_bytes: profile.memory_bytes,
            memory_swap_bytes: profile.memory_bytes, // Same as memory for now
            io_bandwidth: profile.io_bandwidth,
            max_processes: profile.max_processes,
            max_files: profile.max_files,
            oom_score_adj: profile.oom_score_adj,
        };

        self.apply_limits(pid, &limits).await?;

        debug!("Applied profile '{}' to pid {}", profile_name, pid);
        Ok(())
    }

    /// Apply resource limits to a process
    pub async fn apply_limits(&self, pid: u32, limits: &ResourceLimits) -> Result<()> {
        if !self.config.enabled {
            debug!("Resource limiting disabled, skipping");
            return Ok(());
        }

        // Apply via cgroups
        self.cgroup_manager.apply_limits(pid, limits)?;

        // Apply rlimits directly
        self.apply_rlimits(pid, limits)?;

        // Apply OOM score
        self.apply_oom_score(pid, limits.oom_score_adj)?;

        Ok(())
    }

    /// Apply rlimits to a process
    fn apply_rlimits(&self, pid: u32, limits: &ResourceLimits) -> Result<()> {
        // Note: Setting rlimits for another process requires root privileges
        // In practice, we'd set these at spawn time via pre_exec hooks
        // For now, we just log what we'd do

        debug!(
            "Would set rlimits for pid {}: max_files={}, max_processes={}",
            pid, limits.max_files, limits.max_processes
        );

        // These would be applied via prlimit syscall
        // prlimit(pid, RLIMIT_NOFILE, ...) for max_files
        // prlimit(pid, RLIMIT_NPROC, ...) for max_processes

        Ok(())
    }

    /// Apply OOM score adjustment
    fn apply_oom_score(&self, pid: u32, score_adj: i32) -> Result<()> {
        let path = format!("/proc/{}/oom_score_adj", pid);

        // Clamp to valid range
        let score = score_adj.clamp(-1000, 1000);

        std::fs::write(&path, score.to_string())
            .context("Failed to set OOM score")?;

        debug!("Set OOM score adj for pid {} to {}", pid, score);
        Ok(())
    }

    /// Get current resource usage for a process
    pub fn get_usage(&self, pid: u32) -> Result<ResourceUsage> {
        // Try to read from /proc
        let stat_path = format!("/proc/{}/stat", pid);
        let statm_path = format!("/proc/{}/statm", pid);

        // Parse CPU time from /proc/[pid]/stat
        let stat_content = std::fs::read_to_string(&stat_path)
            .context("Failed to read process stat")?;
        let stat_parts: Vec<&str> = stat_content.split_whitespace().collect();

        let utime: u64 = stat_parts.get(13)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let stime: u64 = stat_parts.get(14)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let num_threads: u32 = stat_parts.get(19)
            .and_then(|s| s.parse().ok())
            .unwrap_or(1);

        // Parse memory from /proc/[pid]/statm
        let statm_content = std::fs::read_to_string(&statm_path)
            .context("Failed to read process statm")?;
        let statm_parts: Vec<&str> = statm_content.split_whitespace().collect();

        let page_size = 4096u64; // Typical page size
        let total_pages: u64 = statm_parts.get(0)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let resident_pages: u64 = statm_parts.get(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let shared_pages: u64 = statm_parts.get(2)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        Ok(ResourceUsage {
            cpu_time_user: utime,
            cpu_time_system: stime,
            memory_virtual: total_pages * page_size,
            memory_resident: resident_pages * page_size,
            memory_shared: shared_pages * page_size,
            num_threads,
            io_read_bytes: 0, // Would need /proc/[pid]/io
            io_write_bytes: 0,
        })
    }

    /// Check if a process exceeds its limits
    pub fn check_limits(&self, pid: u32, limits: &ResourceLimits) -> Vec<LimitViolation> {
        let mut violations = Vec::new();

        if let Ok(usage) = self.get_usage(pid) {
            // Check memory
            if limits.memory_bytes > 0 && usage.memory_resident > limits.memory_bytes {
                violations.push(LimitViolation::MemoryExceeded {
                    current: usage.memory_resident,
                    limit: limits.memory_bytes,
                });
            }

            // Note: CPU percentage checks would require sampling over time
        }

        violations
    }

    /// Get system-wide resource availability
    pub fn system_resources(&self) -> Result<SystemResources> {
        // Read /proc/meminfo
        let meminfo = std::fs::read_to_string("/proc/meminfo")?;
        let mut total_memory = 0u64;
        let mut free_memory = 0u64;
        let mut available_memory = 0u64;

        for line in meminfo.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let value: u64 = parts[1].parse().unwrap_or(0) * 1024; // KB to bytes
                match parts[0] {
                    "MemTotal:" => total_memory = value,
                    "MemFree:" => free_memory = value,
                    "MemAvailable:" => available_memory = value,
                    _ => {}
                }
            }
        }

        // Read /proc/stat for CPU info
        let stat = std::fs::read_to_string("/proc/stat")?;
        let cpu_line = stat.lines().next().unwrap_or("");
        let cpu_parts: Vec<&str> = cpu_line.split_whitespace().collect();

        let cpu_user: u64 = cpu_parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
        let cpu_system: u64 = cpu_parts.get(3).and_then(|s| s.parse().ok()).unwrap_or(0);
        let cpu_idle: u64 = cpu_parts.get(4).and_then(|s| s.parse().ok()).unwrap_or(0);
        let cpu_total = cpu_user + cpu_system + cpu_idle;

        // Count CPUs
        let num_cpus = stat.lines()
            .filter(|l| l.starts_with("cpu") && l.chars().nth(3).map_or(false, |c| c.is_ascii_digit()))
            .count() as u32;

        Ok(SystemResources {
            total_memory,
            free_memory,
            available_memory,
            num_cpus,
            cpu_usage_percent: if cpu_total > 0 {
                ((cpu_user + cpu_system) * 100 / cpu_total) as u32
            } else {
                0
            },
        })
    }
}

/// Resource usage statistics
#[derive(Debug, Clone)]
pub struct ResourceUsage {
    /// User CPU time (jiffies)
    pub cpu_time_user: u64,
    /// System CPU time (jiffies)
    pub cpu_time_system: u64,
    /// Virtual memory size (bytes)
    pub memory_virtual: u64,
    /// Resident memory size (bytes)
    pub memory_resident: u64,
    /// Shared memory size (bytes)
    pub memory_shared: u64,
    /// Number of threads
    pub num_threads: u32,
    /// Bytes read
    pub io_read_bytes: u64,
    /// Bytes written
    pub io_write_bytes: u64,
}

/// Limit violation types
#[derive(Debug, Clone)]
pub enum LimitViolation {
    MemoryExceeded { current: u64, limit: u64 },
    CpuExceeded { current: u32, limit: u32 },
    IoExceeded { current: u64, limit: u64 },
    ProcessesExceeded { current: u32, limit: u32 },
}

/// System resource information
#[derive(Debug, Clone)]
pub struct SystemResources {
    /// Total memory (bytes)
    pub total_memory: u64,
    /// Free memory (bytes)
    pub free_memory: u64,
    /// Available memory (bytes)
    pub available_memory: u64,
    /// Number of CPUs
    pub num_cpus: u32,
    /// Current CPU usage (percentage)
    pub cpu_usage_percent: u32,
}
