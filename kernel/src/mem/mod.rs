//! Memory management subsystem
//!
//! ## Components
//!
//! - Physical frame allocator (buddy allocator with NUMA awareness)
//! - Virtual memory manager (per-process address spaces)
//! - Kernel heap allocator
//! - Safe userspace memory access primitives
//! - Memory tagging for spatial safety (ARM MTE / Intel LAM)

mod frame;
mod heap;
pub mod user;
pub mod virt;

pub use frame::FrameAllocator;
pub use user::{copy_from_user, copy_string_from_user, copy_to_user, UserMemError};
pub use virt::{AddressSpace, Protection, VirtualMemory};

/// Convert physical address to virtual address (identity mapping for kernel)
#[inline]
pub fn phys_to_virt(phys: PhysAddr) -> u64 {
    // In the kernel, physical memory is typically identity-mapped or offset-mapped
    // For simplicity, we assume identity mapping in the higher half
    const KERNEL_PHYS_OFFSET: u64 = 0xFFFF_8000_0000_0000;
    phys.as_u64() + KERNEL_PHYS_OFFSET
}

use crate::arch::BootInfo;
use spin::Mutex;

/// Global frame allocator
static FRAME_ALLOCATOR: Mutex<Option<FrameAllocator>> = Mutex::new(None);

/// Initialize memory subsystem
pub fn init(boot_info: &BootInfo) {
    log::debug!("Initializing memory subsystem");

    // Initialize frame allocator from memory map
    let mut allocator = FrameAllocator::new();

    for region in boot_info.memory_map {
        if region.region_type == crate::arch::MemoryRegionType::Usable {
            log::trace!(
                "Adding memory region: {:#x} - {:#x} ({} MB)",
                region.start,
                region.start + region.size,
                region.size / 1024 / 1024
            );
            allocator.add_region(region.start, region.size);
        }
    }

    *FRAME_ALLOCATOR.lock() = Some(allocator);

    // Initialize kernel heap
    heap::init();

    log::debug!("Memory subsystem initialized");
}

/// Create a new address space
pub fn create_address_space() -> AddressSpace {
    AddressSpace::new()
}

/// Physical address type
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct PhysAddr(pub u64);

impl PhysAddr {
    /// Create a new physical address
    pub const fn new(addr: u64) -> Self {
        Self(addr)
    }

    /// Get the raw address value
    pub const fn as_u64(self) -> u64 {
        self.0
    }

    /// Align address down to page boundary
    pub const fn align_down(self, align: u64) -> Self {
        Self(self.0 & !(align - 1))
    }

    /// Align address up to page boundary
    pub const fn align_up(self, align: u64) -> Self {
        Self((self.0 + align - 1) & !(align - 1))
    }
}

/// Virtual address type
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct VirtAddr(pub u64);

impl VirtAddr {
    /// Create a new virtual address
    pub const fn new(addr: u64) -> Self {
        Self(addr)
    }

    /// Get the raw address value
    pub const fn as_u64(self) -> u64 {
        self.0
    }

    /// Align address down
    pub const fn align_down(self, align: u64) -> Self {
        Self(self.0 & !(align - 1))
    }

    /// Align address up
    pub const fn align_up(self, align: u64) -> Self {
        Self((self.0 + align - 1) & !(align - 1))
    }

    /// Get page table indices for this address
    pub const fn page_table_indices(self) -> [usize; 4] {
        [
            ((self.0 >> 39) & 0x1FF) as usize, // PML4
            ((self.0 >> 30) & 0x1FF) as usize, // PDPT
            ((self.0 >> 21) & 0x1FF) as usize, // PD
            ((self.0 >> 12) & 0x1FF) as usize, // PT
        ]
    }
}

/// Page size constants
pub const PAGE_SIZE: u64 = 4096;
pub const HUGE_PAGE_SIZE_2M: u64 = 2 * 1024 * 1024;
pub const HUGE_PAGE_SIZE_1G: u64 = 1024 * 1024 * 1024;

/// Allocate a physical frame
pub fn alloc_frame() -> Option<PhysAddr> {
    FRAME_ALLOCATOR.lock().as_mut()?.alloc_frame()
}

/// Free a physical frame
pub fn free_frame(addr: PhysAddr) {
    if let Some(allocator) = FRAME_ALLOCATOR.lock().as_mut() {
        allocator.free_frame(addr);
    }
}

/// Allocate contiguous physical frames
pub fn alloc_frames(count: usize) -> Option<PhysAddr> {
    FRAME_ALLOCATOR.lock().as_mut()?.alloc_frames(count)
}

/// Allocate contiguous physical memory of specified size
///
/// This is used for DMA buffers that require physically contiguous memory.
pub fn alloc_contiguous(size: u64) -> Option<PhysAddr> {
    let num_frames = ((size + PAGE_SIZE - 1) / PAGE_SIZE) as usize;
    alloc_frames(num_frames)
}

/// Free contiguous physical memory
pub fn free_contiguous(addr: PhysAddr, size: u64) {
    let num_frames = ((size + PAGE_SIZE - 1) / PAGE_SIZE) as usize;
    for i in 0..num_frames {
        free_frame(PhysAddr::new(addr.as_u64() + (i as u64) * PAGE_SIZE));
    }
}
