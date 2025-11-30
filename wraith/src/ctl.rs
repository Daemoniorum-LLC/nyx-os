//! wraithctl - Network manager control utility

mod interface;
mod config;
mod dhcp;
mod dns;
mod wifi;
mod profile;
mod ipc;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use crate::ipc::{IpcRequest, IpcResponse};
use crate::profile::{NetworkProfile, IpConfig, ProfileOptions};

#[derive(Parser)]
#[command(name = "wraithctl")]
#[command(about = "Wraith network manager control")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Socket path
    #[arg(long, default_value = "/run/wraith/wraith.sock")]
    socket: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Show network status
    Status,

    /// List network interfaces
    List,

    /// Show interface details
    Show {
        /// Interface name
        interface: String,
    },

    /// Bring interface up
    Up {
        /// Interface name
        interface: String,
    },

    /// Bring interface down
    Down {
        /// Interface name
        interface: String,
    },

    /// Configure interface with DHCP
    Dhcp {
        /// Interface name
        interface: String,
    },

    /// Set static address
    Address {
        /// Interface name
        interface: String,

        /// Address in CIDR notation (e.g., 192.168.1.10/24)
        address: String,

        /// Gateway address
        #[arg(long)]
        gateway: Option<String>,
    },

    /// DNS configuration
    Dns {
        #[command(subcommand)]
        command: DnsCommands,
    },

    /// Profile management
    Profile {
        #[command(subcommand)]
        command: ProfileCommands,
    },

    /// WiFi management
    Wifi {
        #[command(subcommand)]
        command: WifiCommands,
    },
}

#[derive(Subcommand)]
enum DnsCommands {
    /// Show DNS servers
    Show,

    /// Set DNS servers
    Set {
        /// DNS server addresses
        servers: Vec<String>,
    },
}

#[derive(Subcommand)]
enum ProfileCommands {
    /// List profiles
    List,

    /// Create profile
    Create {
        /// Profile name
        name: String,

        /// Interface pattern
        #[arg(long)]
        interface: String,

        /// Use DHCP
        #[arg(long)]
        dhcp: bool,

        /// Static address
        #[arg(long)]
        address: Option<String>,

        /// Gateway
        #[arg(long)]
        gateway: Option<String>,

        /// DNS servers
        #[arg(long)]
        dns: Vec<String>,
    },

    /// Delete profile
    Delete {
        /// Profile name
        name: String,
    },

    /// Apply profile to interface
    Apply {
        /// Profile name
        profile: String,

        /// Interface name
        interface: String,
    },
}

#[derive(Subcommand)]
enum WifiCommands {
    /// Scan for networks
    Scan {
        /// Interface name
        #[arg(default_value = "wlan0")]
        interface: String,
    },

    /// Connect to network
    Connect {
        /// SSID
        ssid: String,

        /// Interface name
        #[arg(long, default_value = "wlan0")]
        interface: String,

        /// Password
        #[arg(short, long)]
        password: Option<String>,
    },

