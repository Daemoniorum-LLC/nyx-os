//! Sleep/suspend/hibernate management

use crate::config::{SleepConfig, SuspendMethod};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::process::Command;
use tracing::{debug, info, warn};

/// Sleep manager
pub struct SleepManager {
    config: SleepConfig,
}

impl SleepManager {
    /// Create new sleep manager
    pub fn new(config: SleepConfig) -> Self {
        Self { config }
    }

    /// Get available sleep states
    pub fn available_states(&self) -> SleepStates {
        let mem_sleep = fs::read_to_string("/sys/power/mem_sleep").unwrap_or_default();
        let state = fs::read_to_string("/sys/power/state").unwrap_or_default();

        SleepStates {
            suspend: state.contains("mem") && self.config.suspend_enabled,
            hibernate: state.contains("disk") && self.config.hibernate_enabled,
            hybrid_sleep: state.contains("disk") && self.config.hybrid_sleep_enabled,
            freeze: state.contains("freeze"),
            standby: state.contains("standby"),
            suspend_methods: parse_mem_sleep(&mem_sleep),
        }
    }

    /// Suspend to RAM
    pub fn suspend(&self) -> Result<()> {
        if !self.config.suspend_enabled {
            return Err(anyhow!("Suspend is disabled"));
        }

        info!("Initiating suspend to RAM");

        // Set suspend method
        self.set_suspend_method()?;

        // Execute pre-suspend hooks
        self.run_hooks("pre-suspend")?;

        // Write to /sys/power/state
        fs::write("/sys/power/state", "mem")?;

        // Execute post-suspend hooks (after wake)
        self.run_hooks("post-suspend")?;

        Ok(())
    }

    /// Hibernate to disk
    pub fn hibernate(&self) -> Result<()> {
        if !self.config.hibernate_enabled {
            return Err(anyhow!("Hibernate is disabled"));
        }

        info!("Initiating hibernate to disk");

        // Check for swap
        if !self.has_adequate_swap()? {
            return Err(anyhow!("Insufficient swap space for hibernate"));
        }

        // Execute pre-hibernate hooks
        self.run_hooks("pre-hibernate")?;

        // Write to /sys/power/state
        fs::write("/sys/power/state", "disk")?;

        // Execute post-hibernate hooks (after resume)
        self.run_hooks("post-hibernate")?;

        Ok(())
    }

    /// Hybrid sleep (suspend + hibernate image)
    pub fn hybrid_sleep(&self) -> Result<()> {
        if !self.config.hybrid_sleep_enabled {
            return Err(anyhow!("Hybrid sleep is disabled"));
        }

        info!("Initiating hybrid sleep");

        // Set disk mode to suspend
        fs::write("/sys/power/disk", "suspend")?;

        // Execute hooks
        self.run_hooks("pre-hibernate")?;

        // Write to state
        fs::write("/sys/power/state", "disk")?;

        self.run_hooks("post-hibernate")?;

        Ok(())
    }

    /// Suspend to idle (freeze)
    pub fn freeze(&self) -> Result<()> {
        info!("Initiating suspend-to-idle (freeze)");

        self.run_hooks("pre-suspend")?;
        fs::write("/sys/power/state", "freeze")?;
        self.run_hooks("post-suspend")?;

        Ok(())
    }

    /// Set suspend method
    fn set_suspend_method(&self) -> Result<()> {
        let method = match self.config.suspend_method {
            SuspendMethod::Platform => "deep",
            SuspendMethod::Freeze => "s2idle",
            SuspendMethod::Standby => "shallow",
        };

        let mem_sleep_path = Path::new("/sys/power/mem_sleep");
        if mem_sleep_path.exists() {
            let available = fs::read_to_string(mem_sleep_path)?;
            if available.contains(method) {
                debug!("Setting suspend method to: {}", method);
                fs::write(mem_sleep_path, method)?;
            } else {
                warn!("Suspend method {} not available, using default", method);
            }
        }

        Ok(())
    }

    /// Check if we have adequate swap for hibernate
    fn has_adequate_swap(&self) -> Result<bool> {
        let meminfo = fs::read_to_string("/proc/meminfo")?;

        let mut mem_total: u64 = 0;
        let mut swap_total: u64 = 0;

        for line in meminfo.lines() {
            if line.starts_with("MemTotal:") {
                mem_total = parse_meminfo_value(line);
            } else if line.starts_with("SwapTotal:") {
                swap_total = parse_meminfo_value(line);
            }
        }

        // Need at least as much swap as RAM for safe hibernate
        Ok(swap_total >= mem_total)
    }

    /// Run sleep hooks
    fn run_hooks(&self, hook_name: &str) -> Result<()> {
        let hooks_dir = Path::new("/etc/slumber/hooks.d");

        if !hooks_dir.exists() {
            return Ok(());
        }

        let hook_path = hooks_dir.join(hook_name);
        if hook_path.exists() && hook_path.is_file() {
            debug!("Running hook: {}", hook_name);
            Command::new(&hook_path).status()?;
        }

        Ok(())
    }

    /// Get sleep status
    pub fn get_status(&self) -> SleepStatus {
        let states = self.available_states();

        SleepStatus {
            suspend_enabled: self.config.suspend_enabled,
            hibernate_enabled: self.config.hibernate_enabled,
            hybrid_sleep_enabled: self.config.hybrid_sleep_enabled,
            available_states: states,
            suspend_method: format!("{:?}", self.config.suspend_method),
            lock_before_sleep: self.config.lock_before_sleep,
        }
    }
}

/// Available sleep states
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SleepStates {
    pub suspend: bool,
    pub hibernate: bool,
    pub hybrid_sleep: bool,
    pub freeze: bool,
    pub standby: bool,
    pub suspend_methods: Vec<String>,
}

/// Sleep status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SleepStatus {
    pub suspend_enabled: bool,
    pub hibernate_enabled: bool,
    pub hybrid_sleep_enabled: bool,
    pub available_states: SleepStates,
    pub suspend_method: String,
    pub lock_before_sleep: bool,
}

/// Parse mem_sleep file
fn parse_mem_sleep(content: &str) -> Vec<String> {
    content
        .split_whitespace()
        .map(|s| s.trim_matches(|c| c == '[' || c == ']').to_string())
        .collect()
}

/// Parse meminfo value (e.g., "MemTotal:       16384000 kB")
fn parse_meminfo_value(line: &str) -> u64 {
    line.split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}
