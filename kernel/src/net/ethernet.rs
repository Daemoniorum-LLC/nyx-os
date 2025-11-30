//! Ethernet frame handling
//!
//! Provides parsing and building of Ethernet frames.

use super::{MacAddress, NetError};
use alloc::vec::Vec;

/// IPv4 EtherType
pub const ETHERTYPE_IPV4: u16 = 0x0800;
/// IPv6 EtherType
pub const ETHERTYPE_IPV6: u16 = 0x86DD;
/// ARP EtherType
pub const ETHERTYPE_ARP: u16 = 0x0806;
/// VLAN tag EtherType
pub const ETHERTYPE_VLAN: u16 = 0x8100;

/// Ethernet frame header size
pub const ETHERNET_HEADER_SIZE: usize = 14;
/// Minimum Ethernet frame size (excluding FCS)
pub const ETHERNET_MIN_SIZE: usize = 60;
/// Maximum Ethernet frame size (excluding FCS)
pub const ETHERNET_MAX_SIZE: usize = 1514;
/// Maximum payload size (MTU)
pub const ETHERNET_MTU: usize = 1500;

/// Parsed Ethernet frame
#[derive(Clone, Debug)]
pub struct EthernetFrame<'a> {
    /// Destination MAC address
    pub dest: MacAddress,
    /// Source MAC address
    pub src: MacAddress,
    /// EtherType
    pub ethertype: u16,
    /// VLAN tag (if present)
    pub vlan: Option<VlanTag>,
    /// Payload data
    pub payload: &'a [u8],
}

/// VLAN tag
#[derive(Clone, Copy, Debug)]
pub struct VlanTag {
    /// Priority Code Point (3 bits)
    pub pcp: u8,
    /// Drop Eligible Indicator (1 bit)
    pub dei: bool,
    /// VLAN ID (12 bits)
    pub vid: u16,
}

impl<'a> EthernetFrame<'a> {
    /// Parse an Ethernet frame from raw bytes
    pub fn parse(data: &'a [u8]) -> Result<Self, NetError> {
        if data.len() < ETHERNET_HEADER_SIZE {
            return Err(NetError::BufferTooSmall);
        }

        let dest = MacAddress::new([
            data[0], data[1], data[2], data[3], data[4], data[5],
        ]);
        let src = MacAddress::new([
            data[6], data[7], data[8], data[9], data[10], data[11],
        ]);

        let mut offset = 12;
        let mut ethertype = u16::from_be_bytes([data[offset], data[offset + 1]]);
        offset += 2;

        // Check for VLAN tag
        let vlan = if ethertype == ETHERTYPE_VLAN {
            if data.len() < offset + 4 {
                return Err(NetError::BufferTooSmall);
            }

            let tci = u16::from_be_bytes([data[offset], data[offset + 1]]);
            let vlan = VlanTag {
                pcp: ((tci >> 13) & 0x07) as u8,
                dei: ((tci >> 12) & 0x01) != 0,
                vid: tci & 0x0FFF,
            };
            offset += 2;

            ethertype = u16::from_be_bytes([data[offset], data[offset + 1]]);
            offset += 2;

            Some(vlan)
        } else {
            None
        };

        let payload = &data[offset..];

        Ok(EthernetFrame {
            dest,
            src,
            ethertype,
            vlan,
            payload,
        })
    }

    /// Build an Ethernet frame
    pub fn build(
        src: MacAddress,
        dest: MacAddress,
        ethertype: u16,
        payload: &[u8],
    ) -> Vec<u8> {
        let mut frame = Vec::with_capacity(ETHERNET_HEADER_SIZE + payload.len());

        // Destination MAC
        frame.extend_from_slice(dest.as_bytes());
        // Source MAC
        frame.extend_from_slice(src.as_bytes());
        // EtherType
        frame.extend_from_slice(&ethertype.to_be_bytes());
        // Payload
        frame.extend_from_slice(payload);

        // Pad to minimum size if necessary
        while frame.len() < ETHERNET_MIN_SIZE {
            frame.push(0);
        }

        frame
    }

