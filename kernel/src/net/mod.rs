//! # Network Stack
//!
//! Provides networking infrastructure for the Nyx microkernel.
//!
//! ## Architecture
//!
//! The network stack follows a layered design:
//!
//! ```text
//! ┌─────────────────────────────────────────────────────┐
//! │                   Applications                       │
//! ├─────────────────────────────────────────────────────┤
//! │                    Socket API                        │
//! ├──────────────┬───────────────┬──────────────────────┤
//! │     TCP      │      UDP      │       ICMP           │
//! ├──────────────┴───────────────┴──────────────────────┤
//! │                   IP (v4/v6)                         │
//! ├─────────────────────────────────────────────────────┤
//! │              Network Interface Layer                 │
//! ├─────────────────────────────────────────────────────┤
//! │                  Device Drivers                      │
//! └─────────────────────────────────────────────────────┘
//! ```

pub mod ethernet;
pub mod interface;
pub mod ip;
pub mod socket;
pub mod tcp;
pub mod udp;

use crate::cap::{Capability, ObjectId, ObjectType, Rights};
use crate::process::ProcessId;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::RwLock;

/// Global network interfaces
static INTERFACES: RwLock<BTreeMap<InterfaceId, interface::NetworkInterface>> =
    RwLock::new(BTreeMap::new());

/// Next interface ID
static NEXT_INTERFACE_ID: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(1);

/// Network interface identifier
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InterfaceId(pub u64);

impl InterfaceId {
    pub fn new() -> Self {
        Self(NEXT_INTERFACE_ID.fetch_add(1, core::sync::atomic::Ordering::SeqCst))
    }
}

impl Default for InterfaceId {
    fn default() -> Self {
        Self::new()
    }
}

/// Network stack errors
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetError {
    /// Interface not found
    InterfaceNotFound,
    /// Socket not found
    SocketNotFound,
    /// Address in use
    AddressInUse,
    /// Connection refused
    ConnectionRefused,
    /// Connection reset
    ConnectionReset,
    /// Connection timed out
    TimedOut,
    /// Network unreachable
    NetworkUnreachable,
    /// Host unreachable
    HostUnreachable,
    /// Port unreachable
    PortUnreachable,
    /// No route to host
    NoRoute,
    /// Invalid address
    InvalidAddress,
    /// Would block
    WouldBlock,
    /// Buffer too small
    BufferTooSmall,
    /// Not connected
    NotConnected,
    /// Already connected
    AlreadyConnected,
    /// Invalid state
    InvalidState,
    /// Permission denied
    PermissionDenied,
    /// Out of memory
    OutOfMemory,
    /// Protocol error
    ProtocolError,
}

/// Initialize network stack
pub fn init() {
    log::info!("Initializing network stack");

    // Initialize socket subsystem
    socket::init();

    log::info!("Network stack initialized");
}

/// MAC address (48-bit)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MacAddress(pub [u8; 6]);

impl MacAddress {
    pub const BROADCAST: Self = Self([0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
    pub const ZERO: Self = Self([0, 0, 0, 0, 0, 0]);

    pub fn new(bytes: [u8; 6]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 6] {
        &self.0
    }

    pub fn is_broadcast(&self) -> bool {
        *self == Self::BROADCAST
    }

    pub fn is_multicast(&self) -> bool {
        (self.0[0] & 0x01) != 0
    }

    pub fn is_unicast(&self) -> bool {
        !self.is_multicast()
    }
}

impl core::fmt::Display for MacAddress {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            self.0[0], self.0[1], self.0[2], self.0[3], self.0[4], self.0[5]
        )
    }
}

/// IPv4 address
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Ipv4Addr(pub [u8; 4]);

impl Ipv4Addr {
    pub const UNSPECIFIED: Self = Self([0, 0, 0, 0]);
    pub const BROADCAST: Self = Self([255, 255, 255, 255]);
    pub const LOCALHOST: Self = Self([127, 0, 0, 1]);

    pub fn new(a: u8, b: u8, c: u8, d: u8) -> Self {
        Self([a, b, c, d])
    }

    pub fn from_bytes(bytes: [u8; 4]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 4] {
        &self.0
    }

    pub fn to_u32(&self) -> u32 {
        u32::from_be_bytes(self.0)
    }

    pub fn from_u32(value: u32) -> Self {
        Self(value.to_be_bytes())
    }

    pub fn is_unspecified(&self) -> bool {
        *self == Self::UNSPECIFIED
    }

