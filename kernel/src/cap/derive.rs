//! Capability derivation rules and verification
//!
//! This module contains the formally verified derivation logic.

use super::{CapError, Capability, Rights};

/// Verify that a derivation is valid
///
/// # Verified Properties (Lean 4)
///
/// 1. Rights monotonicity: derived.rights ⊆ parent.rights
/// 2. Object identity: derived.object_id = parent.object_id
/// 3. Generation preservation: derived.generation = parent.generation
#[inline]
pub fn verify_derivation(parent: &Capability, derived: &Capability) -> Result<(), CapError> {
    // Check rights monotonicity
    if !derived.rights.is_subset_of(parent.rights) {
        return Err(CapError::InsufficientRights {
            required: derived.rights,
            actual: parent.rights,
        });
    }

    // Check object identity
    if derived.object_id != parent.object_id {
        return Err(CapError::ObjectNotFound);
    }

    // Check generation
    if derived.generation != parent.generation {
        return Err(CapError::Revoked);
    }

    Ok(())
}

/// Compute the derived rights given parent rights and a mask
#[inline]
pub fn compute_derived_rights(parent_rights: Rights, mask: Rights, keep_grant: bool) -> Rights {
    let mut result = parent_rights & mask;

    if !keep_grant {
        result &= !Rights::GRANT;
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cap::ObjectId;

    #[test]
    fn test_verify_valid_derivation() {
        let parent = Capability {
            object_id: ObjectId::from_raw(1),
            rights: Rights::READ | Rights::WRITE | Rights::GRANT,
            generation: 1,
        };

        let derived = Capability {
            object_id: ObjectId::from_raw(1),
            rights: Rights::READ,
            generation: 1,
        };

        assert!(verify_derivation(&parent, &derived).is_ok());
    }

    #[test]
    fn test_verify_invalid_rights() {
        let parent = Capability {
            object_id: ObjectId::from_raw(1),
            rights: Rights::READ,
            generation: 1,
        };

        let derived = Capability {
            object_id: ObjectId::from_raw(1),
            rights: Rights::READ | Rights::WRITE, // WRITE not in parent!
            generation: 1,
        };

        assert!(verify_derivation(&parent, &derived).is_err());
    }
}
