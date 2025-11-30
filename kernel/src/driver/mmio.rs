//! Memory-Mapped I/O (MMIO) support
//!
//! Manages MMIO regions for device drivers.

use super::DriverError;
use crate::mem::{PhysAddr, VirtAddr, PAGE_SIZE};
use alloc::collections::BTreeMap;
use spin::RwLock;

/// Registered MMIO regions
static MMIO_REGIONS: RwLock<BTreeMap<PhysAddr, MmioRegion>> = RwLock::new(BTreeMap::new());

/// Cached mappings (phys -> virt)
static MMIO_MAPPINGS: RwLock<BTreeMap<PhysAddr, VirtAddr>> = RwLock::new(BTreeMap::new());

/// Next MMIO virtual address
static NEXT_MMIO_VIRT: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(MMIO_BASE);

/// MMIO region base in virtual address space
const MMIO_BASE: u64 = 0xFFFF_FF00_0000_0000;

/// MMIO region info
#[derive(Clone, Debug)]
pub struct MmioRegion {
    /// Physical address
    pub phys_addr: PhysAddr,
    /// Size in bytes
    pub size: u64,
    /// Virtual address (if mapped)
    pub virt_addr: Option<VirtAddr>,
    /// Owner process
    pub owner: Option<crate::process::ProcessId>,
    /// Flags
    pub flags: MmioFlags,
}

bitflags::bitflags! {
    /// MMIO region flags
    #[derive(Clone, Copy, Debug, Default)]
    pub struct MmioFlags: u32 {
        /// Write-combining cache mode
        const WRITE_COMBINE = 1 << 0;
        /// Uncacheable (default for MMIO)
        const UNCACHED = 1 << 1;
        /// Write-through
        const WRITE_THROUGH = 1 << 2;
        /// Prefetchable
        const PREFETCHABLE = 1 << 3;
    }
}

/// Register an MMIO region
pub fn register_region(phys_addr: PhysAddr, size: u64) -> Result<(), DriverError> {
    let aligned_phys = PhysAddr::new(phys_addr.as_u64() & !(PAGE_SIZE - 1));
    let aligned_size = (size + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);

    let regions = MMIO_REGIONS.read();

    // Check for overlaps
    for (existing_phys, existing) in regions.iter() {
        let existing_end = existing_phys.as_u64() + existing.size;
        let new_end = aligned_phys.as_u64() + aligned_size;

        if aligned_phys.as_u64() < existing_end && new_end > existing_phys.as_u64() {
            return Err(DriverError::MmioConflict);
        }
    }
    drop(regions);

    let region = MmioRegion {
        phys_addr: aligned_phys,
        size: aligned_size,
        virt_addr: None,
        owner: None,
        flags: MmioFlags::UNCACHED,
    };

    MMIO_REGIONS.write().insert(aligned_phys, region);

    log::debug!(
        "Registered MMIO region {:016x}-{:016x}",
        aligned_phys.as_u64(),
        aligned_phys.as_u64() + aligned_size
    );

    Ok(())
}

/// Map an MMIO region into kernel virtual address space
pub fn map_region(phys_addr: PhysAddr, size: u64) -> Result<VirtAddr, DriverError> {
    // Check if already mapped
    if let Some(&virt) = MMIO_MAPPINGS.read().get(&phys_addr) {
        return Ok(virt);
    }

    let aligned_size = (size + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);

    // Allocate virtual address space
    let virt = VirtAddr::new(
        NEXT_MMIO_VIRT.fetch_add(aligned_size, core::sync::atomic::Ordering::SeqCst),
    );

    // Map pages
    for offset in (0..aligned_size).step_by(PAGE_SIZE as usize) {
        let page_phys = PhysAddr::new(phys_addr.as_u64() + offset);
        let page_virt = VirtAddr::new(virt.as_u64() + offset);

        unsafe {
            map_mmio_page(page_virt, page_phys)?;
        }
    }

    MMIO_MAPPINGS.write().insert(phys_addr, virt);

    // Update region info
    if let Some(region) = MMIO_REGIONS.write().get_mut(&phys_addr) {
        region.virt_addr = Some(virt);
    }

    log::debug!(
        "Mapped MMIO {:016x} -> {:016x}",
        phys_addr.as_u64(),
        virt.as_u64()
    );

    Ok(virt)
}

/// Map a single MMIO page
unsafe fn map_mmio_page(virt: VirtAddr, phys: PhysAddr) -> Result<(), DriverError> {
    use crate::arch::paging::{PageFlags, PageMapper};

    let root = crate::arch::paging::get_kernel_page_table();
    let mut mapper = PageMapper::new(root);

    let flags = PageFlags::PRESENT
        | PageFlags::WRITABLE
        | PageFlags::NO_CACHE
        | PageFlags::WRITE_THROUGH
        | PageFlags::NO_EXECUTE;

    let mut allocator = || crate::mem::alloc_frame();

    mapper
        .map_page(virt, phys, flags, &mut allocator)
        .map_err(|_| DriverError::OutOfResources)?;

    Ok(())
}

