//! Network interface management

use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::net::IpAddr;
use std::process::Command;

/// Network interface information
#[derive(Debug, Clone)]
pub struct NetworkInterface {
    pub name: String,
    pub index: u32,
    pub mac_address: Option<String>,
    pub ipv4_addresses: Vec<Ipv4Info>,
    pub ipv6_addresses: Vec<Ipv6Info>,
    pub flags: InterfaceFlags,
    pub mtu: u32,
    pub state: InterfaceState,
    pub stats: InterfaceStats,
}

#[derive(Debug, Clone)]
pub struct Ipv4Info {
    pub address: std::net::Ipv4Addr,
    pub netmask: std::net::Ipv4Addr,
    pub broadcast: Option<std::net::Ipv4Addr>,
}

#[derive(Debug, Clone)]
pub struct Ipv6Info {
    pub address: std::net::Ipv6Addr,
    pub prefix_len: u8,
    pub scope: Ipv6Scope,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Ipv6Scope {
    Link,
    Site,
    Global,
}

#[derive(Debug, Clone, Default)]
pub struct InterfaceFlags {
    pub up: bool,
    pub broadcast: bool,
    pub loopback: bool,
    pub point_to_point: bool,
    pub multicast: bool,
    pub promisc: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InterfaceState {
    Up,
    Down,
    Unknown,
}

#[derive(Debug, Clone, Default)]
pub struct InterfaceStats {
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub rx_packets: u64,
    pub tx_packets: u64,
    pub rx_errors: u64,
    pub tx_errors: u64,
    pub rx_dropped: u64,
    pub tx_dropped: u64,
}

/// Interface manager
pub struct InterfaceManager {
    interfaces: HashMap<String, NetworkInterface>,
}

impl InterfaceManager {
    pub fn new() -> Self {
        Self {
            interfaces: HashMap::new(),
        }
    }

    /// Refresh interface list
    pub async fn refresh(&mut self) -> Result<()> {
        self.interfaces.clear();

        // Read from /sys/class/net
        let entries = std::fs::read_dir("/sys/class/net")?;

        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();

            if let Ok(iface) = self.read_interface(&name).await {
                self.interfaces.insert(name, iface);
            }
        }

        Ok(())
    }

    async fn read_interface(&self, name: &str) -> Result<NetworkInterface> {
        let sys_path = format!("/sys/class/net/{}", name);

        // Read basic info
        let index = read_sys_file(&format!("{}/ifindex", sys_path))?
            .trim()
            .parse()
            .unwrap_or(0);

        let mtu = read_sys_file(&format!("{}/mtu", sys_path))?
            .trim()
            .parse()
            .unwrap_or(1500);

        let mac_address = read_sys_file(&format!("{}/address", sys_path))
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| s != "00:00:00:00:00:00");

        let flags_val: u32 = read_sys_file(&format!("{}/flags", sys_path))?
            .trim()
            .trim_start_matches("0x")
            .parse()
            .unwrap_or(0);

        let flags = InterfaceFlags {
            up: (flags_val & 0x1) != 0,
            broadcast: (flags_val & 0x2) != 0,
            loopback: (flags_val & 0x8) != 0,
            point_to_point: (flags_val & 0x10) != 0,
            multicast: (flags_val & 0x1000) != 0,
            promisc: (flags_val & 0x100) != 0,
        };

        let state = match read_sys_file(&format!("{}/operstate", sys_path))
            .unwrap_or_default()
            .trim()
        {
            "up" => InterfaceState::Up,
            "down" => InterfaceState::Down,
            _ => InterfaceState::Unknown,
        };

        // Read stats
        let stats = InterfaceStats {
            rx_bytes: read_stat(&sys_path, "rx_bytes"),
            tx_bytes: read_stat(&sys_path, "tx_bytes"),
            rx_packets: read_stat(&sys_path, "rx_packets"),
            tx_packets: read_stat(&sys_path, "tx_packets"),
            rx_errors: read_stat(&sys_path, "rx_errors"),
            tx_errors: read_stat(&sys_path, "tx_errors"),
            rx_dropped: read_stat(&sys_path, "rx_dropped"),
            tx_dropped: read_stat(&sys_path, "tx_dropped"),
        };

        // Get IP addresses using ip command
        let (ipv4_addresses, ipv6_addresses) = self.get_addresses(name).await?;

        Ok(NetworkInterface {
            name: name.to_string(),
            index,
            mac_address,
            ipv4_addresses,
            ipv6_addresses,
            flags,
            mtu,
            state,
            stats,
        })
    }

