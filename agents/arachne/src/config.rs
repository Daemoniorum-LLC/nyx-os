//! Arachne configuration

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ArachneConfig {
    #[serde(default)]
    pub firewall: FirewallConfig,
    #[serde(default)]
    pub dns: DnsConfig,
    #[serde(default)]
    pub monitor: MonitorConfig,
    #[serde(default)]
    pub vpn: VpnConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirewallConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub default_policy: DefaultPolicy,
    #[serde(default)]
    pub rules: Vec<FirewallRule>,
    #[serde(default = "default_true")]
    pub log_blocked: bool,
}

impl Default for FirewallConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            default_policy: DefaultPolicy::Drop,
            rules: default_firewall_rules(),
            log_blocked: true,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DefaultPolicy { Accept, #[default] Drop, Reject }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirewallRule {
    pub name: String,
    pub direction: Direction,
    pub action: Action,
    #[serde(default)]
    pub protocol: Option<String>,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub destination: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction { In, Out, Both }

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action { Accept, Drop, Reject, Log }

fn default_firewall_rules() -> Vec<FirewallRule> {
    vec![
        FirewallRule {
            name: "allow-loopback".into(),
            direction: Direction::Both,
            action: Action::Accept,
            protocol: None, port: None,
            source: Some("127.0.0.0/8".into()),
            destination: Some("127.0.0.0/8".into()),
        },
        FirewallRule {
            name: "allow-established".into(),
            direction: Direction::In,
            action: Action::Accept,
            protocol: None, port: None,
            source: None, destination: None,
        },
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub server_enabled: bool,
    #[serde(default = "default_dns_port")]
    pub port: u16,
    #[serde(default = "default_upstream")]
    pub upstream: Vec<String>,
    #[serde(default = "default_true")]
    pub cache_enabled: bool,
    #[serde(default = "default_cache_size")]
    pub cache_size: usize,
    #[serde(default)]
    pub blocklist: Vec<String>,
}

impl Default for DnsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            server_enabled: false,
            port: default_dns_port(),
            upstream: default_upstream(),
            cache_enabled: true,
            cache_size: default_cache_size(),
            blocklist: Vec::new(),
        }
    }
}

fn default_dns_port() -> u16 { 53 }
fn default_upstream() -> Vec<String> { vec!["1.1.1.1".into(), "8.8.8.8".into()] }
fn default_cache_size() -> usize { 10000 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_interval")]
    pub interval_secs: u64,
    #[serde(default = "default_true")]
    pub track_connections: bool,
    #[serde(default = "default_true")]
    pub track_bandwidth: bool,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_secs: default_interval(),
            track_connections: true,
            track_bandwidth: true,
        }
    }
}

fn default_interval() -> u64 { 5 }

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VpnConfig {
    #[serde(default)]
    pub wireguard: Option<WireGuardConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireGuardConfig {
    pub interface: String,
    pub private_key: String,
    pub address: String,
    pub peers: Vec<WireGuardPeer>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireGuardPeer {
    pub public_key: String,
    pub endpoint: Option<String>,
    pub allowed_ips: Vec<String>,
}

fn default_true() -> bool { true }

pub async fn load_config(path: &Path) -> Result<ArachneConfig> {
    if path.exists() {
        let contents = tokio::fs::read_to_string(path).await?;
        Ok(serde_yaml::from_str(&contents)?)
    } else {
        Ok(ArachneConfig::default())
    }
}
