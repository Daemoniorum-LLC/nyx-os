//! Device representation and database

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

/// Device information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    /// Sysfs path (e.g., /sys/devices/pci0000:00/...)
    pub syspath: String,
    /// Device path relative to /sys/devices
    pub devpath: String,
    /// Subsystem (e.g., block, net, input)
    pub subsystem: Option<String>,
    /// Device type
    pub devtype: Option<String>,
    /// Device node path (e.g., /dev/sda)
    pub devnode: Option<String>,
    /// Major number
    pub major: Option<u32>,
    /// Minor number
    pub minor: Option<u32>,
    /// Driver name
    pub driver: Option<String>,
    /// Kernel name (e.g., sda, eth0)
    pub sysname: String,
    /// Device number
    pub devnum: Option<u64>,
    /// Parent device path
    pub parent: Option<String>,
    /// Device properties
    pub properties: HashMap<String, String>,
    /// Sysfs attributes
    pub attributes: HashMap<String, String>,
    /// Tags
    pub tags: Vec<String>,
}

impl Device {
    /// Create device from sysfs path
    pub fn from_syspath(syspath: &str) -> Result<Self> {
        let path = Path::new(syspath);

        if !path.exists() {
            return Err(anyhow!("Device path does not exist: {}", syspath));
        }

        // Extract devpath (relative to /sys/devices)
        let devpath = if syspath.starts_with("/sys/devices") {
            syspath["/sys/devices".len()..].to_string()
        } else if syspath.starts_with("/sys") {
            syspath["/sys".len()..].to_string()
        } else {
            syspath.to_string()
        };

        // Extract sysname (last component)
        let sysname = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        let mut device = Device {
            syspath: syspath.to_string(),
            devpath,
            subsystem: None,
            devtype: None,
            devnode: None,
            major: None,
            minor: None,
            driver: None,
            sysname,
            devnum: None,
            parent: None,
            properties: HashMap::new(),
            attributes: HashMap::new(),
            tags: Vec::new(),
        };

        // Read subsystem
        let subsystem_link = path.join("subsystem");
        if subsystem_link.exists() {
            if let Ok(target) = std::fs::read_link(&subsystem_link) {
                if let Some(name) = target.file_name().and_then(|n| n.to_str()) {
                    device.subsystem = Some(name.to_string());
                }
            }
        }

        // Read device type
        device.devtype = read_sysfs_attr(path, "devtype");

        // Read driver
        let driver_link = path.join("driver");
        if driver_link.exists() {
            if let Ok(target) = std::fs::read_link(&driver_link) {
                if let Some(name) = target.file_name().and_then(|n| n.to_str()) {
                    device.driver = Some(name.to_string());
                }
            }
        }

        // Read device node info from uevent
        if let Some(uevent) = read_sysfs_attr(path, "uevent") {
            for line in uevent.lines() {
                if let Some((key, value)) = line.split_once('=') {
                    device.properties.insert(key.to_string(), value.to_string());

                    match key {
                        "MAJOR" => device.major = value.parse().ok(),
                        "MINOR" => device.minor = value.parse().ok(),
                        "DEVNAME" => device.devnode = Some(format!("/dev/{}", value)),
                        "DEVTYPE" => device.devtype = Some(value.to_string()),
                        _ => {}
                    }
                }
            }
        }

        // Calculate devnum
        if let (Some(major), Some(minor)) = (device.major, device.minor) {
            device.devnum = Some(makedev(major, minor));
        }

        // Read common attributes
        for attr in ["vendor", "device", "model", "serial", "idVendor", "idProduct"] {
            if let Some(value) = read_sysfs_attr(path, attr) {
                device.attributes.insert(attr.to_string(), value);
            }
        }

        Ok(device)
    }

    /// Get a property value
    pub fn property(&self, key: &str) -> Option<&str> {
        self.properties.get(key).map(|s| s.as_str())
    }

    /// Get an attribute value
    pub fn attribute(&self, key: &str) -> Option<&str> {
        self.attributes.get(key).map(|s| s.as_str())
    }

    /// Check if device matches a filter
    pub fn matches(&self, filter: &DeviceFilter) -> bool {
        if let Some(subsystem) = &filter.subsystem {
            if self.subsystem.as_ref() != Some(subsystem) {
                return false;
            }
        }

        if let Some(devtype) = &filter.devtype {
            if self.devtype.as_ref() != Some(devtype) {
                return false;
            }
        }

        if let Some(driver) = &filter.driver {
            if self.driver.as_ref() != Some(driver) {
                return false;
            }
        }

        for (key, pattern) in &filter.attributes {
            match self.attributes.get(key) {
                Some(value) if pattern_matches(value, pattern) => {}
                _ => return false,
            }
        }

        for (key, pattern) in &filter.properties {
            match self.properties.get(key) {
                Some(value) if pattern_matches(value, pattern) => {}
                _ => return false,
            }
        }

        true
    }

    /// Add a tag
    pub fn add_tag(&mut self, tag: &str) {
        if !self.tags.contains(&tag.to_string()) {
            self.tags.push(tag.to_string());
        }
    }

