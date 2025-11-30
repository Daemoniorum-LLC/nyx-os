//! TCP (Transmission Control Protocol) implementation
//!
//! Provides TCP connection management, congestion control, and reliable delivery.

use super::{IpAddr, Ipv4Addr, NetError, SocketAddr};
use alloc::collections::{BTreeMap, VecDeque};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use spin::RwLock;

/// TCP header minimum size
pub const TCP_HEADER_MIN_SIZE: usize = 20;
/// TCP maximum segment size (common default)
pub const TCP_DEFAULT_MSS: u16 = 1460;
/// TCP window scale shift
pub const TCP_WINDOW_SCALE: u8 = 7;

/// TCP connections
static CONNECTIONS: RwLock<BTreeMap<TcpConnectionKey, TcpConnection>> =
    RwLock::new(BTreeMap::new());

/// Next connection ID
static NEXT_CONN_ID: AtomicU64 = AtomicU64::new(1);

/// Initial sequence number
static ISN_COUNTER: AtomicU32 = AtomicU32::new(0);

/// TCP connection key
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TcpConnectionKey {
    pub local: SocketAddr,
    pub remote: SocketAddr,
}

/// TCP connection state
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TcpState {
    /// Connection closed
    Closed,
    /// Listening for connections
    Listen,
    /// SYN sent, waiting for SYN-ACK
    SynSent,
    /// SYN received, sent SYN-ACK
    SynReceived,
    /// Connection established
    Established,
    /// Received FIN, waiting for application close
    CloseWait,
    /// Application closed, sent FIN
    LastAck,
    /// Application closed, sent FIN
    FinWait1,
    /// Received ACK for FIN
    FinWait2,
    /// Received FIN after sending FIN
    Closing,
    /// Waiting for timeout after close
    TimeWait,
}

/// TCP connection
#[derive(Clone, Debug)]
pub struct TcpConnection {
    /// Connection ID
    pub id: u64,
    /// Local endpoint
    pub local: SocketAddr,
    /// Remote endpoint
    pub remote: SocketAddr,
    /// Current state
    pub state: TcpState,
    /// Send state
    pub send: TcpSendState,
    /// Receive state
    pub recv: TcpRecvState,
    /// Congestion control state
    pub congestion: CongestionState,
    /// Options
    pub options: TcpOptions,
    /// Retransmission queue
    pub retransmit_queue: VecDeque<TcpSegment>,
    /// Receive buffer
    pub recv_buffer: VecDeque<u8>,
    /// Send buffer
    pub send_buffer: VecDeque<u8>,
}

/// TCP send state
#[derive(Clone, Debug)]
pub struct TcpSendState {
    /// Send unacknowledged
    pub una: u32,
    /// Send next
    pub nxt: u32,
    /// Send window
    pub wnd: u32,
    /// Send window scale
    pub wnd_scale: u8,
    /// Maximum segment size
    pub mss: u16,
}

/// TCP receive state
#[derive(Clone, Debug)]
pub struct TcpRecvState {
    /// Receive next (expected sequence)
    pub nxt: u32,
    /// Receive window
    pub wnd: u32,
    /// Receive window scale
    pub wnd_scale: u8,
}

/// Congestion control state
#[derive(Clone, Debug)]
pub struct CongestionState {
    /// Congestion window (in bytes)
    pub cwnd: u32,
    /// Slow start threshold
    pub ssthresh: u32,
    /// Smoothed RTT (microseconds)
    pub srtt: u32,
    /// RTT variance
    pub rttvar: u32,
    /// Retransmission timeout (milliseconds)
    pub rto: u32,
    /// Duplicate ACK count
    pub dup_acks: u8,
}

/// TCP options
#[derive(Clone, Debug)]
pub struct TcpOptions {
    /// Timestamp enabled
    pub timestamps: bool,
    /// SACK permitted
    pub sack_permitted: bool,
    /// Window scaling enabled
    pub window_scaling: bool,
}

impl Default for TcpOptions {
    fn default() -> Self {
        Self {
            timestamps: true,
            sack_permitted: true,
            window_scaling: true,
        }
    }
}

/// TCP segment for retransmission
#[derive(Clone, Debug)]
pub struct TcpSegment {
    /// Sequence number
    pub seq: u32,
    /// Data
    pub data: Vec<u8>,
    /// Timestamp when sent
    pub sent_at: u64,
    /// Number of retransmissions
    pub retransmits: u8,
}