    /// Disconnect
    Disconnect {
        /// Interface name
        #[arg(default_value = "wlan0")]
        interface: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let request = match cli.command {
        Commands::Status => IpcRequest::GetStatus,

        Commands::List => IpcRequest::ListInterfaces,

        Commands::Show { interface } => IpcRequest::GetInterface { name: interface },

        Commands::Up { interface } => IpcRequest::SetInterfaceState {
            name: interface,
            up: true,
        },

        Commands::Down { interface } => IpcRequest::SetInterfaceState {
            name: interface,
            up: false,
        },

        Commands::Dhcp { interface } => IpcRequest::StartDhcp { interface },

        Commands::Address { interface, address, gateway } => {
            // Set address first
            send_request(&cli.socket, IpcRequest::SetAddress {
                interface: interface.clone(),
                address,
            }).await?;

            // Set gateway if provided
            if let Some(gw) = gateway {
                // Would need a SetGateway request
                println!("Gateway: {} (not implemented)", gw);
            }

            return Ok(());
        }

        Commands::Dns { command } => match command {
            DnsCommands::Show => IpcRequest::GetDns,
            DnsCommands::Set { servers } => IpcRequest::SetDns { servers },
        },

        Commands::Profile { command } => match command {
            ProfileCommands::List => IpcRequest::ListProfiles,

            ProfileCommands::Create { name, interface, dhcp, address, gateway, dns } => {
                let config = if dhcp {
                    IpConfig::Dhcp
                } else if let Some(addr) = address {
                    IpConfig::Static {
                        address: addr,
                        gateway,
                        dns,
                    }
                } else {
                    IpConfig::Dhcp
                };

                IpcRequest::SaveProfile {
                    profile: NetworkProfile {
                        name,
                        interface_match: interface,
                        config,
                        priority: 0,
                        options: ProfileOptions::default(),
                    },
                }
            }

            ProfileCommands::Delete { name } => IpcRequest::DeleteProfile { name },

            ProfileCommands::Apply { profile, interface } => {
                IpcRequest::ApplyProfile { interface, profile }
            }
        },

        Commands::Wifi { command } => match command {
            WifiCommands::Scan { interface } => IpcRequest::WifiScan { interface },

            WifiCommands::Connect { ssid, interface, password } => {
                IpcRequest::WifiConnect { interface, ssid, password }
            }

            WifiCommands::Disconnect { interface } => {
                IpcRequest::WifiDisconnect { interface }
            }
        },
    };

    let response = send_request(&cli.socket, request).await?;
    print_response(&response);

    Ok(())
}

async fn send_request(socket_path: &str, request: IpcRequest) -> Result<IpcResponse> {
    let mut stream = UnixStream::connect(socket_path).await?;

    let json = serde_json::to_string(&request)?;
    stream.write_all(json.as_bytes()).await?;
    stream.write_all(b"\n").await?;
    stream.flush().await?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).await?;

    Ok(serde_json::from_str(&line)?)
}

fn print_response(response: &IpcResponse) {
    match response {
        IpcResponse::Success { message } => {
            println!("{}", message);
        }

        IpcResponse::Interfaces(interfaces) => {
            println!("{:<15} {:<17} {:<8} {:<10} {}", "INTERFACE", "MAC", "STATE", "TYPE", "ADDRESSES");
            for iface in interfaces {
                let state = if iface.up { "up" } else { "down" };
                let mac = iface.mac_address.as_deref().unwrap_or("-");
                let addrs = iface.addresses.join(", ");
                println!("{:<15} {:<17} {:<8} {:<10} {}", iface.name, mac, state, iface.interface_type, addrs);
            }
        }

        IpcResponse::Interface(iface) => {
            println!("Interface: {}", iface.name);
            if let Some(mac) = &iface.mac_address {
                println!("  MAC:     {}", mac);
            }
            println!("  State:   {}", if iface.up { "up" } else { "down" });
            println!("  Running: {}", if iface.running { "yes" } else { "no" });
            println!("  Type:    {}", iface.interface_type);
            println!("  Addresses:");
            for addr in &iface.addresses {
                println!("    {}", addr);
            }
        }

        IpcResponse::DnsServers(servers) => {
            println!("DNS Servers:");
            for server in servers {
                println!("  {}", server);
            }
        }

        IpcResponse::WifiNetworks(networks) => {
            println!("{:<32} {:<8} {:<15} {}", "SSID", "SIGNAL", "SECURITY", "CONNECTED");
            for net in networks {
                let connected = if net.connected { "*" } else { "" };
                println!("{:<32} {:<8} {:<15} {}", net.ssid, net.signal, net.security, connected);
            }
        }

        IpcResponse::Profiles(profiles) => {
            println!("{:<20} {:<15} {}", "NAME", "INTERFACE", "TYPE");
            for profile in profiles {
                println!("{:<20} {:<15} {}", profile.name, profile.interface_match, profile.config_type);
            }
        }

        IpcResponse::Status(status) => {
            println!("Hostname: {}", status.hostname);
            println!("\nDNS Servers: {}", status.dns_servers.join(", "));
            println!("\nInterfaces:");
            for iface in &status.interfaces {
                let state = if iface.up { "up" } else { "down" };
                let addrs = if iface.addresses.is_empty() {
                    "no address".to_string()
                } else {
                    iface.addresses.join(", ")
                };
                println!("  {}: {} - {}", iface.name, state, addrs);
            }
        }

        IpcResponse::Error { message } => {
            eprintln!("Error: {}", message);
        }
    }
}
