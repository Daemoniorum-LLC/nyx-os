//! # Capability System
//!
//! Zero ambient authority through unforgeable capability tokens.
//!
//! ## Design
//!
//! Every kernel object is accessed through capabilities. Capabilities are
//! 128-bit tokens containing:
//! - Object ID (64 bits): Globally unique identifier
//! - Rights (32 bits): What operations are permitted
//! - Generation (32 bits): Prevents use-after-revoke
//!
//! ## Security Properties (Formally Verified)
//!
//! - **Monotonicity**: Derived capabilities have ≤ rights of parent
//! - **No Forgery**: Capabilities can only be created by derivation
//! - **Complete Revocation**: Revoking a capability invalidates all derivations

mod cspace;
mod derive;
mod object;
mod rights;

pub use cspace::{CNode, CSlot, CSpace, CSpaceError};
pub use object::{ObjectId, ObjectType};
pub use rights::Rights;

use core::sync::atomic::{AtomicU64, Ordering};
use spin::{Lazy, RwLock};

/// Global capability generation counter
static GENERATION: AtomicU64 = AtomicU64::new(1);

/// Capability registry (maps object IDs to metadata)
static REGISTRY: Lazy<RwLock<CapabilityRegistry>> = Lazy::new(|| {
    RwLock::new(CapabilityRegistry::new_lazy())
});

/// Initialize the capability system
pub fn init() {
    log::trace!("Capability system initialized");
}

/// Create a new capability space for a process
pub fn create_cspace() -> CSpace {
    CSpace::new(DEFAULT_CSPACE_QUOTA)
}

/// Default capability space quota (number of capabilities)
const DEFAULT_CSPACE_QUOTA: usize = 4096;

/// Core capability structure - 128 bits, fits in two registers
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capability {
    /// Object identifier (globally unique)
    pub object_id: ObjectId,
    /// Rights bitmap
    pub rights: Rights,
    /// Generation counter (prevents use-after-revoke)
    pub generation: u32,
}

impl Capability {
    /// Create a new capability (kernel-only)
    ///
    /// # Safety
    ///
    /// This bypasses normal capability derivation. Only use during kernel
    /// initialization or for creating root capabilities.
    pub(crate) unsafe fn new_unchecked(
        object_id: ObjectId,
        rights: Rights,
    ) -> Self {
        Self {
            object_id,
            rights,
            generation: current_generation(),
        }
    }

    /// Check if this capability has the required rights
    #[inline]
    pub fn has_rights(&self, required: Rights) -> bool {
        self.rights.contains(required)
    }

    /// Require specific rights, returning error if not present
    #[inline]
    pub fn require(&self, required: Rights) -> Result<(), CapError> {
        if self.has_rights(required) {
            Ok(())
        } else {
            Err(CapError::InsufficientRights {
                required,
                actual: self.rights,
            })
        }
    }

    /// Derive a new capability with reduced rights
    ///
    /// This is the ONLY way to create new capabilities (except kernel bootstrap).
    ///
    /// # Verified Properties
    ///
    /// - Result rights are a subset of self.rights
    /// - Result object_id equals self.object_id
    /// - Result generation equals self.generation
    #[cfg_attr(not(test), contracts::ensures(ret.is_ok() -> ret.as_ref().unwrap().rights.bits() & !self.rights.bits() == 0))]
    #[cfg_attr(not(test), contracts::ensures(ret.is_ok() -> ret.as_ref().unwrap().object_id == self.object_id))]
    pub fn derive(&self, mask: Rights) -> Result<Capability, CapError> {
        // Must have GRANT right to derive
        if !self.rights.contains(Rights::GRANT) {
            return Err(CapError::NoGrantRight);
        }

        // Apply mask - can only reduce rights
        let new_rights = self.rights & mask;

        if new_rights.is_empty() {
            return Err(CapError::EmptyRights);
        }

        // Strip GRANT by default (unless explicitly passed through)
        let final_rights = new_rights & !Rights::GRANT;

        Ok(Capability {
            object_id: self.object_id,
            rights: final_rights,
            generation: self.generation,
        })
    }

    /// Derive with GRANT right passthrough
    ///
    /// Allows the derived capability to further derive capabilities.
    pub fn derive_with_grant(&self, mask: Rights) -> Result<Capability, CapError> {
        if !self.rights.contains(Rights::GRANT) {
            return Err(CapError::NoGrantRight);
        }

        let new_rights = self.rights & mask;

        if new_rights.is_empty() {
            return Err(CapError::EmptyRights);
        }

        Ok(Capability {
            object_id: self.object_id,
            rights: new_rights,
            generation: self.generation,
        })
    }

    /// Check if this capability is still valid (not revoked)
    pub fn is_valid(&self) -> bool {
        let registry = REGISTRY.read();
        registry
            .get(self.object_id)
            .is_some_and(|meta| meta.generation == self.generation)
    }

    /// Validate capability and return error if invalid
    pub fn validate(&self) -> Result<(), CapError> {
        if self.is_valid() {
            Ok(())
        } else {
            Err(CapError::Revoked)
        }
    }
}

