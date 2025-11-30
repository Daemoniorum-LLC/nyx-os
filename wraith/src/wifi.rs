//! WiFi management

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::process::Stdio;
use tracing::{info, debug, warn};

/// WiFi network
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WifiNetwork {
    pub ssid: String,
    pub bssid: String,
    pub signal_strength: i32,
    pub frequency: u32,
    pub security: WifiSecurity,
    pub connected: bool,
}

/// WiFi security type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum WifiSecurity {
    Open,
    Wep,
    WpaPsk,
    Wpa2Psk,
    Wpa3Sae,
    Enterprise,
}

/// Saved WiFi credentials
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WifiCredentials {
    pub ssid: String,
    pub security: WifiSecurity,
    pub password: Option<String>,
    pub auto_connect: bool,
    pub priority: i32,
}

/// WiFi manager
pub struct WifiManager {
    interface: String,
    supplicant_socket: Option<String>,
    known_networks: HashMap<String, WifiCredentials>,
}

impl WifiManager {
    pub fn new(interface: &str) -> Result<Self> {
        Ok(Self {
            interface: interface.to_string(),
            supplicant_socket: None,
            known_networks: HashMap::new(),
        })
    }

    /// Start WiFi on interface
    pub async fn start(&mut self) -> Result<()> {
        info!("Starting WiFi on {}", self.interface);

        // Bring interface up
        let status = tokio::process::Command::new("ip")
            .args(["link", "set", &self.interface, "up"])
            .status()
            .await?;

        if !status.success() {
            return Err(anyhow!("Failed to bring up interface"));
        }

        // Start wpa_supplicant
        let socket_path = format!("/run/wpa_supplicant/{}", self.interface);

        let _child = tokio::process::Command::new("wpa_supplicant")
            .args([
                "-i", &self.interface,
                "-D", "nl80211,wext",
                "-c", "/etc/wpa_supplicant/wpa_supplicant.conf",
                "-C", "/run/wpa_supplicant",
                "-B",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;

        // Wait for socket
        for _ in 0..50 {
            if std::path::Path::new(&socket_path).exists() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }

        self.supplicant_socket = Some(socket_path);

        Ok(())
    }

    /// Stop WiFi
    pub async fn stop(&mut self) -> Result<()> {
        info!("Stopping WiFi on {}", self.interface);

        // Kill wpa_supplicant
        let _ = tokio::process::Command::new("pkill")
            .args(["-f", &format!("wpa_supplicant.*{}", self.interface)])
            .status()
            .await;

        self.supplicant_socket = None;

        Ok(())
    }

    /// Scan for networks
    pub async fn scan(&self) -> Result<Vec<WifiNetwork>> {
        debug!("Scanning for WiFi networks");

        // Trigger scan
        self.wpa_cli(&["scan"]).await?;

        // Wait for results
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;

        // Get results
        let output = self.wpa_cli(&["scan_results"]).await?;

        let mut networks = Vec::new();

        for line in output.lines().skip(1) {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 5 {
                let bssid = parts[0].to_string();
                let frequency: u32 = parts[1].parse().unwrap_or(0);
                let signal: i32 = parts[2].parse().unwrap_or(-100);
                let flags = parts[3];
                let ssid = parts[4].to_string();

                let security = self.parse_security_flags(flags);

                networks.push(WifiNetwork {
                    ssid,
                    bssid,
                    signal_strength: signal,
                    frequency,
                    security,
                    connected: false,
                });
            }
        }

        // Sort by signal strength
        networks.sort_by(|a, b| b.signal_strength.cmp(&a.signal_strength));

        Ok(networks)
    }

    fn parse_security_flags(&self, flags: &str) -> WifiSecurity {
        if flags.contains("WPA3") || flags.contains("SAE") {
            WifiSecurity::Wpa3Sae
        } else if flags.contains("WPA2") {
            if flags.contains("EAP") {
                WifiSecurity::Enterprise
            } else {
                WifiSecurity::Wpa2Psk
            }
        } else if flags.contains("WPA") {
            WifiSecurity::WpaPsk
        } else if flags.contains("WEP") {
            WifiSecurity::Wep
        } else {
            WifiSecurity::Open
        }
    }

    /// Connect to a network
    pub async fn connect(&mut self, ssid: &str, password: Option<&str>) -> Result<()> {
        info!("Connecting to WiFi network: {}", ssid);

        // Add network
        let output = self.wpa_cli(&["add_network"]).await?;
        let network_id = output.trim().parse::<u32>()
            .map_err(|_| anyhow!("Failed to add network"))?;

        // Set SSID
        self.wpa_cli(&[
            "set_network",
            &network_id.to_string(),
            "ssid",
            &format!("\"{}\"", ssid),
        ]).await?;

        // Set password if provided
        if let Some(pwd) = password {
            self.wpa_cli(&[
                "set_network",
                &network_id.to_string(),
                "psk",
                &format!("\"{}\"", pwd),
            ]).await?;
        } else {
            self.wpa_cli(&[
                "set_network",
                &network_id.to_string(),
                "key_mgmt",
                "NONE",
            ]).await?;
        }

        // Enable and connect
        self.wpa_cli(&["enable_network", &network_id.to_string()]).await?;
        self.wpa_cli(&["select_network", &network_id.to_string()]).await?;

        // Wait for connection
        for _ in 0..100 {
            let status = self.get_status().await?;
            if status.get("wpa_state").map(|s| s.as_str()) == Some("COMPLETED") {
                info!("Connected to {}", ssid);
                return Ok(());
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }

        Err(anyhow!("Connection timeout"))
    }

    /// Disconnect from current network
    pub async fn disconnect(&self) -> Result<()> {
        self.wpa_cli(&["disconnect"]).await?;
        info!("Disconnected from WiFi");
        Ok(())
    }

    /// Get current status
    pub async fn get_status(&self) -> Result<HashMap<String, String>> {
        let output = self.wpa_cli(&["status"]).await?;

        let mut status = HashMap::new();
        for line in output.lines() {
            if let Some((key, value)) = line.split_once('=') {
                status.insert(key.to_string(), value.to_string());
            }
        }

        Ok(status)
    }

    /// Save network for auto-connect
    pub fn save_network(&mut self, creds: WifiCredentials) {
        self.known_networks.insert(creds.ssid.clone(), creds);
    }

    /// Remove saved network
    pub fn forget_network(&mut self, ssid: &str) {
        self.known_networks.remove(ssid);
    }

    /// Get saved networks
    pub fn get_saved_networks(&self) -> Vec<&WifiCredentials> {
        self.known_networks.values().collect()
    }

    async fn wpa_cli(&self, args: &[&str]) -> Result<String> {
        let socket = self.supplicant_socket.as_ref()
            .ok_or_else(|| anyhow!("wpa_supplicant not running"))?;

        let output = tokio::process::Command::new("wpa_cli")
            .args(["-p", "/run/wpa_supplicant", "-i", &self.interface])
            .args(args)
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("wpa_cli failed: {}", stderr));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}