    pub fn is_broadcast(&self) -> bool {
        *self == Self::BROADCAST
    }

    pub fn is_loopback(&self) -> bool {
        self.0[0] == 127
    }

    pub fn is_private(&self) -> bool {
        match self.0 {
            [10, ..] => true,
            [172, b, ..] if (16..=31).contains(&b) => true,
            [192, 168, ..] => true,
            _ => false,
        }
    }

    pub fn is_multicast(&self) -> bool {
        (224..=239).contains(&self.0[0])
    }
}

impl core::fmt::Display for Ipv4Addr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}.{}.{}.{}", self.0[0], self.0[1], self.0[2], self.0[3])
    }
}

/// IPv6 address
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Ipv6Addr(pub [u8; 16]);

impl Ipv6Addr {
    pub const UNSPECIFIED: Self = Self([0; 16]);
    pub const LOCALHOST: Self = Self([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);

    pub fn new(segments: [u16; 8]) -> Self {
        let mut bytes = [0u8; 16];
        for (i, &seg) in segments.iter().enumerate() {
            let be = seg.to_be_bytes();
            bytes[i * 2] = be[0];
            bytes[i * 2 + 1] = be[1];
        }
        Self(bytes)
    }

    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    pub fn segments(&self) -> [u16; 8] {
        let mut segs = [0u16; 8];
        for i in 0..8 {
            segs[i] = u16::from_be_bytes([self.0[i * 2], self.0[i * 2 + 1]]);
        }
        segs
    }

    pub fn is_unspecified(&self) -> bool {
        *self == Self::UNSPECIFIED
    }

    pub fn is_loopback(&self) -> bool {
        *self == Self::LOCALHOST
    }
}

/// IP address (v4 or v6)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IpAddr {
    V4(Ipv4Addr),
    V6(Ipv6Addr),
}

impl IpAddr {
    pub fn is_unspecified(&self) -> bool {
        match self {
            IpAddr::V4(addr) => addr.is_unspecified(),
            IpAddr::V6(addr) => addr.is_unspecified(),
        }
    }

    pub fn is_loopback(&self) -> bool {
        match self {
            IpAddr::V4(addr) => addr.is_loopback(),
            IpAddr::V6(addr) => addr.is_loopback(),
        }
    }
}

impl From<Ipv4Addr> for IpAddr {
    fn from(addr: Ipv4Addr) -> Self {
        IpAddr::V4(addr)
    }
}

impl From<Ipv6Addr> for IpAddr {
    fn from(addr: Ipv6Addr) -> Self {
        IpAddr::V6(addr)
    }
}

/// Socket address
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SocketAddr {
    pub ip: IpAddr,
    pub port: u16,
}

impl SocketAddr {
    pub fn new(ip: IpAddr, port: u16) -> Self {
        Self { ip, port }
    }

    pub fn new_v4(addr: Ipv4Addr, port: u16) -> Self {
        Self {
            ip: IpAddr::V4(addr),
            port,
        }
    }

    pub fn new_v6(addr: Ipv6Addr, port: u16) -> Self {
        Self {
            ip: IpAddr::V6(addr),
            port,
        }
    }
}

/// Network protocol
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Protocol {
    Tcp,
    Udp,
    Icmp,
    Raw(u8),
}

impl Protocol {
    pub fn to_ip_protocol(&self) -> u8 {
        match self {
            Protocol::Icmp => 1,
            Protocol::Tcp => 6,
            Protocol::Udp => 17,
            Protocol::Raw(p) => *p,
        }
    }

    pub fn from_ip_protocol(proto: u8) -> Self {
        match proto {
            1 => Protocol::Icmp,
            6 => Protocol::Tcp,
            17 => Protocol::Udp,
            p => Protocol::Raw(p),
        }
    }
}

// ============================================================================
// Interface Management
// ============================================================================

/// Register a network interface
pub fn register_interface(
    name: String,
    mac: MacAddress,
    device_id: crate::driver::DeviceId,
) -> Result<InterfaceId, NetError> {
    let id = InterfaceId::new();

    let iface = interface::NetworkInterface {
        id,
        name: name.clone(),
        mac,
        device_id,
        ipv4_addrs: Vec::new(),
        ipv6_addrs: Vec::new(),
        mtu: 1500,
        state: interface::InterfaceState::Down,
        flags: interface::InterfaceFlags::empty(),
        statistics: interface::InterfaceStats::default(),
    };

    INTERFACES.write().insert(id, iface);

    log::info!("Registered network interface {} ({})", name, mac);

    Ok(id)
}

