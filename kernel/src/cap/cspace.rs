//! Capability Space (CSpace) implementation

use super::{CapError, Capability};
use alloc::boxed::Box;

/// Default CSpace quota
pub const DEFAULT_QUOTA: usize = 4096;

/// CNode size (slots per node)
const CNODE_SIZE: usize = 256;

/// Capability space - radix tree of capabilities
#[derive(Clone)]
pub struct CSpace {
    root: Box<CNode>,
    count: usize,
    quota: usize,
}

/// CNode - a node in the capability tree
#[derive(Clone)]
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cap::{ObjectId, Rights};

    fn make_test_cap(id: u64) -> Capability {
        unsafe { Capability::new_unchecked(ObjectId::from_raw(id), Rights::READ | Rights::WRITE) }
    }

    #[test]
    fn test_cspace_new() {
        let cspace = CSpace::new(100);
        assert!(cspace.is_empty());
        assert_eq!(cspace.len(), 0);
    }

    #[test]
    fn test_cspace_insert_lookup() {
        let mut cspace = CSpace::new(100);
        let cap = make_test_cap(1);

        assert!(cspace.insert(0, cap).is_ok());
        assert_eq!(cspace.len(), 1);

        let found = cspace.lookup(0);
        assert!(found.is_some());
        assert_eq!(found.unwrap().object_id.raw(), 1);
    }

    #[test]
    fn test_cspace_insert_multiple() {
        let mut cspace = CSpace::new(100);

        for i in 0..10 {
            let cap = make_test_cap(i as u64);
            assert!(cspace.insert(i, cap).is_ok());
        }

        assert_eq!(cspace.len(), 10);

        for i in 0..10 {
            assert!(cspace.lookup(i).is_some());
        }
    }

    #[test]
    fn test_cspace_quota_exceeded() {
        let mut cspace = CSpace::new(2);

        assert!(cspace.insert(0, make_test_cap(1)).is_ok());
        assert!(cspace.insert(1, make_test_cap(2)).is_ok());

        let result = cspace.insert(2, make_test_cap(3));
        assert_eq!(result, Err(CSpaceError::QuotaExceeded));
    }

    #[test]
    fn test_cspace_slot_occupied() {
        let mut cspace = CSpace::new(100);

        assert!(cspace.insert(0, make_test_cap(1)).is_ok());

        let result = cspace.insert(0, make_test_cap(2));
        assert_eq!(result, Err(CSpaceError::SlotOccupied));
    }

    #[test]
    fn test_cspace_remove() {
        let mut cspace = CSpace::new(100);
        let cap = make_test_cap(1);

        assert!(cspace.insert(0, cap).is_ok());
        assert_eq!(cspace.len(), 1);

        let removed = cspace.remove(0);
        assert!(removed.is_ok());
        assert_eq!(removed.unwrap().object_id.raw(), 1);
        assert_eq!(cspace.len(), 0);
    }

    #[test]
    fn test_cspace_remove_empty_slot() {
        let mut cspace = CSpace::new(100);

        let result = cspace.remove(0);
        assert_eq!(result, Err(CSpaceError::EmptySlot));
    }

    #[test]
    fn test_cspace_lookup_empty() {
        let cspace = CSpace::new(100);
        assert!(cspace.lookup(0).is_none());
        assert!(cspace.lookup(255).is_none());
    }

    #[test]
    fn test_cspace_is_empty() {
        let mut cspace = CSpace::new(100);
        assert!(cspace.is_empty());

        cspace.insert(0, make_test_cap(1)).unwrap();
        assert!(!cspace.is_empty());

        cspace.remove(0).unwrap();
        assert!(cspace.is_empty());
    }

    #[test]
    fn test_cspace_insert_many() {
        let mut cspace = CSpace::new(100);

        let caps = vec![
            (0, make_test_cap(1)),
            (1, make_test_cap(2)),
            (2, make_test_cap(3)),
        ];

        assert!(cspace.insert_many(&caps).is_ok());
        assert_eq!(cspace.len(), 3);
    }

    #[test]
    fn test_cspace_insert_many_partial_fail() {
        let mut cspace = CSpace::new(100);
        cspace.insert(1, make_test_cap(99)).unwrap();

        let caps = vec![
            (0, make_test_cap(1)),
            (1, make_test_cap(2)), // This slot is occupied
            (2, make_test_cap(3)),
        ];

        let result = cspace.insert_many(&caps);
        assert_eq!(result, Err(CSpaceError::SlotOccupied));
    }

    #[test]
    fn test_cspace_insert_next() {
        let mut cspace = CSpace::new(100);

        let slot1 = cspace.insert_next(make_test_cap(1));
        assert_eq!(slot1, Ok(0));

        let slot2 = cspace.insert_next(make_test_cap(2));
        assert_eq!(slot2, Ok(1));

        let slot3 = cspace.insert_next(make_test_cap(3));
        assert_eq!(slot3, Ok(2));

        assert_eq!(cspace.len(), 3);
    }

    #[test]
    fn test_cspace_insert_next_fills_gaps() {
        let mut cspace = CSpace::new(100);

        cspace.insert_next(make_test_cap(1)).unwrap();
        cspace.insert_next(make_test_cap(2)).unwrap();
        cspace.insert_next(make_test_cap(3)).unwrap();

        cspace.remove(1).unwrap();

        let slot = cspace.insert_next(make_test_cap(4));
        assert_eq!(slot, Ok(1)); // Should fill the gap
    }

    #[test]
    fn test_cspace_get() {
        let mut cspace = CSpace::new(100);
        cspace.insert(5, make_test_cap(42)).unwrap();

        let cap = cspace.get(5);
        assert!(cap.is_some());
        assert_eq!(cap.unwrap().object_id.raw(), 42);

        assert!(cspace.get(0).is_none());
    }

    #[test]
    fn test_cspace_clone_cspace() {
        let mut cspace = CSpace::new(100);
        cspace.insert(0, make_test_cap(1)).unwrap();
        cspace.insert(5, make_test_cap(2)).unwrap();
        cspace.insert(10, make_test_cap(3)).unwrap();

        let cloned = cspace.clone_cspace();

        assert_eq!(cloned.len(), 3);
        assert!(cloned.lookup(0).is_some());
        assert!(cloned.lookup(5).is_some());
        assert!(cloned.lookup(10).is_some());
    }

    #[test]
    fn test_cspace_export_all() {
        let mut cspace = CSpace::new(100);
        cspace.insert(0, make_test_cap(1)).unwrap();
        cspace.insert(5, make_test_cap(2)).unwrap();

        let exported = cspace.export_all();

        assert_eq!(exported.len(), 2);
        assert!(exported.contains_key(&0));
        assert!(exported.contains_key(&5));
    }

    #[test]
    fn test_cslot_default() {
        let slot = CSlot::default();
        assert!(matches!(slot, CSlot::Empty));
    }

    #[test]
    fn test_cspace_error_from_caperror() {
        let cap_err = CapError::Revoked;
        let cspace_err: CSpaceError = cap_err.into();
        assert!(matches!(cspace_err, CSpaceError::Cap(CapError::Revoked)));
    }
}
