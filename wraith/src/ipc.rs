//! IPC interface for Wraith

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::RwLock;
use tracing::{info, error, debug};

use crate::state::WraithState;
use crate::interface::NetworkInterface;
use crate::profile::{NetworkProfile, IpConfig};

/// IPC request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum IpcRequest {
    /// List network interfaces
    ListInterfaces,

    /// Get interface details
    GetInterface { name: String },

    /// Set interface up/down
    SetInterfaceState { name: String, up: bool },

    /// Set interface address
    SetAddress { interface: String, address: String },

    /// Start DHCP on interface
    StartDhcp { interface: String },

    /// Get DNS servers
    GetDns,

    /// Set DNS servers
    SetDns { servers: Vec<String> },

    /// Scan WiFi networks
    WifiScan { interface: String },

    /// Connect to WiFi
    WifiConnect {
        interface: String,
        ssid: String,
        password: Option<String>,
    },

    /// Disconnect WiFi
    WifiDisconnect { interface: String },

    /// List profiles
    ListProfiles,

    /// Apply profile
    ApplyProfile { interface: String, profile: String },

    /// Create/update profile
    SaveProfile { profile: NetworkProfile },

    /// Delete profile
    DeleteProfile { name: String },

    /// Get overall status
    GetStatus,
}

/// IPC response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum IpcResponse {
    Success { message: String },
    Interfaces(Vec<InterfaceInfo>),
    Interface(InterfaceInfo),
    DnsServers(Vec<String>),
    WifiNetworks(Vec<WifiNetworkInfo>),
    Profiles(Vec<ProfileInfo>),
    Status(NetworkStatus),
    Error { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterfaceInfo {
    pub name: String,
    pub mac_address: Option<String>,
    pub addresses: Vec<String>,
    pub up: bool,
    pub running: bool,
    pub interface_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WifiNetworkInfo {
    pub ssid: String,
    pub signal: i32,
    pub security: String,
    pub connected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileInfo {
    pub name: String,
    pub interface_match: String,
    pub config_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkStatus {
    pub interfaces: Vec<InterfaceInfo>,
    pub dns_servers: Vec<String>,
    pub hostname: String,
}

impl From<&NetworkInterface> for InterfaceInfo {
    fn from(iface: &NetworkInterface) -> Self {
        Self {
            name: iface.name.clone(),
            mac_address: iface.mac_address.clone(),
            addresses: iface.addresses.iter()
                .map(|a| format!("{}/{}", a.address, a.prefix_len))
                .collect(),
            up: iface.flags.up,
            running: iface.flags.running,
            interface_type: format!("{:?}", iface.interface_type),
        }
    }
}

/// IPC server
pub struct WraithServer {
    socket_path: String,
    state: Arc<RwLock<WraithState>>,
}

impl WraithServer {
    pub fn new(socket_path: &str, state: Arc<RwLock<WraithState>>) -> Self {
        Self {
            socket_path: socket_path.to_string(),
            state,
        }
    }

    pub async fn run(&self) -> Result<()> {
        let _ = std::fs::remove_file(&self.socket_path);

        if let Some(parent) = std::path::Path::new(&self.socket_path).parent() {
            std::fs::create_dir_all(parent)?;
        }

        let listener = UnixListener::bind(&self.socket_path)?;

        info!("Wraith IPC listening on {}", self.socket_path);

        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let state = self.state.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_client(stream, state).await {
                            error!("Client error: {}", e);
                        }
                    });
                }
                Err(e) => error!("Accept error: {}", e),
            }
        }
    }
}

async fn handle_client(
    stream: UnixStream,
    state: Arc<RwLock<WraithState>>,
) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    while reader.read_line(&mut line).await? > 0 {
        let response = match serde_json::from_str::<IpcRequest>(&line) {
            Ok(request) => process_request(request, &state).await,
            Err(e) => IpcResponse::Error { message: e.to_string() },
        };

        let json = serde_json::to_string(&response)?;
        writer.write_all(json.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;

        line.clear();
    }

    Ok(())
}

