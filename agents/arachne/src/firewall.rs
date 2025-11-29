//! Firewall management using nftables

use crate::config::{Action, DefaultPolicy, Direction, FirewallConfig, FirewallRule};
use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::process::Command;
use tokio::sync::RwLock;

/// Firewall manager using nftables
pub struct Firewall {
    config: RwLock<FirewallConfig>,
    active_rules: RwLock<HashMap<String, ActiveRule>>,
}

#[derive(Debug, Clone)]
pub struct ActiveRule {
    pub name: String,
    pub handle: u64,
    pub chain: String,
    pub packets: u64,
    pub bytes: u64,
}

#[derive(Debug, Clone)]
pub struct BlockedConnection {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub source: String,
    pub destination: String,
    pub protocol: String,
    pub port: Option<u16>,
    pub rule: String,
}

impl Firewall {
    pub fn new(config: FirewallConfig) -> Self {
        Self {
            config: RwLock::new(config),
            active_rules: RwLock::new(HashMap::new()),
        }
    }

    /// Initialize the firewall
    pub async fn init(&self) -> Result<()> {
        let config = self.config.read().await;

        if !config.enabled {
            tracing::info!("Firewall disabled in configuration");
            return Ok(());
        }

        // Create base nftables structure
        self.setup_tables().await?;

        // Set default policy
        self.set_default_policy(config.default_policy).await?;

        // Apply configured rules
        for rule in &config.rules {
            if let Err(e) = self.add_rule(rule).await {
                tracing::warn!("Failed to add rule {}: {}", rule.name, e);
            }
        }

        tracing::info!("Firewall initialized with {} rules", config.rules.len());
        Ok(())
    }

    async fn setup_tables(&self) -> Result<()> {
        // Create nyx table and chains
        let commands = r#"
            table inet nyx {
                chain input {
                    type filter hook input priority 0; policy drop;
                    ct state established,related accept
                    iif lo accept
                }
                chain output {
                    type filter hook output priority 0; policy accept;
                }
                chain forward {
                    type filter hook forward priority 0; policy drop;
                }
            }
        "#;

        self.nft_command(&["-f", "-"], Some(commands)).await?;
        Ok(())
    }

    async fn set_default_policy(&self, policy: DefaultPolicy) -> Result<()> {
        let policy_str = match policy {
            DefaultPolicy::Accept => "accept",
            DefaultPolicy::Drop => "drop",
            DefaultPolicy::Reject => "drop", // nftables doesn't have reject as chain policy
        };

        self.nft_command(&["chain", "inet", "nyx", "input", &format!("{{ policy {} }}", policy_str)], None).await?;
        Ok(())
    }

    /// Add a firewall rule
    pub async fn add_rule(&self, rule: &FirewallRule) -> Result<()> {
        let chain = match rule.direction {
            Direction::In => "input",
            Direction::Out => "output",
            Direction::Both => {
                // Add to both chains
                let mut input_rule = rule.clone();
                let mut output_rule = rule.clone();
                input_rule.name = format!("{}-in", rule.name);
                output_rule.name = format!("{}-out", rule.name);
                self.add_rule_to_chain(&input_rule, "input").await?;
                self.add_rule_to_chain(&output_rule, "output").await?;
                return Ok(());
            }
        };

        self.add_rule_to_chain(rule, chain).await
    }

    async fn add_rule_to_chain(&self, rule: &FirewallRule, chain: &str) -> Result<()> {
        let mut nft_rule = String::new();

        // Protocol
        if let Some(ref proto) = rule.protocol {
            nft_rule.push_str(&format!("{} ", proto.to_lowercase()));
        }

        // Source
        if let Some(ref src) = rule.source {
            nft_rule.push_str(&format!("ip saddr {} ", src));
        }

        // Destination
        if let Some(ref dst) = rule.destination {
            nft_rule.push_str(&format!("ip daddr {} ", dst));
        }

        // Port
        if let Some(port) = rule.port {
            let proto = rule.protocol.as_deref().unwrap_or("tcp");
            nft_rule.push_str(&format!("{} dport {} ", proto, port));
        }

        // Action
        let action = match rule.action {
            Action::Accept => "accept",
            Action::Drop => "drop",
            Action::Reject => "reject",
            Action::Log => "log prefix \"nyx-fw: \"",
        };
        nft_rule.push_str(action);

        // Comment
        nft_rule.push_str(&format!(" comment \"{}\"", rule.name));

        self.nft_command(&["add", "rule", "inet", "nyx", chain, &nft_rule], None).await?;

        tracing::debug!("Added rule {} to chain {}", rule.name, chain);
        Ok(())
    }

    /// Remove a firewall rule by name
    pub async fn remove_rule(&self, name: &str) -> Result<()> {
        let rules = self.active_rules.read().await;

        if let Some(rule) = rules.get(name) {
            self.nft_command(&[
                "delete", "rule", "inet", "nyx", &rule.chain,
                "handle", &rule.handle.to_string()
            ], None).await?;

            drop(rules);
            self.active_rules.write().await.remove(name);

            tracing::info!("Removed rule: {}", name);
        }

        Ok(())
    }

