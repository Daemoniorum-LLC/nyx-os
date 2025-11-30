//! Cgroups v2 resource management

use crate::unit::ResourceConfig;
use anyhow::{Result, Context};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{info, warn, debug};

const CGROUP_ROOT: &str = "/sys/fs/cgroup";
const CGROUP_SLICE: &str = "nyx.slice";

/// Cgroup manager for resource limits
pub struct CgroupManager {
    root: PathBuf,
    slice: PathBuf,
}

impl CgroupManager {
    pub fn new() -> Result<Self> {
        let root = PathBuf::from(CGROUP_ROOT);

        // Check for cgroups v2
        if !root.join("cgroup.controllers").exists() {
            return Err(anyhow::anyhow!("Cgroups v2 not available"));
        }

        let slice = root.join(CGROUP_SLICE);

        // Create our slice
        if !slice.exists() {
            fs::create_dir(&slice)
                .with_context(|| format!("Failed to create cgroup slice: {:?}", slice))?;
        }

        // Enable controllers for slice
        let controllers = read_available_controllers(&root)?;
        enable_controllers(&root, &slice, &controllers)?;

        info!("Cgroup manager initialized at {:?}", slice);

        Ok(Self { root, slice })
    }

    /// Create a cgroup for a service
    pub fn create_service_cgroup(&self, name: &str, config: &ResourceConfig) -> Result<PathBuf> {
        let cgroup_path = self.slice.join(format!("{}.scope", name));

        if !cgroup_path.exists() {
            fs::create_dir(&cgroup_path)
                .with_context(|| format!("Failed to create cgroup: {:?}", cgroup_path))?;
        }

        // Apply resource limits
        self.apply_limits(&cgroup_path, config)?;

        debug!("Created cgroup for {}: {:?}", name, cgroup_path);
        Ok(cgroup_path)
    }

    /// Apply resource limits to a cgroup
    fn apply_limits(&self, cgroup_path: &Path, config: &ResourceConfig) -> Result<()> {
        // Memory limits
        if config.memory_max > 0 {
            write_cgroup_file(cgroup_path, "memory.max", &config.memory_max.to_string())?;
        }

        if config.memory_high > 0 {
            write_cgroup_file(cgroup_path, "memory.high", &config.memory_high.to_string())?;
        }

        // CPU limits
        if config.cpu_weight != 100 {
            write_cgroup_file(cgroup_path, "cpu.weight", &config.cpu_weight.to_string())?;
        }

        if config.cpu_quota > 0 {
            // cpu.max format: "$MAX $PERIOD" (e.g., "50000 100000" for 50%)
            let max = (config.cpu_quota as u64) * 1000;
            write_cgroup_file(cgroup_path, "cpu.max", &format!("{} 100000", max))?;
        }

        // Tasks limit
        if config.tasks_max > 0 {
            write_cgroup_file(cgroup_path, "pids.max", &config.tasks_max.to_string())?;
        }

        // IO weight
        if config.io_weight != 100 {
            write_cgroup_file(cgroup_path, "io.weight", &format!("default {}", config.io_weight))?;
        }

        Ok(())
    }

    /// Add a process to a service cgroup
    pub fn add_process(&self, name: &str, pid: u32) -> Result<()> {
        let cgroup_path = self.slice.join(format!("{}.scope", name));

        if !cgroup_path.exists() {
            return Err(anyhow::anyhow!("Cgroup does not exist for {}", name));
        }

        write_cgroup_file(&cgroup_path, "cgroup.procs", &pid.to_string())?;

        debug!("Added PID {} to cgroup {}", pid, name);
        Ok(())
    }

    /// Get resource usage for a service
    pub fn get_usage(&self, name: &str) -> Result<ResourceUsage> {
        let cgroup_path = self.slice.join(format!("{}.scope", name));

        if !cgroup_path.exists() {
            return Err(anyhow::anyhow!("Cgroup does not exist for {}", name));
        }

        let memory_current = read_cgroup_u64(&cgroup_path, "memory.current")?;
        let cpu_stat = read_cgroup_file(&cgroup_path, "cpu.stat")?;

        let usage_usec = parse_cpu_stat(&cpu_stat);

        Ok(ResourceUsage {
            memory_bytes: memory_current,
            cpu_usage_usec: usage_usec,
        })
    }

    /// Remove a service cgroup
    pub fn remove_cgroup(&self, name: &str) -> Result<()> {
        let cgroup_path = self.slice.join(format!("{}.scope", name));

        if cgroup_path.exists() {
            // Kill all processes first
            if let Ok(procs) = read_cgroup_file(&cgroup_path, "cgroup.procs") {
                for line in procs.lines() {
                    if let Ok(pid) = line.parse::<i32>() {
                        let _ = nix::sys::signal::kill(
                            nix::unistd::Pid::from_raw(pid),
                            nix::sys::signal::Signal::SIGKILL
                        );
                    }
                }
            }

            // Wait for processes to exit
            std::thread::sleep(std::time::Duration::from_millis(100));

            // Remove cgroup
            fs::remove_dir(&cgroup_path)
                .with_context(|| format!("Failed to remove cgroup: {:?}", cgroup_path))?;

            debug!("Removed cgroup for {}", name);
        }

        Ok(())
    }

