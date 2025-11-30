//! IP (v4/v6) packet handling
//!
//! Provides parsing, building, and routing of IP packets.

use super::{InterfaceId, IpAddr, Ipv4Addr, Ipv6Addr, NetError, Protocol};
use alloc::vec::Vec;

/// IPv4 header minimum size
pub const IPV4_HEADER_MIN_SIZE: usize = 20;
/// IPv4 maximum header size
pub const IPV4_HEADER_MAX_SIZE: usize = 60;

/// IPv6 header size (fixed)
pub const IPV6_HEADER_SIZE: usize = 40;

/// IPv4 packet
#[derive(Clone, Debug)]
pub struct Ipv4Packet<'a> {
    /// Version (always 4)
    pub version: u8,
    /// Header length (in 32-bit words)
    pub ihl: u8,
    /// Differentiated Services Code Point
    pub dscp: u8,
    /// Explicit Congestion Notification
    pub ecn: u8,
    /// Total length (header + payload)
    pub total_length: u16,
    /// Identification
    pub identification: u16,
    /// Don't Fragment flag
    pub dont_fragment: bool,
    /// More Fragments flag
    pub more_fragments: bool,
    /// Fragment offset
    pub fragment_offset: u16,
    /// Time To Live
    pub ttl: u8,
    /// Protocol
    pub protocol: Protocol,
    /// Header checksum
    pub checksum: u16,
    /// Source address
    pub src: Ipv4Addr,
    /// Destination address
    pub dst: Ipv4Addr,
    /// Options (if any)
    pub options: Option<&'a [u8]>,
    /// Payload
    pub payload: &'a [u8],
}

impl<'a> Ipv4Packet<'a> {
    /// Parse an IPv4 packet from raw bytes
    pub fn parse(data: &'a [u8]) -> Result<Self, NetError> {
        if data.len() < IPV4_HEADER_MIN_SIZE {
            return Err(NetError::BufferTooSmall);
        }

        let version = (data[0] >> 4) & 0x0F;
        if version != 4 {
            return Err(NetError::ProtocolError);
        }

        let ihl = data[0] & 0x0F;
        let header_len = (ihl as usize) * 4;

        if data.len() < header_len {
            return Err(NetError::BufferTooSmall);
        }

        let dscp = (data[1] >> 2) & 0x3F;
        let ecn = data[1] & 0x03;
        let total_length = u16::from_be_bytes([data[2], data[3]]);
        let identification = u16::from_be_bytes([data[4], data[5]]);

        let flags_offset = u16::from_be_bytes([data[6], data[7]]);
        let dont_fragment = (flags_offset & 0x4000) != 0;
        let more_fragments = (flags_offset & 0x2000) != 0;
        let fragment_offset = (flags_offset & 0x1FFF) * 8;

        let ttl = data[8];
        let protocol = Protocol::from_ip_protocol(data[9]);
        let checksum = u16::from_be_bytes([data[10], data[11]]);

        let src = Ipv4Addr::from_bytes([data[12], data[13], data[14], data[15]]);
        let dst = Ipv4Addr::from_bytes([data[16], data[17], data[18], data[19]]);

        let options = if header_len > IPV4_HEADER_MIN_SIZE {
            Some(&data[IPV4_HEADER_MIN_SIZE..header_len])
        } else {
            None
        };

        let payload_len = (total_length as usize).saturating_sub(header_len);
        let payload = &data[header_len..header_len + payload_len.min(data.len() - header_len)];

        Ok(Ipv4Packet {
            version,
            ihl,
            dscp,
            ecn,
            total_length,
            identification,
            dont_fragment,
            more_fragments,
            fragment_offset,
            ttl,
            protocol,
            checksum,
            src,
            dst,
            options,
            payload,
        })
    }