    /// Build an Ethernet frame with VLAN tag
    pub fn build_with_vlan(
        src: MacAddress,
        dest: MacAddress,
        vlan: VlanTag,
        ethertype: u16,
        payload: &[u8],
    ) -> Vec<u8> {
        let mut frame = Vec::with_capacity(ETHERNET_HEADER_SIZE + 4 + payload.len());

        // Destination MAC
        frame.extend_from_slice(dest.as_bytes());
        // Source MAC
        frame.extend_from_slice(src.as_bytes());
        // VLAN EtherType
        frame.extend_from_slice(&ETHERTYPE_VLAN.to_be_bytes());
        // VLAN TCI
        let tci = ((vlan.pcp as u16) << 13)
            | (if vlan.dei { 1 << 12 } else { 0 })
            | (vlan.vid & 0x0FFF);
        frame.extend_from_slice(&tci.to_be_bytes());
        // Real EtherType
        frame.extend_from_slice(&ethertype.to_be_bytes());
        // Payload
        frame.extend_from_slice(payload);

        // Pad to minimum size if necessary
        while frame.len() < ETHERNET_MIN_SIZE + 4 {
            frame.push(0);
        }

        frame
    }
}

/// ARP operation codes
#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArpOperation {
    Request = 1,
    Reply = 2,
}

/// ARP packet (for IPv4 over Ethernet)
#[derive(Clone, Debug)]
pub struct ArpPacket {
    /// Hardware type (1 = Ethernet)
    pub hw_type: u16,
    /// Protocol type (0x0800 = IPv4)
    pub proto_type: u16,
    /// Hardware address length (6 for Ethernet)
    pub hw_len: u8,
    /// Protocol address length (4 for IPv4)
    pub proto_len: u8,
    /// Operation
    pub operation: ArpOperation,
    /// Sender hardware address
    pub sender_hw: MacAddress,
    /// Sender protocol address
    pub sender_ip: super::Ipv4Addr,
    /// Target hardware address
    pub target_hw: MacAddress,
    /// Target protocol address
    pub target_ip: super::Ipv4Addr,
}

impl ArpPacket {
    /// ARP packet size for Ethernet/IPv4
    pub const SIZE: usize = 28;

    /// Parse an ARP packet
    pub fn parse(data: &[u8]) -> Result<Self, NetError> {
        if data.len() < Self::SIZE {
            return Err(NetError::BufferTooSmall);
        }

        let hw_type = u16::from_be_bytes([data[0], data[1]]);
        let proto_type = u16::from_be_bytes([data[2], data[3]]);
        let hw_len = data[4];
        let proto_len = data[5];
        let op = u16::from_be_bytes([data[6], data[7]]);

        let operation = match op {
            1 => ArpOperation::Request,
            2 => ArpOperation::Reply,
            _ => return Err(NetError::ProtocolError),
        };

        let sender_hw = MacAddress::new([
            data[8], data[9], data[10], data[11], data[12], data[13],
        ]);
        let sender_ip = super::Ipv4Addr::from_bytes([
            data[14], data[15], data[16], data[17],
        ]);
        let target_hw = MacAddress::new([
            data[18], data[19], data[20], data[21], data[22], data[23],
        ]);
        let target_ip = super::Ipv4Addr::from_bytes([
            data[24], data[25], data[26], data[27],
        ]);

        Ok(ArpPacket {
            hw_type,
            proto_type,
            hw_len,
            proto_len,
            operation,
            sender_hw,
            sender_ip,
            target_hw,
            target_ip,
        })
    }

    /// Build an ARP packet
    pub fn build(&self) -> Vec<u8> {
        let mut packet = Vec::with_capacity(Self::SIZE);

        packet.extend_from_slice(&self.hw_type.to_be_bytes());
        packet.extend_from_slice(&self.proto_type.to_be_bytes());
        packet.push(self.hw_len);
        packet.push(self.proto_len);
        packet.extend_from_slice(&(self.operation as u16).to_be_bytes());
        packet.extend_from_slice(self.sender_hw.as_bytes());
        packet.extend_from_slice(self.sender_ip.as_bytes());
        packet.extend_from_slice(self.target_hw.as_bytes());
        packet.extend_from_slice(self.target_ip.as_bytes());

        packet
    }

    /// Create an ARP request
    pub fn request(sender_hw: MacAddress, sender_ip: super::Ipv4Addr, target_ip: super::Ipv4Addr) -> Self {
        ArpPacket {
            hw_type: 1,          // Ethernet
            proto_type: 0x0800,  // IPv4
            hw_len: 6,
            proto_len: 4,
            operation: ArpOperation::Request,
            sender_hw,
            sender_ip,
            target_hw: MacAddress::ZERO,
            target_ip,
        }
    }

    /// Create an ARP reply
    pub fn reply(sender_hw: MacAddress, sender_ip: super::Ipv4Addr, target_hw: MacAddress, target_ip: super::Ipv4Addr) -> Self {
        ArpPacket {
            hw_type: 1,
            proto_type: 0x0800,
            hw_len: 6,
            proto_len: 4,
            operation: ArpOperation::Reply,
            sender_hw,
            sender_ip,
            target_hw,
            target_ip,
        }
    }
}
