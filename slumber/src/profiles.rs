//! Power profile management

use crate::config::{PowerProfile, ProfilesConfig};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use tracing::{debug, info, warn};

/// Profile manager
pub struct ProfileManager {
    config: ProfilesConfig,
    current_profile: String,
}

impl ProfileManager {
    /// Create new profile manager
    pub fn new(config: ProfilesConfig) -> Self {
        let current_profile = config.default_profile.clone();
        Self {
            config,
            current_profile,
        }
    }

    /// Get current profile name
    pub fn current(&self) -> &str {
        &self.current_profile
    }

    /// Get current profile
    pub fn current_profile(&self) -> Option<&PowerProfile> {
        self.get_profile(&self.current_profile)
    }

    /// Get profile by name
    pub fn get_profile(&self, name: &str) -> Option<&PowerProfile> {
        self.config.profiles.iter().find(|p| p.name == name)
    }

    /// List all profiles
    pub fn list_profiles(&self) -> Vec<&PowerProfile> {
        self.config.profiles.iter().collect()
    }

    /// Set active profile
    pub fn set_profile(&mut self, name: &str) -> Result<()> {
        let profile = self
            .get_profile(name)
            .ok_or_else(|| anyhow!("Profile not found: {}", name))?
            .clone();

        info!("Switching to power profile: {}", name);

        // Apply CPU governor
        if let Err(e) = self.set_cpu_governor(&profile.cpu_governor) {
            warn!("Failed to set CPU governor: {}", e);
        }

        // Apply CPU frequency scaling
        if let Err(e) = self.set_cpu_max_freq(profile.cpu_max_freq_percent) {
            warn!("Failed to set CPU max frequency: {}", e);
        }

        // Apply turbo boost setting
        if let Err(e) = self.set_turbo_boost(profile.turbo_boost) {
            warn!("Failed to set turbo boost: {}", e);
        }

        // Apply PCI power management
        if let Err(e) = self.set_pci_pm(profile.pci_pm) {
            warn!("Failed to set PCI PM: {}", e);
        }

        // Apply USB autosuspend
        if let Err(e) = self.set_usb_autosuspend(profile.usb_autosuspend) {
            warn!("Failed to set USB autosuspend: {}", e);
        }

        self.current_profile = name.to_string();
        Ok(())
    }

    /// Set CPU governor for all CPUs
    fn set_cpu_governor(&self, governor: &str) -> Result<()> {
        let cpufreq_path = Path::new("/sys/devices/system/cpu/cpufreq");

        if !cpufreq_path.exists() {
            debug!("cpufreq not available");
            return Ok(());
        }

        for entry in fs::read_dir(cpufreq_path)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();

            if name.starts_with("policy") {
                let governor_path = entry.path().join("scaling_governor");
                if governor_path.exists() {
                    debug!("Setting {} governor to {}", name, governor);
                    fs::write(&governor_path, governor)?;
                }
            }
        }

        Ok(())
    }

    /// Set CPU max frequency percentage
    fn set_cpu_max_freq(&self, percent: u8) -> Result<()> {
        let cpufreq_path = Path::new("/sys/devices/system/cpu/cpufreq");

        if !cpufreq_path.exists() {
            return Ok(());
        }

        for entry in fs::read_dir(cpufreq_path)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();

            if name.starts_with("policy") {
                let max_path = entry.path().join("cpuinfo_max_freq");
                let scaling_max_path = entry.path().join("scaling_max_freq");

                if max_path.exists() && scaling_max_path.exists() {
                    let max_freq: u64 = fs::read_to_string(&max_path)?.trim().parse()?;
                    let target_freq = (max_freq as f64 * percent as f64 / 100.0) as u64;

                    debug!("Setting {} max freq to {} ({}%)", name, target_freq, percent);
                    fs::write(&scaling_max_path, target_freq.to_string())?;
                }
            }
        }

        Ok(())
    }

    /// Set turbo boost state
    fn set_turbo_boost(&self, enabled: bool) -> Result<()> {
        // Intel: /sys/devices/system/cpu/intel_pstate/no_turbo
        let intel_path = Path::new("/sys/devices/system/cpu/intel_pstate/no_turbo");
        if intel_path.exists() {
            let value = if enabled { "0" } else { "1" };
            debug!("Setting Intel turbo boost: {}", enabled);
            fs::write(intel_path, value)?;
            return Ok(());
        }

        // AMD: /sys/devices/system/cpu/cpufreq/boost
        let amd_path = Path::new("/sys/devices/system/cpu/cpufreq/boost");
        if amd_path.exists() {
            let value = if enabled { "1" } else { "0" };
            debug!("Setting AMD boost: {}", enabled);
            fs::write(amd_path, value)?;
            return Ok(());
        }

        debug!("Turbo boost control not available");
        Ok(())
    }

    /// Set PCI power management
    fn set_pci_pm(&self, enabled: bool) -> Result<()> {
        let pci_path = Path::new("/sys/bus/pci/devices");

        if !pci_path.exists() {
            return Ok(());
        }

        let control_value = if enabled { "auto" } else { "on" };

        for entry in fs::read_dir(pci_path)? {
            let entry = entry?;
            let power_control = entry.path().join("power/control");

            if power_control.exists() {
                let _ = fs::write(&power_control, control_value);
            }
        }

        debug!("Set PCI PM to: {}", control_value);
        Ok(())
    }

    /// Set USB autosuspend
    fn set_usb_autosuspend(&self, timeout_secs: u32) -> Result<()> {
        let usb_path = Path::new("/sys/bus/usb/devices");

        if !usb_path.exists() {
            return Ok(());
        }

        let autosuspend = if timeout_secs > 0 {
            timeout_secs.to_string()
        } else {
            "-1".to_string() // Disable autosuspend
        };

        for entry in fs::read_dir(usb_path)? {
            let entry = entry?;
            let autosuspend_path = entry.path().join("power/autosuspend");

            if autosuspend_path.exists() {
                let _ = fs::write(&autosuspend_path, &autosuspend);
            }
        }

        debug!("Set USB autosuspend to: {}", autosuspend);
        Ok(())
    }

    /// Get profile status
    pub fn get_status(&self) -> ProfileStatus {
        ProfileStatus {
            current: self.current_profile.clone(),
            available: self.config.profiles.iter().map(|p| p.name.clone()).collect(),
        }
    }
}

/// Profile status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileStatus {
    /// Current profile name
    pub current: String,
    /// Available profile names
    pub available: Vec<String>,
}
