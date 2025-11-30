//! Network interface management

use anyhow::{Result, anyhow};
use std::collections::HashMap;
use std::net::IpAddr;
use tracing::{info, debug};

/// Network interface information
#[derive(Debug, Clone)]
pub struct NetworkInterface {
    pub name: String,
    pub index: u32,
    pub mac_address: Option<String>,
    pub addresses: Vec<InterfaceAddress>,
    pub flags: InterfaceFlags,
    pub mtu: u32,
    pub interface_type: InterfaceType,
}

#[derive(Debug, Clone)]
pub struct InterfaceAddress {
    pub address: IpAddr,
    pub prefix_len: u8,
    pub broadcast: Option<IpAddr>,
}

#[derive(Debug, Clone, Default)]
pub struct InterfaceFlags {
    pub up: bool,
    pub running: bool,
    pub loopback: bool,
    pub multicast: bool,
    pub broadcast: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum InterfaceType {
    Ethernet,
    Wireless,
    Loopback,
    Bridge,
    Tunnel,
    Virtual,
    Unknown,
}

/// Interface manager
pub struct InterfaceManager {
    interfaces: HashMap<String, NetworkInterface>,
    handle: rtnetlink::Handle,
}

impl InterfaceManager {
    pub async fn new() -> Result<Self> {
        let (connection, handle, _) = rtnetlink::new_connection()?;
        tokio::spawn(connection);

        let mut manager = Self {
            interfaces: HashMap::new(),
            handle,
        };

        manager.refresh().await?;

        Ok(manager)
    }

    /// Refresh interface list
    pub async fn refresh(&mut self) -> Result<()> {
        use futures::TryStreamExt;
        use netlink_packet_route::link::{LinkAttribute, LinkFlag};

        self.interfaces.clear();

        let mut links = self.handle.link().get().execute();

        while let Some(msg) = links.try_next().await? {
            let mut name = String::new();
            let mut mac = None;
            let mut mtu = 1500u32;

            for attr in msg.attributes.iter() {
                match attr {
                    LinkAttribute::IfName(n) => name = n.clone(),
                    LinkAttribute::Address(addr) => {
                        if addr.len() == 6 {
                            mac = Some(format!(
                                "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                                addr[0], addr[1], addr[2], addr[3], addr[4], addr[5]
                            ));
                        }
                    }
                    LinkAttribute::Mtu(m) => mtu = *m,
                    _ => {}
                }
            }

            let flags = InterfaceFlags {
                up: msg.header.flags.contains(&LinkFlag::Up),
                running: msg.header.flags.contains(&LinkFlag::Running),
                loopback: msg.header.flags.contains(&LinkFlag::Loopback),
                multicast: msg.header.flags.contains(&LinkFlag::Multicast),
                broadcast: msg.header.flags.contains(&LinkFlag::Broadcast),
            };

            let interface_type = self.detect_type(&name, &flags);

            self.interfaces.insert(name.clone(), NetworkInterface {
                name,
                index: msg.header.index,
                mac_address: mac,
                addresses: Vec::new(),
                flags,
                mtu,
                interface_type,
            });
        }

        // Get addresses
        self.refresh_addresses().await?;

        Ok(())
    }

    async fn refresh_addresses(&mut self) -> Result<()> {
        use futures::TryStreamExt;
        use netlink_packet_route::address::AddressAttribute;

        let mut addrs = self.handle.address().get().execute();

        while let Some(msg) = addrs.try_next().await? {
            let mut address = None;
            let mut broadcast = None;

            for attr in msg.attributes.iter() {
                match attr {
                    AddressAttribute::Address(addr) => {
                        address = Some(*addr);
                    }
                    AddressAttribute::Broadcast(bcast) => {
                        broadcast = Some(IpAddr::V4(*bcast));
                    }
                    _ => {}
                }
            }

            if let Some(addr) = address {
                // Find interface by index
                for iface in self.interfaces.values_mut() {
                    if iface.index == msg.header.index {
                        iface.addresses.push(InterfaceAddress {
                            address: addr,
                            prefix_len: msg.header.prefix_len,
                            broadcast,
                        });
                        break;
                    }
                }
            }
        }

        Ok(())
    }