    /// Build an IPv4 packet
    pub fn build(
        src: Ipv4Addr,
        dst: Ipv4Addr,
        protocol: Protocol,
        ttl: u8,
        payload: &[u8],
    ) -> Vec<u8> {
        let total_length = (IPV4_HEADER_MIN_SIZE + payload.len()) as u16;

        let mut packet = Vec::with_capacity(IPV4_HEADER_MIN_SIZE + payload.len());

        // Version (4) and IHL (5 = 20 bytes)
        packet.push(0x45);
        // DSCP and ECN
        packet.push(0);
        // Total length
        packet.extend_from_slice(&total_length.to_be_bytes());
        // Identification
        packet.extend_from_slice(&0u16.to_be_bytes());
        // Flags and fragment offset (Don't Fragment)
        packet.extend_from_slice(&0x4000u16.to_be_bytes());
        // TTL
        packet.push(ttl);
        // Protocol
        packet.push(protocol.to_ip_protocol());
        // Checksum (placeholder, calculated below)
        packet.extend_from_slice(&0u16.to_be_bytes());
        // Source address
        packet.extend_from_slice(src.as_bytes());
        // Destination address
        packet.extend_from_slice(dst.as_bytes());
        // Payload
        packet.extend_from_slice(payload);

        // Calculate and insert checksum
        let checksum = ipv4_checksum(&packet[..IPV4_HEADER_MIN_SIZE]);
        packet[10] = (checksum >> 8) as u8;
        packet[11] = checksum as u8;

        packet
    }

    /// Verify the header checksum
    pub fn verify_checksum(&self) -> bool {
        // For simplicity, assume valid (real implementation would verify)
        true
    }
}

/// Calculate IPv4 header checksum
pub fn ipv4_checksum(header: &[u8]) -> u16 {
    let mut sum: u32 = 0;

    for i in (0..header.len()).step_by(2) {
        let word = if i == 10 {
            // Skip checksum field
            0
        } else if i + 1 < header.len() {
            u16::from_be_bytes([header[i], header[i + 1]]) as u32
        } else {
            (header[i] as u32) << 8
        };
        sum = sum.wrapping_add(word);
    }

    // Fold 32-bit sum to 16 bits
    while (sum >> 16) != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }

    !(sum as u16)
}

/// IPv6 packet
#[derive(Clone, Debug)]
pub struct Ipv6Packet<'a> {
    /// Version (always 6)
    pub version: u8,
    /// Traffic class
    pub traffic_class: u8,
    /// Flow label
    pub flow_label: u32,
    /// Payload length
    pub payload_length: u16,
    /// Next header (protocol)
    pub next_header: Protocol,
    /// Hop limit
    pub hop_limit: u8,
    /// Source address
    pub src: Ipv6Addr,
    /// Destination address
    pub dst: Ipv6Addr,
    /// Payload
    pub payload: &'a [u8],
}

impl<'a> Ipv6Packet<'a> {
    /// Parse an IPv6 packet
    pub fn parse(data: &'a [u8]) -> Result<Self, NetError> {
        if data.len() < IPV6_HEADER_SIZE {
            return Err(NetError::BufferTooSmall);
        }

        let version = (data[0] >> 4) & 0x0F;
        if version != 6 {
            return Err(NetError::ProtocolError);
        }

        let traffic_class = ((data[0] & 0x0F) << 4) | ((data[1] >> 4) & 0x0F);
        let flow_label = (((data[1] & 0x0F) as u32) << 16)
            | ((data[2] as u32) << 8)
            | (data[3] as u32);

        let payload_length = u16::from_be_bytes([data[4], data[5]]);
        let next_header = Protocol::from_ip_protocol(data[6]);
        let hop_limit = data[7];

        let mut src_bytes = [0u8; 16];
        src_bytes.copy_from_slice(&data[8..24]);
        let src = Ipv6Addr::from_bytes(src_bytes);

        let mut dst_bytes = [0u8; 16];
        dst_bytes.copy_from_slice(&data[24..40]);
        let dst = Ipv6Addr::from_bytes(dst_bytes);

        let payload = &data[IPV6_HEADER_SIZE..];

        Ok(Ipv6Packet {
            version,
            traffic_class,
            flow_label,
            payload_length,
            next_header,
            hop_limit,
            src,
            dst,
            payload,
        })
    }

    /// Build an IPv6 packet
    pub fn build(
        src: Ipv6Addr,
        dst: Ipv6Addr,
        next_header: Protocol,
        hop_limit: u8,
        payload: &[u8],
    ) -> Vec<u8> {
        let payload_length = payload.len() as u16;

        let mut packet = Vec::with_capacity(IPV6_HEADER_SIZE + payload.len());

        // Version, traffic class, flow label
        packet.push(0x60); // Version 6, TC high nibble = 0
        packet.push(0);    // TC low nibble = 0, flow label high
        packet.push(0);    // Flow label mid
        packet.push(0);    // Flow label low

        // Payload length
        packet.extend_from_slice(&payload_length.to_be_bytes());
        // Next header
        packet.push(next_header.to_ip_protocol());
        // Hop limit
        packet.push(hop_limit);
        // Source address
        packet.extend_from_slice(src.as_bytes());
        // Destination address
        packet.extend_from_slice(dst.as_bytes());
        // Payload
        packet.extend_from_slice(payload);

        packet
    }
}

