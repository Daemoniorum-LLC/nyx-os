//! VPN management (WireGuard)

use crate::config::{VpnConfig, WireGuardConfig, WireGuardPeer};
use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::process::Command;
use tokio::sync::RwLock;

/// VPN manager
pub struct VpnManager {
    config: RwLock<VpnConfig>,
    active_connections: RwLock<HashMap<String, VpnConnection>>,
}

#[derive(Debug, Clone)]
pub struct VpnConnection {
    pub interface: String,
    pub vpn_type: VpnType,
    pub status: VpnStatus,
    pub local_address: String,
    pub peers: Vec<PeerStatus>,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub last_handshake: Option<std::time::SystemTime>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VpnType {
    WireGuard,
    // Future: OpenVPN, IPSec
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VpnStatus {
    Connected,
    Connecting,
    Disconnected,
    Error,
}

#[derive(Debug, Clone)]
pub struct PeerStatus {
    pub public_key: String,
    pub endpoint: Option<String>,
    pub allowed_ips: Vec<String>,
    pub last_handshake: Option<std::time::SystemTime>,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
}

impl VpnManager {
    pub fn new(config: VpnConfig) -> Self {
        Self {
            config: RwLock::new(config),
            active_connections: RwLock::new(HashMap::new()),
        }
    }

    /// Initialize configured VPNs
    pub async fn init(&self) -> Result<()> {
        let config = self.config.read().await;

        if let Some(ref wg) = config.wireguard {
            if let Err(e) = self.setup_wireguard(wg).await {
                tracing::warn!("Failed to initialize WireGuard: {}", e);
            }
        }

        Ok(())
    }

    /// Setup WireGuard interface
    async fn setup_wireguard(&self, config: &WireGuardConfig) -> Result<()> {
        let interface = &config.interface;

        // Check if interface exists
        let exists = Command::new("ip")
            .args(["link", "show", interface])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        if !exists {
            // Create interface
            let output = Command::new("ip")
                .args(["link", "add", interface, "type", "wireguard"])
                .output()?;

            if !output.status.success() {
                return Err(anyhow!("Failed to create WireGuard interface: {}",
                    String::from_utf8_lossy(&output.stderr)));
            }
        }

        // Configure private key
        let key_output = Command::new("wg")
            .args(["set", interface, "private-key", "/dev/stdin"])
            .stdin(std::process::Stdio::piped())
            .spawn()?
            .wait_with_output()?;

        // Set address
        let output = Command::new("ip")
            .args(["addr", "add", &config.address, "dev", interface])
            .output()?;

        // Add peers
        for peer in &config.peers {
            self.add_wireguard_peer(interface, peer).await?;
        }

        // Bring up interface
        let output = Command::new("ip")
            .args(["link", "set", interface, "up"])
            .output()?;

        if !output.status.success() {
            return Err(anyhow!("Failed to bring up WireGuard interface"));
        }

        // Track connection
        self.active_connections.write().await.insert(
            interface.clone(),
            VpnConnection {
                interface: interface.clone(),
                vpn_type: VpnType::WireGuard,
                status: VpnStatus::Connected,
                local_address: config.address.clone(),
                peers: config.peers.iter().map(|p| PeerStatus {
                    public_key: p.public_key.clone(),
                    endpoint: p.endpoint.clone(),
                    allowed_ips: p.allowed_ips.clone(),
                    last_handshake: None,
                    rx_bytes: 0,
                    tx_bytes: 0,
                }).collect(),
                rx_bytes: 0,
                tx_bytes: 0,
                last_handshake: None,
            },
        );

        tracing::info!("WireGuard interface {} configured", interface);
        Ok(())
    }

    async fn add_wireguard_peer(&self, interface: &str, peer: &WireGuardPeer) -> Result<()> {
        let mut args = vec![
            "set".to_string(),
            interface.to_string(),
            "peer".to_string(),
            peer.public_key.clone(),
        ];

        if let Some(ref endpoint) = peer.endpoint {
            args.push("endpoint".to_string());
            args.push(endpoint.clone());
        }

        if !peer.allowed_ips.is_empty() {
            args.push("allowed-ips".to_string());
            args.push(peer.allowed_ips.join(","));
        }

        let output = Command::new("wg")
            .args(&args)
            .output()?;

        if !output.status.success() {
            return Err(anyhow!("Failed to add peer: {}",
                String::from_utf8_lossy(&output.stderr)));
        }

        Ok(())
    }

    /// Connect to VPN
    pub async fn connect(&self, name: &str) -> Result<()> {
        let config = self.config.read().await;

        if let Some(ref wg) = config.wireguard {
            if wg.interface == name {
                self.setup_wireguard(wg).await?;
                return Ok(());
            }
        }

        Err(anyhow!("VPN configuration not found: {}", name))
    }

    /// Disconnect VPN
    pub async fn disconnect(&self, name: &str) -> Result<()> {
        // Bring down interface
        let output = Command::new("ip")
            .args(["link", "set", name, "down"])
            .output()?;

        // Remove interface
        let output = Command::new("ip")
            .args(["link", "del", name])
            .output()?;

        self.active_connections.write().await.remove(name);
        tracing::info!("Disconnected VPN: {}", name);
        Ok(())
    }

    /// Get VPN status
    pub async fn status(&self, name: &str) -> Result<VpnConnection> {
        // Update stats from wg show
        if let Some(conn) = self.active_connections.read().await.get(name) {
            let mut conn = conn.clone();

            // Get live stats
            let output = Command::new("wg")
                .args(["show", name, "dump"])
                .output()?;

            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                self.parse_wg_stats(&stdout, &mut conn);
            }

            return Ok(conn);
        }

        Err(anyhow!("VPN not found: {}", name))
    }

    fn parse_wg_stats(&self, output: &str, conn: &mut VpnConnection) {
        let lines: Vec<&str> = output.lines().collect();

        // First line is interface info
        if let Some(line) = lines.first() {
            // private_key, public_key, listen_port, fwmark
        }

        // Rest are peer info
        for line in lines.iter().skip(1) {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 5 {
                let public_key = parts[0];
                let endpoint = if parts[2] != "(none)" { Some(parts[2].to_string()) } else { None };
                let allowed_ips: Vec<String> = parts[3].split(',').map(|s| s.to_string()).collect();
                let last_handshake = parts[4].parse::<u64>().ok()
                    .filter(|&t| t > 0)
                    .map(|t| std::time::UNIX_EPOCH + std::time::Duration::from_secs(t));
                let rx_bytes: u64 = parts.get(5).and_then(|s| s.parse().ok()).unwrap_or(0);
                let tx_bytes: u64 = parts.get(6).and_then(|s| s.parse().ok()).unwrap_or(0);

                // Update peer status
                if let Some(peer) = conn.peers.iter_mut()
                    .find(|p| p.public_key == public_key)
                {
                    peer.endpoint = endpoint;
                    peer.last_handshake = last_handshake;
                    peer.rx_bytes = rx_bytes;
                    peer.tx_bytes = tx_bytes;
                }

                conn.rx_bytes += rx_bytes;
                conn.tx_bytes += tx_bytes;
                conn.last_handshake = last_handshake;
            }
        }
    }

    /// List all VPN connections
    pub async fn list(&self) -> Vec<VpnConnection> {
        self.active_connections.read().await.values().cloned().collect()
    }

    /// Generate new WireGuard keypair
    pub fn generate_keypair() -> Result<(String, String)> {
        // Generate private key
        let output = Command::new("wg")
            .args(["genkey"])
            .output()?;

        if !output.status.success() {
            return Err(anyhow!("Failed to generate private key"));
        }

        let private_key = String::from_utf8_lossy(&output.stdout).trim().to_string();

        // Derive public key
        let mut child = Command::new("wg")
            .args(["pubkey"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()?;

        {
            use std::io::Write;
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(private_key.as_bytes())?;
            }
        }

        let output = child.wait_with_output()?;
        let public_key = String::from_utf8_lossy(&output.stdout).trim().to_string();

        Ok((private_key, public_key))
    }

    /// Generate preshared key
    pub fn generate_psk() -> Result<String> {
        let output = Command::new("wg")
            .args(["genpsk"])
            .output()?;

        if !output.status.success() {
            return Err(anyhow!("Failed to generate PSK"));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Import WireGuard configuration
    pub async fn import_config(&self, interface: &str, config_path: &str) -> Result<()> {
        // Use wg-quick style config
        let output = Command::new("wg")
            .args(["setconf", interface, config_path])
            .output()?;

        if !output.status.success() {
            return Err(anyhow!("Failed to import config: {}",
                String::from_utf8_lossy(&output.stderr)));
        }

        Ok(())
    }

    /// Export WireGuard configuration
    pub async fn export_config(&self, interface: &str) -> Result<String> {
        let output = Command::new("wg")
            .args(["showconf", interface])
            .output()?;

        if !output.status.success() {
            return Err(anyhow!("Failed to export config: {}",
                String::from_utf8_lossy(&output.stderr)));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}
