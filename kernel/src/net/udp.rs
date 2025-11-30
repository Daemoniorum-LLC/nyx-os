//! UDP (User Datagram Protocol) implementation
//!
//! Provides connectionless, unreliable datagram delivery.

use super::{IpAddr, Ipv4Addr, NetError, SocketAddr};
use alloc::collections::{BTreeMap, VecDeque};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::RwLock;

/// UDP header size
pub const UDP_HEADER_SIZE: usize = 8;
/// Maximum UDP payload size
pub const UDP_MAX_PAYLOAD: usize = 65507; // 65535 - 8 (UDP) - 20 (IP)

/// UDP sockets
static SOCKETS: RwLock<BTreeMap<UdpSocketId, UdpSocket>> = RwLock::new(BTreeMap::new());

/// Port bindings
static PORT_BINDINGS: RwLock<BTreeMap<u16, UdpSocketId>> = RwLock::new(BTreeMap::new());

/// Next socket ID
static NEXT_SOCKET_ID: AtomicU64 = AtomicU64::new(1);

/// UDP socket identifier
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UdpSocketId(pub u64);

/// UDP socket
#[derive(Clone, Debug)]
pub struct UdpSocket {
    /// Socket ID
    pub id: UdpSocketId,
    /// Local address
    pub local: SocketAddr,
    /// Connected remote (if any)
    pub remote: Option<SocketAddr>,
    /// Receive queue
    pub recv_queue: VecDeque<UdpDatagram>,
    /// Maximum receive queue size
    pub max_recv_queue: usize,
    /// Socket options
    pub options: UdpOptions,
}

/// UDP datagram in receive queue
#[derive(Clone, Debug)]
pub struct UdpDatagram {
    /// Source address
    pub src: SocketAddr,
    /// Data
    pub data: Vec<u8>,
}

/// UDP socket options
#[derive(Clone, Debug)]
pub struct UdpOptions {
    /// Broadcast enabled
    pub broadcast: bool,
    /// Multicast TTL
    pub multicast_ttl: u8,
    /// Multicast loopback
    pub multicast_loop: bool,
    /// Receive buffer size
    pub recv_buffer_size: usize,
    /// Send buffer size
    pub send_buffer_size: usize,
}

impl Default for UdpOptions {
    fn default() -> Self {
        Self {
            broadcast: false,
            multicast_ttl: 1,
            multicast_loop: true,
            recv_buffer_size: 212992,
            send_buffer_size: 212992,
        }
    }
}

/// UDP header
#[derive(Clone, Debug)]
pub struct UdpHeader {
    /// Source port
    pub src_port: u16,
    /// Destination port
    pub dst_port: u16,
    /// Length (header + data)
    pub length: u16,
    /// Checksum
    pub checksum: u16,
}

impl UdpHeader {
    /// Parse UDP header from raw bytes
    pub fn parse(data: &[u8]) -> Result<(Self, &[u8]), NetError> {
        if data.len() < UDP_HEADER_SIZE {
            return Err(NetError::BufferTooSmall);
        }

        let src_port = u16::from_be_bytes([data[0], data[1]]);
        let dst_port = u16::from_be_bytes([data[2], data[3]]);
        let length = u16::from_be_bytes([data[4], data[5]]);
        let checksum = u16::from_be_bytes([data[6], data[7]]);

        if (length as usize) < UDP_HEADER_SIZE || data.len() < length as usize {
            return Err(NetError::BufferTooSmall);
        }

        let payload = &data[UDP_HEADER_SIZE..length as usize];

        Ok((
            UdpHeader {
                src_port,
                dst_port,
                length,
                checksum,
            },
            payload,
        ))
    }

    /// Build UDP header
    pub fn build(src_port: u16, dst_port: u16, payload: &[u8]) -> Vec<u8> {
        let length = (UDP_HEADER_SIZE + payload.len()) as u16;
        let mut packet = Vec::with_capacity(length as usize);

        packet.extend_from_slice(&src_port.to_be_bytes());
        packet.extend_from_slice(&dst_port.to_be_bytes());
        packet.extend_from_slice(&length.to_be_bytes());
        packet.extend_from_slice(&0u16.to_be_bytes()); // Checksum (optional for UDP)
        packet.extend_from_slice(payload);

        packet
    }
}

// ============================================================================
// Socket Management
// ============================================================================

/// Create a new UDP socket
pub fn create_socket() -> Result<UdpSocketId, NetError> {
    let id = UdpSocketId(NEXT_SOCKET_ID.fetch_add(1, Ordering::SeqCst));

    let socket = UdpSocket {
        id,
        local: SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0),
        remote: None,
        recv_queue: VecDeque::new(),
        max_recv_queue: 64,
        options: UdpOptions::default(),
    };

    SOCKETS.write().insert(id, socket);

    Ok(id)
}

/// Bind socket to a local address
pub fn bind(socket_id: UdpSocketId, addr: SocketAddr) -> Result<(), NetError> {
    let mut sockets = SOCKETS.write();
    let mut bindings = PORT_BINDINGS.write();

    // Check if port is already bound
    if bindings.contains_key(&addr.port) {
        return Err(NetError::AddressInUse);
    }

    let socket = sockets.get_mut(&socket_id).ok_or(NetError::SocketNotFound)?;
    socket.local = addr;

    bindings.insert(addr.port, socket_id);

    Ok(())
}

/// Connect socket to remote address (sets default destination)
pub fn connect(socket_id: UdpSocketId, addr: SocketAddr) -> Result<(), NetError> {
    let mut sockets = SOCKETS.write();
    let socket = sockets.get_mut(&socket_id).ok_or(NetError::SocketNotFound)?;
    socket.remote = Some(addr);
    Ok(())
}