/// TCP header
#[derive(Clone, Debug)]
pub struct TcpHeader {
    /// Source port
    pub src_port: u16,
    /// Destination port
    pub dst_port: u16,
    /// Sequence number
    pub seq: u32,
    /// Acknowledgment number
    pub ack: u32,
    /// Data offset (header length in 32-bit words)
    pub data_offset: u8,
    /// Flags
    pub flags: TcpFlags,
    /// Window size
    pub window: u16,
    /// Checksum
    pub checksum: u16,
    /// Urgent pointer
    pub urgent_ptr: u16,
}

bitflags::bitflags! {
    /// TCP flags
    #[derive(Clone, Copy, Debug, Default)]
    pub struct TcpFlags: u8 {
        /// FIN - No more data from sender
        const FIN = 1 << 0;
        /// SYN - Synchronize sequence numbers
        const SYN = 1 << 1;
        /// RST - Reset connection
        const RST = 1 << 2;
        /// PSH - Push function
        const PSH = 1 << 3;
        /// ACK - Acknowledgment field significant
        const ACK = 1 << 4;
        /// URG - Urgent pointer field significant
        const URG = 1 << 5;
        /// ECE - ECN-Echo
        const ECE = 1 << 6;
        /// CWR - Congestion Window Reduced
        const CWR = 1 << 7;
    }
}

impl TcpHeader {
    /// Parse TCP header from raw bytes
    pub fn parse(data: &[u8]) -> Result<(Self, &[u8]), NetError> {
        if data.len() < TCP_HEADER_MIN_SIZE {
            return Err(NetError::BufferTooSmall);
        }

        let src_port = u16::from_be_bytes([data[0], data[1]]);
        let dst_port = u16::from_be_bytes([data[2], data[3]]);
        let seq = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
        let ack = u32::from_be_bytes([data[8], data[9], data[10], data[11]]);

        let data_offset = (data[12] >> 4) & 0x0F;
        let header_len = (data_offset as usize) * 4;

        if data.len() < header_len {
            return Err(NetError::BufferTooSmall);
        }

        let flags = TcpFlags::from_bits_truncate(data[13]);
        let window = u16::from_be_bytes([data[14], data[15]]);
        let checksum = u16::from_be_bytes([data[16], data[17]]);
        let urgent_ptr = u16::from_be_bytes([data[18], data[19]]);

        let header = TcpHeader {
            src_port,
            dst_port,
            seq,
            ack,
            data_offset,
            flags,
            window,
            checksum,
            urgent_ptr,
        };

        let payload = &data[header_len..];

        Ok((header, payload))
    }

    /// Build TCP header
    pub fn build(&self, payload: &[u8]) -> Vec<u8> {
        let header_len = TCP_HEADER_MIN_SIZE;
        let mut packet = Vec::with_capacity(header_len + payload.len());

        // Source port
        packet.extend_from_slice(&self.src_port.to_be_bytes());
        // Destination port
        packet.extend_from_slice(&self.dst_port.to_be_bytes());
        // Sequence number
        packet.extend_from_slice(&self.seq.to_be_bytes());
        // Acknowledgment number
        packet.extend_from_slice(&self.ack.to_be_bytes());
        // Data offset (5 = 20 bytes) and reserved
        packet.push((5 << 4) | 0);
        // Flags
        packet.push(self.flags.bits());
        // Window
        packet.extend_from_slice(&self.window.to_be_bytes());
        // Checksum (placeholder)
        packet.extend_from_slice(&0u16.to_be_bytes());
        // Urgent pointer
        packet.extend_from_slice(&self.urgent_ptr.to_be_bytes());
        // Payload
        packet.extend_from_slice(payload);

        packet
    }
}

// ============================================================================
// Connection Management
// ============================================================================

/// Generate initial sequence number
fn generate_isn() -> u32 {
    // In real implementation, use cryptographic random + timestamp
    ISN_COUNTER.fetch_add(64000, Ordering::Relaxed)
}

/// Create a new TCP connection (active open)
pub fn connect(local: SocketAddr, remote: SocketAddr) -> Result<u64, NetError> {
    let id = NEXT_CONN_ID.fetch_add(1, Ordering::SeqCst);
    let isn = generate_isn();

    let conn = TcpConnection {
        id,
        local,
        remote,
        state: TcpState::SynSent,
        send: TcpSendState {
            una: isn,
            nxt: isn + 1,
            wnd: 0,
            wnd_scale: TCP_WINDOW_SCALE,
            mss: TCP_DEFAULT_MSS,
        },
        recv: TcpRecvState {
            nxt: 0,
            wnd: 65535,
            wnd_scale: TCP_WINDOW_SCALE,
        },
        congestion: CongestionState {
            cwnd: TCP_DEFAULT_MSS as u32 * 10, // Initial window
            ssthresh: 65535,
            srtt: 0,
            rttvar: 0,
            rto: 1000, // 1 second initial RTO
            dup_acks: 0,
        },
        options: TcpOptions::default(),
        retransmit_queue: VecDeque::new(),
        recv_buffer: VecDeque::new(),
        send_buffer: VecDeque::new(),
    };

    let key = TcpConnectionKey { local, remote };
    CONNECTIONS.write().insert(key, conn);

    // Send SYN
    send_syn(local, remote, isn)?;

    Ok(id)
}

