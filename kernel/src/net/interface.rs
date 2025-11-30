//! Network interface management
//!
//! Manages network interface configuration and state.

use super::{InterfaceId, Ipv4Addr, Ipv6Addr, MacAddress};
use crate::driver::DeviceId;
use alloc::string::String;
use alloc::vec::Vec;

/// Network interface
#[derive(Clone, Debug)]
pub struct NetworkInterface {
    /// Unique identifier
    pub id: InterfaceId,
    /// Interface name (e.g., "eth0")
    pub name: String,
    /// MAC address
    pub mac: MacAddress,
    /// Underlying device ID
    pub device_id: DeviceId,
    /// IPv4 addresses
    pub ipv4_addrs: Vec<Ipv4Config>,
    /// IPv6 addresses
    pub ipv6_addrs: Vec<Ipv6Config>,
    /// Maximum transmission unit
    pub mtu: u16,
    /// Interface state
    pub state: InterfaceState,
    /// Interface flags
    pub flags: InterfaceFlags,
    /// Statistics
    pub statistics: InterfaceStats,
}

/// IPv4 address configuration
#[derive(Clone, Debug)]
pub struct Ipv4Config {
    /// IP address
    pub address: Ipv4Addr,
    /// Prefix length (e.g., 24 for /24)
    pub prefix_len: u8,
}

impl Ipv4Config {
    /// Get the network mask
    pub fn netmask(&self) -> Ipv4Addr {
        let mask = if self.prefix_len >= 32 {
            0xFFFF_FFFF
        } else {
            !((1u32 << (32 - self.prefix_len)) - 1)
        };
        Ipv4Addr::from_u32(mask)
    }

    /// Get the network address
    pub fn network(&self) -> Ipv4Addr {
        let addr = self.address.to_u32();
        let mask = self.netmask().to_u32();
        Ipv4Addr::from_u32(addr & mask)
    }

    /// Get the broadcast address
    pub fn broadcast(&self) -> Ipv4Addr {
        let addr = self.address.to_u32();
        let mask = self.netmask().to_u32();
        Ipv4Addr::from_u32(addr | !mask)
    }

    /// Check if an address is in this network
    pub fn contains(&self, addr: Ipv4Addr) -> bool {
        let network = self.network().to_u32();
        let target = addr.to_u32();
        let mask = self.netmask().to_u32();
        (target & mask) == network
    }
}

/// IPv6 address configuration
#[derive(Clone, Debug)]
pub struct Ipv6Config {
    /// IP address
    pub address: Ipv6Addr,
    /// Prefix length
    pub prefix_len: u8,
    /// Address scope
    pub scope: Ipv6Scope,
}

/// IPv6 address scope
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ipv6Scope {
    /// Link-local (fe80::/10)
    LinkLocal,
    /// Site-local (deprecated)
    SiteLocal,
    /// Global unicast
    Global,
    /// Loopback (::1)
    Loopback,
}

/// Interface state
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InterfaceState {
    /// Interface is down
    Down,
    /// Interface is up
    Up,
    /// Interface is testing
    Testing,
    /// Interface is dormant
    Dormant,
    /// Interface state is unknown
    Unknown,
}

bitflags::bitflags! {
    /// Interface flags
    #[derive(Clone, Copy, Debug, Default)]
    pub struct InterfaceFlags: u32 {
        /// Interface supports broadcast
        const BROADCAST = 1 << 0;
        /// Interface supports multicast
        const MULTICAST = 1 << 1;
        /// Interface is point-to-point
        const POINTOPOINT = 1 << 2;
        /// Interface is loopback
        const LOOPBACK = 1 << 3;
        /// Interface is running
        const RUNNING = 1 << 4;
        /// Interface is promiscuous
        const PROMISC = 1 << 5;
        /// Interface has all-multicast mode
        const ALLMULTI = 1 << 6;
        /// No ARP
        const NOARP = 1 << 7;
        /// Debug mode
        const DEBUG = 1 << 8;
    }
}

/// Interface statistics
#[derive(Clone, Debug, Default)]
pub struct InterfaceStats {
    /// Received packets
    pub rx_packets: u64,
    /// Transmitted packets
    pub tx_packets: u64,
    /// Received bytes
    pub rx_bytes: u64,
    /// Transmitted bytes
    pub tx_bytes: u64,
    /// Receive errors
    pub rx_errors: u64,
    /// Transmit errors
    pub tx_errors: u64,
    /// Dropped received packets
    pub rx_dropped: u64,
    /// Dropped transmitted packets
    pub tx_dropped: u64,
    /// Multicast packets received
    pub rx_multicast: u64,
    /// Collisions
    pub collisions: u64,
}

impl NetworkInterface {
    /// Check if interface is up
    pub fn is_up(&self) -> bool {
        self.state == InterfaceState::Up
    }

    /// Check if interface has an IPv4 address
    pub fn has_ipv4(&self) -> bool {
        !self.ipv4_addrs.is_empty()
    }

    /// Check if interface has an IPv6 address
    pub fn has_ipv6(&self) -> bool {
        !self.ipv6_addrs.is_empty()
    }

    /// Get primary IPv4 address
    pub fn primary_ipv4(&self) -> Option<Ipv4Addr> {
        self.ipv4_addrs.first().map(|c| c.address)
    }

    /// Get primary IPv6 address
    pub fn primary_ipv6(&self) -> Option<Ipv6Addr> {
        self.ipv6_addrs.first().map(|c| c.address)
    }

    /// Check if an IPv4 address belongs to this interface
    pub fn owns_ipv4(&self, addr: Ipv4Addr) -> bool {
        self.ipv4_addrs.iter().any(|c| c.address == addr)
    }

    /// Check if an IPv4 address is on the same network
    pub fn is_same_network_v4(&self, addr: Ipv4Addr) -> bool {
        self.ipv4_addrs.iter().any(|c| c.contains(addr))
    }
}

/// Routing table entry
#[derive(Clone, Debug)]
pub struct Route {
    /// Destination network
    pub destination: super::IpAddr,
    /// Prefix length
    pub prefix_len: u8,
    /// Gateway (None for direct routes)
    pub gateway: Option<super::IpAddr>,
    /// Output interface
    pub interface: InterfaceId,
    /// Metric (lower is preferred)
    pub metric: u32,
    /// Route flags
    pub flags: RouteFlags,
}

bitflags::bitflags! {
    /// Route flags
    #[derive(Clone, Copy, Debug, Default)]
    pub struct RouteFlags: u32 {
        /// Route is up
        const UP = 1 << 0;
        /// Route is a gateway
        const GATEWAY = 1 << 1;
        /// Host route
        const HOST = 1 << 2;
        /// Reject route
        const REJECT = 1 << 3;
        /// Dynamic route
        const DYNAMIC = 1 << 4;
        /// Modified route
        const MODIFIED = 1 << 5;
    }
}