    /// Freeze a service (pause all processes)
    pub fn freeze(&self, name: &str) -> Result<()> {
        let cgroup_path = self.slice.join(format!("{}.scope", name));
        write_cgroup_file(&cgroup_path, "cgroup.freeze", "1")?;
        info!("Froze service {}", name);
        Ok(())
    }

    /// Thaw a service (resume all processes)
    pub fn thaw(&self, name: &str) -> Result<()> {
        let cgroup_path = self.slice.join(format!("{}.scope", name));
        write_cgroup_file(&cgroup_path, "cgroup.freeze", "0")?;
        info!("Thawed service {}", name);
        Ok(())
    }

    /// Get list of PIDs in a service cgroup
    pub fn get_pids(&self, name: &str) -> Result<Vec<u32>> {
        let cgroup_path = self.slice.join(format!("{}.scope", name));
        let procs = read_cgroup_file(&cgroup_path, "cgroup.procs")?;

        let pids: Vec<u32> = procs
            .lines()
            .filter_map(|l| l.parse().ok())
            .collect();

        Ok(pids)
    }
}

/// Resource usage information
#[derive(Debug, Clone, Default)]
pub struct ResourceUsage {
    pub memory_bytes: u64,
    pub cpu_usage_usec: u64,
}

fn read_available_controllers(root: &Path) -> Result<Vec<String>> {
    let content = fs::read_to_string(root.join("cgroup.controllers"))?;
    Ok(content.split_whitespace().map(String::from).collect())
}

fn enable_controllers(root: &Path, target: &Path, controllers: &[String]) -> Result<()> {
    // Enable controllers by writing to parent's cgroup.subtree_control
    let subtree_control = root.join("cgroup.subtree_control");

    for controller in controllers {
        // Skip controllers that may not be delegatable
        if matches!(controller.as_str(), "cpuset" | "misc") {
            continue;
        }

        let content = format!("+{}", controller);
        if let Err(e) = fs::write(&subtree_control, &content) {
            warn!("Could not enable {} controller: {}", controller, e);
        }
    }

    Ok(())
}

fn write_cgroup_file(cgroup_path: &Path, file: &str, value: &str) -> Result<()> {
    let path = cgroup_path.join(file);
    fs::write(&path, value)
        .with_context(|| format!("Failed to write {:?}", path))
}

fn read_cgroup_file(cgroup_path: &Path, file: &str) -> Result<String> {
    let path = cgroup_path.join(file);
    fs::read_to_string(&path)
        .with_context(|| format!("Failed to read {:?}", path))
}

fn read_cgroup_u64(cgroup_path: &Path, file: &str) -> Result<u64> {
    let content = read_cgroup_file(cgroup_path, file)?;
    content.trim()
        .parse()
        .with_context(|| format!("Failed to parse {} as u64", file))
}

fn parse_cpu_stat(stat: &str) -> u64 {
    for line in stat.lines() {
        if line.starts_with("usage_usec") {
            if let Some(value) = line.split_whitespace().nth(1) {
                return value.parse().unwrap_or(0);
            }
        }
    }
    0
}

/// Set OOM score adjustment for a process
pub fn set_oom_score_adj(pid: u32, score: i32) -> Result<()> {
    let path = format!("/proc/{}/oom_score_adj", pid);
    let score = score.max(-1000).min(1000);
    fs::write(&path, score.to_string())
        .with_context(|| format!("Failed to set OOM score for PID {}", pid))
}

/// Set process resource limits (rlimits)
pub fn set_rlimits(nofile: u64, nproc: u64) -> Result<()> {
    use libc::{RLIMIT_NOFILE, RLIMIT_NPROC, rlimit, setrlimit};

    if nofile > 0 {
        let rlim = rlimit {
            rlim_cur: nofile,
            rlim_max: nofile,
        };
        if unsafe { setrlimit(RLIMIT_NOFILE, &rlim) } != 0 {
            warn!("Failed to set RLIMIT_NOFILE");
        }
    }

    if nproc > 0 {
        let rlim = rlimit {
            rlim_cur: nproc,
            rlim_max: nproc,
        };
        if unsafe { setrlimit(RLIMIT_NPROC, &rlim) } != 0 {
            warn!("Failed to set RLIMIT_NPROC");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cpu_stat() {
        let stat = "usage_usec 12345678\nuser_usec 10000000\nsystem_usec 2345678";
        assert_eq!(parse_cpu_stat(stat), 12345678);
    }
}
