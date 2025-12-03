//! System status monitoring for Nyx Shell

use crate::messages::{
    AudioStatus, BatteryStatus, BluetoothStatus, ConnectionType, NetworkStatus, PowerProfile,
};
use sysinfo::{System, SystemExt, CpuExt};

/// System status manager
pub struct SystemStatus {
    /// sysinfo system instance
    system: System,
    /// Battery status
    pub battery: BatteryStatus,
    /// Network status
    pub network: NetworkStatus,
    /// Audio status
    pub audio: AudioStatus,
    /// Bluetooth status
    pub bluetooth: BluetoothStatus,
    /// Power profile
    pub power_profile: PowerProfile,
    /// CPU usage (0-100)
    pub cpu_usage: f32,
    /// Memory usage (0-100)
    pub memory_usage: f32,
}

impl Default for SystemStatus {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemStatus {
    /// Create a new system status monitor
    pub fn new() -> Self {
        let mut system = System::new_all();
        system.refresh_all();

        Self {
            system,
            battery: BatteryStatus::default(),
            network: NetworkStatus::default(),
            audio: AudioStatus {
                volume: 50,
                muted: false,
                output_device: Some("Built-in Speakers".to_string()),
                mic_active: false,
                mic_muted: false,
            },
            bluetooth: BluetoothStatus::default(),
            power_profile: PowerProfile::Balanced,
            cpu_usage: 0.0,
            memory_usage: 0.0,
        }
    }

    /// Refresh all system status
    pub fn refresh(&mut self) {
        self.system.refresh_all();
        self.update_cpu();
        self.update_memory();
        self.update_battery();
        self.update_network();
    }

    fn update_cpu(&mut self) {
        let cpu_usage: f32 = self.system.cpus().iter().map(|cpu| cpu.cpu_usage()).sum();
        let cpu_count = self.system.cpus().len() as f32;
        self.cpu_usage = if cpu_count > 0.0 {
            cpu_usage / cpu_count
        } else {
            0.0
        };
    }

    fn update_memory(&mut self) {
        let total = self.system.total_memory();
        let used = self.system.used_memory();
        self.memory_usage = if total > 0 {
            (used as f32 / total as f32) * 100.0
        } else {
            0.0
        };
    }

    fn update_battery(&mut self) {
        // In a real implementation, this would read from /sys/class/power_supply
        // For now, simulate battery status
        self.battery = BatteryStatus {
            percentage: 85,
            charging: false,
            plugged: true,
            time_remaining: Some(180),
        };
    }

    fn update_network(&mut self) {
        // In a real implementation, this would check network interfaces
        // For now, simulate network status
        self.network = NetworkStatus {
            connected: true,
            connection_type: ConnectionType::Wifi,
            ssid: Some("Nyx-Network".to_string()),
            signal_strength: 75,
            vpn_active: false,
        };
    }

    /// Get formatted uptime string
    pub fn uptime_string(&self) -> String {
        let uptime = System::uptime();
        let hours = uptime / 3600;
        let minutes = (uptime % 3600) / 60;

        if hours > 0 {
            format!("{}h {}m", hours, minutes)
        } else {
            format!("{}m", minutes)
        }
    }

    /// Get system hostname
    pub fn hostname(&self) -> String {
        System::host_name().unwrap_or_else(|| "nyx".to_string())
    }

    /// Get OS name
    pub fn os_name(&self) -> String {
        System::name().unwrap_or_else(|| "Nyx OS".to_string())
    }

    /// Get kernel version
    pub fn kernel_version(&self) -> String {
        System::kernel_version().unwrap_or_else(|| "unknown".to_string())
    }
}

/// Format bytes to human-readable string
pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    const TB: u64 = GB * 1024;

    if bytes >= TB {
        format!("{:.1} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}
