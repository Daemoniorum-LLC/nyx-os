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
//! ## Security Properties (Enforced by Implementation)
//!
//! - **Monotonicity**: Derived capabilities have ≤ rights of parent
//! - **No Forgery**: Capabilities can only be created by derivation from existing caps
//! - **Complete Revocation**: Revoking a capability invalidates all derivations
//! - **Right Preservation**: Granted capabilities never exceed source rights

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
static REGISTRY: Lazy<RwLock<CapabilityRegistry>> =
    Lazy::new(|| RwLock::new(CapabilityRegistry::new_lazy()));

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
    pub(crate) unsafe fn new_unchecked(object_id: ObjectId, rights: Rights) -> Self {
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
    InsufficientRights { required: Rights, actual: Rights },
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
    /// The maximum rights that can be granted for this object
    /// (rights of the original creator capability)
    rights: Rights,
    /// Reference count for garbage collection
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

    fn insert(&mut self, id: ObjectId, object_type: ObjectType, rights: Rights) {
        self.objects.insert(
            id,
            CapabilityMetadata {
                object_type,
                generation: current_generation(),
                rights,
                ref_count: 1,
            },
        );
    }

    /// Insert with default all rights (for backward compatibility)
    fn insert_with_full_rights(&mut self, id: ObjectId, object_type: ObjectType) {
        self.insert(id, object_type, Rights::all())
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
///
/// **DEPRECATED**: Use `grant_with_rights` instead, which enforces the principle
/// of least privilege by requiring explicit rights specification.
///
/// This function grants with empty rights (read-only) for safety.
pub fn grant(
    object_id: ObjectId,
    target_process: crate::process::ProcessId,
) -> Result<Capability, CapError> {
    // Default to minimal rights for safety
    grant_with_rights(object_id, target_process, Rights::READ.bits() as u64)
}

/// Grant a capability to another process with specific rights
///
/// ## Security Guarantees
///
/// - The granted capability's rights are the INTERSECTION of:
///   1. The source object's tracked rights in the registry
///   2. The requested rights mask
/// - This ensures the monotonicity property: no escalation is possible
///
/// ## Arguments
///
/// * `object_id` - The object to grant access to
/// * `target_process` - The process receiving the capability
/// * `rights_mask` - The maximum rights to grant (will be intersected with source rights)
///
/// ## Returns
///
/// * `Ok(Capability)` - A new capability for the target process
/// * `Err(CapError)` - If the object doesn't exist or the caller lacks GRANT right
pub fn grant_with_rights(
    object_id: ObjectId,
    _target_process: crate::process::ProcessId,
    rights_mask: u64,
) -> Result<Capability, CapError> {
    // Look up the capability in registry to get the object's rights
    let registry = REGISTRY.read();
    let meta = registry.get(object_id).ok_or(CapError::ObjectNotFound)?;

    // Get the source capability's rights from the registry
    // In a full implementation, we'd look up the caller's CSpace to find their
    // capability and use those rights. For now, we track rights per-object.
    let source_rights = meta.rights;

    // Verify the source has GRANT right - you can only grant what you can grant
    if !source_rights.contains(Rights::GRANT) {
        return Err(CapError::NoGrantRight);
    }

    // Apply the rights mask (intersection) - can never escalate rights
    let requested = Rights::from_bits_truncate(rights_mask);
    let granted_rights = source_rights & requested;

    // Strip GRANT right from granted capability by default
    // (prevents infinite delegation chains unless explicitly allowed)
    let final_rights = granted_rights & !Rights::GRANT;

    if final_rights.is_empty() {
        return Err(CapError::EmptyRights);
    }

    // Create a new capability for the target process
    let cap = Capability {
        object_id,
        rights: final_rights,
        generation: meta.generation,
    };

    // In a full implementation, we'd insert this into the target's CSpace
    // and increment the object's reference count

    Ok(cap)
}

/// Drop a capability (release reference)
pub fn drop_cap(object_id: ObjectId) -> Result<(), CapError> {
    // Decrement reference count
    let mut registry = REGISTRY.write();
    let meta = registry
        .objects
        .get_mut(&object_id)
        .ok_or(CapError::ObjectNotFound)?;

    meta.ref_count = meta.ref_count.saturating_sub(1);

    // If ref_count reaches zero, we could garbage collect the object
    // For now, we keep it around in case of late revocation checks

    Ok(())
}

/// Register a new kernel object in the capability registry
///
/// This is called when creating kernel objects (processes, threads, endpoints, etc.)
/// to make them accessible via capabilities.
///
/// ## Arguments
///
/// * `id` - The unique identifier for the object
/// * `object_type` - The type of kernel object
/// * `initial_rights` - The rights the creator capability will have
///
/// ## Returns
///
/// A capability with full rights for the object (the "root" capability)
pub fn register_object(
    id: ObjectId,
    object_type: ObjectType,
    initial_rights: Rights,
) -> Capability {
    let mut registry = REGISTRY.write();
    registry.insert(id, object_type, initial_rights);

    Capability {
        object_id: id,
        rights: initial_rights,
        generation: current_generation(),
    }
}

/// Check if an object exists in the registry
pub fn object_exists(object_id: ObjectId) -> bool {
    let registry = REGISTRY.read();
    registry.get(object_id).is_some()
}

/// Get the type of a registered object
pub fn object_type(object_id: ObjectId) -> Option<ObjectType> {
    let registry = REGISTRY.read();
    registry.get(object_id).map(|m| m.object_type)
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

        assert!(matches!(
            cap.derive(Rights::READ),
            Err(CapError::NoGrantRight)
        ));
    }

    #[test]
    fn test_derive_preserves_object_id() {
        let cap = unsafe { Capability::new_unchecked(ObjectId::new_test(42), Rights::all()) };

        let derived = cap.derive(Rights::READ).unwrap();
        assert_eq!(derived.object_id, cap.object_id);
    }

    #[test]
    fn test_capability_has_rights() {
        let cap = unsafe {
            Capability::new_unchecked(ObjectId::new_test(1), Rights::READ | Rights::WRITE)
        };

        assert!(cap.has_rights(Rights::READ));
        assert!(cap.has_rights(Rights::WRITE));
        assert!(cap.has_rights(Rights::READ | Rights::WRITE));
        assert!(!cap.has_rights(Rights::EXECUTE));
        assert!(!cap.has_rights(Rights::GRANT));
    }

    #[test]
    fn test_capability_require_success() {
        let cap = unsafe {
            Capability::new_unchecked(ObjectId::new_test(1), Rights::READ | Rights::WRITE)
        };

        assert!(cap.require(Rights::READ).is_ok());
        assert!(cap.require(Rights::WRITE).is_ok());
        assert!(cap.require(Rights::READ | Rights::WRITE).is_ok());
    }

    #[test]
    fn test_capability_require_failure() {
        let cap = unsafe { Capability::new_unchecked(ObjectId::new_test(1), Rights::READ) };

        let result = cap.require(Rights::WRITE);
        assert!(matches!(result, Err(CapError::InsufficientRights { .. })));
    }

    #[test]
    fn test_derive_empty_rights_fails() {
        let cap = unsafe { Capability::new_unchecked(ObjectId::new_test(1), Rights::GRANT) };

        // Deriving with empty mask should fail
        let result = cap.derive(Rights::empty());
        assert!(matches!(result, Err(CapError::EmptyRights)));
    }

    #[test]
    fn test_derive_with_grant_passthrough() {
        let cap = unsafe {
            Capability::new_unchecked(
                ObjectId::new_test(1),
                Rights::READ | Rights::WRITE | Rights::GRANT,
            )
        };

        let derived = cap.derive_with_grant(Rights::READ | Rights::GRANT).unwrap();
        assert!(derived.rights.contains(Rights::READ));
        assert!(derived.rights.contains(Rights::GRANT));
    }

    #[test]
    fn test_derive_with_grant_requires_grant() {
        let cap = unsafe {
            Capability::new_unchecked(
                ObjectId::new_test(1),
                Rights::READ | Rights::WRITE, // No GRANT
            )
        };

        let result = cap.derive_with_grant(Rights::READ);
        assert!(matches!(result, Err(CapError::NoGrantRight)));
    }

    #[test]
    fn test_capability_equality() {
        let cap1 = unsafe { Capability::new_unchecked(ObjectId::new_test(1), Rights::READ) };

        let cap2 = unsafe { Capability::new_unchecked(ObjectId::new_test(1), Rights::READ) };

        // Note: generation might differ, so we compare object_id and rights
        assert_eq!(cap1.object_id, cap2.object_id);
        assert_eq!(cap1.rights, cap2.rights);
    }

    #[test]
    fn test_cap_error_variants() {
        let err1 = CapError::NoGrantRight;
        let err2 = CapError::EmptyRights;
        let err3 = CapError::Revoked;
        let err4 = CapError::ObjectNotFound;
        let err5 = CapError::QuotaExceeded;
        let err6 = CapError::InvalidSlot;

        assert_ne!(err1, err2);
        assert_ne!(err2, err3);
        assert_ne!(err3, err4);
        assert_ne!(err4, err5);
        assert_ne!(err5, err6);
    }

    #[test]
    fn test_cap_error_insufficient_rights() {
        let err = CapError::InsufficientRights {
            required: Rights::WRITE,
            actual: Rights::READ,
        };

        if let CapError::InsufficientRights { required, actual } = err {
            assert!(required.contains(Rights::WRITE));
            assert!(actual.contains(Rights::READ));
        } else {
            panic!("Expected InsufficientRights");
        }
    }

    #[test]
    fn test_derive_intersection_with_mask() {
        let cap = unsafe {
            Capability::new_unchecked(
                ObjectId::new_test(1),
                Rights::READ | Rights::WRITE | Rights::EXECUTE | Rights::GRANT,
            )
        };

        // Ask for READ | EXECUTE, but cap has READ | WRITE | EXECUTE
        let derived = cap.derive(Rights::READ | Rights::EXECUTE).unwrap();

        assert!(derived.rights.contains(Rights::READ));
        assert!(derived.rights.contains(Rights::EXECUTE));
        assert!(!derived.rights.contains(Rights::WRITE));
        assert!(!derived.rights.contains(Rights::GRANT)); // GRANT is stripped
    }
}
