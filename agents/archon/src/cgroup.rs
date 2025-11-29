//! Cgroup management
//!
//! Manages cgroups v2 for resource isolation and limiting.

use crate::config::CgroupConfig;
use crate::resource::ResourceLimits;
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

/// Cgroup manager for v2 cgroups
pub struct CgroupManager {
    /// Configuration
    config: CgroupConfig,
    /// Cgroup mount point
    mount_point: PathBuf,
    /// Root cgroup path
    root_path: PathBuf,
    /// Whether cgroups are available
    available: bool,
}

impl CgroupManager {
    /// Create a new cgroup manager
    pub fn new(config: &CgroupConfig) -> Result<Self> {
        let mount_point = PathBuf::from(&config.mount_point);
        let root_path = mount_point.join(&config.root_name);

        // Check if cgroups are available
        let available = if config.enabled {
            Self::check_cgroup_availability(&mount_point)
        } else {
            false
        };

        if available {
            // Create root cgroup
            if !root_path.exists() {
                fs::create_dir_all(&root_path)
                    .context("Failed to create root cgroup")?;
                info!("Created cgroup root at {}", root_path.display());
            }

            // Enable controllers
            Self::enable_controllers(&mount_point, &config.controllers)?;
        } else {
            warn!("Cgroups not available or disabled");
        }

        Ok(Self {
            config: config.clone(),
            mount_point,
            root_path,
            available,
        })
    }

    fn check_cgroup_availability(mount_point: &Path) -> bool {
        // Check if cgroup2 is mounted
        let controllers_path = mount_point.join("cgroup.controllers");
        if controllers_path.exists() {
            debug!("cgroup v2 detected at {}", mount_point.display());
            return true;
        }

        // Check for cgroup v1 (legacy)
        let cgroup_v1 = mount_point.join("cpu");
        if cgroup_v1.exists() {
            debug!("cgroup v1 detected (legacy mode)");
            return true;
        }

        false
    }

    fn enable_controllers(mount_point: &Path, controllers: &[String]) -> Result<()> {
        let subtree_control = mount_point.join("cgroup.subtree_control");

        if !subtree_control.exists() {
            return Ok(()); // v1 or not available
        }

        let controller_str: String = controllers
            .iter()
            .map(|c| format!("+{}", c))
            .collect::<Vec<_>>()
            .join(" ");

        fs::write(&subtree_control, &controller_str)
            .context("Failed to enable cgroup controllers")?;

        debug!("Enabled cgroup controllers: {}", controller_str);
        Ok(())
    }

    /// Check if cgroups are available
    pub fn is_available(&self) -> bool {
        self.available
    }

    /// Create a cgroup for a process
    pub fn create_cgroup(&self, name: &str) -> Result<PathBuf> {
        if !self.available {
            return Err(anyhow::anyhow!("Cgroups not available"));
        }

        let cgroup_path = self.root_path.join(name);

        if !cgroup_path.exists() {
            fs::create_dir_all(&cgroup_path)
                .context("Failed to create cgroup")?;
            debug!("Created cgroup: {}", cgroup_path.display());
        }

        // Enable controllers for this cgroup
        let subtree_control = cgroup_path.join("cgroup.subtree_control");
        if subtree_control.exists() {
            let controller_str: String = self.config.controllers
                .iter()
                .map(|c| format!("+{}", c))
                .collect::<Vec<_>>()
                .join(" ");
            let _ = fs::write(&subtree_control, &controller_str);
        }

        Ok(cgroup_path)
    }

    /// Add a process to a cgroup
    pub fn add_process(&self, cgroup_name: &str, pid: u32) -> Result<()> {
        if !self.available {
            return Ok(()); // Silently succeed if not available
        }

        let cgroup_path = self.root_path.join(cgroup_name);
        let procs_path = cgroup_path.join("cgroup.procs");

        fs::write(&procs_path, pid.to_string())
            .context("Failed to add process to cgroup")?;

        debug!("Added pid {} to cgroup {}", pid, cgroup_name);
        Ok(())
    }

