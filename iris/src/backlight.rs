//! Backlight and brightness control

use crate::config::BacklightConfig;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, info, warn};

/// Backlight device information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacklightInfo {
    /// Device name
    pub name: String,
    /// Device type
    pub device_type: String,
    /// Current brightness
    pub brightness: u32,
    /// Maximum brightness
    pub max_brightness: u32,
    /// Brightness percentage
    pub percent: u8,
}

/// Backlight manager
pub struct BacklightManager {
    config: BacklightConfig,
    devices: Vec<PathBuf>,
    current_device: Option<PathBuf>,
}

impl BacklightManager {
    /// Create new backlight manager
    pub fn new(config: BacklightConfig) -> Self {
        let mut manager = Self {
            config,
            devices: Vec::new(),
            current_device: None,
        };
        manager.detect_devices();
        manager
    }

    /// Detect backlight devices
    fn detect_devices(&mut self) {
        let backlight_path = Path::new("/sys/class/backlight");

        if !backlight_path.exists() {
            warn!("No backlight sysfs directory found");
            return;
        }

        if let Ok(entries) = fs::read_dir(backlight_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.join("brightness").exists() && path.join("max_brightness").exists() {
                    self.devices.push(path);
                }
            }
        }

        // Try to use configured device first
        let config_device = Path::new(&self.config.device);
        if config_device.exists() {
            self.current_device = Some(config_device.to_path_buf());
        } else if !self.devices.is_empty() {
            // Prefer intel_backlight, then acpi_video, then first available
            self.current_device = self
                .devices
                .iter()
                .find(|d| d.file_name().map(|n| n == "intel_backlight").unwrap_or(false))
                .or_else(|| {
                    self.devices
                        .iter()
                        .find(|d| d.file_name().map(|n| n.to_string_lossy().contains("acpi")).unwrap_or(false))
                })
                .or(self.devices.first())
                .cloned();
        }

        debug!(
            "Detected {} backlight devices, using: {:?}",
            self.devices.len(),
            self.current_device
        );
    }

    /// Get current backlight info
    pub fn get_info(&self) -> Option<BacklightInfo> {
        let device = self.current_device.as_ref()?;
        let name = device.file_name()?.to_string_lossy().to_string();

        let brightness = self.read_value(device, "brightness")?;
        let max_brightness = self.read_value(device, "max_brightness")?;
        let device_type = fs::read_to_string(device.join("type"))
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|_| "unknown".to_string());

        let percent = if max_brightness > 0 {
            ((brightness as f64 / max_brightness as f64) * 100.0) as u8
        } else {
            0
        };

        Some(BacklightInfo {
            name,
            device_type,
            brightness,
            max_brightness,
            percent,
        })
    }

    /// List all backlight devices
    pub fn list_devices(&self) -> Vec<BacklightInfo> {
        self.devices
            .iter()
            .filter_map(|device| {
                let name = device.file_name()?.to_string_lossy().to_string();
                let brightness = self.read_value(device, "brightness")?;
                let max_brightness = self.read_value(device, "max_brightness")?;
                let device_type = fs::read_to_string(device.join("type"))
                    .map(|s| s.trim().to_string())
                    .unwrap_or_else(|_| "unknown".to_string());

                let percent = if max_brightness > 0 {
                    ((brightness as f64 / max_brightness as f64) * 100.0) as u8
                } else {
                    0
                };

                Some(BacklightInfo {
                    name,
                    device_type,
                    brightness,
                    max_brightness,
                    percent,
                })
            })
            .collect()
    }

    /// Get current brightness percentage
    pub fn get_brightness(&self) -> Option<u8> {
        self.get_info().map(|i| i.percent)
    }

    /// Set brightness percentage
    pub async fn set_brightness(&self, percent: u8) -> Result<()> {
        let device = self
            .current_device
            .as_ref()
            .ok_or_else(|| anyhow!("No backlight device available"))?;

        // Clamp to minimum brightness
        let percent = percent.max(self.config.min_brightness);

        let max_brightness = self
            .read_value(device, "max_brightness")
            .ok_or_else(|| anyhow!("Failed to read max_brightness"))?;

        let target_value = ((percent as f64 / 100.0) * max_brightness as f64) as u32;

        if self.config.smooth_transitions {
            self.smooth_transition(device, target_value).await?;
        } else {
            self.write_brightness(device, target_value)?;
        }

        info!("Set brightness to {}%", percent);
        Ok(())
    }

    /// Increase brightness
    pub async fn increase_brightness(&self, step: u8) -> Result<u8> {
        let current = self.get_brightness().unwrap_or(50);
        let new = (current as u16 + step as u16).min(100) as u8;
        self.set_brightness(new).await?;
        Ok(new)
    }

    /// Decrease brightness
    pub async fn decrease_brightness(&self, step: u8) -> Result<u8> {
        let current = self.get_brightness().unwrap_or(50);
        let new = (current as i16 - step as i16).max(self.config.min_brightness as i16) as u8;
        self.set_brightness(new).await?;
        Ok(new)
    }

    /// Smooth transition to target brightness
    async fn smooth_transition(&self, device: &Path, target: u32) -> Result<()> {
        let current = self
            .read_value(device, "brightness")
            .ok_or_else(|| anyhow!("Failed to read brightness"))?;

        if current == target {
            return Ok(());
        }

        let steps = 20; // Number of steps
        let step_duration = Duration::from_millis(self.config.transition_ms as u64 / steps);
        let diff = target as i64 - current as i64;
        let step_size = diff as f64 / steps as f64;

        for i in 1..=steps {
            let value = (current as f64 + step_size * i as f64) as u32;
            self.write_brightness(device, value)?;
            sleep(step_duration).await;
        }

        // Ensure we hit exact target
        self.write_brightness(device, target)?;

        Ok(())
    }

    /// Read a sysfs value
    fn read_value(&self, device: &Path, name: &str) -> Option<u32> {
        fs::read_to_string(device.join(name))
            .ok()?
            .trim()
            .parse()
            .ok()
    }

    /// Write brightness value
    fn write_brightness(&self, device: &Path, value: u32) -> Result<()> {
        fs::write(device.join("brightness"), value.to_string())?;
        Ok(())
    }
}