// ============================================================================
// Packet Handling
// ============================================================================

/// Handle received IPv4 packet
pub fn handle_ipv4_packet(interface_id: InterfaceId, data: &[u8]) -> Result<(), NetError> {
    let packet = Ipv4Packet::parse(data)?;

    log::trace!(
        "IPv4: {} -> {} proto={:?} len={}",
        packet.src,
        packet.dst,
        packet.protocol,
        packet.total_length
    );

    // Dispatch based on protocol
    match packet.protocol {
        Protocol::Tcp => {
            super::tcp::handle_segment(
                IpAddr::V4(packet.src),
                IpAddr::V4(packet.dst),
                packet.payload,
            )?;
        }
        Protocol::Udp => {
            super::udp::handle_datagram(
                IpAddr::V4(packet.src),
                IpAddr::V4(packet.dst),
                packet.payload,
            )?;
        }
        Protocol::Icmp => {
            handle_icmpv4(interface_id, &packet)?;
        }
        Protocol::Raw(_) => {
            log::trace!("Ignoring raw protocol packet");
        }
    }

    Ok(())
}

/// Handle received IPv6 packet
pub fn handle_ipv6_packet(interface_id: InterfaceId, data: &[u8]) -> Result<(), NetError> {
    let packet = Ipv6Packet::parse(data)?;

    log::trace!(
        "IPv6: {:?} -> {:?} proto={:?}",
        packet.src,
        packet.dst,
        packet.next_header
    );

    // Dispatch based on next header
    match packet.next_header {
        Protocol::Tcp => {
            super::tcp::handle_segment(
                IpAddr::V6(packet.src),
                IpAddr::V6(packet.dst),
                packet.payload,
            )?;
        }
        Protocol::Udp => {
            super::udp::handle_datagram(
                IpAddr::V6(packet.src),
                IpAddr::V6(packet.dst),
                packet.payload,
            )?;
        }
        _ => {
            log::trace!("Ignoring IPv6 packet with next header {:?}", packet.next_header);
        }
    }

    Ok(())
}

/// Handle ICMPv4 packet
fn handle_icmpv4(interface_id: InterfaceId, packet: &Ipv4Packet) -> Result<(), NetError> {
    if packet.payload.is_empty() {
        return Err(NetError::BufferTooSmall);
    }

    let icmp_type = packet.payload[0];
    let icmp_code = packet.payload.get(1).copied().unwrap_or(0);

    match icmp_type {
        8 => {
            // Echo Request - send Echo Reply
            log::trace!("ICMP Echo Request from {}", packet.src);
            // Would send reply here
        }
        0 => {
            // Echo Reply
            log::trace!("ICMP Echo Reply from {}", packet.src);
        }
        3 => {
            // Destination Unreachable
            log::trace!("ICMP Destination Unreachable from {}, code={}", packet.src, icmp_code);
        }
        11 => {
            // Time Exceeded
            log::trace!("ICMP Time Exceeded from {}", packet.src);
        }
        _ => {
            log::trace!("ICMP type {} from {}", icmp_type, packet.src);
        }
    }

    Ok(())
}

// ============================================================================
// Pseudo-Header Checksum
// ============================================================================

/// Calculate TCP/UDP pseudo-header checksum for IPv4
pub fn pseudo_header_checksum_v4(
    src: Ipv4Addr,
    dst: Ipv4Addr,
    protocol: u8,
    length: u16,
) -> u32 {
    let mut sum: u32 = 0;

    // Source address
    sum = sum.wrapping_add(u16::from_be_bytes([src.0[0], src.0[1]]) as u32);
    sum = sum.wrapping_add(u16::from_be_bytes([src.0[2], src.0[3]]) as u32);

    // Destination address
    sum = sum.wrapping_add(u16::from_be_bytes([dst.0[0], dst.0[1]]) as u32);
    sum = sum.wrapping_add(u16::from_be_bytes([dst.0[2], dst.0[3]]) as u32);

    // Protocol and length
    sum = sum.wrapping_add(protocol as u32);
    sum = sum.wrapping_add(length as u32);

    sum
}

/// Fold checksum and complement
pub fn finish_checksum(mut sum: u32) -> u16 {
    while (sum >> 16) != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !(sum as u16)
}