    /// Apply resource limits to a process via cgroups
    pub fn apply_limits(&self, pid: u32, limits: &ResourceLimits) -> Result<()> {
        if !self.available {
            return Ok(());
        }

        // Create a cgroup for this process
        let cgroup_name = format!("pid_{}", pid);
        let cgroup_path = self.create_cgroup(&cgroup_name)?;

        // Add process to cgroup
        self.add_process(&cgroup_name, pid)?;

        // Apply CPU limits
        if limits.cpu_percent > 0 && limits.cpu_percent < 100 {
            self.set_cpu_limit(&cgroup_path, limits.cpu_percent)?;
        }
        self.set_cpu_shares(&cgroup_path, limits.cpu_shares)?;

        // Apply memory limits
        if limits.memory_bytes > 0 {
            self.set_memory_limit(&cgroup_path, limits.memory_bytes)?;
        }

        // Apply IO limits
        if limits.io_bandwidth > 0 {
            self.set_io_limit(&cgroup_path, limits.io_bandwidth)?;
        }

        // Apply process limits
        if limits.max_processes > 0 {
            self.set_pids_limit(&cgroup_path, limits.max_processes)?;
        }

        Ok(())
    }

    fn set_cpu_limit(&self, cgroup_path: &Path, percent: u32) -> Result<()> {
        let cpu_max_path = cgroup_path.join("cpu.max");

        if cpu_max_path.exists() {
            // cpu.max format: "$MAX $PERIOD" in microseconds
            // 100000 = 100ms period
            let period = 100000u64;
            let quota = (period * percent as u64) / 100;
            let value = format!("{} {}", quota, period);

            fs::write(&cpu_max_path, &value)
                .context("Failed to set CPU limit")?;
            debug!("Set CPU limit to {}% ({}/{})", percent, quota, period);
        }

        Ok(())
    }

    fn set_cpu_shares(&self, cgroup_path: &Path, shares: u32) -> Result<()> {
        let cpu_weight_path = cgroup_path.join("cpu.weight");

        if cpu_weight_path.exists() {
            // cpu.weight is 1-10000, with 100 being the default
            // Convert from shares (1024 default) to weight scale
            let weight = ((shares as u64 * 100) / 1024).clamp(1, 10000) as u32;

            fs::write(&cpu_weight_path, weight.to_string())
                .context("Failed to set CPU weight")?;
            debug!("Set CPU weight to {} (from shares {})", weight, shares);
        }

        Ok(())
    }

    fn set_memory_limit(&self, cgroup_path: &Path, bytes: u64) -> Result<()> {
        let memory_max_path = cgroup_path.join("memory.max");

        if memory_max_path.exists() {
            fs::write(&memory_max_path, bytes.to_string())
                .context("Failed to set memory limit")?;
            debug!("Set memory limit to {} bytes", bytes);
        }

        // Also set memory.high for soft limit (90% of max)
        let memory_high_path = cgroup_path.join("memory.high");
        if memory_high_path.exists() {
            let high = bytes * 90 / 100;
            let _ = fs::write(&memory_high_path, high.to_string());
        }

        Ok(())
    }

    fn set_io_limit(&self, cgroup_path: &Path, bytes_per_sec: u64) -> Result<()> {
        let io_max_path = cgroup_path.join("io.max");

        if io_max_path.exists() {
            // io.max format: "$MAJ:$MIN rbps=$READ wbps=$WRITE riops=max wiops=max"
            // We'd need to know the device major:minor numbers
            // For now, we just log what we'd do
            debug!("Would set IO limit to {} bytes/sec", bytes_per_sec);
        }

        Ok(())
    }

