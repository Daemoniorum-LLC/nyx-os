//! Display management

use crate::config::DisplaysConfig;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use tracing::{debug, info, warn};

/// Display mode
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayMode {
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
    /// Refresh rate in Hz
    pub refresh: f32,
    /// Is this the preferred mode
    pub preferred: bool,
    /// Is this the current mode
    pub current: bool,
}

/// Display connection type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionType {
    Unknown,
    VGA,
    DVI,
    HDMI,
    DisplayPort,
    LVDS,
    EDP,
    Virtual,
}

/// Display connection status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionStatus {
    Connected,
    Disconnected,
    Unknown,
}

/// Display information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayInfo {
    /// Display name/identifier
    pub name: String,
    /// Connection type
    pub connection: ConnectionType,
    /// Connection status
    pub status: ConnectionStatus,
    /// Is primary display
    pub primary: bool,
    /// Is enabled
    pub enabled: bool,
    /// Current mode (if enabled)
    pub current_mode: Option<DisplayMode>,
    /// Available modes
    pub modes: Vec<DisplayMode>,
    /// Physical size in mm (width, height)
    pub physical_size: Option<(u32, u32)>,
    /// Position (x, y)
    pub position: (i32, i32),
    /// Rotation in degrees
    pub rotation: u16,
    /// Scale factor
    pub scale: f32,
    /// EDID info
    pub edid: Option<EdidInfo>,
}

/// EDID information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdidInfo {
    /// Manufacturer ID
    pub manufacturer: String,
    /// Product name
    pub product_name: Option<String>,
    /// Serial number
    pub serial: Option<String>,
}

/// Display manager
pub struct DisplayManager {
    config: DisplaysConfig,
    displays: HashMap<String, DisplayInfo>,
}

impl DisplayManager {
    /// Create new display manager
    pub fn new(config: DisplaysConfig) -> Self {
        Self {
            config,
            displays: HashMap::new(),
        }
    }

    /// Detect connected displays
    pub fn detect(&mut self) -> Result<()> {
        self.displays.clear();

        // Try DRM first
        if let Ok(displays) = self.detect_drm() {
            for display in displays {
                self.displays.insert(display.name.clone(), display);
            }
            return Ok(());
        }

        // Fallback to sysfs
        self.detect_sysfs()?;

        Ok(())
    }

    /// Detect displays via DRM
    fn detect_drm(&self) -> Result<Vec<DisplayInfo>> {
        let drm_path = Path::new("/sys/class/drm");
        let mut displays = Vec::new();

        if !drm_path.exists() {
            return Err(anyhow!("DRM not available"));
        }

        for entry in fs::read_dir(drm_path)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();

            // Look for card*-* entries (e.g., card0-HDMI-A-1)
            if !name.starts_with("card") || !name.contains('-') {
                continue;
            }

            let path = entry.path();
            let status_path = path.join("status");

            let status = if status_path.exists() {
                let status_str = fs::read_to_string(&status_path)?;
                match status_str.trim() {
                    "connected" => ConnectionStatus::Connected,
                    "disconnected" => ConnectionStatus::Disconnected,
                    _ => ConnectionStatus::Unknown,
                }
            } else {
                ConnectionStatus::Unknown
            };

            // Parse connector name (e.g., "HDMI-A-1" from "card0-HDMI-A-1")
            let connector_name = name.split('-').skip(1).collect::<Vec<_>>().join("-");
            let connection = parse_connection_type(&connector_name);

            let modes = self.parse_modes(&path.join("modes"))?;
            let enabled = path.join("enabled").exists()
                && fs::read_to_string(path.join("enabled"))
                    .map(|s| s.trim() == "enabled")
                    .unwrap_or(false);

            let current_mode = if enabled && !modes.is_empty() {
                modes.iter().find(|m| m.current).cloned()
            } else {
                None
            };

            displays.push(DisplayInfo {
                name: connector_name,
                connection,
                status,
                primary: false, // Will be set later
                enabled,
                current_mode,
                modes,
                physical_size: None, // Would need EDID parsing
                position: (0, 0),
                rotation: 0,
                scale: 1.0,
                edid: None,
            });
        }

        // Set primary display
        let primary_name = self.config.primary.as_deref();
        for display in &mut displays {
            if Some(display.name.as_str()) == primary_name {
                display.primary = true;
            }
        }

        // If no primary set, use first connected display
        if !displays.iter().any(|d| d.primary) {
            if let Some(display) = displays
                .iter_mut()
                .find(|d| d.status == ConnectionStatus::Connected)
            {
                display.primary = true;
            }
        }