/// Create a listening socket
pub fn listen(_local: SocketAddr) -> Result<u64, NetError> {
    let id = NEXT_CONN_ID.fetch_add(1, Ordering::SeqCst);
    // In real implementation, add to listener table
    Ok(id)
}

/// Accept a connection
pub fn accept(_listener_id: u64) -> Result<(u64, SocketAddr), NetError> {
    // In real implementation, dequeue from accept queue
    Err(NetError::WouldBlock)
}

/// Send data on a connection
pub fn send(conn_id: u64, data: &[u8]) -> Result<usize, NetError> {
    let connections = CONNECTIONS.read();
    let conn = connections
        .values()
        .find(|c| c.id == conn_id)
        .ok_or(NetError::SocketNotFound)?;

    if conn.state != TcpState::Established {
        return Err(NetError::NotConnected);
    }

    // In real implementation, queue data and send
    Ok(data.len())
}

/// Receive data from a connection
pub fn recv(conn_id: u64, buffer: &mut [u8]) -> Result<usize, NetError> {
    let mut connections = CONNECTIONS.write();
    let conn = connections
        .values_mut()
        .find(|c| c.id == conn_id)
        .ok_or(NetError::SocketNotFound)?;

    if conn.state != TcpState::Established && conn.state != TcpState::CloseWait {
        return Err(NetError::NotConnected);
    }

    let available = conn.recv_buffer.len().min(buffer.len());
    if available == 0 {
        return Err(NetError::WouldBlock);
    }

    for i in 0..available {
        buffer[i] = conn.recv_buffer.pop_front().unwrap();
    }

    Ok(available)
}

/// Close a connection
pub fn close(conn_id: u64) -> Result<(), NetError> {
    let mut connections = CONNECTIONS.write();
    let conn = connections
        .values_mut()
        .find(|c| c.id == conn_id)
        .ok_or(NetError::SocketNotFound)?;

    match conn.state {
        TcpState::Established => {
            conn.state = TcpState::FinWait1;
            // Send FIN
        }
        TcpState::CloseWait => {
            conn.state = TcpState::LastAck;
            // Send FIN
        }
        _ => {}
    }

    Ok(())
}

// ============================================================================
// Segment Handling
// ============================================================================

/// Handle incoming TCP segment
pub fn handle_segment(src_ip: IpAddr, dst_ip: IpAddr, data: &[u8]) -> Result<(), NetError> {
    let (header, payload) = TcpHeader::parse(data)?;

    let src = SocketAddr::new(src_ip, header.src_port);
    let dst = SocketAddr::new(dst_ip, header.dst_port);

    log::trace!(
        "TCP: {}:{} -> {}:{} flags={:?} seq={} ack={} len={}",
        src.ip, src.port,
        dst.ip, dst.port,
        header.flags,
        header.seq,
        header.ack,
        payload.len()
    );

    let key = TcpConnectionKey {
        local: dst,
        remote: src,
    };

    let mut connections = CONNECTIONS.write();

    if let Some(conn) = connections.get_mut(&key) {
        process_segment(conn, &header, payload)?;
    } else if header.flags.contains(TcpFlags::SYN) && !header.flags.contains(TcpFlags::ACK) {
        // New connection attempt - check for listener
        log::trace!("SYN to unlistened port {}:{}", dst.ip, dst.port);
        // Send RST
        drop(connections);
        send_rst(dst, src, header.ack, header.seq.wrapping_add(1))?;
    } else if !header.flags.contains(TcpFlags::RST) {
        // Send RST for unknown connection
        drop(connections);
        send_rst(dst, src, header.ack, header.seq.wrapping_add(1))?;
    }

    Ok(())
}

