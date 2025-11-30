//! Battery monitoring and management

use crate::config::{BatteryAction, BatteryConfig};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

/// Battery state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BatteryState {
    Unknown,
    Charging,
    Discharging,
    NotCharging,
    Full,
}

impl Default for BatteryState {
    fn default() -> Self {
        Self::Unknown
    }
}

/// Battery information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatteryInfo {
    /// Battery name
    pub name: String,
    /// Current state
    pub state: BatteryState,
    /// Capacity percentage (0-100)
    pub capacity: u8,
    /// Energy now (µWh)
    pub energy_now: Option<u64>,
    /// Energy full (µWh)
    pub energy_full: Option<u64>,
    /// Energy full design (µWh)
    pub energy_full_design: Option<u64>,
    /// Voltage now (µV)
    pub voltage_now: Option<u64>,
    /// Current now (µA, negative = discharging)
    pub current_now: Option<i64>,
    /// Power now (µW)
    pub power_now: Option<u64>,
    /// Time to empty (seconds)
    pub time_to_empty: Option<u64>,
    /// Time to full (seconds)
    pub time_to_full: Option<u64>,
    /// Cycle count
    pub cycle_count: Option<u32>,
    /// Battery health percentage
    pub health: Option<u8>,
    /// Technology (Li-ion, etc.)
    pub technology: Option<String>,
}

/// AC adapter information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcInfo {
    /// Adapter name
    pub name: String,
    /// Is online/plugged in
    pub online: bool,
}

/// Power supply status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerStatus {
    /// AC adapters
    pub ac_adapters: Vec<AcInfo>,
    /// Batteries
    pub batteries: Vec<BatteryInfo>,
    /// On AC power
    pub on_ac_power: bool,
    /// Combined battery capacity
    pub total_capacity: u8,
    /// Combined battery state
    pub combined_state: BatteryState,
}

/// Battery monitor
pub struct BatteryMonitor {
    config: BatteryConfig,
    power_supply_path: PathBuf,
    last_status: Option<PowerStatus>,
}

impl BatteryMonitor {
    /// Create new battery monitor
    pub fn new(config: BatteryConfig) -> Self {
        Self {
            config,
            power_supply_path: PathBuf::from("/sys/class/power_supply"),
            last_status: None,
        }
    }

    /// Get current power status
    pub fn get_status(&mut self) -> Result<PowerStatus> {
        let mut ac_adapters = Vec::new();
        let mut batteries = Vec::new();

        if !self.power_supply_path.exists() {
            return Ok(PowerStatus {
                ac_adapters,
                batteries,
                on_ac_power: true, // Assume AC if no power supply info
                total_capacity: 100,
                combined_state: BatteryState::Unknown,
            });
        }

        for entry in fs::read_dir(&self.power_supply_path)? {
            let entry = entry?;
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();

            let supply_type = read_sysfs_string(&path.join("type")).unwrap_or_default();

            match supply_type.trim() {
                "Mains" | "USB" => {
                    if let Some(ac) = self.read_ac_adapter(&path, &name) {
                        ac_adapters.push(ac);
                    }
                }
                "Battery" => {
                    if let Some(battery) = self.read_battery(&path, &name) {
                        batteries.push(battery);
                    }
                }
                _ => {}
            }
        }

        let on_ac_power = ac_adapters.iter().any(|ac| ac.online) || ac_adapters.is_empty();

        let (total_capacity, combined_state) = if batteries.is_empty() {
            (100, BatteryState::Unknown)
        } else {
            let total: u32 = batteries.iter().map(|b| b.capacity as u32).sum();
            let avg = (total / batteries.len() as u32) as u8;
            let state = batteries.first().map(|b| b.state).unwrap_or_default();
            (avg, state)
        };

        let status = PowerStatus {
            ac_adapters,
            batteries,
            on_ac_power,
            total_capacity,
            combined_state,
        };

        self.last_status = Some(status.clone());
        Ok(status)
    }

    /// Read AC adapter info
    fn read_ac_adapter(&self, path: &Path, name: &str) -> Option<AcInfo> {
        let online = read_sysfs_int(&path.join("online")).unwrap_or(0) == 1;

        Some(AcInfo {
            name: name.to_string(),
            online,
        })
    }

