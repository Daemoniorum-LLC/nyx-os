//! IPC message types

use crate::cap::Rights;

/// Message header
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct MessageHeader {
    /// Message length (including header)
    pub length: u32,
    /// Message type/tag
    pub tag: u32,
    /// Number of capabilities being transferred
    pub cap_count: u8,
    /// Flags
    pub flags: u8,
    /// Reserved
    pub _reserved: [u8; 6],
}

/// Complete message structure
#[repr(C)]
pub struct Message {
    /// Header
    pub header: MessageHeader,
    /// Inline data (for small messages)
    pub inline_data: [u8; 256],
    /// Capability slots being transferred
    pub caps: [u32; 4],
    /// Memory grant for large data
    pub memory_grant: Option<MemoryGrant>,
}

impl Default for Message {
    fn default() -> Self {
        Self {
            header: MessageHeader::default(),
            inline_data: [0; 256],
            caps: [0; 4],
            memory_grant: None,
        }
    }
}

/// Memory grant for zero-copy transfers
#[derive(Clone, Copy, Debug)]
pub struct MemoryGrant {
    /// Capability slot for memory region
    pub cap_slot: u32,
    /// Offset into memory region
    pub offset: u64,
    /// Length of grant
    pub length: u64,
    /// Rights being granted
    pub rights: Rights,
}

impl Message {
    /// Create a simple message with inline data
    pub fn simple(tag: u32, data: &[u8]) -> Self {
        let mut msg = Self::default();
        msg.header.tag = tag;
        msg.header.length = (core::mem::size_of::<MessageHeader>() + data.len()) as u32;

        let copy_len = data.len().min(256);
        msg.inline_data[..copy_len].copy_from_slice(&data[..copy_len]);

        msg
    }

    /// Create a message with capability transfer
    pub fn with_caps(tag: u32, caps: &[u32]) -> Self {
        let mut msg = Self::default();
        msg.header.tag = tag;
        msg.header.cap_count = caps.len().min(4) as u8;

        for (i, &cap) in caps.iter().take(4).enumerate() {
            msg.caps[i] = cap;
        }

        msg
    }

    /// Get inline data
    pub fn data(&self) -> &[u8] {
        let data_len = (self.header.length as usize)
            .saturating_sub(core::mem::size_of::<MessageHeader>())
            .min(256);
        &self.inline_data[..data_len]
    }
}
