//! Network configuration

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Network manager configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Hostname
    pub hostname: String,

    /// DNS configuration
    pub dns: DnsConfig,

    /// Global settings
    pub settings: GlobalSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsConfig {
    /// Upstream DNS servers
    pub servers: Vec<String>,

    /// Search domains
    pub search: Vec<String>,

    /// Enable DNS caching
    pub cache_enabled: bool,

    /// Cache size
    pub cache_size: usize,

    /// Enable DNSSEC validation
    pub dnssec: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalSettings {
    /// Auto-connect to known networks
    pub auto_connect: bool,

    /// Prefer IPv6
    pub prefer_ipv6: bool,

    /// Enable link-local addresses
    pub link_local: bool,

    /// Connection timeout (seconds)
    pub timeout: u32,

    /// Enable network metering detection
    pub metering: bool,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            hostname: "nyx".to_string(),
            dns: DnsConfig {
                servers: vec![
                    "1.1.1.1".to_string(),
                    "8.8.8.8".to_string(),
                ],
                search: vec![],
                cache_enabled: true,
                cache_size: 1000,
                dnssec: false,
            },
            settings: GlobalSettings {
                auto_connect: true,
                prefer_ipv6: false,
                link_local: true,
                timeout: 30,
                metering: true,
            },
        }
    }
}

impl NetworkConfig {
    pub fn load(config_dir: &str) -> Result<Self> {
        let config_path = Path::new(config_dir).join("wraith.toml");

        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            Ok(toml::from_str(&content)?)
        } else {
            // Create default config
            let config = Self::default();

            std::fs::create_dir_all(config_dir)?;
            let content = toml::to_string_pretty(&config)?;
            std::fs::write(&config_path, &content)?;

            Ok(config)
        }
    }

    pub fn save(&self, config_dir: &str) -> Result<()> {
        let config_path = Path::new(config_dir).join("wraith.toml");
        let content = toml::to_string_pretty(self)?;
        std::fs::write(&config_path, &content)?;
        Ok(())
    }
}
