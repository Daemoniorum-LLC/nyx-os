//! Wraith state management

use crate::config::NetworkConfig;
use crate::dhcp::DhcpClient;
use crate::dns::DnsManager;
use crate::interface::InterfaceManager;
use crate::profile::{NetworkProfile, ProfileManager, IpConfig};
use anyhow::Result;
use tracing::info;

/// Network manager state
pub struct WraithState {
    pub interfaces: InterfaceManager,
    pub dns: DnsManager,
    pub profiles: ProfileManager,
    pub config: NetworkConfig,
}

impl WraithState {
    /// Create a new wraith state
    pub async fn new(config_dir: &str) -> Result<Self> {
        let config = NetworkConfig::load(config_dir)?;
        let interfaces = InterfaceManager::new().await?;
        let dns = DnsManager::new(&config)?;
        let profiles = ProfileManager::load(&format!("{}/profiles", config_dir))?;

        Ok(Self {
            interfaces,
            dns,
            profiles,
            config,
        })
    }

    pub async fn apply_saved_profiles(&mut self) -> Result<()> {
        // Collect interface names and their matching profiles first
        let to_apply: Vec<(String, NetworkProfile)> = self.interfaces.list()?
            .iter()
            .filter_map(|iface| {
                self.profiles.get_for_interface(&iface.name)
                    .map(|p| (iface.name.clone(), p.clone()))
            })
            .collect();

        // Now apply the profiles
        for (iface_name, profile) in to_apply {
            info!("Applying profile {} to {}", profile.name, iface_name);
            self.apply_profile(&iface_name, &profile).await?;
        }
        Ok(())
    }

    pub async fn apply_profile(&mut self, iface: &str, profile: &NetworkProfile) -> Result<()> {
        match &profile.config {
            IpConfig::Dhcp => {
                self.start_dhcp(iface).await?;
            }
            IpConfig::Static { address, gateway, dns } => {
                self.interfaces.set_address(iface, address).await?;
                if let Some(gw) = gateway {
                    self.interfaces.set_gateway(iface, gw).await?;
                }
                if !dns.is_empty() {
                    self.dns.set_servers(dns)?;
                }
            }
        }

        // Bring interface up
        self.interfaces.set_up(iface, true).await?;

        Ok(())
    }

    pub async fn start_dhcp(&mut self, iface: &str) -> Result<()> {
        let client = DhcpClient::new(iface)?;
        let lease = client.request().await?;

        self.interfaces.set_address(iface, &lease.address.to_string()).await?;
        if let Some(gw) = lease.gateway {
            self.interfaces.set_gateway(iface, &gw.to_string()).await?;
        }
        if !lease.dns_servers.is_empty() {
            let servers: Vec<String> = lease.dns_servers.iter()
                .map(|ip| ip.to_string())
                .collect();
            self.dns.set_servers(&servers)?;
        }

        Ok(())
    }
}
