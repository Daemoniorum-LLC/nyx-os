//! Network profiles

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{info, warn, debug};

/// Network profile
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkProfile {
    /// Profile name
    pub name: String,

    /// Interface pattern (e.g., "eth*", "wlan0")
    pub interface_match: String,

    /// IP configuration
    pub config: IpConfig,

    /// Auto-connect priority (higher = more preferred)
    pub priority: i32,

    /// Additional options
    pub options: ProfileOptions,
}

/// IP configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum IpConfig {
    Dhcp,
    Static {
        address: String,
        gateway: Option<String>,
        dns: Vec<String>,
    },
}

/// Additional profile options
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProfileOptions {
    /// Custom MTU
    pub mtu: Option<u32>,

    /// IPv6 mode
    pub ipv6: Ipv6Mode,

    /// Custom routes
    pub routes: Vec<Route>,

    /// Metered connection
    pub metered: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum Ipv6Mode {
    #[default]
    Auto,
    Dhcp,
    Disabled,
    LinkLocal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Route {
    pub destination: String,
    pub gateway: Option<String>,
    pub metric: Option<u32>,
}

/// Profile manager
pub struct ProfileManager {
    profiles_dir: PathBuf,
    profiles: HashMap<String, NetworkProfile>,
    interface_mapping: HashMap<String, String>,
}

impl ProfileManager {
    pub fn load(profiles_dir: &str) -> Result<Self> {
        let profiles_dir = PathBuf::from(profiles_dir);
        std::fs::create_dir_all(&profiles_dir)?;

        let mut profiles = HashMap::new();

        for entry in std::fs::read_dir(&profiles_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|e| e.to_str()) == Some("toml") {
                match Self::load_profile(&path) {
                    Ok(profile) => {
                        info!("Loaded profile: {}", profile.name);
                        profiles.insert(profile.name.clone(), profile);
                    }
                    Err(e) => warn!("Failed to load profile {:?}: {}", path, e),
                }
            }
        }

        Ok(Self {
            profiles_dir,
            profiles,
            interface_mapping: HashMap::new(),
        })
    }

    fn load_profile(path: &Path) -> Result<NetworkProfile> {
        let content = std::fs::read_to_string(path)?;
        Ok(toml::from_str(&content)?)
    }

    /// Save a profile
    pub fn save(&self, profile: &NetworkProfile) -> Result<()> {
        let filename = format!("{}.toml", profile.name.replace(' ', "_"));
        let path = self.profiles_dir.join(&filename);

        let content = toml::to_string_pretty(profile)?;
        std::fs::write(&path, &content)?;

        info!("Saved profile: {}", profile.name);
        Ok(())
    }

    /// Delete a profile
    pub fn delete(&mut self, name: &str) -> Result<()> {
        if let Some(profile) = self.profiles.remove(name) {
            let filename = format!("{}.toml", profile.name.replace(' ', "_"));
            let path = self.profiles_dir.join(&filename);
            let _ = std::fs::remove_file(&path);
            info!("Deleted profile: {}", name);
        }
        Ok(())
    }

    /// Get profile for an interface
    pub fn get_for_interface(&self, interface: &str) -> Option<&NetworkProfile> {
        // Check explicit mapping first
        if let Some(profile_name) = self.interface_mapping.get(interface) {
            return self.profiles.get(profile_name);
        }

        // Find matching profile by pattern
        let mut matching: Vec<_> = self.profiles.values()
            .filter(|p| self.matches_interface(interface, &p.interface_match))
            .collect();

        // Sort by priority (highest first)
        matching.sort_by_key(|p| -p.priority);

        matching.first().copied()
    }

    fn matches_interface(&self, interface: &str, pattern: &str) -> bool {
        if pattern.ends_with('*') {
            let prefix = &pattern[..pattern.len() - 1];
            interface.starts_with(prefix)
        } else {
            interface == pattern
        }
    }

    /// Set explicit interface-to-profile mapping
    pub fn set_mapping(&mut self, interface: &str, profile: &str) {
        self.interface_mapping.insert(interface.to_string(), profile.to_string());
    }

    /// List all profiles
    pub fn list(&self) -> Vec<&NetworkProfile> {
        self.profiles.values().collect()
    }

    /// Get profile by name
    pub fn get(&self, name: &str) -> Option<&NetworkProfile> {
        self.profiles.get(name)
    }

    /// Add or update profile
    pub fn add(&mut self, profile: NetworkProfile) -> Result<()> {
        self.save(&profile)?;
        self.profiles.insert(profile.name.clone(), profile);
        Ok(())
    }
}

/// Create default profiles
pub fn create_defaults(profiles_dir: &str) -> Result<()> {
    let manager = ProfileManager::load(profiles_dir)?;

    // DHCP profile for ethernet
    let eth_dhcp = NetworkProfile {
        name: "Wired Connection".to_string(),
        interface_match: "eth*".to_string(),
        config: IpConfig::Dhcp,
        priority: 100,
        options: ProfileOptions::default(),
    };

    // DHCP profile for wireless
    let wlan_dhcp = NetworkProfile {
        name: "Wireless Connection".to_string(),
        interface_match: "wl*".to_string(),
        config: IpConfig::Dhcp,
        priority: 50,
        options: ProfileOptions::default(),
    };

    manager.save(&eth_dhcp)?;
    manager.save(&wlan_dhcp)?;

    Ok(())
}