        Ok(displays)
    }

    /// Parse available modes
    fn parse_modes(&self, path: &Path) -> Result<Vec<DisplayMode>> {
        let mut modes = Vec::new();

        if !path.exists() {
            return Ok(modes);
        }

        let content = fs::read_to_string(path)?;
        for (i, line) in content.lines().enumerate() {
            if let Some(mode) = parse_mode_line(line.trim()) {
                let mut mode = mode;
                mode.preferred = i == 0; // First mode is usually preferred
                modes.push(mode);
            }
        }

        Ok(modes)
    }

    /// Detect displays via sysfs (fallback)
    fn detect_sysfs(&mut self) -> Result<()> {
        let backlight_path = Path::new("/sys/class/backlight");

        if backlight_path.exists() {
            for entry in fs::read_dir(backlight_path)? {
                let entry = entry?;
                let name = entry.file_name().to_string_lossy().to_string();

                // Create a basic display entry for internal panels
                self.displays.insert(
                    name.clone(),
                    DisplayInfo {
                        name,
                        connection: ConnectionType::EDP,
                        status: ConnectionStatus::Connected,
                        primary: true,
                        enabled: true,
                        current_mode: None,
                        modes: Vec::new(),
                        physical_size: None,
                        position: (0, 0),
                        rotation: 0,
                        scale: 1.0,
                        edid: None,
                    },
                );
            }
        }

        Ok(())
    }

    /// Get all displays
    pub fn list(&self) -> Vec<&DisplayInfo> {
        self.displays.values().collect()
    }

    /// Get display by name
    pub fn get(&self, name: &str) -> Option<&DisplayInfo> {
        self.displays.get(name)
    }

    /// Get primary display
    pub fn primary(&self) -> Option<&DisplayInfo> {
        self.displays.values().find(|d| d.primary)
    }

    /// Set display mode
    pub fn set_mode(&mut self, name: &str, width: u32, height: u32, refresh: f32) -> Result<()> {
        let display = self
            .displays
            .get_mut(name)
            .ok_or_else(|| anyhow!("Display not found: {}", name))?;

        // Find matching mode
        let mode = display
            .modes
            .iter()
            .find(|m| m.width == width && m.height == height && (m.refresh - refresh).abs() < 0.5)
            .ok_or_else(|| anyhow!("Mode not available: {}x{}@{}", width, height, refresh))?;

        info!("Setting mode {}x{}@{:.2} on {}", width, height, refresh, name);

        // In a real implementation, this would use DRM/KMS or xrandr
        // For now, just update the local state
        let mode = mode.clone();
        display.current_mode = Some(mode);

        Ok(())
    }

    /// Enable or disable a display
    pub fn set_enabled(&mut self, name: &str, enabled: bool) -> Result<()> {
        let display = self
            .displays
            .get_mut(name)
            .ok_or_else(|| anyhow!("Display not found: {}", name))?;

        info!("Setting display {} enabled={}", name, enabled);
        display.enabled = enabled;

        Ok(())
    }

    /// Set display as primary
    pub fn set_primary(&mut self, name: &str) -> Result<()> {
        if !self.displays.contains_key(name) {
            return Err(anyhow!("Display not found: {}", name));
        }

        for display in self.displays.values_mut() {
            display.primary = display.name == name;
        }

        info!("Set {} as primary display", name);
        Ok(())
    }

    /// Set display position
    pub fn set_position(&mut self, name: &str, x: i32, y: i32) -> Result<()> {
        let display = self
            .displays
            .get_mut(name)
            .ok_or_else(|| anyhow!("Display not found: {}", name))?;

        display.position = (x, y);
        info!("Set display {} position to ({}, {})", name, x, y);
        Ok(())
    }

    /// Set display rotation
    pub fn set_rotation(&mut self, name: &str, rotation: u16) -> Result<()> {
        if rotation != 0 && rotation != 90 && rotation != 180 && rotation != 270 {
            return Err(anyhow!("Invalid rotation: {}. Must be 0, 90, 180, or 270", rotation));
        }

        let display = self
            .displays
            .get_mut(name)
            .ok_or_else(|| anyhow!("Display not found: {}", name))?;

        display.rotation = rotation;
        info!("Set display {} rotation to {}°", name, rotation);
        Ok(())
    }
}

/// Parse connection type from connector name
fn parse_connection_type(name: &str) -> ConnectionType {
    let upper = name.to_uppercase();
    if upper.starts_with("HDMI") {
        ConnectionType::HDMI
    } else if upper.starts_with("DP") || upper.starts_with("DISPLAYPORT") {
        ConnectionType::DisplayPort
    } else if upper.starts_with("DVI") {
        ConnectionType::DVI
    } else if upper.starts_with("VGA") {
        ConnectionType::VGA
    } else if upper.starts_with("LVDS") {
        ConnectionType::LVDS
    } else if upper.starts_with("EDP") || upper.contains("EDP") {
        ConnectionType::EDP
    } else if upper.starts_with("VIRTUAL") {
        ConnectionType::Virtual
    } else {
        ConnectionType::Unknown
    }
}

/// Parse a mode line (e.g., "1920x1080")
fn parse_mode_line(line: &str) -> Option<DisplayMode> {
    let parts: Vec<&str> = line.split('x').collect();
    if parts.len() == 2 {
        let width: u32 = parts[0].parse().ok()?;
        let height: u32 = parts[1].parse().ok()?;
        Some(DisplayMode {
            width,
            height,
            refresh: 60.0, // Default, would need more parsing for actual refresh
            preferred: false,
            current: false,
        })
    } else {
        None
    }
}