    async fn get_addresses(&self, name: &str) -> Result<(Vec<Ipv4Info>, Vec<Ipv6Info>)> {
        let mut ipv4 = Vec::new();
        let mut ipv6 = Vec::new();

        let output = Command::new("ip")
            .args(["-o", "addr", "show", "dev", name])
            .output()?;

        let stdout = String::from_utf8_lossy(&output.stdout);

        for line in stdout.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();

            if parts.len() < 4 {
                continue;
            }

            match parts[2] {
                "inet" => {
                    if let Some(addr_str) = parts.get(3) {
                        if let Some((addr, prefix)) = addr_str.split_once('/') {
                            if let Ok(address) = addr.parse() {
                                let prefix_len: u8 = prefix.parse().unwrap_or(24);
                                let netmask = prefix_to_netmask(prefix_len);

                                let broadcast = parts.iter()
                                    .position(|&p| p == "brd")
                                    .and_then(|i| parts.get(i + 1))
                                    .and_then(|b| b.parse().ok());

                                ipv4.push(Ipv4Info {
                                    address,
                                    netmask,
                                    broadcast,
                                });
                            }
                        }
                    }
                }
                "inet6" => {
                    if let Some(addr_str) = parts.get(3) {
                        if let Some((addr, prefix)) = addr_str.split_once('/') {
                            if let Ok(address) = addr.parse() {
                                let prefix_len: u8 = prefix.parse().unwrap_or(64);

                                let scope = if parts.contains(&"link") {
                                    Ipv6Scope::Link
                                } else if parts.contains(&"site") {
                                    Ipv6Scope::Site
                                } else {
                                    Ipv6Scope::Global
                                };

                                ipv6.push(Ipv6Info {
                                    address,
                                    prefix_len,
                                    scope,
                                });
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        Ok((ipv4, ipv6))
    }

    /// Get interface by name
    pub fn get(&self, name: &str) -> Option<&NetworkInterface> {
        self.interfaces.get(name)
    }

    /// List all interfaces
    pub fn list(&self) -> Vec<&NetworkInterface> {
        self.interfaces.values().collect()
    }

    /// Bring interface up
    pub async fn bring_up(&self, name: &str) -> Result<()> {
        let output = Command::new("ip")
            .args(["link", "set", name, "up"])
            .output()?;

        if !output.status.success() {
            return Err(anyhow!("Failed to bring up {}: {}",
                name, String::from_utf8_lossy(&output.stderr)));
        }

        tracing::info!("Interface {} brought up", name);
        Ok(())
    }

    /// Bring interface down
    pub async fn bring_down(&self, name: &str) -> Result<()> {
        let output = Command::new("ip")
            .args(["link", "set", name, "down"])
            .output()?;

        if !output.status.success() {
            return Err(anyhow!("Failed to bring down {}: {}",
                name, String::from_utf8_lossy(&output.stderr)));
        }

        tracing::info!("Interface {} brought down", name);
        Ok(())
    }

    /// Add IP address to interface
    pub async fn add_address(&self, name: &str, address: &str) -> Result<()> {
        let output = Command::new("ip")
            .args(["addr", "add", address, "dev", name])
            .output()?;

        if !output.status.success() {
            return Err(anyhow!("Failed to add address to {}: {}",
                name, String::from_utf8_lossy(&output.stderr)));
        }

        Ok(())
    }

    /// Remove IP address from interface
    pub async fn remove_address(&self, name: &str, address: &str) -> Result<()> {
        let output = Command::new("ip")
            .args(["addr", "del", address, "dev", name])
            .output()?;

        if !output.status.success() {
            return Err(anyhow!("Failed to remove address from {}: {}",
                name, String::from_utf8_lossy(&output.stderr)));
        }

        Ok(())
    }

    /// Set interface MTU
    pub async fn set_mtu(&self, name: &str, mtu: u32) -> Result<()> {
        let output = Command::new("ip")
            .args(["link", "set", name, "mtu", &mtu.to_string()])
            .output()?;

        if !output.status.success() {
            return Err(anyhow!("Failed to set MTU on {}: {}",
                name, String::from_utf8_lossy(&output.stderr)));
        }

        Ok(())
    }

    /// Get default gateway
    pub async fn get_default_gateway(&self) -> Result<Option<IpAddr>> {
        let output = Command::new("ip")
            .args(["-4", "route", "show", "default"])
            .output()?;

        let stdout = String::from_utf8_lossy(&output.stdout);

        for line in stdout.lines() {
            if line.starts_with("default via") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if let Some(gateway) = parts.get(2) {
                    if let Ok(ip) = gateway.parse() {
                        return Ok(Some(ip));
                    }
                }
            }
        }

        Ok(None)
    }
}

impl Default for InterfaceManager {
    fn default() -> Self {
        Self::new()
    }
}

fn read_sys_file(path: &str) -> Result<String> {
    std::fs::read_to_string(path).map_err(|e| anyhow!("Failed to read {}: {}", path, e))
}

fn read_stat(sys_path: &str, stat: &str) -> u64 {
    read_sys_file(&format!("{}/statistics/{}", sys_path, stat))
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

fn prefix_to_netmask(prefix: u8) -> std::net::Ipv4Addr {
    let mask: u32 = if prefix == 0 {
        0
    } else {
        !0u32 << (32 - prefix)
    };
    std::net::Ipv4Addr::from(mask.to_be_bytes())
}
