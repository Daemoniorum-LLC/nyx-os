//! IPC server for Arachne

use crate::dns::DnsResolver;
use crate::firewall::Firewall;
use crate::interfaces::InterfaceManager;
use crate::monitor::NetworkMonitor;
use crate::routing::RoutingTable;
use crate::vpn::VpnManager;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::RwLock;

/// IPC request types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum IpcRequest {
    // Firewall operations
    FirewallStatus,
    FirewallAddRule { name: String, rule: FirewallRuleSpec },
    FirewallRemoveRule { name: String },
    FirewallBlockIp { ip: String, reason: String },
    FirewallUnblockIp { ip: String },
    FirewallAllowPort { port: u16, protocol: String },

    // DNS operations
    DnsResolve { hostname: String },
    DnsBlockDomain { domain: String },
    DnsUnblockDomain { domain: String },
    DnsClearCache,
    DnsStats,

    // Interface operations
    InterfaceList,
    InterfaceGet { name: String },
    InterfaceUp { name: String },
    InterfaceDown { name: String },
    InterfaceSetMtu { name: String, mtu: u32 },

    // Routing operations
    RouteList,
    RouteAdd { destination: String, gateway: Option<String>, interface: String },
    RouteRemove { destination: String },
    SetDefaultGateway { gateway: String, interface: String },

    // Monitor operations
    GetConnections,
    GetBandwidth { interface: String },
    GetConnectionStats,
    FindPortOwner { port: u16, protocol: String },

    // VPN operations
    VpnList,
    VpnConnect { name: String },
    VpnDisconnect { name: String },
    VpnStatus { name: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirewallRuleSpec {
    pub direction: String,
    pub action: String,
    pub protocol: Option<String>,
    pub port: Option<u16>,
    pub source: Option<String>,
    pub destination: Option<String>,
}

/// IPC response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum IpcResponse {
    Success { data: serde_json::Value },
    Error { message: String },
}

/// IPC server
pub struct IpcServer {
    firewall: Arc<Firewall>,
    dns: Arc<DnsResolver>,
    interfaces: Arc<RwLock<InterfaceManager>>,
    routing: Arc<RwLock<RoutingTable>>,
    monitor: Arc<NetworkMonitor>,
    vpn: Arc<VpnManager>,
}

impl IpcServer {
    pub fn new(
        firewall: Arc<Firewall>,
        dns: Arc<DnsResolver>,
        interfaces: Arc<RwLock<InterfaceManager>>,
        routing: Arc<RwLock<RoutingTable>>,
        monitor: Arc<NetworkMonitor>,
        vpn: Arc<VpnManager>,
    ) -> Self {
        Self {
            firewall,
            dns,
            interfaces,
            routing,
            monitor,
            vpn,
        }
    }