    /// Check if device has tag
    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.contains(&tag.to_string())
    }
}

/// Device filter for queries
#[derive(Debug, Clone, Default)]
pub struct DeviceFilter {
    pub subsystem: Option<String>,
    pub devtype: Option<String>,
    pub driver: Option<String>,
    pub attributes: HashMap<String, String>,
    pub properties: HashMap<String, String>,
    pub tags: Vec<String>,
}

impl DeviceFilter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn subsystem(mut self, subsystem: &str) -> Self {
        self.subsystem = Some(subsystem.to_string());
        self
    }

    pub fn devtype(mut self, devtype: &str) -> Self {
        self.devtype = Some(devtype.to_string());
        self
    }

    pub fn attribute(mut self, key: &str, pattern: &str) -> Self {
        self.attributes.insert(key.to_string(), pattern.to_string());
        self
    }
}

/// Device database
pub struct DeviceDatabase {
    devices: HashMap<String, Device>,
}

impl DeviceDatabase {
    pub fn new() -> Self {
        Self {
            devices: HashMap::new(),
        }
    }

    /// Enumerate all devices from sysfs
    pub fn enumerate(&mut self) -> Result<usize> {
        self.devices.clear();
        let mut count = 0;

        // Walk /sys/devices
        if let Err(e) = self.walk_sysfs("/sys/devices") {
            warn!("Error walking /sys/devices: {}", e);
        }

        count = self.devices.len();

        // Also enumerate virtual devices
        if let Err(e) = self.walk_sysfs("/sys/class") {
            warn!("Error walking /sys/class: {}", e);
        }

        Ok(count)
    }

    fn walk_sysfs(&mut self, base: &str) -> Result<()> {
        let base_path = Path::new(base);
        if !base_path.exists() {
            return Ok(());
        }

        self.walk_recursive(base_path)?;
        Ok(())
    }

    fn walk_recursive(&mut self, path: &Path) -> Result<()> {
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                let entry_path = entry.path();

                // Check if this is a device (has uevent)
                if entry_path.join("uevent").exists() {
                    if let Ok(device) = Device::from_syspath(&entry_path.to_string_lossy()) {
                        debug!("Found device: {}", device.syspath);
                        self.devices.insert(device.syspath.clone(), device);
                    }
                }

                // Recurse into directories (but not symlinks)
                if entry_path.is_dir() && !entry_path.is_symlink() {
                    let _ = self.walk_recursive(&entry_path);
                }
            }
        }

        Ok(())
    }

    /// Add a device
    pub fn add(&mut self, device: Device) {
        self.devices.insert(device.syspath.clone(), device);
    }

    /// Remove a device
    pub fn remove(&mut self, syspath: &str) {
        self.devices.remove(syspath);
    }

    /// Get a device by syspath
    pub fn get(&self, syspath: &str) -> Option<&Device> {
        self.devices.get(syspath)
    }

    /// Get all devices
    pub fn all(&self) -> impl Iterator<Item = &Device> {
        self.devices.values()
    }

    /// Find devices matching a filter
    pub fn find(&self, filter: &DeviceFilter) -> Vec<&Device> {
        self.devices.values()
            .filter(|d| d.matches(filter))
            .collect()
    }

    /// Get devices by subsystem
    pub fn by_subsystem(&self, subsystem: &str) -> Vec<&Device> {
        self.devices.values()
            .filter(|d| d.subsystem.as_deref() == Some(subsystem))
            .collect()
    }

    /// Get device count
    pub fn count(&self) -> usize {
        self.devices.len()
    }
}

impl Default for DeviceDatabase {
    fn default() -> Self {
        Self::new()
    }
}

/// Read a sysfs attribute
fn read_sysfs_attr(base: &Path, attr: &str) -> Option<String> {
    let path = base.join(attr);
    std::fs::read_to_string(&path)
        .ok()
        .map(|s| s.trim().to_string())
}

/// Create device number from major/minor
fn makedev(major: u32, minor: u32) -> u64 {
    ((major as u64) << 8) | (minor as u64 & 0xff) | ((minor as u64 & !0xff) << 12)
}

/// Pattern matching (supports * wildcard)
fn pattern_matches(value: &str, pattern: &str) -> bool {
    if pattern == "*" {
        return true;
    }

    if pattern.contains('*') {
        let parts: Vec<&str> = pattern.split('*').collect();

        if parts.len() == 2 {
            // prefix* or *suffix
            if parts[0].is_empty() {
                return value.ends_with(parts[1]);
            } else if parts[1].is_empty() {
                return value.starts_with(parts[0]);
            } else {
                return value.starts_with(parts[0]) && value.ends_with(parts[1]);
            }
        }
    }

    value == pattern
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pattern_matches() {
        assert!(pattern_matches("sda", "*"));
        assert!(pattern_matches("sda1", "sda*"));
        assert!(pattern_matches("input0", "*0"));
        assert!(pattern_matches("eth0", "eth0"));
        assert!(!pattern_matches("sdb", "sda*"));
    }
}
