//! Socket abstraction layer
//!
//! Provides a unified socket interface for TCP, UDP, and other protocols.

use super::{tcp, udp, IpAddr, Ipv4Addr, NetError, Protocol, SocketAddr};
use crate::cap::{Capability, ObjectId, ObjectType, Rights};
use crate::process::ProcessId;
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::RwLock;

/// Socket registry
static SOCKETS: RwLock<BTreeMap<SocketId, Socket>> = RwLock::new(BTreeMap::new());

/// Next socket ID
static NEXT_SOCKET_ID: AtomicU64 = AtomicU64::new(1);

/// Socket identifier
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SocketId(pub u64);

impl SocketId {
    pub fn new() -> Self {
        Self(NEXT_SOCKET_ID.fetch_add(1, Ordering::SeqCst))
    }
}

impl Default for SocketId {
    fn default() -> Self {
        Self::new()
    }
}

/// Socket handle
#[derive(Clone, Debug)]
pub struct Socket {
    /// Socket ID
    pub id: SocketId,
    /// Socket domain
    pub domain: SocketDomain,
    /// Socket type
    pub socket_type: SocketType,
    /// Protocol
    pub protocol: Protocol,
    /// Socket state
    pub state: SocketState,
    /// Owner process
    pub owner: ProcessId,
    /// Protocol-specific data
    pub proto_data: SocketProtoData,
    /// Socket options
    pub options: SocketOptions,
}

/// Socket domain (address family)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SocketDomain {
    /// IPv4
    Inet,
    /// IPv6
    Inet6,
    /// Unix domain
    Unix,
}

/// Socket type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SocketType {
    /// Stream (TCP)
    Stream,
    /// Datagram (UDP)
    Datagram,
    /// Raw socket
    Raw,
    /// Sequenced packet
    SeqPacket,
}

/// Socket state
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SocketState {
    /// Unbound
    Unbound,
    /// Bound to local address
    Bound,
    /// Listening for connections
    Listening,
    /// Connecting to remote
    Connecting,
    /// Connected
    Connected,
    /// Closing
    Closing,
    /// Closed
    Closed,
}

/// Protocol-specific socket data
#[derive(Clone, Debug)]
pub enum SocketProtoData {
    /// TCP connection ID
    Tcp(u64),
    /// UDP socket ID
    Udp(udp::UdpSocketId),
    /// Raw socket data
    Raw { protocol: u8 },
    /// No protocol data yet
    None,
}

/// Socket options
#[derive(Clone, Debug, Default)]
pub struct SocketOptions {
    /// SO_REUSEADDR
    pub reuse_addr: bool,
    /// SO_REUSEPORT
    pub reuse_port: bool,
    /// SO_KEEPALIVE
    pub keepalive: bool,
    /// SO_LINGER
    pub linger: Option<u32>,
    /// SO_RCVBUF
    pub recv_buffer_size: Option<usize>,
    /// SO_SNDBUF
    pub send_buffer_size: Option<usize>,
    /// SO_RCVTIMEO (milliseconds)
    pub recv_timeout: Option<u64>,
    /// SO_SNDTIMEO (milliseconds)
    pub send_timeout: Option<u64>,
    /// TCP_NODELAY
    pub tcp_nodelay: bool,
}

/// Initialize socket subsystem
pub fn init() {
    log::debug!("Initializing socket subsystem");
}

// ============================================================================
// Socket API
// ============================================================================

/// Create a new socket
pub fn create(
    owner: ProcessId,
    domain: SocketDomain,
    socket_type: SocketType,
    protocol: Option<Protocol>,
) -> Result<SocketId, NetError> {
    let protocol = protocol.unwrap_or_else(|| match socket_type {
        SocketType::Stream => Protocol::Tcp,
        SocketType::Datagram => Protocol::Udp,
        SocketType::Raw => Protocol::Raw(0),
        SocketType::SeqPacket => Protocol::Tcp,
    });

    let id = SocketId::new();

    let socket = Socket {
        id,
        domain,
        socket_type,
        protocol,
        state: SocketState::Unbound,
        owner,
        proto_data: SocketProtoData::None,
        options: SocketOptions::default(),
    };

    SOCKETS.write().insert(id, socket);

    log::debug!(
        "Created socket {:?} for process {:?}",
        id,
        owner
    );

    Ok(id)
}

/// Bind socket to local address
pub fn bind(socket_id: SocketId, addr: SocketAddr) -> Result<(), NetError> {
    let mut sockets = SOCKETS.write();
    let socket = sockets.get_mut(&socket_id).ok_or(NetError::SocketNotFound)?;

    if socket.state != SocketState::Unbound {
        return Err(NetError::InvalidState);
    }

    match socket.protocol {
        Protocol::Tcp => {
            // TCP binding handled when listen/connect
        }
        Protocol::Udp => {
            let udp_id = udp::create_socket()?;
            udp::bind(udp_id, addr)?;
            socket.proto_data = SocketProtoData::Udp(udp_id);
        }
        _ => {}
    }

    socket.state = SocketState::Bound;

    Ok(())
}

