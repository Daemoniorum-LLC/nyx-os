//! Shared Memory Regions
//!
//! Provides shared memory regions that can be mapped into multiple
//! address spaces. Used for zero-copy IPC and shared buffers.

use crate::cap::{Capability, ObjectId, ObjectType, Rights};
use crate::mem::{PhysAddr, PAGE_SIZE};
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use spin::RwLock;

/// Global shared memory region registry
static SHARED_REGIONS: RwLock<BTreeMap<ObjectId, SharedRegion>> = RwLock::new(BTreeMap::new());

/// A shared memory region
pub struct SharedRegion {
    /// Region ID
    pub id: ObjectId,
    /// Total size in bytes
    pub size: u64,
    /// Physical frames backing this region
    frames: Vec<PhysAddr>,
    /// Reference count (number of mappings)
    ref_count: u32,
    /// Flags
    flags: SharedFlags,
}

bitflags::bitflags! {
    /// Shared region flags
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct SharedFlags: u32 {
        /// Region is locked in physical memory
        const LOCKED = 1 << 0;
        /// Region supports huge pages
        const HUGE_PAGES = 1 << 1;
        /// Region is GPU-accessible
        const GPU_ACCESSIBLE = 1 << 2;
    }
}

/// Shared memory errors
#[derive(Debug, Clone)]
pub enum ShmError {
    /// Region not found
    NotFound,
    /// Out of memory
    OutOfMemory,
    /// Invalid size
    InvalidSize,
    /// Permission denied
    PermissionDenied,
}

impl SharedRegion {
    /// Create a new shared region
    pub fn new(size: u64, flags: SharedFlags) -> Result<Self, ShmError> {
        if size == 0 {
            return Err(ShmError::InvalidSize);
        }

        // Calculate number of pages needed
        let num_pages = ((size + PAGE_SIZE - 1) / PAGE_SIZE) as usize;

        // Allocate physical frames
        let mut frames = Vec::with_capacity(num_pages);
        for _ in 0..num_pages {
            let frame = crate::mem::alloc_frame().ok_or(ShmError::OutOfMemory)?;
            frames.push(frame);
        }

        Ok(Self {
            id: ObjectId::new(ObjectType::SharedMemory),
            size,
            frames,
            ref_count: 1,
            flags,
        })
    }

    /// Get the physical frame for a given offset
    pub fn get_frame(&self, offset: u64) -> Option<PhysAddr> {
        let page_index = (offset / PAGE_SIZE) as usize;
        self.frames.get(page_index).copied()
    }

    /// Get all frames in this region
    pub fn frames(&self) -> &[PhysAddr] {
        &self.frames
    }

    /// Increment reference count
    pub fn add_ref(&mut self) {
        self.ref_count = self.ref_count.saturating_add(1);
    }

    /// Decrement reference count, returns true if region should be freed
    pub fn release(&mut self) -> bool {
        self.ref_count = self.ref_count.saturating_sub(1);
        self.ref_count == 0
    }
}

// ============================================================================
// Public API
// ============================================================================

/// Create a new shared memory region
pub fn create(size: u64, flags: SharedFlags) -> Result<Capability, ShmError> {
    let region = SharedRegion::new(size, flags)?;
    let object_id = region.id;

    SHARED_REGIONS.write().insert(object_id, region);

    let cap = unsafe {
        Capability::new_unchecked(
            object_id,
            Rights::READ | Rights::WRITE | Rights::MAP | Rights::GRANT,
        )
    };

    log::debug!("Created shared memory region {:?}: {} bytes", object_id, size);

    Ok(cap)
}

/// Destroy a shared memory region
pub fn destroy(cap: Capability) -> Result<(), ShmError> {
    cap.require(Rights::WRITE).map_err(|_| ShmError::PermissionDenied)?;

    let mut regions = SHARED_REGIONS.write();
    if let Some(region) = regions.get_mut(&cap.object_id) {
        if region.release() {
            // Free all frames
            for frame in &region.frames {
                crate::mem::free_frame(*frame);
            }
            regions.remove(&cap.object_id);
            log::debug!("Destroyed shared memory region {:?}", cap.object_id);
        }
    }

    Ok(())
}

/// Get the physical frame for a shared region at a given offset
///
/// This is called from the virtual memory fault handler when a
/// shared memory region needs to be mapped.
pub fn get_frame(region_id: ObjectId, offset: u64) -> Option<PhysAddr> {
    let regions = SHARED_REGIONS.read();
    let region = regions.get(&region_id)?;
    region.get_frame(offset)
}

/// Get region size
pub fn get_size(region_id: ObjectId) -> Option<u64> {
    SHARED_REGIONS.read().get(&region_id).map(|r| r.size)
}

/// Add a reference to a shared region (when mapping into new address space)
pub fn add_ref(region_id: ObjectId) -> bool {
    if let Some(region) = SHARED_REGIONS.write().get_mut(&region_id) {
        region.add_ref();
        true
    } else {
        false
    }
}

/// Release a reference to a shared region (when unmapping)
pub fn release_ref(region_id: ObjectId) {
    let mut regions = SHARED_REGIONS.write();
    let should_free = if let Some(region) = regions.get_mut(&region_id) {
        region.release()
    } else {
        false
    };

    if should_free {
        if let Some(region) = regions.remove(&region_id) {
            for frame in &region.frames {
                crate::mem::free_frame(*frame);
            }
            log::debug!("Freed shared memory region {:?}", region_id);
        }
    }
}