    fn detect_type(&self, name: &str, flags: &InterfaceFlags) -> InterfaceType {
        if flags.loopback {
            InterfaceType::Loopback
        } else if name.starts_with("wl") || name.starts_with("wlan") {
            InterfaceType::Wireless
        } else if name.starts_with("eth") || name.starts_with("en") {
            InterfaceType::Ethernet
        } else if name.starts_with("br") {
            InterfaceType::Bridge
        } else if name.starts_with("tun") || name.starts_with("tap") {
            InterfaceType::Tunnel
        } else if name.starts_with("veth") || name.starts_with("docker") {
            InterfaceType::Virtual
        } else {
            InterfaceType::Unknown
        }
    }

    /// List all interfaces
    pub fn list(&self) -> Result<Vec<NetworkInterface>> {
        Ok(self.interfaces.values().cloned().collect())
    }

    /// Get interface by name
    pub fn get(&self, name: &str) -> Option<&NetworkInterface> {
        self.interfaces.get(name)
    }

    /// Set interface up/down
    pub async fn set_up(&mut self, name: &str, up: bool) -> Result<()> {
        let iface = self.interfaces.get(name)
            .ok_or_else(|| anyhow!("Interface not found: {}", name))?;

        let mut request = self.handle.link().set(iface.index);

        if up {
            request = request.up();
        } else {
            request = request.down();
        }

        request.execute().await?;

        info!("Set {} {}", name, if up { "up" } else { "down" });
        Ok(())
    }

    /// Set interface address
    pub async fn set_address(&mut self, name: &str, address: &str) -> Result<()> {
        let index = self.interfaces.get(name)
            .ok_or_else(|| anyhow!("Interface not found: {}", name))?
            .index;

        let network: ipnetwork::IpNetwork = address.parse()?;

        // Remove existing addresses
        self.flush_addresses(name).await?;

        // Add new address
        let mut request = self.handle.address().add(
            index,
            network.ip(),
            network.prefix(),
        );

        request.execute().await?;

        info!("Set {} address to {}", name, address);
        Ok(())
    }

    /// Flush all addresses from interface
    pub async fn flush_addresses(&mut self, name: &str) -> Result<()> {
        use futures::TryStreamExt;

        let iface = self.interfaces.get(name)
            .ok_or_else(|| anyhow!("Interface not found: {}", name))?;

        let mut addrs = self.handle.address()
            .get()
            .set_link_index_filter(iface.index)
            .execute();

        while let Some(msg) = addrs.try_next().await? {
            self.handle.address().del(msg).execute().await?;
        }

        Ok(())
    }

    /// Set default gateway
    pub async fn set_gateway(&mut self, _name: &str, gateway: &str) -> Result<()> {
        let gw: IpAddr = gateway.parse()?;

        // Remove existing default routes
        self.flush_default_routes().await?;

        // Add new default route
        match gw {
            IpAddr::V4(gw4) => {
                self.handle.route()
                    .add()
                    .v4()
                    .gateway(gw4)
                    .execute()
                    .await?;
            }
            IpAddr::V6(gw6) => {
                self.handle.route()
                    .add()
                    .v6()
                    .gateway(gw6)
                    .execute()
                    .await?;
            }
        }

        info!("Set default gateway to {}", gateway);
        Ok(())
    }

    async fn flush_default_routes(&mut self) -> Result<()> {
        use futures::TryStreamExt;
        use netlink_packet_route::route::RouteAttribute;

        let mut routes = self.handle.route().get(rtnetlink::IpVersion::V4).execute();

        while let Some(msg) = routes.try_next().await? {
            // Check if this is a default route (destination 0.0.0.0/0)
            if msg.header.destination_prefix_length == 0 {
                self.handle.route().del(msg).execute().await?;
            }
        }

        Ok(())
    }

    /// Set MTU
    pub async fn set_mtu(&mut self, name: &str, mtu: u32) -> Result<()> {
        let iface = self.interfaces.get(name)
            .ok_or_else(|| anyhow!("Interface not found: {}", name))?;

        self.handle.link()
            .set(iface.index)
            .mtu(mtu)
            .execute()
            .await?;

        info!("Set {} MTU to {}", name, mtu);
        Ok(())
    }

    /// Handle netlink event
    pub async fn handle_netlink_event(&mut self, _msg: &netlink_packet_core::NetlinkMessage<netlink_packet_route::RouteNetlinkMessage>) -> Result<()> {
        // Refresh interface state on any event
        self.refresh().await?;
        Ok(())
    }
}