/// Send datagram to specified address
pub fn sendto(
    socket_id: UdpSocketId,
    data: &[u8],
    dest: SocketAddr,
) -> Result<usize, NetError> {
    if data.len() > UDP_MAX_PAYLOAD {
        return Err(NetError::BufferTooSmall);
    }

    let sockets = SOCKETS.read();
    let socket = sockets.get(&socket_id).ok_or(NetError::SocketNotFound)?;

    // Build and send datagram
    let packet = UdpHeader::build(socket.local.port, dest.port, data);
    send_datagram(socket.local.ip, dest.ip, &packet)?;

    Ok(data.len())
}

/// Send datagram to connected address
pub fn send(socket_id: UdpSocketId, data: &[u8]) -> Result<usize, NetError> {
    let sockets = SOCKETS.read();
    let socket = sockets.get(&socket_id).ok_or(NetError::SocketNotFound)?;

    let dest = socket.remote.ok_or(NetError::NotConnected)?;
    drop(sockets);

    sendto(socket_id, data, dest)
}

/// Receive datagram with source address
pub fn recvfrom(
    socket_id: UdpSocketId,
    buffer: &mut [u8],
) -> Result<(usize, SocketAddr), NetError> {
    let mut sockets = SOCKETS.write();
    let socket = sockets.get_mut(&socket_id).ok_or(NetError::SocketNotFound)?;

    let datagram = socket.recv_queue.pop_front().ok_or(NetError::WouldBlock)?;

    let len = datagram.data.len().min(buffer.len());
    buffer[..len].copy_from_slice(&datagram.data[..len]);

    Ok((len, datagram.src))
}

/// Receive datagram (connected socket)
pub fn recv(socket_id: UdpSocketId, buffer: &mut [u8]) -> Result<usize, NetError> {
    let (len, _) = recvfrom(socket_id, buffer)?;
    Ok(len)
}

/// Close socket
pub fn close(socket_id: UdpSocketId) -> Result<(), NetError> {
    let mut sockets = SOCKETS.write();
    let mut bindings = PORT_BINDINGS.write();

    if let Some(socket) = sockets.remove(&socket_id) {
        if socket.local.port != 0 {
            bindings.remove(&socket.local.port);
        }
    }

    Ok(())
}

// ============================================================================
// Datagram Handling
// ============================================================================

/// Handle incoming UDP datagram
pub fn handle_datagram(src_ip: IpAddr, dst_ip: IpAddr, data: &[u8]) -> Result<(), NetError> {
    let (header, payload) = UdpHeader::parse(data)?;

    let src = SocketAddr::new(src_ip, header.src_port);
    let dst = SocketAddr::new(dst_ip, header.dst_port);

    log::trace!(
        "UDP: {}:{} -> {}:{} len={}",
        src.ip, src.port,
        dst.ip, dst.port,
        payload.len()
    );

    // Find socket bound to destination port
    let socket_id = {
        let bindings = PORT_BINDINGS.read();
        bindings.get(&dst.port).cloned()
    };

    if let Some(socket_id) = socket_id {
        let mut sockets = SOCKETS.write();
        if let Some(socket) = sockets.get_mut(&socket_id) {
            // Check if connected and source matches
            if let Some(remote) = socket.remote {
                if remote != src {
                    // Datagram from unexpected source, ignore
                    return Ok(());
                }
            }

            // Queue datagram if space available
            if socket.recv_queue.len() < socket.max_recv_queue {
                socket.recv_queue.push_back(UdpDatagram {
                    src,
                    data: payload.to_vec(),
                });
            } else {
                log::trace!("UDP: Dropping datagram, receive queue full");
            }
        }
    } else {
        // No socket bound, send ICMP port unreachable
        log::trace!("UDP: No socket bound to port {}", dst.port);
    }

    Ok(())
}

/// Send a UDP datagram via IP
fn send_datagram(src: IpAddr, dst: IpAddr, datagram: &[u8]) -> Result<(), NetError> {
    // In real implementation, route and send via IP layer
    log::trace!(
        "Sending UDP datagram {} -> {} ({} bytes)",
        match src {
            IpAddr::V4(a) => alloc::format!("{}", a),
            IpAddr::V6(_) => "v6".into(),
        },
        match dst {
            IpAddr::V4(a) => alloc::format!("{}", a),
            IpAddr::V6(_) => "v6".into(),
        },
        datagram.len()
    );
    Ok(())
}

// ============================================================================
// Socket Options
// ============================================================================

/// Set socket option
pub fn set_option(
    socket_id: UdpSocketId,
    option: UdpSocketOption,
) -> Result<(), NetError> {
    let mut sockets = SOCKETS.write();
    let socket = sockets.get_mut(&socket_id).ok_or(NetError::SocketNotFound)?;

    match option {
        UdpSocketOption::Broadcast(v) => socket.options.broadcast = v,
        UdpSocketOption::MulticastTtl(v) => socket.options.multicast_ttl = v,
        UdpSocketOption::MulticastLoop(v) => socket.options.multicast_loop = v,
        UdpSocketOption::RecvBufferSize(v) => socket.options.recv_buffer_size = v,
        UdpSocketOption::SendBufferSize(v) => socket.options.send_buffer_size = v,
    }

    Ok(())
}

/// Socket option values
#[derive(Clone, Debug)]
pub enum UdpSocketOption {
    Broadcast(bool),
    MulticastTtl(u8),
    MulticastLoop(bool),
    RecvBufferSize(usize),
    SendBufferSize(usize),
}