/// Unmap an MMIO region
pub fn unmap_region(phys_addr: PhysAddr) -> Result<(), DriverError> {
    let mappings = MMIO_MAPPINGS.read();
    let virt = mappings.get(&phys_addr).ok_or(DriverError::DeviceNotFound)?;

    let regions = MMIO_REGIONS.read();
    let size = regions
        .get(&phys_addr)
        .map(|r| r.size)
        .ok_or(DriverError::DeviceNotFound)?;
    drop(regions);

    // Unmap pages
    for offset in (0..size).step_by(PAGE_SIZE as usize) {
        let page_virt = VirtAddr::new(virt.as_u64() + offset);
        unsafe {
            unmap_mmio_page(page_virt)?;
        }
    }
    drop(mappings);

    MMIO_MAPPINGS.write().remove(&phys_addr);

    // Update region info
    if let Some(region) = MMIO_REGIONS.write().get_mut(&phys_addr) {
        region.virt_addr = None;
    }

    Ok(())
}

/// Unmap a single MMIO page
unsafe fn unmap_mmio_page(virt: VirtAddr) -> Result<(), DriverError> {
    use crate::arch::paging::PageMapper;

    let root = crate::arch::paging::get_kernel_page_table();
    let mut mapper = PageMapper::new(root);

    mapper
        .unmap_page(virt)
        .map_err(|_| DriverError::DeviceNotFound)?;

    crate::arch::paging::flush_tlb_page(virt);

    Ok(())
}

/// Read from MMIO
pub fn read(phys_addr: PhysAddr, size: u8) -> Result<u64, DriverError> {
    // Ensure region is mapped
    let virt = ensure_mapped(phys_addr)?;

    let value = unsafe {
        match size {
            1 => core::ptr::read_volatile(virt.as_u64() as *const u8) as u64,
            2 => core::ptr::read_volatile(virt.as_u64() as *const u16) as u64,
            4 => core::ptr::read_volatile(virt.as_u64() as *const u32) as u64,
            8 => core::ptr::read_volatile(virt.as_u64() as *const u64),
            _ => return Err(DriverError::InvalidConfig),
        }
    };

    Ok(value)
}

/// Write to MMIO
pub fn write(phys_addr: PhysAddr, size: u8, value: u64) -> Result<(), DriverError> {
    // Ensure region is mapped
    let virt = ensure_mapped(phys_addr)?;

    unsafe {
        match size {
            1 => core::ptr::write_volatile(virt.as_u64() as *mut u8, value as u8),
            2 => core::ptr::write_volatile(virt.as_u64() as *mut u16, value as u16),
            4 => core::ptr::write_volatile(virt.as_u64() as *mut u32, value as u32),
            8 => core::ptr::write_volatile(virt.as_u64() as *mut u64, value),
            _ => return Err(DriverError::InvalidConfig),
        }
    }

    Ok(())
}

/// Ensure an address is mapped
fn ensure_mapped(phys_addr: PhysAddr) -> Result<VirtAddr, DriverError> {
    let page_phys = PhysAddr::new(phys_addr.as_u64() & !(PAGE_SIZE - 1));
    let offset = phys_addr.as_u64() & (PAGE_SIZE - 1);

    // Check if page is already mapped
    if let Some(&virt) = MMIO_MAPPINGS.read().get(&page_phys) {
        return Ok(VirtAddr::new(virt.as_u64() + offset));
    }

    // Map a single page
    let virt = map_region(page_phys, PAGE_SIZE)?;
    Ok(VirtAddr::new(virt.as_u64() + offset))
}

/// MMIO accessor helper for drivers
#[repr(C)]
pub struct MmioAccessor {
    base: VirtAddr,
    size: u64,
}

impl MmioAccessor {
    /// Create an accessor for a mapped region
    pub fn new(phys_addr: PhysAddr, size: u64) -> Result<Self, DriverError> {
        let base = map_region(phys_addr, size)?;
        Ok(Self { base, size })
    }

    /// Read u8 at offset
    pub fn read_u8(&self, offset: u64) -> u8 {
        assert!(offset < self.size);
        unsafe { core::ptr::read_volatile((self.base.as_u64() + offset) as *const u8) }
    }

    /// Read u16 at offset
    pub fn read_u16(&self, offset: u64) -> u16 {
        assert!(offset + 2 <= self.size);
        unsafe { core::ptr::read_volatile((self.base.as_u64() + offset) as *const u16) }
    }

    /// Read u32 at offset
    pub fn read_u32(&self, offset: u64) -> u32 {
        assert!(offset + 4 <= self.size);
        unsafe { core::ptr::read_volatile((self.base.as_u64() + offset) as *const u32) }
    }

    /// Read u64 at offset
    pub fn read_u64(&self, offset: u64) -> u64 {
        assert!(offset + 8 <= self.size);
        unsafe { core::ptr::read_volatile((self.base.as_u64() + offset) as *const u64) }
    }

    /// Write u8 at offset
    pub fn write_u8(&self, offset: u64, value: u8) {
        assert!(offset < self.size);
        unsafe { core::ptr::write_volatile((self.base.as_u64() + offset) as *mut u8, value) }
    }

    /// Write u16 at offset
    pub fn write_u16(&self, offset: u64, value: u16) {
        assert!(offset + 2 <= self.size);
        unsafe { core::ptr::write_volatile((self.base.as_u64() + offset) as *mut u16, value) }
    }

    /// Write u32 at offset
    pub fn write_u32(&self, offset: u64, value: u32) {
        assert!(offset + 4 <= self.size);
        unsafe { core::ptr::write_volatile((self.base.as_u64() + offset) as *mut u32, value) }
    }

    /// Write u64 at offset
    pub fn write_u64(&self, offset: u64, value: u64) {
        assert!(offset + 8 <= self.size);
        unsafe { core::ptr::write_volatile((self.base.as_u64() + offset) as *mut u64, value) }
    }

    /// Get base virtual address
    pub fn base(&self) -> VirtAddr {
        self.base
    }
}