/// Process segment for existing connection
fn process_segment(
    conn: &mut TcpConnection,
    header: &TcpHeader,
    payload: &[u8],
) -> Result<(), NetError> {
    match conn.state {
        TcpState::SynSent => {
            if header.flags.contains(TcpFlags::SYN | TcpFlags::ACK) {
                // Received SYN-ACK
                if header.ack == conn.send.nxt {
                    conn.recv.nxt = header.seq.wrapping_add(1);
                    conn.send.una = header.ack;
                    conn.state = TcpState::Established;
                    // Send ACK
                    send_ack(conn)?;
                }
            }
        }
        TcpState::Established => {
            // Check sequence number
            if header.seq == conn.recv.nxt {
                // In-order data
                if !payload.is_empty() {
                    conn.recv_buffer.extend(payload);
                    conn.recv.nxt = conn.recv.nxt.wrapping_add(payload.len() as u32);
                }

                // Process ACK
                if header.flags.contains(TcpFlags::ACK) {
                    // Update send.una
                    if wrapping_lt(conn.send.una, header.ack)
                        && wrapping_le(header.ack, conn.send.nxt)
                    {
                        conn.send.una = header.ack;
                    }
                }

                // Check for FIN
                if header.flags.contains(TcpFlags::FIN) {
                    conn.recv.nxt = conn.recv.nxt.wrapping_add(1);
                    conn.state = TcpState::CloseWait;
                }

                // Send ACK
                send_ack(conn)?;
            }
        }
        TcpState::FinWait1 => {
            if header.flags.contains(TcpFlags::ACK) {
                if header.ack == conn.send.nxt {
                    conn.state = TcpState::FinWait2;
                }
            }
            if header.flags.contains(TcpFlags::FIN) {
                conn.recv.nxt = conn.recv.nxt.wrapping_add(1);
                conn.state = TcpState::TimeWait;
                send_ack(conn)?;
            }
        }
        TcpState::FinWait2 => {
            if header.flags.contains(TcpFlags::FIN) {
                conn.recv.nxt = conn.recv.nxt.wrapping_add(1);
                conn.state = TcpState::TimeWait;
                send_ack(conn)?;
            }
        }
        TcpState::LastAck => {
            if header.flags.contains(TcpFlags::ACK) {
                if header.ack == conn.send.nxt {
                    conn.state = TcpState::Closed;
                }
            }
        }
        _ => {}
    }

    Ok(())
}

// ============================================================================
// Segment Sending
// ============================================================================

/// Send SYN segment
fn send_syn(local: SocketAddr, remote: SocketAddr, seq: u32) -> Result<(), NetError> {
    let header = TcpHeader {
        src_port: local.port,
        dst_port: remote.port,
        seq,
        ack: 0,
        data_offset: 5,
        flags: TcpFlags::SYN,
        window: 65535,
        checksum: 0,
        urgent_ptr: 0,
    };

    let segment = header.build(&[]);
    send_segment(local.ip, remote.ip, &segment)
}

/// Send ACK segment
fn send_ack(conn: &TcpConnection) -> Result<(), NetError> {
    let header = TcpHeader {
        src_port: conn.local.port,
        dst_port: conn.remote.port,
        seq: conn.send.nxt,
        ack: conn.recv.nxt,
        data_offset: 5,
        flags: TcpFlags::ACK,
        window: conn.recv.wnd as u16,
        checksum: 0,
        urgent_ptr: 0,
    };

    let segment = header.build(&[]);
    send_segment(conn.local.ip, conn.remote.ip, &segment)
}

/// Send RST segment
fn send_rst(local: SocketAddr, remote: SocketAddr, seq: u32, ack: u32) -> Result<(), NetError> {
    let header = TcpHeader {
        src_port: local.port,
        dst_port: remote.port,
        seq,
        ack,
        data_offset: 5,
        flags: TcpFlags::RST | TcpFlags::ACK,
        window: 0,
        checksum: 0,
        urgent_ptr: 0,
    };

    let segment = header.build(&[]);
    send_segment(local.ip, remote.ip, &segment)
}

/// Send a TCP segment via IP
fn send_segment(src: IpAddr, dst: IpAddr, segment: &[u8]) -> Result<(), NetError> {
    // In real implementation, route and send via IP layer
    log::trace!("Sending TCP segment {} -> {} ({} bytes)",
        match src { IpAddr::V4(a) => alloc::format!("{}", a), IpAddr::V6(_) => "v6".into() },
        match dst { IpAddr::V4(a) => alloc::format!("{}", a), IpAddr::V6(_) => "v6".into() },
        segment.len()
    );
    Ok(())
}

// ============================================================================
// Helpers
// ============================================================================

/// Wrapping less-than for sequence numbers
fn wrapping_lt(a: u32, b: u32) -> bool {
    (a.wrapping_sub(b) as i32) < 0
}

/// Wrapping less-than-or-equal for sequence numbers
fn wrapping_le(a: u32, b: u32) -> bool {
    a == b || wrapping_lt(a, b)
}
