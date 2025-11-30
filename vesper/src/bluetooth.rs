//! Bluetooth audio support

use crate::device::{AudioDevice, DeviceState, DeviceType, FormFactor};
use anyhow::{Result, anyhow};
use std::collections::HashMap;
use tracing::{info, warn, debug};

/// Bluetooth audio manager
pub struct BluetoothAudio {
    devices: HashMap<String, BluetoothDevice>,
    enabled: bool,
}

/// Bluetooth audio device
#[derive(Debug, Clone)]
pub struct BluetoothDevice {
    /// Device address (MAC)
    pub address: String,
    /// Device name
    pub name: String,
    /// Is connected
    pub connected: bool,
    /// Supported profiles
    pub profiles: Vec<BluetoothProfile>,
    /// Current profile
    pub active_profile: Option<BluetoothProfile>,
    /// Battery level (if available)
    pub battery: Option<u8>,
}

/// Bluetooth audio profiles
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BluetoothProfile {
    /// Advanced Audio Distribution Profile (high quality playback)
    A2dpSink,
    /// A2DP Source (streaming from device)
    A2dpSource,
    /// Hands-Free Profile (calls)
    Hfp,
    /// Headset Profile
    Hsp,
}

impl BluetoothProfile {
    pub fn is_high_quality(&self) -> bool {
        matches!(self, BluetoothProfile::A2dpSink | BluetoothProfile::A2dpSource)
    }

    pub fn supports_microphone(&self) -> bool {
        matches!(self, BluetoothProfile::Hfp | BluetoothProfile::Hsp)
    }
}

impl BluetoothAudio {
    pub fn new() -> Result<Self> {
        // Check if BlueZ is available
        if !std::path::Path::new("/org/bluez").exists() &&
           !std::path::Path::new("/run/dbus").exists() {
            return Err(anyhow!("BlueZ not available"));
        }

        Ok(Self {
            devices: HashMap::new(),
            enabled: true,
        })
    }

    /// Scan for Bluetooth audio devices
    pub fn scan(&mut self) -> Result<Vec<BluetoothDevice>> {
        debug!("Scanning for Bluetooth audio devices");

        // In practice, would use BlueZ D-Bus API
        // For now, return empty list

        Ok(self.devices.values().cloned().collect())
    }

    /// Connect to a device
    pub fn connect(&mut self, address: &str) -> Result<()> {
        debug!("Connecting to Bluetooth device: {}", address);

        if let Some(device) = self.devices.get_mut(address) {
            device.connected = true;
            info!("Connected to Bluetooth device: {}", device.name);
            Ok(())
        } else {
            Err(anyhow!("Device not found: {}", address))
        }
    }

    /// Disconnect from a device
    pub fn disconnect(&mut self, address: &str) -> Result<()> {
        debug!("Disconnecting from Bluetooth device: {}", address);

        if let Some(device) = self.devices.get_mut(address) {
            device.connected = false;
            device.active_profile = None;
            info!("Disconnected from Bluetooth device: {}", device.name);
            Ok(())
        } else {
            Err(anyhow!("Device not found: {}", address))
        }
    }

    /// Set active profile
    pub fn set_profile(&mut self, address: &str, profile: BluetoothProfile) -> Result<()> {
        if let Some(device) = self.devices.get_mut(address) {
            if device.profiles.contains(&profile) {
                device.active_profile = Some(profile);
                info!("Set profile {:?} for {}", profile, device.name);
                Ok(())
            } else {
                Err(anyhow!("Profile not supported"))
            }
        } else {
            Err(anyhow!("Device not found: {}", address))
        }
    }

    /// Get connected devices
    pub fn connected_devices(&self) -> impl Iterator<Item = &BluetoothDevice> {
        self.devices.values().filter(|d| d.connected)
    }

    /// Get all devices
    pub fn all_devices(&self) -> impl Iterator<Item = &BluetoothDevice> {
        self.devices.values()
    }

    /// Add a discovered device
    pub fn add_device(&mut self, device: BluetoothDevice) {
        info!("Discovered Bluetooth device: {} ({})", device.name, device.address);
        self.devices.insert(device.address.clone(), device);
    }

    /// Remove a device
    pub fn remove_device(&mut self, address: &str) {
        if let Some(device) = self.devices.remove(address) {
            info!("Removed Bluetooth device: {}", device.name);
        }
    }

    /// Convert to AudioDevice for Vesper
    pub fn to_audio_device(device: &BluetoothDevice) -> Option<AudioDevice> {
        if !device.connected {
            return None;
        }

        let device_type = if device.active_profile.map(|p| p.supports_microphone()).unwrap_or(false) {
            DeviceType::Duplex
        } else {
            DeviceType::Playback
        };

        let mut audio_dev = AudioDevice::new(
            &format!("bluez_sink.{}", device.address.replace(':', "_")),
            device_type,
        );

        audio_dev.description = device.name.clone();
        audio_dev.is_bluetooth = true;
        audio_dev.form_factor = FormFactor::Headphones;
        audio_dev.state = DeviceState::Active;

        // Bluetooth typically supports these rates
        audio_dev.sample_rates = if device.active_profile.map(|p| p.is_high_quality()).unwrap_or(false) {
            vec![44100, 48000]
        } else {
            vec![8000, 16000] // HFP/HSP
        };

        Some(audio_dev)
    }

    /// Enable Bluetooth audio
    pub fn enable(&mut self) {
        self.enabled = true;
        info!("Bluetooth audio enabled");
    }

    /// Disable Bluetooth audio
    pub fn disable(&mut self) {
        self.enabled = false;
        // Disconnect all devices
        for device in self.devices.values_mut() {
            device.connected = false;
            device.active_profile = None;
        }
        info!("Bluetooth audio disabled");
    }

    /// Check if enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

/// Bluetooth codec information
#[derive(Debug, Clone, Copy)]
pub enum BluetoothCodec {
    /// SBC (mandatory baseline)
    Sbc,
    /// AAC (Apple devices)
    Aac,
    /// aptX (Qualcomm)
    AptX,
    /// aptX HD
    AptXHd,
    /// LDAC (Sony)
    Ldac,
}

impl BluetoothCodec {
    pub fn bitrate(&self) -> u32 {
        match self {
            BluetoothCodec::Sbc => 328,
            BluetoothCodec::Aac => 256,
            BluetoothCodec::AptX => 384,
            BluetoothCodec::AptXHd => 576,
            BluetoothCodec::Ldac => 990,
        }
    }
}