    /// Read battery info
    fn read_battery(&self, path: &Path, name: &str) -> Option<BatteryInfo> {
        let state_str = read_sysfs_string(&path.join("status")).unwrap_or_default();
        let state = match state_str.trim() {
            "Charging" => BatteryState::Charging,
            "Discharging" => BatteryState::Discharging,
            "Not charging" => BatteryState::NotCharging,
            "Full" => BatteryState::Full,
            _ => BatteryState::Unknown,
        };

        let capacity = read_sysfs_int(&path.join("capacity")).unwrap_or(0) as u8;

        let energy_now = read_sysfs_u64(&path.join("energy_now"));
        let energy_full = read_sysfs_u64(&path.join("energy_full"));
        let energy_full_design = read_sysfs_u64(&path.join("energy_full_design"));
        let voltage_now = read_sysfs_u64(&path.join("voltage_now"));
        let current_now = read_sysfs_i64(&path.join("current_now"));
        let power_now = read_sysfs_u64(&path.join("power_now"));
        let cycle_count = read_sysfs_int(&path.join("cycle_count")).map(|v| v as u32);
        let technology = read_sysfs_string(&path.join("technology")).map(|s| s.trim().to_string());

        // Calculate time estimates
        let (time_to_empty, time_to_full) = self.calculate_time_estimates(
            state,
            energy_now,
            energy_full,
            power_now,
        );

        // Calculate health
        let health = match (energy_full, energy_full_design) {
            (Some(full), Some(design)) if design > 0 => {
                Some(((full as f64 / design as f64) * 100.0) as u8)
            }
            _ => None,
        };

        Some(BatteryInfo {
            name: name.to_string(),
            state,
            capacity,
            energy_now,
            energy_full,
            energy_full_design,
            voltage_now,
            current_now,
            power_now,
            time_to_empty,
            time_to_full,
            cycle_count,
            health,
            technology,
        })
    }

    /// Calculate time estimates
    fn calculate_time_estimates(
        &self,
        state: BatteryState,
        energy_now: Option<u64>,
        energy_full: Option<u64>,
        power_now: Option<u64>,
    ) -> (Option<u64>, Option<u64>) {
        let power = match power_now {
            Some(p) if p > 0 => p,
            _ => return (None, None),
        };

        match state {
            BatteryState::Discharging => {
                if let Some(energy) = energy_now {
                    // Time in seconds = (energy_now / power_now) * 3600
                    let hours = energy as f64 / power as f64;
                    let seconds = (hours * 3600.0) as u64;
                    (Some(seconds), None)
                } else {
                    (None, None)
                }
            }
            BatteryState::Charging => {
                if let (Some(now), Some(full)) = (energy_now, energy_full) {
                    let remaining = full.saturating_sub(now);
                    let hours = remaining as f64 / power as f64;
                    let seconds = (hours * 3600.0) as u64;
                    (None, Some(seconds))
                } else {
                    (None, None)
                }
            }
            _ => (None, None),
        }
    }

    /// Check battery thresholds and return required action
    pub fn check_thresholds(&self, status: &PowerStatus) -> Option<BatteryAction> {
        if status.on_ac_power {
            return None;
        }

        if status.total_capacity <= self.config.critical_threshold {
            debug!(
                "Battery critical: {}% <= {}%",
                status.total_capacity, self.config.critical_threshold
            );
            return Some(self.config.critical_action);
        }

        if status.total_capacity <= self.config.low_threshold {
            debug!(
                "Battery low: {}% <= {}%",
                status.total_capacity, self.config.low_threshold
            );
            return Some(self.config.low_action);
        }

        None
    }

    /// Check if power source changed
    pub fn power_source_changed(&self, status: &PowerStatus) -> bool {
        match &self.last_status {
            Some(last) => last.on_ac_power != status.on_ac_power,
            None => false,
        }
    }

    /// Is on battery power
    pub fn on_battery(&self) -> bool {
        self.last_status
            .as_ref()
            .map(|s| !s.on_ac_power)
            .unwrap_or(false)
    }
}

// Helper functions for reading sysfs values
fn read_sysfs_string(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok()
}

fn read_sysfs_int(path: &Path) -> Option<i32> {
    read_sysfs_string(path)?
        .trim()
        .parse()
        .ok()
}

fn read_sysfs_u64(path: &Path) -> Option<u64> {
    read_sysfs_string(path)?
        .trim()
        .parse()
        .ok()
}

fn read_sysfs_i64(path: &Path) -> Option<i64> {
    read_sysfs_string(path)?
        .trim()
        .parse()
        .ok()
}
