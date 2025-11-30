//! Capability Space (CSpace) implementation

use super::{Capability, CapError};
use alloc::boxed::Box;

/// Default CSpace quota
pub const DEFAULT_QUOTA: usize = 4096;

/// CNode size (slots per node)
const CNODE_SIZE: usize = 256;

/// Capability space - radix tree of capabilities
pub struct CSpace {
    root: Box<CNode>,
    count: usize,
    quota: usize,
}

/// CNode - a node in the capability tree
pub struct CNode {
    slots: [CSlot; CNODE_SIZE],
}

/// Slot in a CNode
#[derive(Clone)]
pub enum CSlot {
    Empty,
    Cap(Capability),
    Node(Box<CNode>),
}

impl Default for CSlot {
    fn default() -> Self {
        CSlot::Empty
    }
}

impl CSpace {
    /// Create a new capability space
    pub fn new(quota: usize) -> Self {
        Self {
            root: Box::new(CNode::new()),
            count: 0,
            quota,
        }
    }

    /// Lookup capability by slot index
    pub fn lookup(&self, slot: usize) -> Option<&Capability> {
        match &self.root.slots[slot % CNODE_SIZE] {
            CSlot::Cap(cap) => Some(cap),
            _ => None,
        }
    }

    /// Insert capability at slot
    pub fn insert(&mut self, slot: usize, cap: Capability) -> Result<(), CSpaceError> {
        if self.count >= self.quota {
            return Err(CSpaceError::QuotaExceeded);
        }

        let idx = slot % CNODE_SIZE;
        if !matches!(self.root.slots[idx], CSlot::Empty) {
            return Err(CSpaceError::SlotOccupied);
        }

        self.root.slots[idx] = CSlot::Cap(cap);
        self.count += 1;
        Ok(())
    }

    /// Remove capability from slot
    pub fn remove(&mut self, slot: usize) -> Result<Capability, CSpaceError> {
        let idx = slot % CNODE_SIZE;
        match core::mem::take(&mut self.root.slots[idx]) {
            CSlot::Cap(cap) => {
                self.count -= 1;
                Ok(cap)
            }
            _ => Err(CSpaceError::EmptySlot),
        }
    }

    /// Get number of capabilities
    pub fn len(&self) -> usize {
        self.count
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Insert multiple capabilities
    pub fn insert_many(&mut self, caps: &[(usize, Capability)]) -> Result<(), CSpaceError> {
        for (slot, cap) in caps {
            self.insert(*slot, *cap)?;
        }
        Ok(())
    }

    /// Insert capability at next available slot, returning the slot number
    pub fn insert_next(&mut self, cap: Capability) -> Result<u32, CSpaceError> {
        if self.count >= self.quota {
            return Err(CSpaceError::QuotaExceeded);
        }

        // Find first empty slot
        for i in 0..CNODE_SIZE {
            if matches!(self.root.slots[i], CSlot::Empty) {
                self.root.slots[i] = CSlot::Cap(cap);
                self.count += 1;
                return Ok(i as u32);
            }
        }

        Err(CSpaceError::QuotaExceeded)
    }

    /// Get capability by slot
    pub fn get(&self, slot: u32) -> Option<&Capability> {
        self.lookup(slot as usize)
    }

    /// Clone the CSpace (for fork)
    pub fn clone_cspace(&self) -> Self {
        let mut new_cspace = Self::new(self.quota);
        for i in 0..CNODE_SIZE {
            if let CSlot::Cap(cap) = &self.root.slots[i] {
                new_cspace.root.slots[i] = CSlot::Cap(*cap);
                new_cspace.count += 1;
            }
        }
        new_cspace
    }

    /// Export all capabilities as a BTreeMap (for checkpointing)
    pub fn export_all(&self) -> alloc::collections::BTreeMap<u32, Capability> {
        let mut map = alloc::collections::BTreeMap::new();
        for i in 0..CNODE_SIZE {
            if let CSlot::Cap(cap) = &self.root.slots[i] {
                map.insert(i as u32, *cap);
            }
        }
        map
    }
}

impl CNode {
    fn new() -> Self {
        Self {
            slots: core::array::from_fn(|_| CSlot::Empty),
        }
    }
}

/// CSpace errors
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CSpaceError {
    /// Quota exceeded
    QuotaExceeded,
    /// Slot already occupied
    SlotOccupied,
    /// Slot is empty
    EmptySlot,
    /// Invalid slot path
    InvalidPath,
    /// Capability error
    Cap(CapError),
}

impl From<CapError> for CSpaceError {
    fn from(err: CapError) -> Self {
        CSpaceError::Cap(err)
    }
}
