//! Routing table management

use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::net::IpAddr;
use std::process::Command;

/// Route entry
#[derive(Debug, Clone)]
pub struct Route {
    pub destination: String,
    pub gateway: Option<IpAddr>,
    pub interface: String,
    pub metric: u32,
    pub protocol: RouteProtocol,
    pub scope: RouteScope,
    pub route_type: RouteType,
    pub source: Option<IpAddr>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RouteProtocol {
    Kernel,
    Boot,
    Static,
    Dhcp,
    Ra,      // Router advertisement
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RouteScope {
    Global,
    Link,
    Host,
    Nowhere,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RouteType {
    Unicast,
    Local,
    Broadcast,
    Multicast,
    Unreachable,
    Prohibit,
    Blackhole,
}

/// Routing table manager
pub struct RoutingTable {
    routes: Vec<Route>,
    policy_rules: Vec<PolicyRule>,
}

#[derive(Debug, Clone)]
pub struct PolicyRule {
    pub priority: u32,
    pub selector: RuleSelector,
    pub action: RuleAction,
}

#[derive(Debug, Clone)]
pub enum RuleSelector {
    From(String),
    To(String),
    Fwmark(u32),
    Iif(String),
    Oif(String),
}

#[derive(Debug, Clone)]
pub enum RuleAction {
    Table(u32),
    Unreachable,
    Blackhole,
    Prohibit,
}

impl RoutingTable {
    pub fn new() -> Self {
        Self {
            routes: Vec::new(),
            policy_rules: Vec::new(),
        }
    }

    /// Refresh routing table
    pub async fn refresh(&mut self) -> Result<()> {
        self.routes.clear();

        // Get IPv4 routes
        let output = Command::new("ip")
            .args(["-4", "-o", "route", "show"])
            .output()?;

        self.parse_routes(&String::from_utf8_lossy(&output.stdout))?;

        // Get IPv6 routes
        let output = Command::new("ip")
            .args(["-6", "-o", "route", "show"])
            .output()?;

        self.parse_routes(&String::from_utf8_lossy(&output.stdout))?;

        // Get policy rules
        self.refresh_policy_rules().await?;

        Ok(())
    }

    fn parse_routes(&mut self, output: &str) -> Result<()> {
        for line in output.lines() {
            if line.trim().is_empty() {
                continue;
            }

            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.is_empty() {
                continue;
            }

            let destination = parts[0].to_string();
            let mut gateway = None;
            let mut interface = String::new();
            let mut metric = 0u32;
            let mut protocol = RouteProtocol::Unknown;
            let mut scope = RouteScope::Global;
            let mut route_type = RouteType::Unicast;
            let mut source = None;

            let mut i = 1;
            while i < parts.len() {
                match parts[i] {
                    "via" => {
                        if i + 1 < parts.len() {
                            gateway = parts[i + 1].parse().ok();
                            i += 1;
                        }
                    }
                    "dev" => {
                        if i + 1 < parts.len() {
                            interface = parts[i + 1].to_string();
                            i += 1;
                        }
                    }
                    "metric" => {
                        if i + 1 < parts.len() {
                            metric = parts[i + 1].parse().unwrap_or(0);
                            i += 1;
                        }
                    }
                    "proto" => {
                        if i + 1 < parts.len() {
                            protocol = match parts[i + 1] {
                                "kernel" => RouteProtocol::Kernel,
                                "boot" => RouteProtocol::Boot,
                                "static" => RouteProtocol::Static,
                                "dhcp" => RouteProtocol::Dhcp,
                                "ra" => RouteProtocol::Ra,
                                _ => RouteProtocol::Unknown,
                            };
                            i += 1;
                        }
                    }
                    "scope" => {
                        if i + 1 < parts.len() {
                            scope = match parts[i + 1] {
                                "global" => RouteScope::Global,
                                "link" => RouteScope::Link,
                                "host" => RouteScope::Host,
                                "nowhere" => RouteScope::Nowhere,
                                _ => RouteScope::Global,
                            };
                            i += 1;
                        }
                    }
                    "src" => {
                        if i + 1 < parts.len() {
                            source = parts[i + 1].parse().ok();
                            i += 1;
                        }
                    }
                    "local" => route_type = RouteType::Local,
                    "broadcast" => route_type = RouteType::Broadcast,
                    "multicast" => route_type = RouteType::Multicast,
                    "unreachable" => route_type = RouteType::Unreachable,
                    "prohibit" => route_type = RouteType::Prohibit,
                    "blackhole" => route_type = RouteType::Blackhole,
                    _ => {}
                }
                i += 1;
            }

            self.routes.push(Route {
                destination,
                gateway,
                interface,
                metric,
                protocol,
                scope,
                route_type,
                source,
            });
        }

        Ok(())
    }

    async fn refresh_policy_rules(&mut self) -> Result<()> {
        self.policy_rules.clear();

        let output = Command::new("ip")
            .args(["rule", "show"])
            .output()?;

        let stdout = String::from_utf8_lossy(&output.stdout);

        for line in stdout.lines() {
            // Parse format: "32766: from all lookup main"
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() < 2 {
                continue;
            }

            let priority: u32 = parts[0].trim().parse().unwrap_or(0);
            let rule_parts: Vec<&str> = parts[1].split_whitespace().collect();

            if rule_parts.len() < 3 {
                continue;
            }

            let selector = match rule_parts[0] {
                "from" => RuleSelector::From(rule_parts[1].to_string()),
                "to" => RuleSelector::To(rule_parts[1].to_string()),
                "fwmark" => {
                    let mark = rule_parts[1].trim_start_matches("0x")
                        .parse()
                        .unwrap_or(0);
                    RuleSelector::Fwmark(mark)
                }
                "iif" => RuleSelector::Iif(rule_parts[1].to_string()),
                "oif" => RuleSelector::Oif(rule_parts[1].to_string()),
                _ => continue,
            };

            let action = if let Some(pos) = rule_parts.iter().position(|&p| p == "lookup") {
                if pos + 1 < rule_parts.len() {
                    let table = match rule_parts[pos + 1] {
                        "main" => 254,
                        "local" => 255,
                        "default" => 253,
                        t => t.parse().unwrap_or(0),
                    };
                    RuleAction::Table(table)
                } else {
                    continue;
                }
            } else if rule_parts.contains(&"unreachable") {
                RuleAction::Unreachable
            } else if rule_parts.contains(&"blackhole") {
                RuleAction::Blackhole
            } else if rule_parts.contains(&"prohibit") {
                RuleAction::Prohibit
            } else {
                continue;
            };

            self.policy_rules.push(PolicyRule {
                priority,
                selector,
                action,
            });
        }

        Ok(())
    }

    /// List all routes
    pub fn list(&self) -> &[Route] {
        &self.routes
    }

    /// Find route for destination
    pub fn lookup(&self, dest: &IpAddr) -> Option<&Route> {
        // Simple longest-prefix match
        self.routes
            .iter()
            .filter(|r| r.destination == "default" || matches_destination(dest, &r.destination))
            .min_by_key(|r| r.metric)
    }

    /// Add a route
    pub async fn add(&mut self, route: &Route) -> Result<()> {
        let mut args = vec!["route", "add", &route.destination];

        let gw_str = route.gateway.map(|gw| gw.to_string());
        if let Some(ref gw) = gw_str {
            args.push("via");
            args.push(gw);
        }

        if !route.interface.is_empty() {
            args.push("dev");
            args.push(&route.interface);
        }

        let metric_str = route.metric.to_string();
        if route.metric > 0 {
            args.push("metric");
            args.push(&metric_str);
        }

        let output = Command::new("ip")
            .args(&args)
            .output()?;

        if !output.status.success() {
            return Err(anyhow!("Failed to add route: {}",
                String::from_utf8_lossy(&output.stderr)));
        }

        self.routes.push(route.clone());
        tracing::info!("Added route to {}", route.destination);
        Ok(())
    }

    /// Remove a route
    pub async fn remove(&mut self, destination: &str) -> Result<()> {
        let output = Command::new("ip")
            .args(["route", "del", destination])
            .output()?;

        if !output.status.success() {
            return Err(anyhow!("Failed to remove route: {}",
                String::from_utf8_lossy(&output.stderr)));
        }

        self.routes.retain(|r| r.destination != destination);
        tracing::info!("Removed route to {}", destination);
        Ok(())
    }

    /// Set default gateway
    pub async fn set_default_gateway(&mut self, gateway: IpAddr, interface: &str) -> Result<()> {
        // Remove existing default route
        let _ = self.remove("default").await;

        let route = Route {
            destination: "default".to_string(),
            gateway: Some(gateway),
            interface: interface.to_string(),
            metric: 0,
            protocol: RouteProtocol::Static,
            scope: RouteScope::Global,
            route_type: RouteType::Unicast,
            source: None,
        };

        self.add(&route).await
    }

    /// Add policy rule
    pub async fn add_policy_rule(&mut self, rule: &PolicyRule) -> Result<()> {
        let mut args = vec!["rule", "add"];

        let priority_str = rule.priority.to_string();
        args.push("priority");
        args.push(&priority_str);

        let selector_str;
        match &rule.selector {
            RuleSelector::From(addr) => {
                args.push("from");
                args.push(addr);
            }
            RuleSelector::To(addr) => {
                args.push("to");
                args.push(addr);
            }
            RuleSelector::Fwmark(mark) => {
                selector_str = format!("0x{:x}", mark);
                args.push("fwmark");
                args.push(&selector_str);
            }
            RuleSelector::Iif(iface) => {
                args.push("iif");
                args.push(iface);
            }
            RuleSelector::Oif(iface) => {
                args.push("oif");
                args.push(iface);
            }
        }

        let table_str;
        match &rule.action {
            RuleAction::Table(table) => {
                table_str = table.to_string();
                args.push("table");
                args.push(&table_str);
            }
            RuleAction::Unreachable => args.push("unreachable"),
            RuleAction::Blackhole => args.push("blackhole"),
            RuleAction::Prohibit => args.push("prohibit"),
        }

        let output = Command::new("ip")
            .args(&args)
            .output()?;

        if !output.status.success() {
            return Err(anyhow!("Failed to add policy rule: {}",
                String::from_utf8_lossy(&output.stderr)));
        }

        self.policy_rules.push(rule.clone());
        Ok(())
    }

    /// List policy rules
    pub fn list_policy_rules(&self) -> &[PolicyRule] {
        &self.policy_rules
    }
}

impl Default for RoutingTable {
    fn default() -> Self {
        Self::new()
    }
}

fn matches_destination(ip: &IpAddr, dest: &str) -> bool {
    if dest == "default" {
        return true;
    }

    if let Some((network, prefix_str)) = dest.split_once('/') {
        let prefix: u8 = prefix_str.parse().unwrap_or(32);

        match (ip, network.parse::<IpAddr>()) {
            (IpAddr::V4(ip), Ok(IpAddr::V4(net))) => {
                let ip_bits = u32::from(*ip);
                let net_bits = u32::from(net);
                let mask = !0u32 << (32 - prefix);
                (ip_bits & mask) == (net_bits & mask)
            }
            (IpAddr::V6(ip), Ok(IpAddr::V6(net))) => {
                let ip_bits = u128::from(*ip);
                let net_bits = u128::from(net);
                let mask = !0u128 << (128 - prefix);
                (ip_bits & mask) == (net_bits & mask)
            }
            _ => false,
        }
    } else {
        ip.to_string() == dest
    }
}
