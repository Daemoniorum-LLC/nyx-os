//! Hardware database for device identification

use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

/// Hardware database entry
#[derive(Debug, Clone)]
pub struct HwdbEntry {
    /// Match patterns
    pub matches: Vec<String>,
    /// Properties to set
    pub properties: HashMap<String, String>,
}

/// Hardware database
pub struct Hwdb {
    entries: Vec<HwdbEntry>,
    usb_ids: HashMap<(u16, u16), String>,
    pci_ids: HashMap<(u16, u16), String>,
}

impl Hwdb {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            usb_ids: HashMap::new(),
            pci_ids: HashMap::new(),
        }
    }

    /// Load hardware database
    pub fn load(&mut self, path: &Path) -> Result<()> {
        if !path.exists() {
            return Ok(());
        }

        // Would load from /lib/udev/hwdb.bin or /etc/udev/hwdb.d/
        // For now, just set up some common entries

        self.load_usb_ids()?;
        self.load_pci_ids()?;

        Ok(())
    }

    /// Load USB IDs database
    fn load_usb_ids(&mut self) -> Result<()> {
        // Common USB vendor IDs
        let usb_vendors = [
            (0x05ac, "Apple, Inc."),
            (0x046d, "Logitech, Inc."),
            (0x8087, "Intel Corp."),
            (0x1d6b, "Linux Foundation"),
            (0x0bda, "Realtek Semiconductor Corp."),
            (0x058f, "Alcor Micro Corp."),
            (0x0951, "Kingston Technology"),
            (0x0781, "SanDisk Corp."),
            (0x18d1, "Google Inc."),
            (0x1532, "Razer USA, Ltd"),
        ];

        for (vendor, name) in usb_vendors {
            self.usb_ids.insert((vendor, 0), name.to_string());
        }

        Ok(())
    }

    /// Load PCI IDs database
    fn load_pci_ids(&mut self) -> Result<()> {
        // Common PCI vendor IDs
        let pci_vendors = [
            (0x8086, "Intel Corporation"),
            (0x10de, "NVIDIA Corporation"),
            (0x1002, "Advanced Micro Devices, Inc."),
            (0x14e4, "Broadcom Inc."),
            (0x10ec, "Realtek Semiconductor Co., Ltd."),
            (0x1b21, "ASMedia Technology Inc."),
            (0x1022, "Advanced Micro Devices, Inc. [AMD]"),
            (0x168c, "Qualcomm Atheros"),
            (0x1969, "Qualcomm Atheros"),
        ];

        for (vendor, name) in pci_vendors {
            self.pci_ids.insert((vendor, 0), name.to_string());
        }

        Ok(())
    }

    /// Lookup USB device name
    pub fn lookup_usb(&self, vendor: u16, product: u16) -> Option<&str> {
        // Try specific product first
        self.usb_ids.get(&(vendor, product))
            .or_else(|| self.usb_ids.get(&(vendor, 0)))
            .map(|s| s.as_str())
    }

    /// Lookup PCI device name
    pub fn lookup_pci(&self, vendor: u16, device: u16) -> Option<&str> {
        // Try specific device first
        self.pci_ids.get(&(vendor, device))
            .or_else(|| self.pci_ids.get(&(vendor, 0)))
            .map(|s| s.as_str())
    }

    /// Find entries matching a modalias
    pub fn match_modalias(&self, modalias: &str) -> Vec<&HwdbEntry> {
        self.entries.iter()
            .filter(|e| e.matches.iter().any(|m| modalias_match(modalias, m)))
            .collect()
    }

    /// Get properties for a device
    pub fn get_properties(&self, modalias: &str) -> HashMap<String, String> {
        let mut props = HashMap::new();

        for entry in self.match_modalias(modalias) {
            for (key, value) in &entry.properties {
                props.insert(key.clone(), value.clone());
            }
        }

        props
    }
}

impl Default for Hwdb {
    fn default() -> Self {
        Self::new()
    }
}

/// Match modalias pattern
fn modalias_match(modalias: &str, pattern: &str) -> bool {
    // Simple glob matching
    if pattern.ends_with('*') {
        let prefix = &pattern[..pattern.len() - 1];
        modalias.starts_with(prefix)
    } else {
        modalias == pattern
    }
}

/// Parse modalias to extract device info
pub fn parse_modalias(modalias: &str) -> Option<DeviceMatch> {
    if modalias.starts_with("usb:") {
        parse_usb_modalias(modalias)
    } else if modalias.starts_with("pci:") {
        parse_pci_modalias(modalias)
    } else if modalias.starts_with("input:") {
        parse_input_modalias(modalias)
    } else {
        None
    }
}

/// Parsed device match information
#[derive(Debug, Clone)]
pub enum DeviceMatch {
    Usb {
        vendor: u16,
        product: u16,
        bcd_device: Option<u16>,
        device_class: Option<u8>,
    },
    Pci {
        vendor: u16,
        device: u16,
        subvendor: Option<u16>,
        subdevice: Option<u16>,
        class: Option<u32>,
    },
    Input {
        bus: u16,
        vendor: u16,
        product: u16,
    },
}

fn parse_usb_modalias(modalias: &str) -> Option<DeviceMatch> {
    // Format: usb:vXXXXpXXXXdXXXXdcXXdscXXdpXXicXXiscXXipXXinXX
    let s = modalias.strip_prefix("usb:")?;

    let vendor = u16::from_str_radix(s.get(1..5)?, 16).ok()?;
    let product = u16::from_str_radix(s.get(6..10)?, 16).ok()?;

    Some(DeviceMatch::Usb {
        vendor,
        product,
        bcd_device: None,
        device_class: None,
    })
}

fn parse_pci_modalias(modalias: &str) -> Option<DeviceMatch> {
    // Format: pci:vXXXXXXXXdXXXXXXXXsvXXXXXXXXsdXXXXXXXXbcXXscXXiXX
    let s = modalias.strip_prefix("pci:")?;

    let vendor = u16::from_str_radix(s.get(1..9)?, 16).ok()? as u16;
    let device = u16::from_str_radix(s.get(10..18)?, 16).ok()? as u16;

    Some(DeviceMatch::Pci {
        vendor,
        device,
        subvendor: None,
        subdevice: None,
        class: None,
    })
}

fn parse_input_modalias(modalias: &str) -> Option<DeviceMatch> {
    // Format: input:bXXXXvXXXXpXXXXeXXXX...
    let s = modalias.strip_prefix("input:")?;

    let bus = u16::from_str_radix(s.get(1..5)?, 16).ok()?;
    let vendor = u16::from_str_radix(s.get(6..10)?, 16).ok()?;
    let product = u16::from_str_radix(s.get(11..15)?, 16).ok()?;

    Some(DeviceMatch::Input {
        bus,
        vendor,
        product,
    })
}