async fn process_request(
    request: IpcRequest,
    state: &RwLock<WraithState>,
) -> IpcResponse {
    match request {
        IpcRequest::ListInterfaces => {
            let state = state.read().await;
            match state.interfaces.list() {
                Ok(interfaces) => {
                    let infos: Vec<InterfaceInfo> = interfaces.iter()
                        .map(InterfaceInfo::from)
                        .collect();
                    IpcResponse::Interfaces(infos)
                }
                Err(e) => IpcResponse::Error { message: e.to_string() },
            }
        }

        IpcRequest::GetInterface { name } => {
            let state = state.read().await;
            match state.interfaces.get(&name) {
                Some(iface) => IpcResponse::Interface(InterfaceInfo::from(iface)),
                None => IpcResponse::Error {
                    message: format!("Interface not found: {}", name),
                },
            }
        }

        IpcRequest::SetInterfaceState { name, up } => {
            let mut state = state.write().await;
            match state.interfaces.set_up(&name, up).await {
                Ok(()) => IpcResponse::Success {
                    message: format!("{} is now {}", name, if up { "up" } else { "down" }),
                },
                Err(e) => IpcResponse::Error { message: e.to_string() },
            }
        }

        IpcRequest::SetAddress { interface, address } => {
            let mut state = state.write().await;
            match state.interfaces.set_address(&interface, &address).await {
                Ok(()) => IpcResponse::Success {
                    message: format!("Address set on {}", interface),
                },
                Err(e) => IpcResponse::Error { message: e.to_string() },
            }
        }

        IpcRequest::StartDhcp { interface } => {
            let mut state = state.write().await;
            match state.start_dhcp(&interface).await {
                Ok(()) => IpcResponse::Success {
                    message: format!("DHCP started on {}", interface),
                },
                Err(e) => IpcResponse::Error { message: e.to_string() },
            }
        }

        IpcRequest::GetDns => {
            let state = state.read().await;
            IpcResponse::DnsServers(state.dns.get_servers().to_vec())
        }

        IpcRequest::SetDns { servers } => {
            let mut state = state.write().await;
            match state.dns.set_servers(&servers) {
                Ok(()) => IpcResponse::Success {
                    message: "DNS servers updated".to_string(),
                },
                Err(e) => IpcResponse::Error { message: e.to_string() },
            }
        }

        IpcRequest::ListProfiles => {
            let state = state.read().await;
            let profiles: Vec<ProfileInfo> = state.profiles.list()
                .iter()
                .map(|p| ProfileInfo {
                    name: p.name.clone(),
                    interface_match: p.interface_match.clone(),
                    config_type: match &p.config {
                        IpConfig::Dhcp => "DHCP".to_string(),
                        IpConfig::Static { .. } => "Static".to_string(),
                    },
                })
                .collect();
            IpcResponse::Profiles(profiles)
        }

        IpcRequest::ApplyProfile { interface, profile } => {
            let mut state = state.write().await;
            if let Some(prof) = state.profiles.get(&profile).cloned() {
                match state.apply_profile(&interface, &prof).await {
                    Ok(()) => IpcResponse::Success {
                        message: format!("Applied {} to {}", profile, interface),
                    },
                    Err(e) => IpcResponse::Error { message: e.to_string() },
                }
            } else {
                IpcResponse::Error {
                    message: format!("Profile not found: {}", profile),
                }
            }
        }

        IpcRequest::SaveProfile { profile } => {
            let mut state = state.write().await;
            match state.profiles.add(profile.clone()) {
                Ok(()) => IpcResponse::Success {
                    message: format!("Saved profile: {}", profile.name),
                },
                Err(e) => IpcResponse::Error { message: e.to_string() },
            }
        }

        IpcRequest::DeleteProfile { name } => {
            let mut state = state.write().await;
            match state.profiles.delete(&name) {
                Ok(()) => IpcResponse::Success {
                    message: format!("Deleted profile: {}", name),
                },
                Err(e) => IpcResponse::Error { message: e.to_string() },
            }
        }

        IpcRequest::GetStatus => {
            let state = state.read().await;
            let interfaces = state.interfaces.list()
                .unwrap_or_default()
                .iter()
                .map(InterfaceInfo::from)
                .collect();

            IpcResponse::Status(NetworkStatus {
                interfaces,
                dns_servers: state.dns.get_servers().to_vec(),
                hostname: state.config.hostname.clone(),
            })
        }

        IpcRequest::WifiScan { .. } |
        IpcRequest::WifiConnect { .. } |
        IpcRequest::WifiDisconnect { .. } => {
            IpcResponse::Error {
                message: "WiFi not yet implemented".to_string(),
            }
        }
    }
}
