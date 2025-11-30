//! Audio device management

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use tracing::{info, warn, debug};

/// Audio device type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeviceType {
    /// Playback device (sink)
    Playback,
    /// Capture device (source)
    Capture,
    /// Both playback and capture
    Duplex,
}

impl std::fmt::Display for DeviceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeviceType::Playback => write!(f, "playback"),
            DeviceType::Capture => write!(f, "capture"),
            DeviceType::Duplex => write!(f, "duplex"),
        }
    }
}

/// Device state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeviceState {
    /// Device is available
    Available,
    /// Device is in use
    Active,
    /// Device is suspended
    Suspended,
    /// Device is unavailable (unplugged)
    Unavailable,
}

impl std::fmt::Display for DeviceState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeviceState::Available => write!(f, "available"),
            DeviceState::Active => write!(f, "active"),
            DeviceState::Suspended => write!(f, "suspended"),
            DeviceState::Unavailable => write!(f, "unavailable"),
        }
    }
}

/// Audio device information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioDevice {
    /// Device name (e.g., "hw:0,0")
    pub name: String,
    /// Display name
    pub description: String,
    /// Device type
    pub device_type: DeviceType,
    /// Current state
    pub state: DeviceState,
    /// ALSA card index
    pub card_index: Option<u32>,
    /// ALSA device index
    pub device_index: Option<u32>,
    /// Supported sample rates
    pub sample_rates: Vec<u32>,
    /// Supported channel counts
    pub channels: Vec<u32>,
    /// Device form factor
    pub form_factor: FormFactor,
    /// Is this a Bluetooth device
    pub is_bluetooth: bool,
    /// Is this a network device
    pub is_network: bool,
}

impl AudioDevice {
    pub fn new(name: &str, device_type: DeviceType) -> Self {
        Self {
            name: name.to_string(),
            description: name.to_string(),
            device_type,
            state: DeviceState::Available,
            card_index: None,
            device_index: None,
            sample_rates: vec![44100, 48000],
            channels: vec![2],
            form_factor: FormFactor::Internal,
            is_bluetooth: false,
            is_network: false,
        }
    }
}

/// Device form factor
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FormFactor {
    Internal,
    Speaker,
    Headphones,
    Headset,
    Handset,
    Webcam,
    Microphone,
    Tv,
    Portables,
}

/// Device manager
pub struct DeviceManager {
    devices: HashMap<String, AudioDevice>,
    default_sink: Option<String>,
    default_source: Option<String>,
}

impl DeviceManager {
    pub fn new() -> Result<Self> {
        Ok(Self {
            devices: HashMap::new(),
            default_sink: None,
            default_source: None,
        })
    }

    /// Enumerate all audio devices
    pub fn enumerate(&mut self) -> Result<()> {
        self.devices.clear();

        // Enumerate ALSA devices
        self.enumerate_alsa()?;

        // Set defaults if not set
        if self.default_sink.is_none() {
            if let Some(dev) = self.playback_devices().next() {
                self.default_sink = Some(dev.name.clone());
            }
        }

        if self.default_source.is_none() {
            if let Some(dev) = self.capture_devices().next() {
                self.default_source = Some(dev.name.clone());
            }
        }

        Ok(())
    }

    /// Enumerate ALSA devices
    fn enumerate_alsa(&mut self) -> Result<()> {
        // Read ALSA cards from /proc/asound/cards
        let cards_path = Path::new("/proc/asound/cards");
        if !cards_path.exists() {
            warn!("ALSA not available (no /proc/asound/cards)");
            return Ok(());
        }

        let content = std::fs::read_to_string(cards_path)?;

        for line in content.lines() {
            // Format: " 0 [CardName      ]: Driver - Description"
            if line.starts_with(' ') && !line.trim().is_empty() {
                if let Some(card_info) = parse_alsa_card(line) {
                    debug!("Found ALSA card {}: {}", card_info.0, card_info.1);

                    // Add playback device
                    let playback_name = format!("alsa_output.pci-{}.analog-stereo", card_info.0);
                    let mut playback = AudioDevice::new(&playback_name, DeviceType::Playback);
                    playback.description = format!("{} (Analog Output)", card_info.1);
                    playback.card_index = Some(card_info.0);
                    playback.device_index = Some(0);
                    self.devices.insert(playback_name, playback);

                    // Add capture device
                    let capture_name = format!("alsa_input.pci-{}.analog-stereo", card_info.0);
                    let mut capture = AudioDevice::new(&capture_name, DeviceType::Capture);
                    capture.description = format!("{} (Analog Input)", card_info.1);
                    capture.card_index = Some(card_info.0);
                    capture.device_index = Some(0);
                    self.devices.insert(capture_name, capture);
                }
            }
        }

        // Add null device for testing
        let null_sink = AudioDevice::new("null", DeviceType::Playback);
        self.devices.insert("null".to_string(), null_sink);

        Ok(())
    }