/// Unregister a network interface
pub fn unregister_interface(id: InterfaceId) -> Result<(), NetError> {
    INTERFACES
        .write()
        .remove(&id)
        .ok_or(NetError::InterfaceNotFound)?;

    Ok(())
}

/// Get interface by ID
pub fn get_interface(id: InterfaceId) -> Option<interface::NetworkInterface> {
    INTERFACES.read().get(&id).cloned()
}

/// List all interfaces
pub fn list_interfaces() -> Vec<InterfaceId> {
    INTERFACES.read().keys().cloned().collect()
}

/// Bring interface up
pub fn interface_up(id: InterfaceId) -> Result<(), NetError> {
    let mut interfaces = INTERFACES.write();
    let iface = interfaces.get_mut(&id).ok_or(NetError::InterfaceNotFound)?;
    iface.state = interface::InterfaceState::Up;
    Ok(())
}

/// Bring interface down
pub fn interface_down(id: InterfaceId) -> Result<(), NetError> {
    let mut interfaces = INTERFACES.write();
    let iface = interfaces.get_mut(&id).ok_or(NetError::InterfaceNotFound)?;
    iface.state = interface::InterfaceState::Down;
    Ok(())
}

/// Add IPv4 address to interface
pub fn add_ipv4_address(
    id: InterfaceId,
    addr: Ipv4Addr,
    prefix_len: u8,
) -> Result<(), NetError> {
    let mut interfaces = INTERFACES.write();
    let iface = interfaces.get_mut(&id).ok_or(NetError::InterfaceNotFound)?;
    iface.ipv4_addrs.push(interface::Ipv4Config {
        address: addr,
        prefix_len,
    });
    Ok(())
}

// ============================================================================
// Capability Granting
// ============================================================================

/// Grant network capability to a process
pub fn grant_socket_capability(
    process_id: ProcessId,
    protocol: Protocol,
) -> Result<Capability, NetError> {
    let cap = unsafe {
        Capability::new_unchecked(
            ObjectId::new(ObjectType::Socket),
            Rights::READ | Rights::WRITE | Rights::POLL | Rights::GRANT,
        )
    };

    log::debug!(
        "Granted {:?} socket capability to process {:?}",
        protocol,
        process_id
    );

    Ok(cap)
}

// ============================================================================
// Packet Processing
// ============================================================================

/// Receive a packet from a network interface
pub fn receive_packet(interface_id: InterfaceId, data: &[u8]) -> Result<(), NetError> {
    // Parse Ethernet frame
    let frame = ethernet::EthernetFrame::parse(data)?;

    // Update interface statistics
    if let Some(mut iface) = INTERFACES.write().get_mut(&interface_id) {
        iface.statistics.rx_packets += 1;
        iface.statistics.rx_bytes += data.len() as u64;
    }

    // Dispatch based on EtherType
    match frame.ethertype {
        ethernet::ETHERTYPE_IPV4 => {
            ip::handle_ipv4_packet(interface_id, frame.payload)?;
        }
        ethernet::ETHERTYPE_IPV6 => {
            ip::handle_ipv6_packet(interface_id, frame.payload)?;
        }
        ethernet::ETHERTYPE_ARP => {
            // Handle ARP
            log::trace!("Received ARP packet");
        }
        _ => {
            log::trace!("Unknown EtherType: {:04x}", frame.ethertype);
        }
    }

    Ok(())
}

/// Send a packet through a network interface
pub fn send_packet(
    interface_id: InterfaceId,
    dest_mac: MacAddress,
    ethertype: u16,
    payload: &[u8],
) -> Result<(), NetError> {
    let iface = INTERFACES
        .read()
        .get(&interface_id)
        .cloned()
        .ok_or(NetError::InterfaceNotFound)?;

    if iface.state != interface::InterfaceState::Up {
        return Err(NetError::NetworkUnreachable);
    }

    // Build Ethernet frame
    let frame = ethernet::EthernetFrame::build(iface.mac, dest_mac, ethertype, payload);

    // Update statistics
    if let Some(mut iface) = INTERFACES.write().get_mut(&interface_id) {
        iface.statistics.tx_packets += 1;
        iface.statistics.tx_bytes += frame.len() as u64;
    }

    // TODO: Send to driver
    log::trace!("Sending {} bytes to {}", frame.len(), dest_mac);

    Ok(())
}
