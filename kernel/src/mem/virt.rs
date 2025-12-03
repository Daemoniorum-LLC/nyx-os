//! Virtual memory manager

use super::{PhysAddr, VirtAddr, PAGE_SIZE};
use crate::arch::x86_64::paging::{flush_tlb_page, PageFlags, PageMapper};
use crate::cap::ObjectId;
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use bitflags::bitflags;

/// Virtual address space
#[derive(Clone)]
pub struct AddressSpace {
    /// Unique ID
    pub id: ObjectId,
    /// Virtual memory areas
    vmas: BTreeMap<VirtAddr, Vma>,
    /// Page table root (physical address)
    page_table_root: PhysAddr,
}

/// Virtual memory area
#[derive(Clone, Debug)]
pub struct Vma {
    /// Start address
    pub start: VirtAddr,
    /// End address (exclusive)
    pub end: VirtAddr,
    /// Protection flags
    pub protection: Protection,
    /// Backing type
    pub backing: VmaBacking,
    /// Flags
    pub flags: VmaFlags,
}

impl Vma {
    /// Check if this VMA is readable
    pub fn is_readable(&self) -> bool {
        self.protection.contains(Protection::READ)
    }

    /// Check if this VMA is writable
    pub fn is_writable(&self) -> bool {
        self.protection.contains(Protection::WRITE)
    }

    /// Check if this VMA is executable
    pub fn is_executable(&self) -> bool {
        self.protection.contains(Protection::EXECUTE)
    }

    /// Check if this VMA is user-accessible
    pub fn is_user(&self) -> bool {
        self.protection.contains(Protection::USER)
    }

    /// Get the size of this VMA
    pub fn size(&self) -> u64 {
        self.end.as_u64() - self.start.as_u64()
    }

    /// Check if an address is within this VMA
    pub fn contains(&self, addr: VirtAddr) -> bool {
        addr.as_u64() >= self.start.as_u64() && addr.as_u64() < self.end.as_u64()
    }

    /// Check if this VMA is a kernel mapping (not user-accessible)
    pub fn is_kernel(&self) -> bool {
        !self.protection.contains(Protection::USER)
    }

    /// Get the start address of this VMA
    pub fn start(&self) -> VirtAddr {
        self.start
    }

    /// Get the end address of this VMA
    pub fn end(&self) -> VirtAddr {
        self.end
    }
}

bitflags! {
    /// Memory protection flags
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct Protection: u8 {
        const READ = 1 << 0;
        const WRITE = 1 << 1;
        const EXECUTE = 1 << 2;
        const USER = 1 << 3;
    }
}

bitflags! {
    /// VMA flags
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct VmaFlags: u32 {
        /// Copy-on-write
        const COW = 1 << 0;
        /// Locked in memory
        const LOCKED = 1 << 1;
        /// Huge pages
        const HUGE_PAGES = 1 << 2;
        /// No dump
        const NO_DUMP = 1 << 3;
        /// GPU accessible
        const GPU_ACCESSIBLE = 1 << 4;
    }
}

/// VMA backing type
#[derive(Clone, Debug)]
pub enum VmaBacking {
    /// Anonymous (demand-paged)
    Anonymous,
    /// File-backed
    File { file: ObjectId, offset: u64 },
    /// Physical memory (MMIO)
    Physical { phys: PhysAddr },
    /// Shared memory
    Shared { region: ObjectId },
    /// Tensor buffer
    Tensor { tensor: ObjectId, offset: u64 },
}

impl AddressSpace {
    /// Create a new address space
    pub fn new() -> Self {
        // Allocate page table root
        let page_table_root = super::alloc_frame().expect("Failed to allocate page table root");

        Self {
            id: ObjectId::new(crate::cap::ObjectType::AddressSpace),
            vmas: BTreeMap::new(),
            page_table_root,
        }
    }

    /// Map a region
    pub fn map(
        &mut self,
        start: VirtAddr,
        size: u64,
        protection: Protection,
        backing: VmaBacking,
    ) -> Result<(), VmError> {
        let end = VirtAddr::new(start.as_u64() + size);

        // Check for overlaps
        for (_, vma) in self.vmas.range(..end) {
            if vma.end.as_u64() > start.as_u64() {
                return Err(VmError::Overlap);
            }
        }

        let vma = Vma {
            start,
            end,
            protection,
            backing,
            flags: VmaFlags::empty(),
        };

        self.vmas.insert(start, vma);
        Ok(())
    }

