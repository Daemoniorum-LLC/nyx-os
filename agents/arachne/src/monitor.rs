//! Network monitoring and statistics

use crate::interfaces::{InterfaceManager, InterfaceStats};
use anyhow::Result;
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Network monitor
pub struct NetworkMonitor {
    interface_manager: Arc<RwLock<InterfaceManager>>,
    connections: Arc<RwLock<Vec<Connection>>>,
    bandwidth_history: Arc<RwLock<HashMap<String, Vec<BandwidthSample>>>>,
    interval: Duration,
}

#[derive(Debug, Clone)]
pub struct Connection {
    pub protocol: Protocol,
    pub local_addr: IpAddr,
    pub local_port: u16,
    pub remote_addr: Option<IpAddr>,
    pub remote_port: Option<u16>,
    pub state: ConnectionState,
    pub pid: Option<u32>,
    pub process_name: Option<String>,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Protocol {
    Tcp,
    Udp,
    Icmp,
    Raw,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConnectionState {
    Established,
    SynSent,
    SynRecv,
    FinWait1,
    FinWait2,
    TimeWait,
    Close,
    CloseWait,
    LastAck,
    Listen,
    Closing,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct BandwidthSample {
    pub timestamp: Instant,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub rx_rate: f64,  // bytes per second
    pub tx_rate: f64,
}

impl NetworkMonitor {
    pub fn new(interface_manager: Arc<RwLock<InterfaceManager>>, interval_secs: u64) -> Self {
        Self {
            interface_manager,
            connections: Arc::new(RwLock::new(Vec::new())),
            bandwidth_history: Arc::new(RwLock::new(HashMap::new())),
            interval: Duration::from_secs(interval_secs),
        }
    }

    /// Start monitoring
    pub async fn start(&self) {
        let connections = Arc::clone(&self.connections);
        let bandwidth = Arc::clone(&self.bandwidth_history);
        let iface_manager = Arc::clone(&self.interface_manager);
        let interval = self.interval;

        tokio::spawn(async move {
            let mut previous_stats: HashMap<String, InterfaceStats> = HashMap::new();
            let mut last_sample = Instant::now();

            loop {
                // Update connections
                if let Ok(conns) = get_connections().await {
                    *connections.write().await = conns;
                }

                // Update bandwidth
                {
                    let mut manager = iface_manager.write().await;
                    if manager.refresh().await.is_ok() {
                        let now = Instant::now();
                        let elapsed = now.duration_since(last_sample).as_secs_f64();

                        let mut bw = bandwidth.write().await;

                        for iface in manager.list() {
                            let rx_rate;
                            let tx_rate;

                            if let Some(prev) = previous_stats.get(&iface.name) {
                                rx_rate = (iface.stats.rx_bytes.saturating_sub(prev.rx_bytes)) as f64 / elapsed;
                                tx_rate = (iface.stats.tx_bytes.saturating_sub(prev.tx_bytes)) as f64 / elapsed;
                            } else {
                                rx_rate = 0.0;
                                tx_rate = 0.0;
                            }

                            let sample = BandwidthSample {
                                timestamp: now,
                                rx_bytes: iface.stats.rx_bytes,
                                tx_bytes: iface.stats.tx_bytes,
                                rx_rate,
                                tx_rate,
                            };

                            let history = bw.entry(iface.name.clone()).or_insert_with(Vec::new);
                            history.push(sample);

                            // Keep only last hour of samples
                            let cutoff = now - Duration::from_secs(3600);
                            history.retain(|s| s.timestamp > cutoff);

                            previous_stats.insert(iface.name.clone(), iface.stats.clone());
                        }

                        last_sample = now;
                    }
                }

                tokio::time::sleep(interval).await;
            }
        });
    }

    /// Get current connections
    pub async fn get_connections(&self) -> Vec<Connection> {
        self.connections.read().await.clone()
    }

    /// Get connections for a specific process
    pub async fn get_process_connections(&self, pid: u32) -> Vec<Connection> {
        self.connections
            .read()
            .await
            .iter()
            .filter(|c| c.pid == Some(pid))
            .cloned()
            .collect()
    }

    /// Get current bandwidth for interface
    pub async fn get_bandwidth(&self, interface: &str) -> Option<BandwidthSample> {
        self.bandwidth_history
            .read()
            .await
            .get(interface)
            .and_then(|h| h.last().cloned())
    }

    /// Get bandwidth history for interface
    pub async fn get_bandwidth_history(&self, interface: &str, duration: Duration) -> Vec<BandwidthSample> {
        let cutoff = Instant::now() - duration;

        self.bandwidth_history
            .read()
            .await
            .get(interface)
            .map(|h| h.iter().filter(|s| s.timestamp > cutoff).cloned().collect())
            .unwrap_or_default()
    }

    /// Get total bandwidth across all interfaces
    pub async fn get_total_bandwidth(&self) -> (f64, f64) {
        let bw = self.bandwidth_history.read().await;

        let mut total_rx = 0.0;
        let mut total_tx = 0.0;

        for history in bw.values() {
            if let Some(sample) = history.last() {
                total_rx += sample.rx_rate;
                total_tx += sample.tx_rate;
            }
        }

        (total_rx, total_tx)
    }

    /// Get connection statistics
    pub async fn get_connection_stats(&self) -> ConnectionStats {
        let connections = self.connections.read().await;

        let mut stats = ConnectionStats::default();

        for conn in connections.iter() {
            match conn.protocol {
                Protocol::Tcp => {
                    stats.tcp_total += 1;
                    match conn.state {
                        ConnectionState::Established => stats.tcp_established += 1,
                        ConnectionState::Listen => stats.tcp_listening += 1,
                        ConnectionState::TimeWait => stats.tcp_time_wait += 1,
                        _ => {}
                    }
                }
                Protocol::Udp => stats.udp_total += 1,
                _ => {}
            }
        }

        stats
    }

    /// Find process using a port
    pub async fn find_port_owner(&self, port: u16, protocol: Protocol) -> Option<(u32, String)> {
        self.connections
            .read()
            .await
            .iter()
            .find(|c| c.local_port == port && c.protocol == protocol)
            .and_then(|c| c.pid.map(|p| (p, c.process_name.clone().unwrap_or_default())))
    }
}

#[derive(Debug, Clone, Default)]
pub struct ConnectionStats {
    pub tcp_total: usize,
    pub tcp_established: usize,
    pub tcp_listening: usize,
    pub tcp_time_wait: usize,
    pub udp_total: usize,
}

/// Parse /proc/net/tcp and /proc/net/udp for connections
async fn get_connections() -> Result<Vec<Connection>> {
    let mut connections = Vec::new();

    // Parse TCP connections
    if let Ok(content) = tokio::fs::read_to_string("/proc/net/tcp").await {
        parse_proc_net(&content, Protocol::Tcp, &mut connections);
    }

    if let Ok(content) = tokio::fs::read_to_string("/proc/net/tcp6").await {
        parse_proc_net(&content, Protocol::Tcp, &mut connections);
    }

    // Parse UDP connections
    if let Ok(content) = tokio::fs::read_to_string("/proc/net/udp").await {
        parse_proc_net(&content, Protocol::Udp, &mut connections);
    }

    if let Ok(content) = tokio::fs::read_to_string("/proc/net/udp6").await {
        parse_proc_net(&content, Protocol::Udp, &mut connections);
    }

    // Try to resolve PIDs
    resolve_connection_pids(&mut connections).await;

    Ok(connections)
}

fn parse_proc_net(content: &str, protocol: Protocol, connections: &mut Vec<Connection>) {
    for line in content.lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 10 {
            continue;
        }

        // Parse local address
        let (local_addr, local_port) = match parse_addr_port(parts[1]) {
            Some(v) => v,
            None => continue,
        };

        // Parse remote address
        let (remote_addr, remote_port) = parse_addr_port(parts[2]).unzip();

        // Parse state (for TCP)
        let state = if protocol == Protocol::Tcp {
            parse_tcp_state(parts[3])
        } else {
            ConnectionState::Unknown
        };

        // Parse inode for PID lookup
        let inode: u64 = parts[9].parse().unwrap_or(0);

        connections.push(Connection {
            protocol,
            local_addr,
            local_port,
            remote_addr,
            remote_port,
            state,
            pid: None,
            process_name: None,
            rx_bytes: 0,
            tx_bytes: 0,
        });
    }
}

fn parse_addr_port(s: &str) -> Option<(IpAddr, u16)> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 2 {
        return None;
    }

    let port = u16::from_str_radix(parts[1], 16).ok()?;

    // Parse hex IP address
    let addr_hex = parts[0];
    let addr = if addr_hex.len() == 8 {
        // IPv4
        let bytes: [u8; 4] = [
            u8::from_str_radix(&addr_hex[6..8], 16).ok()?,
            u8::from_str_radix(&addr_hex[4..6], 16).ok()?,
            u8::from_str_radix(&addr_hex[2..4], 16).ok()?,
            u8::from_str_radix(&addr_hex[0..2], 16).ok()?,
        ];
        IpAddr::V4(std::net::Ipv4Addr::from(bytes))
    } else if addr_hex.len() == 32 {
        // IPv6
        let mut bytes = [0u8; 16];
        for i in 0..16 {
            bytes[i] = u8::from_str_radix(&addr_hex[i * 2..i * 2 + 2], 16).ok()?;
        }
        // Handle endianness
        IpAddr::V6(std::net::Ipv6Addr::from(bytes))
    } else {
        return None;
    };

    Some((addr, port))
}

fn parse_tcp_state(hex: &str) -> ConnectionState {
    match u8::from_str_radix(hex, 16).unwrap_or(0) {
        0x01 => ConnectionState::Established,
        0x02 => ConnectionState::SynSent,
        0x03 => ConnectionState::SynRecv,
        0x04 => ConnectionState::FinWait1,
        0x05 => ConnectionState::FinWait2,
        0x06 => ConnectionState::TimeWait,
        0x07 => ConnectionState::Close,
        0x08 => ConnectionState::CloseWait,
        0x09 => ConnectionState::LastAck,
        0x0A => ConnectionState::Listen,
        0x0B => ConnectionState::Closing,
        _ => ConnectionState::Unknown,
    }
}

async fn resolve_connection_pids(connections: &mut Vec<Connection>) {
    // Build inode -> pid map from /proc
    let mut inode_pid: HashMap<u64, (u32, String)> = HashMap::new();

    if let Ok(mut entries) = tokio::fs::read_dir("/proc").await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();

            // Check if directory is a PID
            if let Ok(pid) = name_str.parse::<u32>() {
                let fd_path = entry.path().join("fd");

                if let Ok(mut fd_entries) = tokio::fs::read_dir(&fd_path).await {
                    while let Ok(Some(fd)) = fd_entries.next_entry().await {
                        if let Ok(link) = tokio::fs::read_link(fd.path()).await {
                            let link_str = link.to_string_lossy();
                            if link_str.starts_with("socket:[") {
                                if let Some(inode_str) = link_str
                                    .strip_prefix("socket:[")
                                    .and_then(|s| s.strip_suffix(']'))
                                {
                                    if let Ok(inode) = inode_str.parse() {
                                        // Get process name
                                        let comm_path = entry.path().join("comm");
                                        let name = tokio::fs::read_to_string(&comm_path)
                                            .await
                                            .unwrap_or_default()
                                            .trim()
                                            .to_string();

                                        inode_pid.insert(inode, (pid, name));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Note: We'd need to track inodes during parsing to complete this
    // This is a simplified version
}
