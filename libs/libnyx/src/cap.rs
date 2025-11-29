//! Capability management

pub use bitflags::bitflags;

bitflags! {
    /// Capability rights
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Rights: u64 {
        const READ = 1 << 0;
        const WRITE = 1 << 1;
        const EXECUTE = 1 << 2;
        const GRANT = 1 << 3;
        const REVOKE = 1 << 4;
        const SEND = 1 << 16;
        const RECEIVE = 1 << 17;
        const INFERENCE = 1 << 42;
    }
}

/// Capability handle (slot in CSpace)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capability(pub u32);

impl Capability {
    /// Invalid capability
    pub const INVALID: Self = Self(u32::MAX);

    /// Create from slot number
    pub const fn from_slot(slot: u32) -> Self {
        Self(slot)
    }

    /// Get slot number
    pub const fn slot(&self) -> u32 {
        self.0
    }

    /// Check if valid
    pub const fn is_valid(&self) -> bool {
        self.0 != u32::MAX
    }
}