/// Listen for connections (TCP only)
pub fn listen(socket_id: SocketId, _backlog: u32) -> Result<(), NetError> {
    let mut sockets = SOCKETS.write();
    let socket = sockets.get_mut(&socket_id).ok_or(NetError::SocketNotFound)?;

    if socket.socket_type != SocketType::Stream {
        return Err(NetError::InvalidState);
    }

    if socket.state != SocketState::Bound {
        return Err(NetError::InvalidState);
    }

    // Create TCP listener
    // let local = get_bound_addr(socket)?;
    // let listener_id = tcp::listen(local)?;
    // socket.proto_data = SocketProtoData::Tcp(listener_id);

    socket.state = SocketState::Listening;

    Ok(())
}

/// Accept a connection (TCP only)
pub fn accept(socket_id: SocketId) -> Result<(SocketId, SocketAddr), NetError> {
    let sockets = SOCKETS.read();
    let socket = sockets.get(&socket_id).ok_or(NetError::SocketNotFound)?;

    if socket.state != SocketState::Listening {
        return Err(NetError::InvalidState);
    }

    // let (conn_id, remote) = match &socket.proto_data {
    //     SocketProtoData::Tcp(listener_id) => tcp::accept(*listener_id)?,
    //     _ => return Err(NetError::InvalidState),
    // };

    // Create new socket for accepted connection
    // ...

    Err(NetError::WouldBlock)
}

/// Connect to remote address
pub fn connect(socket_id: SocketId, addr: SocketAddr) -> Result<(), NetError> {
    let mut sockets = SOCKETS.write();
    let socket = sockets.get_mut(&socket_id).ok_or(NetError::SocketNotFound)?;

    match socket.protocol {
        Protocol::Tcp => {
            if socket.state != SocketState::Unbound && socket.state != SocketState::Bound {
                return Err(NetError::InvalidState);
            }

            // Allocate ephemeral port if not bound
            let local = SocketAddr::new(
                IpAddr::V4(Ipv4Addr::UNSPECIFIED),
                allocate_ephemeral_port(),
            );

            let conn_id = tcp::connect(local, addr)?;
            socket.proto_data = SocketProtoData::Tcp(conn_id);
            socket.state = SocketState::Connecting;
        }
        Protocol::Udp => {
            if let SocketProtoData::Udp(udp_id) = socket.proto_data {
                udp::connect(udp_id, addr)?;
            } else {
                let udp_id = udp::create_socket()?;
                udp::connect(udp_id, addr)?;
                socket.proto_data = SocketProtoData::Udp(udp_id);
            }
            socket.state = SocketState::Connected;
        }
        _ => return Err(NetError::InvalidState),
    }

    Ok(())
}

/// Send data on connected socket
pub fn send(socket_id: SocketId, data: &[u8], _flags: u32) -> Result<usize, NetError> {
    let sockets = SOCKETS.read();
    let socket = sockets.get(&socket_id).ok_or(NetError::SocketNotFound)?;

    match &socket.proto_data {
        SocketProtoData::Tcp(conn_id) => tcp::send(*conn_id, data),
        SocketProtoData::Udp(udp_id) => udp::send(*udp_id, data),
        _ => Err(NetError::NotConnected),
    }
}

/// Send data to specific address
pub fn sendto(
    socket_id: SocketId,
    data: &[u8],
    _flags: u32,
    addr: SocketAddr,
) -> Result<usize, NetError> {
    let sockets = SOCKETS.read();
    let socket = sockets.get(&socket_id).ok_or(NetError::SocketNotFound)?;

    match &socket.proto_data {
        SocketProtoData::Udp(udp_id) => udp::sendto(*udp_id, data, addr),
        _ => Err(NetError::InvalidState),
    }
}

/// Receive data from connected socket
pub fn recv(socket_id: SocketId, buffer: &mut [u8], _flags: u32) -> Result<usize, NetError> {
    let sockets = SOCKETS.read();
    let socket = sockets.get(&socket_id).ok_or(NetError::SocketNotFound)?;

    match &socket.proto_data {
        SocketProtoData::Tcp(conn_id) => tcp::recv(*conn_id, buffer),
        SocketProtoData::Udp(udp_id) => udp::recv(*udp_id, buffer),
        _ => Err(NetError::NotConnected),
    }
}

/// Receive data with source address
pub fn recvfrom(
    socket_id: SocketId,
    buffer: &mut [u8],
    _flags: u32,
) -> Result<(usize, SocketAddr), NetError> {
    let sockets = SOCKETS.read();
    let socket = sockets.get(&socket_id).ok_or(NetError::SocketNotFound)?;

    match &socket.proto_data {
        SocketProtoData::Udp(udp_id) => udp::recvfrom(*udp_id, buffer),
        _ => Err(NetError::InvalidState),
    }
}