    /// Start the IPC server
    pub async fn start(&self, socket_path: &Path) -> Result<()> {
        // Remove existing socket
        let _ = std::fs::remove_file(socket_path);

        let listener = UnixListener::bind(socket_path)?;
        tracing::info!("Arachne IPC server listening on {:?}", socket_path);

        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let firewall = Arc::clone(&self.firewall);
                    let dns = Arc::clone(&self.dns);
                    let interfaces = Arc::clone(&self.interfaces);
                    let routing = Arc::clone(&self.routing);
                    let monitor = Arc::clone(&self.monitor);
                    let vpn = Arc::clone(&self.vpn);

                    tokio::spawn(async move {
                        if let Err(e) = handle_client(
                            stream, firewall, dns, interfaces, routing, monitor, vpn
                        ).await {
                            tracing::error!("Client error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    tracing::error!("Accept error: {}", e);
                }
            }
        }
    }
}

async fn handle_client(
    stream: UnixStream,
    firewall: Arc<Firewall>,
    dns: Arc<DnsResolver>,
    interfaces: Arc<RwLock<InterfaceManager>>,
    routing: Arc<RwLock<RoutingTable>>,
    monitor: Arc<NetworkMonitor>,
    vpn: Arc<VpnManager>,
) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    while reader.read_line(&mut line).await? > 0 {
        let response = match serde_json::from_str::<IpcRequest>(&line) {
            Ok(request) => {
                process_request(
                    request,
                    &firewall,
                    &dns,
                    &interfaces,
                    &routing,
                    &monitor,
                    &vpn,
                ).await
            }
            Err(e) => IpcResponse::Error {
                message: format!("Invalid request: {}", e),
            },
        };

        let response_json = serde_json::to_string(&response)?;
        writer.write_all(response_json.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;

        line.clear();
    }

    Ok(())
}

async fn process_request(
    request: IpcRequest,
    firewall: &Firewall,
    dns: &DnsResolver,
    interfaces: &RwLock<InterfaceManager>,
    routing: &RwLock<RoutingTable>,
    monitor: &NetworkMonitor,
    vpn: &VpnManager,
) -> IpcResponse {
    match request {
        // Firewall operations
        IpcRequest::FirewallStatus => {
            match firewall.get_stats().await {
                Ok(stats) => IpcResponse::Success {
                    data: serde_json::json!({
                        "enabled": stats.enabled,
                        "rules_count": stats.rules_count,
                        "packets_accepted": stats.packets_accepted,
                        "packets_dropped": stats.packets_dropped,
                    }),
                },
                Err(e) => IpcResponse::Error { message: e.to_string() },
            }
        }

        IpcRequest::FirewallBlockIp { ip, reason } => {
            match firewall.block_ip(&ip, &reason).await {
                Ok(()) => IpcResponse::Success {
                    data: serde_json::json!({"blocked": ip}),
                },
                Err(e) => IpcResponse::Error { message: e.to_string() },
            }
        }

        IpcRequest::FirewallUnblockIp { ip } => {
            match firewall.unblock_ip(&ip).await {
                Ok(()) => IpcResponse::Success {
                    data: serde_json::json!({"unblocked": ip}),
                },
                Err(e) => IpcResponse::Error { message: e.to_string() },
            }
        }

        // DNS operations
        IpcRequest::DnsResolve { hostname } => {
            match dns.resolve(&hostname).await {
                Ok(ips) => IpcResponse::Success {
                    data: serde_json::json!({
                        "hostname": hostname,
                        "addresses": ips.iter().map(|ip| ip.to_string()).collect::<Vec<_>>(),
                    }),
                },
                Err(e) => IpcResponse::Error { message: e.to_string() },
            }
        }

        IpcRequest::DnsBlockDomain { domain } => {
            dns.block_domain(&domain).await;
            IpcResponse::Success {
                data: serde_json::json!({"blocked": domain}),
            }
        }

        IpcRequest::DnsUnblockDomain { domain } => {
            dns.unblock_domain(&domain).await;
            IpcResponse::Success {
                data: serde_json::json!({"unblocked": domain}),
            }
        }

        IpcRequest::DnsClearCache => {
            dns.clear_cache().await;
            IpcResponse::Success {
                data: serde_json::json!({"cleared": true}),
            }
        }

        IpcRequest::DnsStats => {
            let stats = dns.get_stats().await;
            IpcResponse::Success {
                data: serde_json::json!({
                    "cache_size": stats.cache_size,
                    "cache_max": stats.cache_max,
                    "total_hits": stats.total_hits,
                    "blocklist_size": stats.blocklist_size,
                }),
            }
        }

        // Interface operations
        IpcRequest::InterfaceList => {
            let manager = interfaces.read().await;
            let ifaces: Vec<_> = manager.list().iter().map(|i| {
                serde_json::json!({
                    "name": i.name,
                    "mac": i.mac_address,
                    "state": format!("{:?}", i.state),
                    "mtu": i.mtu,
                    "ipv4": i.ipv4_addresses.iter().map(|a| a.address.to_string()).collect::<Vec<_>>(),
                    "ipv6": i.ipv6_addresses.iter().map(|a| a.address.to_string()).collect::<Vec<_>>(),
                })
            }).collect();

            IpcResponse::Success {
                data: serde_json::json!({"interfaces": ifaces}),
            }
        }

        IpcRequest::InterfaceUp { name } => {
            let manager = interfaces.read().await;
            match manager.bring_up(&name).await {
                Ok(()) => IpcResponse::Success {
                    data: serde_json::json!({"interface": name, "state": "up"}),
                },
                Err(e) => IpcResponse::Error { message: e.to_string() },
            }
        }

        IpcRequest::InterfaceDown { name } => {
            let manager = interfaces.read().await;
            match manager.bring_down(&name).await {
                Ok(()) => IpcResponse::Success {
                    data: serde_json::json!({"interface": name, "state": "down"}),
                },
                Err(e) => IpcResponse::Error { message: e.to_string() },
            }
        }

        // Routing operations
        IpcRequest::RouteList => {
            let table = routing.read().await;
            let routes: Vec<_> = table.list().iter().map(|r| {
                serde_json::json!({
                    "destination": r.destination,
                    "gateway": r.gateway.map(|g| g.to_string()),
                    "interface": r.interface,
                    "metric": r.metric,
                })
            }).collect();

            IpcResponse::Success {
                data: serde_json::json!({"routes": routes}),
            }
        }

        // Monitor operations
        IpcRequest::GetConnections => {
            let connections = monitor.get_connections().await;
            let conns: Vec<_> = connections.iter().map(|c| {
                serde_json::json!({
                    "protocol": format!("{:?}", c.protocol),
                    "local": format!("{}:{}", c.local_addr, c.local_port),
                    "remote": c.remote_addr.map(|a| format!("{}:{}", a, c.remote_port.unwrap_or(0))),
                    "state": format!("{:?}", c.state),
                    "pid": c.pid,
                    "process": c.process_name,
                })
            }).collect();

            IpcResponse::Success {
                data: serde_json::json!({"connections": conns}),
            }
        }

        IpcRequest::GetBandwidth { interface } => {
            match monitor.get_bandwidth(&interface).await {
                Some(sample) => IpcResponse::Success {
                    data: serde_json::json!({
                        "interface": interface,
                        "rx_rate": sample.rx_rate,
                        "tx_rate": sample.tx_rate,
                        "rx_bytes": sample.rx_bytes,
                        "tx_bytes": sample.tx_bytes,
                    }),
                },
                None => IpcResponse::Error {
                    message: format!("No data for interface: {}", interface),
                },
            }
        }

        IpcRequest::GetConnectionStats => {
            let stats = monitor.get_connection_stats().await;
            IpcResponse::Success {
                data: serde_json::json!({
                    "tcp_total": stats.tcp_total,
                    "tcp_established": stats.tcp_established,
                    "tcp_listening": stats.tcp_listening,
                    "udp_total": stats.udp_total,
                }),
            }
        }

        // VPN operations
        IpcRequest::VpnList => {
            let connections = vpn.list().await;
            let vpns: Vec<_> = connections.iter().map(|c| {
                serde_json::json!({
                    "interface": c.interface,
                    "type": format!("{:?}", c.vpn_type),
                    "status": format!("{:?}", c.status),
                    "address": c.local_address,
                    "peers": c.peers.len(),
                })
            }).collect();

            IpcResponse::Success {
                data: serde_json::json!({"vpns": vpns}),
            }
        }

        IpcRequest::VpnConnect { name } => {
            match vpn.connect(&name).await {
                Ok(()) => IpcResponse::Success {
                    data: serde_json::json!({"connected": name}),
                },
                Err(e) => IpcResponse::Error { message: e.to_string() },
            }
        }

        IpcRequest::VpnDisconnect { name } => {
            match vpn.disconnect(&name).await {
                Ok(()) => IpcResponse::Success {
                    data: serde_json::json!({"disconnected": name}),
                },
                Err(e) => IpcResponse::Error { message: e.to_string() },
            }
        }

        IpcRequest::VpnStatus { name } => {
            match vpn.status(&name).await {
                Ok(status) => IpcResponse::Success {
                    data: serde_json::json!({
                        "interface": status.interface,
                        "status": format!("{:?}", status.status),
                        "address": status.local_address,
                        "rx_bytes": status.rx_bytes,
                        "tx_bytes": status.tx_bytes,
                        "peers": status.peers.iter().map(|p| {
                            serde_json::json!({
                                "public_key": &p.public_key[..8],
                                "endpoint": p.endpoint,
                                "rx": p.rx_bytes,
                                "tx": p.tx_bytes,
                            })
                        }).collect::<Vec<_>>(),
                    }),
                },
                Err(e) => IpcResponse::Error { message: e.to_string() },
            }
        }

        _ => IpcResponse::Error {
            message: "Not implemented".to_string(),
        },
    }
}

/// IPC client for other components
pub struct IpcClient {
    socket_path: std::path::PathBuf,
}

impl IpcClient {
    pub fn new(socket_path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            socket_path: socket_path.into(),
        }
    }

    pub async fn send(&self, request: IpcRequest) -> Result<IpcResponse> {
        let mut stream = UnixStream::connect(&self.socket_path).await?;

        let request_json = serde_json::to_string(&request)?;
        stream.write_all(request_json.as_bytes()).await?;
        stream.write_all(b"\n").await?;
        stream.flush().await?;

        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).await?;

        Ok(serde_json::from_str(&line)?)
    }
}