    fn set_pids_limit(&self, cgroup_path: &Path, max_pids: u32) -> Result<()> {
        let pids_max_path = cgroup_path.join("pids.max");

        if pids_max_path.exists() {
            fs::write(&pids_max_path, max_pids.to_string())
                .context("Failed to set pids limit")?;
            debug!("Set pids limit to {}", max_pids);
        }

        Ok(())
    }

    /// Get cgroup statistics
    pub fn get_stats(&self, cgroup_name: &str) -> Result<CgroupStats> {
        let cgroup_path = self.root_path.join(cgroup_name);

        if !cgroup_path.exists() {
            return Err(anyhow::anyhow!("Cgroup not found: {}", cgroup_name));
        }

        let mut stats = CgroupStats::default();

        // Read CPU stats
        let cpu_stat_path = cgroup_path.join("cpu.stat");
        if cpu_stat_path.exists() {
            if let Ok(content) = fs::read_to_string(&cpu_stat_path) {
                for line in content.lines() {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        match parts[0] {
                            "usage_usec" => stats.cpu_usage_usec = parts[1].parse().unwrap_or(0),
                            "user_usec" => stats.cpu_user_usec = parts[1].parse().unwrap_or(0),
                            "system_usec" => stats.cpu_system_usec = parts[1].parse().unwrap_or(0),
                            _ => {}
                        }
                    }
                }
            }
        }

        // Read memory stats
        let memory_current_path = cgroup_path.join("memory.current");
        if memory_current_path.exists() {
            if let Ok(content) = fs::read_to_string(&memory_current_path) {
                stats.memory_current = content.trim().parse().unwrap_or(0);
            }
        }

        let memory_stat_path = cgroup_path.join("memory.stat");
        if memory_stat_path.exists() {
            if let Ok(content) = fs::read_to_string(&memory_stat_path) {
                for line in content.lines() {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 && parts[0] == "anon" {
                        stats.memory_anon = parts[1].parse().unwrap_or(0);
                    }
                }
            }
        }

        // Read pids stats
        let pids_current_path = cgroup_path.join("pids.current");
        if pids_current_path.exists() {
            if let Ok(content) = fs::read_to_string(&pids_current_path) {
                stats.pids_current = content.trim().parse().unwrap_or(0);
            }
        }

        Ok(stats)
    }

    /// Remove a cgroup
    pub fn remove_cgroup(&self, name: &str) -> Result<()> {
        let cgroup_path = self.root_path.join(name);

        if cgroup_path.exists() {
            // First, move any processes to parent
            let procs_path = cgroup_path.join("cgroup.procs");
            if procs_path.exists() {
                if let Ok(content) = fs::read_to_string(&procs_path) {
                    let parent_procs = self.root_path.join("cgroup.procs");
                    for pid in content.lines() {
                        let _ = fs::write(&parent_procs, pid);
                    }
                }
            }

            // Remove the cgroup directory
            fs::remove_dir(&cgroup_path)
                .context("Failed to remove cgroup")?;
            debug!("Removed cgroup: {}", name);
        }

        Ok(())
    }

    /// List all cgroups under root
    pub fn list_cgroups(&self) -> Result<Vec<String>> {
        let mut cgroups = Vec::new();

        if self.root_path.exists() {
            for entry in fs::read_dir(&self.root_path)? {
                let entry = entry?;
                if entry.file_type()?.is_dir() {
                    if let Some(name) = entry.file_name().to_str() {
                        cgroups.push(name.to_string());
                    }
                }
            }
        }

        Ok(cgroups)
    }
}

/// Cgroup statistics
#[derive(Debug, Clone, Default)]
pub struct CgroupStats {
    /// CPU usage in microseconds
    pub cpu_usage_usec: u64,
    /// User CPU time in microseconds
    pub cpu_user_usec: u64,
    /// System CPU time in microseconds
    pub cpu_system_usec: u64,
    /// Current memory usage
    pub memory_current: u64,
    /// Anonymous memory
    pub memory_anon: u64,
    /// Current number of processes
    pub pids_current: u32,
}