    /// Unmap a region
    pub fn unmap(&mut self, start: VirtAddr, size: u64) -> Result<(), VmError> {
        let end = start.as_u64() + size;

        // Find and remove overlapping VMAs
        let to_remove: Vec<_> = self
            .vmas
            .range(..VirtAddr::new(end))
            .filter(|(_, vma)| vma.end.as_u64() > start.as_u64())
            .map(|(k, _)| *k)
            .collect();

        for key in to_remove {
            self.vmas.remove(&key);
        }

        Ok(())
    }

    /// Handle page fault
    pub fn handle_fault(&mut self, addr: VirtAddr, write: bool) -> Result<(), VmError> {
        // Find VMA containing the address
        let vma = self
            .vmas
            .range(..=addr)
            .next_back()
            .filter(|(_, vma)| vma.end.as_u64() > addr.as_u64())
            .map(|(_, vma)| vma);

        let vma = match vma {
            Some(v) => v,
            None => return Err(VmError::NotMapped),
        };

        // Check permissions
        if write && !vma.protection.contains(Protection::WRITE) {
            return Err(VmError::PermissionDenied);
        }

        // Allocate and map page based on backing
        match &vma.backing {
            VmaBacking::Anonymous => {
                let frame = super::alloc_frame().ok_or(VmError::OutOfMemory)?;
                self.map_page(addr, frame, vma.protection)?;
            }
            VmaBacking::Physical { phys } => {
                let offset = addr.as_u64() - vma.start.as_u64();
                let phys_addr = PhysAddr::new(phys.as_u64() + offset);
                self.map_page(addr, phys_addr, vma.protection)?;
            }
            VmaBacking::File { file, offset } => {
                // File-backed mapping: allocate a frame and read from file
                let frame = super::alloc_frame().ok_or(VmError::OutOfMemory)?;

                // Calculate file offset for this page
                let page_offset = addr.as_u64() - vma.start.as_u64();
                let file_offset = offset + page_offset;

                // Read page data from file
                if let Err(_) = self.read_file_page(*file, file_offset, frame) {
                    // If file read fails, zero the page (sparse file behavior)
                    let virt_ptr = super::phys_to_virt(frame) as *mut u8;
                    unsafe {
                        core::ptr::write_bytes(virt_ptr, 0, super::PAGE_SIZE as usize);
                    }
                }

                self.map_page(addr, frame, vma.protection)?;
            }
            VmaBacking::Shared { region } => {
                // Shared memory: look up physical frame from shared region
                let frame = self
                    .lookup_shared_frame(*region, addr.as_u64() - vma.start.as_u64())
                    .ok_or(VmError::OutOfMemory)?;

                self.map_page(addr, frame, vma.protection)?;
            }
            VmaBacking::Tensor { tensor, offset } => {
                // Tensor buffer: map from tensor's device memory
                // For CPU tensors, we can map directly
                // For GPU tensors, this should trigger a migration or fault
                let frame = self
                    .lookup_tensor_frame(*tensor, *offset + (addr.as_u64() - vma.start.as_u64()))
                    .ok_or(VmError::DeviceMemory)?;

                self.map_page(addr, frame, vma.protection)?;
            }
        }

        Ok(())
    }

    /// Map a single page
    pub fn map_page(
        &mut self,
        virt: VirtAddr,
        phys: PhysAddr,
        prot: Protection,
    ) -> Result<(), VmError> {
        // Convert protection flags to page flags
        let mut flags = PageFlags::PRESENT;

        if prot.contains(Protection::WRITE) {
            flags |= PageFlags::WRITABLE;
        }

        if prot.contains(Protection::USER) {
            flags |= PageFlags::USER;
        }

        if !prot.contains(Protection::EXECUTE) {
            flags |= PageFlags::NO_EXECUTE;
        }

        // Create a page mapper for this address space
        let mut mapper = PageMapper::new(self.page_table_root);

        // Allocate page tables as needed
        let mut allocator = || super::alloc_frame();

        mapper
            .map_page(virt, phys, flags, &mut allocator)
            .map_err(|e| match e {
                crate::arch::x86_64::paging::MapError::AlreadyMapped => VmError::Overlap,
                crate::arch::x86_64::paging::MapError::OutOfMemory => VmError::OutOfMemory,
                _ => VmError::NotImplemented,
            })?;

        // Flush TLB for this page
        flush_tlb_page(virt);

        Ok(())
    }