/// Shutdown socket
pub fn shutdown(socket_id: SocketId, how: ShutdownHow) -> Result<(), NetError> {
    let mut sockets = SOCKETS.write();
    let socket = sockets.get_mut(&socket_id).ok_or(NetError::SocketNotFound)?;

    match &socket.proto_data {
        SocketProtoData::Tcp(conn_id) => {
            if how == ShutdownHow::Write || how == ShutdownHow::Both {
                tcp::close(*conn_id)?;
            }
        }
        _ => {}
    }

    if how == ShutdownHow::Both {
        socket.state = SocketState::Closing;
    }

    Ok(())
}

/// Close socket
pub fn close(socket_id: SocketId) -> Result<(), NetError> {
    let mut sockets = SOCKETS.write();

    if let Some(socket) = sockets.remove(&socket_id) {
        match socket.proto_data {
            SocketProtoData::Tcp(conn_id) => {
                tcp::close(conn_id)?;
            }
            SocketProtoData::Udp(udp_id) => {
                udp::close(udp_id)?;
            }
            _ => {}
        }
    }

    Ok(())
}

/// Shutdown direction
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShutdownHow {
    /// Shutdown read
    Read,
    /// Shutdown write
    Write,
    /// Shutdown both
    Both,
}

// ============================================================================
// Socket Options
// ============================================================================

/// Set socket option
pub fn setsockopt(socket_id: SocketId, option: SocketOption) -> Result<(), NetError> {
    let mut sockets = SOCKETS.write();
    let socket = sockets.get_mut(&socket_id).ok_or(NetError::SocketNotFound)?;

    match option {
        SocketOption::ReuseAddr(v) => socket.options.reuse_addr = v,
        SocketOption::ReusePort(v) => socket.options.reuse_port = v,
        SocketOption::KeepAlive(v) => socket.options.keepalive = v,
        SocketOption::Linger(v) => socket.options.linger = v,
        SocketOption::RecvBufferSize(v) => socket.options.recv_buffer_size = Some(v),
        SocketOption::SendBufferSize(v) => socket.options.send_buffer_size = Some(v),
        SocketOption::RecvTimeout(v) => socket.options.recv_timeout = Some(v),
        SocketOption::SendTimeout(v) => socket.options.send_timeout = Some(v),
        SocketOption::TcpNoDelay(v) => socket.options.tcp_nodelay = v,
    }

    Ok(())
}

/// Socket option values
#[derive(Clone, Debug)]
pub enum SocketOption {
    ReuseAddr(bool),
    ReusePort(bool),
    KeepAlive(bool),
    Linger(Option<u32>),
    RecvBufferSize(usize),
    SendBufferSize(usize),
    RecvTimeout(u64),
    SendTimeout(u64),
    TcpNoDelay(bool),
}

// ============================================================================
// Helpers
// ============================================================================

/// Ephemeral port counter
static EPHEMERAL_PORT: AtomicU64 = AtomicU64::new(49152);

/// Allocate an ephemeral port
fn allocate_ephemeral_port() -> u16 {
    let port = EPHEMERAL_PORT.fetch_add(1, Ordering::SeqCst);
    // Wrap around if needed (49152-65535)
    ((port - 49152) % (65535 - 49152 + 1) + 49152) as u16
}

/// Poll socket for events
pub fn poll(socket_id: SocketId, events: PollEvents) -> Result<PollEvents, NetError> {
    let sockets = SOCKETS.read();
    let socket = sockets.get(&socket_id).ok_or(NetError::SocketNotFound)?;

    let mut revents = PollEvents::empty();

    // Check based on socket state and type
    match socket.state {
        SocketState::Connected => {
            if events.contains(PollEvents::WRITABLE) {
                revents |= PollEvents::WRITABLE;
            }
            // Would check recv buffer for readable
        }
        SocketState::Listening => {
            // Would check accept queue for readable
        }
        SocketState::Closed => {
            revents |= PollEvents::HUP;
        }
        _ => {}
    }

    Ok(revents)
}

bitflags::bitflags! {
    /// Poll events
    #[derive(Clone, Copy, Debug, Default)]
    pub struct PollEvents: u16 {
        /// Ready to read
        const READABLE = 1 << 0;
        /// Ready to write
        const WRITABLE = 1 << 1;
        /// Error condition
        const ERROR = 1 << 2;
        /// Hang up
        const HUP = 1 << 3;
        /// Invalid
        const INVALID = 1 << 4;
    }
}

// ============================================================================
// Capability Integration
// ============================================================================

/// Create socket capability
pub fn create_socket_capability(socket_id: SocketId) -> Capability {
    unsafe {
        Capability::new_unchecked(
            ObjectId::new(ObjectType::Socket),
            Rights::READ | Rights::WRITE | Rights::POLL | Rights::GRANT,
        )
    }
}

/// Get socket from capability
pub fn socket_from_capability(cap: &Capability) -> Result<SocketId, NetError> {
    if cap.object_id.object_type() != ObjectType::Socket {
        return Err(NetError::PermissionDenied);
    }

    // In real implementation, would lookup socket by capability object ID
    Err(NetError::SocketNotFound)
}