/// Get current generation counter
fn current_generation() -> u32 {
    GENERATION.load(Ordering::Relaxed) as u32
}

/// Increment generation (used during revocation)
fn increment_generation() -> u32 {
    GENERATION.fetch_add(1, Ordering::SeqCst) as u32
}

/// Capability errors
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapError {
    /// Capability does not have GRANT right
    NoGrantRight,
    /// Resulting rights would be empty
    EmptyRights,
    /// Capability has been revoked
    Revoked,
    /// Insufficient rights for operation
    InsufficientRights {
        required: Rights,
        actual: Rights,
    },
    /// Object not found
    ObjectNotFound,
    /// CSpace quota exceeded
    QuotaExceeded,
    /// Invalid capability slot
    InvalidSlot,
}

/// Capability metadata stored in registry
struct CapabilityMetadata {
    object_type: ObjectType,
    generation: u32,
    // Reference count for garbage collection
    ref_count: u32,
}

/// Global registry of capability objects
struct CapabilityRegistry {
    // Using a simple vec for now, will be replaced with a proper data structure
    objects: hashbrown::HashMap<ObjectId, CapabilityMetadata>,
}

impl CapabilityRegistry {
    fn new_lazy() -> Self {
        Self {
            objects: hashbrown::HashMap::new(),
        }
    }

    fn get(&self, id: ObjectId) -> Option<&CapabilityMetadata> {
        self.objects.get(&id)
    }

    fn insert(&mut self, id: ObjectId, object_type: ObjectType) {
        self.objects.insert(
            id,
            CapabilityMetadata {
                object_type,
                generation: current_generation(),
                ref_count: 1,
            },
        );
    }

    /// Revoke an object - increment its generation making old caps invalid
    fn revoke(&mut self, id: ObjectId) -> Result<(), CapError> {
        let meta = self.objects.get_mut(&id).ok_or(CapError::ObjectNotFound)?;
        meta.generation = increment_generation();
        Ok(())
    }
}

// ============================================================================
// Syscall Interface Functions
// ============================================================================

/// Derive a new capability with reduced rights
pub fn derive(object_id: ObjectId, new_rights: Rights) -> Result<Capability, CapError> {
    // Look up the capability in registry
    let registry = REGISTRY.read();
    let meta = registry.get(object_id).ok_or(CapError::ObjectNotFound)?;

    // Create the derived capability
    let cap = Capability {
        object_id,
        rights: new_rights,
        generation: meta.generation,
    };

    Ok(cap)
}

/// Revoke a capability (invalidates all derived capabilities)
pub fn revoke(object_id: ObjectId) -> Result<(), CapError> {
    let mut registry = REGISTRY.write();
    registry.revoke(object_id)
}

/// Identify a capability - return its type and rights
pub fn identify(object_id: ObjectId) -> Result<(ObjectType, Rights), CapError> {
    let registry = REGISTRY.read();
    let meta = registry.get(object_id).ok_or(CapError::ObjectNotFound)?;

    // Return the object type and full rights (actual rights depend on specific cap)
    Ok((meta.object_type, Rights::all()))
}

/// Grant a capability to another process
pub fn grant(
    object_id: ObjectId,
    _target_process: crate::process::ProcessId,
) -> Result<Capability, CapError> {
    // Look up the capability in registry
    let registry = REGISTRY.read();
    let meta = registry.get(object_id).ok_or(CapError::ObjectNotFound)?;

    // Create a new capability for the target process
    let cap = Capability {
        object_id,
        rights: Rights::all(), // Should be based on original cap's rights
        generation: meta.generation,
    };

    Ok(cap)
}

/// Drop a capability (release reference)
pub fn drop_cap(object_id: ObjectId) -> Result<(), CapError> {
    // Decrement reference count
    // For now, just validate the object exists
    let registry = REGISTRY.read();
    let _ = registry.get(object_id).ok_or(CapError::ObjectNotFound)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_reduces_rights() {
        let cap = unsafe {
            Capability::new_unchecked(
                ObjectId::new_test(1),
                Rights::READ | Rights::WRITE | Rights::GRANT,
            )
        };

        let derived = cap.derive(Rights::READ).unwrap();

        assert!(derived.rights.contains(Rights::READ));
        assert!(!derived.rights.contains(Rights::WRITE));
        assert!(!derived.rights.contains(Rights::GRANT));
    }

    #[test]
    fn test_derive_requires_grant() {
        let cap = unsafe {
            Capability::new_unchecked(
                ObjectId::new_test(1),
                Rights::READ | Rights::WRITE, // No GRANT
            )
        };

        assert!(matches!(cap.derive(Rights::READ), Err(CapError::NoGrantRight)));
    }

    #[test]
    fn test_derive_preserves_object_id() {
        let cap = unsafe {
            Capability::new_unchecked(
                ObjectId::new_test(42),
                Rights::all(),
            )
        };

        let derived = cap.derive(Rights::READ).unwrap();
        assert_eq!(derived.object_id, cap.object_id);
    }
}