    /// Unmap a single page
    fn unmap_page(&mut self, virt: VirtAddr) -> Result<PhysAddr, VmError> {
        let mut mapper = PageMapper::new(self.page_table_root);

        mapper.unmap_page(virt).map_err(|e| match e {
            crate::arch::x86_64::paging::MapError::NotMapped => VmError::NotMapped,
            _ => VmError::NotImplemented,
        })
    }

    /// Get the page table root physical address
    pub fn page_table_root(&self) -> PhysAddr {
        self.page_table_root
    }

    /// Switch to this address space
    pub fn activate(&self) {
        crate::arch::x86_64::paging::switch_address_space(self.page_table_root);
    }

    /// Get iterator over memory regions (for checkpointing)
    pub fn regions(&self) -> impl Iterator<Item = &Vma> {
        self.vmas.values()
    }

    /// Get iterator over VMAs (alias for regions)
    pub fn iter_vmas(&self) -> impl Iterator<Item = &Vma> {
        self.vmas.values()
    }

    /// Translate virtual address to physical address
    pub fn translate(&self, virt: VirtAddr) -> Option<PhysAddr> {
        let mapper = PageMapper::new(self.page_table_root);
        mapper.translate(virt)
    }

    /// Map a range of addresses with given protection
    pub fn map_range(
        &mut self,
        start: VirtAddr,
        size: u64,
        protection: Protection,
    ) -> Result<(), VmError> {
        let aligned_size = (size + super::PAGE_SIZE - 1) & !(super::PAGE_SIZE - 1);

        // Map using anonymous backing (pages allocated on demand)
        self.map(start, aligned_size, protection, VmaBacking::Anonymous)?;

        // Pre-allocate and map pages
        let mut addr = start.as_u64();
        let end = start.as_u64() + aligned_size;

        while addr < end {
            let frame = super::alloc_frame().ok_or(VmError::OutOfMemory)?;
            self.map_page(VirtAddr::new(addr), frame, protection)?;
            addr += super::PAGE_SIZE;
        }

        Ok(())
    }

    // =========================================================================
    // Private helper methods for page fault handling
    // =========================================================================

    /// Read a page from a file into a physical frame
    fn read_file_page(
        &self,
        file_id: crate::cap::ObjectId,
        offset: u64,
        frame: PhysAddr,
    ) -> Result<(), VmError> {
        // Get kernel virtual address for the frame
        let virt_ptr = super::phys_to_virt(frame) as *mut u8;
        let buffer = unsafe {
            core::slice::from_raw_parts_mut(virt_ptr, super::PAGE_SIZE as usize)
        };

        // Read from the file system
        match crate::fs::read_at(file_id, offset, buffer) {
            Ok(bytes_read) => {
                // Zero out any remaining bytes if we read less than a page
                if bytes_read < super::PAGE_SIZE as usize {
                    unsafe {
                        core::ptr::write_bytes(
                            virt_ptr.add(bytes_read),
                            0,
                            super::PAGE_SIZE as usize - bytes_read,
                        );
                    }
                }
                Ok(())
            }
            Err(_) => Err(VmError::IoError),
        }
    }

    /// Look up the physical frame for a shared memory region
    fn lookup_shared_frame(
        &self,
        region_id: crate::cap::ObjectId,
        offset: u64,
    ) -> Option<PhysAddr> {
        // Look up shared region in the IPC shared memory registry
        crate::ipc::shm::get_frame(region_id, offset)
    }

    /// Look up the physical frame for a tensor buffer
    fn lookup_tensor_frame(
        &self,
        tensor_id: crate::cap::ObjectId,
        offset: u64,
    ) -> Option<PhysAddr> {
        // Look up tensor in the tensor registry
        // For CPU tensors, we can return the physical address directly
        // For GPU/NPU tensors, we need to trigger a migration first
        crate::tensor::get_tensor_frame(tensor_id, offset)
    }
}

impl Default for AddressSpace {
    fn default() -> Self {
        Self::new()
    }
}

/// Virtual memory subsystem interface
pub struct VirtualMemory;

/// VM errors
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VmError {
    /// Region overlaps with existing mapping
    Overlap,
    /// Address not mapped
    NotMapped,
    /// Permission denied
    PermissionDenied,
    /// Out of memory
    OutOfMemory,
    /// Not implemented
    NotImplemented,
    /// Device memory not accessible
    DeviceMemory,
    /// I/O error
    IoError,
}