    /// Get a device by name
    pub fn get(&self, name: &str) -> Option<&AudioDevice> {
        self.devices.get(name)
    }

    /// Get all devices
    pub fn all(&self) -> impl Iterator<Item = &AudioDevice> {
        self.devices.values()
    }

    /// Get playback devices (sinks)
    pub fn playback_devices(&self) -> impl Iterator<Item = &AudioDevice> {
        self.devices.values()
            .filter(|d| matches!(d.device_type, DeviceType::Playback | DeviceType::Duplex))
    }

    /// Get capture devices (sources)
    pub fn capture_devices(&self) -> impl Iterator<Item = &AudioDevice> {
        self.devices.values()
            .filter(|d| matches!(d.device_type, DeviceType::Capture | DeviceType::Duplex))
    }

    /// Get device count
    pub fn device_count(&self) -> usize {
        self.devices.len()
    }

    /// Get default sink
    pub fn default_sink(&self) -> Option<&str> {
        self.default_sink.as_deref()
    }

    /// Set default sink
    pub fn set_default_sink(&mut self, name: &str) -> Result<()> {
        if self.devices.contains_key(name) {
            self.default_sink = Some(name.to_string());
            info!("Default sink set to {}", name);
            Ok(())
        } else {
            Err(anyhow!("Device not found: {}", name))
        }
    }

    /// Get default source
    pub fn default_source(&self) -> Option<&str> {
        self.default_source.as_deref()
    }

    /// Set default source
    pub fn set_default_source(&mut self, name: &str) -> Result<()> {
        if self.devices.contains_key(name) {
            self.default_source = Some(name.to_string());
            info!("Default source set to {}", name);
            Ok(())
        } else {
            Err(anyhow!("Device not found: {}", name))
        }
    }

    /// Update device state
    pub fn set_state(&mut self, name: &str, state: DeviceState) {
        if let Some(device) = self.devices.get_mut(name) {
            device.state = state;
        }
    }

    /// Add a device (e.g., from hotplug)
    pub fn add_device(&mut self, device: AudioDevice) {
        info!("Device added: {} ({})", device.name, device.description);
        self.devices.insert(device.name.clone(), device);
    }

    /// Remove a device (e.g., from unplug)
    pub fn remove_device(&mut self, name: &str) {
        if let Some(device) = self.devices.remove(name) {
            info!("Device removed: {} ({})", device.name, device.description);

            // Update defaults if needed
            if self.default_sink.as_deref() == Some(name) {
                self.default_sink = self.playback_devices().next().map(|d| d.name.clone());
            }
            if self.default_source.as_deref() == Some(name) {
                self.default_source = self.capture_devices().next().map(|d| d.name.clone());
            }
        }
    }
}

impl Default for DeviceManager {
    fn default() -> Self {
        Self::new().expect("Failed to create device manager")
    }
}

/// Parse ALSA card line
fn parse_alsa_card(line: &str) -> Option<(u32, String)> {
    // Format: " 0 [CardName      ]: Driver - Description"
    let trimmed = line.trim();

    let card_num: u32 = trimmed.split_whitespace().next()?.parse().ok()?;

    // Extract description after ": "
    let desc_start = trimmed.find(':')?;
    let description = trimmed[desc_start + 1..].trim();

    // Get just the driver/description part
    let desc = if let Some(dash_pos) = description.find(" - ") {
        description[dash_pos + 3..].to_string()
    } else {
        description.to_string()
    };

    Some((card_num, desc))
}