    /// Block an IP address
    pub async fn block_ip(&self, ip: &str, reason: &str) -> Result<()> {
        let rule = FirewallRule {
            name: format!("block-{}", ip.replace(['.', ':'], "-")),
            direction: Direction::Both,
            action: Action::Drop,
            protocol: None,
            port: None,
            source: Some(ip.to_string()),
            destination: None,
        };

        self.add_rule(&rule).await?;
        tracing::warn!("Blocked IP {}: {}", ip, reason);
        Ok(())
    }

    /// Unblock an IP address
    pub async fn unblock_ip(&self, ip: &str) -> Result<()> {
        let rule_name = format!("block-{}", ip.replace(['.', ':'], "-"));
        self.remove_rule(&rule_name).await
    }

    /// Allow a port
    pub async fn allow_port(&self, port: u16, protocol: &str, direction: Direction) -> Result<()> {
        let rule = FirewallRule {
            name: format!("allow-{}-{}", protocol, port),
            direction,
            action: Action::Accept,
            protocol: Some(protocol.to_string()),
            port: Some(port),
            source: None,
            destination: None,
        };

        self.add_rule(&rule).await
    }

    /// Get firewall statistics
    pub async fn get_stats(&self) -> Result<FirewallStats> {
        let output = self.nft_command(&["list", "table", "inet", "nyx", "-j"], None).await?;

        // Parse JSON output for statistics
        let stats = FirewallStats {
            enabled: self.config.read().await.enabled,
            rules_count: self.active_rules.read().await.len(),
            packets_accepted: 0, // Would parse from nft output
            packets_dropped: 0,
            packets_rejected: 0,
            bytes_total: 0,
        };

        Ok(stats)
    }

    /// List all rules
    pub async fn list_rules(&self) -> Result<Vec<ActiveRule>> {
        let rules = self.active_rules.read().await;
        Ok(rules.values().cloned().collect())
    }

    /// Reload configuration
    pub async fn reload(&self, config: FirewallConfig) -> Result<()> {
        // Flush existing rules
        self.nft_command(&["flush", "table", "inet", "nyx"], None).await?;

        // Update config
        *self.config.write().await = config;

        // Reinitialize
        self.init().await
    }

    /// Check if a connection would be allowed
    pub async fn check_connection(
        &self,
        source: &str,
        dest: &str,
        port: u16,
        protocol: &str,
    ) -> bool {
        let config = self.config.read().await;

        for rule in &config.rules {
            // Check if rule matches
            let proto_match = rule.protocol.as_ref()
                .map(|p| p.eq_ignore_ascii_case(protocol))
                .unwrap_or(true);

            let port_match = rule.port.map(|p| p == port).unwrap_or(true);

            let src_match = rule.source.as_ref()
                .map(|s| ip_matches(source, s))
                .unwrap_or(true);

            let dst_match = rule.destination.as_ref()
                .map(|d| ip_matches(dest, d))
                .unwrap_or(true);

            if proto_match && port_match && src_match && dst_match {
                return matches!(rule.action, Action::Accept);
            }
        }

        // Fall back to default policy
        matches!(config.default_policy, DefaultPolicy::Accept)
    }

    async fn nft_command(&self, args: &[&str], stdin: Option<&str>) -> Result<String> {
        let mut cmd = Command::new("nft");
        cmd.args(args);

        if let Some(input) = stdin {
            use std::io::Write;
            use std::process::Stdio;

            cmd.stdin(Stdio::piped());
            let mut child = cmd.spawn()?;

            if let Some(mut child_stdin) = child.stdin.take() {
                child_stdin.write_all(input.as_bytes())?;
            }

            let output = child.wait_with_output()?;

            if !output.status.success() {
                return Err(anyhow!("nft command failed: {}",
                    String::from_utf8_lossy(&output.stderr)));
            }

            return Ok(String::from_utf8_lossy(&output.stdout).to_string());
        }

        let output = cmd.output()?;

        if !output.status.success() {
            return Err(anyhow!("nft command failed: {}",
                String::from_utf8_lossy(&output.stderr)));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

#[derive(Debug, Clone)]
pub struct FirewallStats {
    pub enabled: bool,
    pub rules_count: usize,
    pub packets_accepted: u64,
    pub packets_dropped: u64,
    pub packets_rejected: u64,
    pub bytes_total: u64,
}

/// Check if an IP matches a CIDR pattern
fn ip_matches(ip: &str, pattern: &str) -> bool {
    if pattern.contains('/') {
        // CIDR notation - simplified check
        let parts: Vec<&str> = pattern.split('/').collect();
        if parts.len() == 2 {
            let network = parts[0];
            // Simple prefix match for now
            ip.starts_with(&network[..network.rfind('.').unwrap_or(0)])
        } else {
            false
        }
    } else {
        ip == pattern
    }
}
