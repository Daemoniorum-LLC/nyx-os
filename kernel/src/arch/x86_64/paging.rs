//! Page table management for x86_64
//!
//! Implements 4-level paging (PML4 -> PDPT -> PD -> PT) with support for:
//! - 4KB pages (standard)
//! - 2MB huge pages (for performance)
//! - 1GB huge pages (where supported)

use core::arch::asm;
use bitflags::bitflags;

use crate::mem::{PhysAddr, VirtAddr, PAGE_SIZE};

/// Page table levels
pub const PAGE_TABLE_LEVELS: usize = 4;

/// Entries per page table
pub const ENTRIES_PER_TABLE: usize = 512;

/// Virtual address mask for each level
const LEVEL_MASK: u64 = 0x1FF; // 9 bits

/// Kernel page table root (set during boot)
static mut KERNEL_PAGE_TABLE_ROOT: u64 = 0;

/// Initialize paging (kernel page tables)
pub fn init() {
    // Read current CR3 (page table root set by bootloader)
    let cr3: u64;
    unsafe {
        asm!("mov {}, cr3", out(reg) cr3, options(nostack, preserves_flags));
        KERNEL_PAGE_TABLE_ROOT = cr3 & 0x000F_FFFF_FFFF_F000;
    }

    log::trace!("Current CR3: {:#x}", cr3);

    // Enable additional paging features if available
    enable_paging_features();

    log::trace!("Paging initialized");
}

/// Get the kernel page table root address
pub fn get_kernel_page_table() -> PhysAddr {
    unsafe { PhysAddr::new(KERNEL_PAGE_TABLE_ROOT) }
}

/// Enable additional paging features (NX, PCID, etc.)
fn enable_paging_features() {
    unsafe {
        // Enable NX bit (No-Execute) via EFER MSR
        let efer = rdmsr(0xC0000080);
        wrmsr(0xC0000080, efer | (1 << 11)); // NXE bit

        // Enable global pages
        let mut cr4: u64;
        asm!("mov {}, cr4", out(reg) cr4, options(nostack, preserves_flags));
        cr4 |= 1 << 7; // PGE (Page Global Enable)

        // Enable PCID if supported (for TLB efficiency)
        if has_pcid() {
            cr4 |= 1 << 17; // PCIDE
        }

        // Enable SMEP if supported (Supervisor Mode Execution Prevention)
        if has_smep() {
            cr4 |= 1 << 20;
        }

        // Enable SMAP if supported (Supervisor Mode Access Prevention)
        if has_smap() {
            cr4 |= 1 << 21;
        }

        asm!("mov cr4, {}", in(reg) cr4, options(nostack, preserves_flags));
    }
}

/// Check if PCID is supported
fn has_pcid() -> bool {
    let (_, _, ecx, _) = cpuid(1);
    (ecx & (1 << 17)) != 0
}

/// Check if SMEP is supported
fn has_smep() -> bool {
    let (_, ebx, _, _) = cpuid_extended(7, 0);
    (ebx & (1 << 7)) != 0
}

/// Check if SMAP is supported
fn has_smap() -> bool {
    let (_, ebx, _, _) = cpuid_extended(7, 0);
    (ebx & (1 << 20)) != 0
}

/// Read MSR
unsafe fn rdmsr(msr: u32) -> u64 {
    let (low, high): (u32, u32);
    asm!(
        "rdmsr",
        in("ecx") msr,
        out("eax") low,
        out("edx") high,
        options(nostack, preserves_flags)
    );
    ((high as u64) << 32) | (low as u64)
}

/// Write MSR
unsafe fn wrmsr(msr: u32, value: u64) {
    let low = value as u32;
    let high = (value >> 32) as u32;
    asm!(
        "wrmsr",
        in("ecx") msr,
        in("eax") low,
        in("edx") high,
        options(nostack, preserves_flags)
    );
}

/// CPUID instruction
fn cpuid(leaf: u32) -> (u32, u32, u32, u32) {
    let (eax, ebx, ecx, edx): (u32, u32, u32, u32);
    unsafe {
        asm!(
            "cpuid",
            inout("eax") leaf => eax,
            out("ebx") ebx,
            out("ecx") ecx,
            out("edx") edx,
            options(nostack, preserves_flags)
        );
    }
    (eax, ebx, ecx, edx)
}

/// CPUID with subleaf
fn cpuid_extended(leaf: u32, subleaf: u32) -> (u32, u32, u32, u32) {
    let (eax, ebx, ecx, edx): (u32, u32, u32, u32);
    unsafe {
        asm!(
            "cpuid",
            inout("eax") leaf => eax,
            out("ebx") ebx,
            inout("ecx") subleaf => ecx,
            out("edx") edx,
            options(nostack, preserves_flags)
        );
    }
    (eax, ebx, ecx, edx)
}

bitflags! {
    /// Page table entry flags
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct PageFlags: u64 {
        /// Page is present in memory
        const PRESENT = 1 << 0;
        /// Page is writable
        const WRITABLE = 1 << 1;
        /// Page is accessible from user mode
        const USER = 1 << 2;
        /// Write-through caching
        const WRITE_THROUGH = 1 << 3;
        /// Disable caching
        const NO_CACHE = 1 << 4;
        /// Page has been accessed
        const ACCESSED = 1 << 5;
        /// Page has been written to (dirty)
        const DIRTY = 1 << 6;
        /// Huge page (2MB or 1GB)
        const HUGE_PAGE = 1 << 7;
        /// Global page (not flushed on CR3 switch)
        const GLOBAL = 1 << 8;
        /// No execute (requires NX bit enabled)
        const NO_EXECUTE = 1 << 63;
    }
}

/// Page table entry
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct PageTableEntry(u64);

impl PageTableEntry {
    /// Create an empty (not present) entry
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Create a new entry pointing to a frame
    pub fn new(addr: PhysAddr, flags: PageFlags) -> Self {
        Self((addr.as_u64() & 0x000F_FFFF_FFFF_F000) | flags.bits())
    }

    /// Create a huge page entry (2MB)
    pub fn huge_page(addr: PhysAddr, flags: PageFlags) -> Self {
        Self((addr.as_u64() & 0x000F_FFFF_FFE0_0000) | flags.bits() | PageFlags::HUGE_PAGE.bits())
    }

    /// Check if entry is present
    pub fn is_present(&self) -> bool {
        self.0 & PageFlags::PRESENT.bits() != 0
    }

    /// Check if entry is a huge page
    pub fn is_huge(&self) -> bool {
        self.0 & PageFlags::HUGE_PAGE.bits() != 0
    }

    /// Get the physical address
    pub fn addr(&self) -> PhysAddr {
        PhysAddr::new(self.0 & 0x000F_FFFF_FFFF_F000)
    }

    /// Get flags
    pub fn flags(&self) -> PageFlags {
        PageFlags::from_bits_truncate(self.0)
    }

    /// Set flags
    pub fn set_flags(&mut self, flags: PageFlags) {
        self.0 = (self.0 & 0x000F_FFFF_FFFF_F000) | flags.bits();
    }

    /// Get raw value
    pub fn raw(&self) -> u64 {
        self.0
    }
}

/// Page table (512 entries, 4KB aligned)
#[repr(C, align(4096))]
pub struct PageTable {
    entries: [PageTableEntry; ENTRIES_PER_TABLE],
}

impl PageTable {
    /// Create an empty page table
    pub const fn new() -> Self {
        Self {
            entries: [PageTableEntry::empty(); ENTRIES_PER_TABLE],
        }
    }

    /// Get entry at index
    pub fn entry(&self, index: usize) -> &PageTableEntry {
        &self.entries[index]
    }

    /// Get mutable entry at index
    pub fn entry_mut(&mut self, index: usize) -> &mut PageTableEntry {
        &mut self.entries[index]
    }

    /// Iterate over entries
    pub fn iter(&self) -> impl Iterator<Item = &PageTableEntry> {
        self.entries.iter()
    }

    /// Zero all entries
    pub fn zero(&mut self) {
        for entry in &mut self.entries {
            *entry = PageTableEntry::empty();
        }
    }
}

impl Default for PageTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Page table walker for address translation
pub struct PageTableWalker {
    root: PhysAddr,
}

impl PageTableWalker {
    /// Create a new walker with the given root table
    pub fn new(root: PhysAddr) -> Self {
        Self { root }
    }

    /// Create a walker for the current address space
    pub fn current() -> Self {
        let cr3: u64;
        unsafe {
            asm!("mov {}, cr3", out(reg) cr3, options(nostack, preserves_flags));
        }
        Self {
            root: PhysAddr::new(cr3 & 0x000F_FFFF_FFFF_F000),
        }
    }

    /// Translate virtual address to physical
    pub fn translate(&self, virt: VirtAddr) -> Option<PhysAddr> {
        let indices = Self::indices(virt);

        let mut table_addr = self.root;

        // Walk PML4 -> PDPT -> PD
        for level in 0..3 {
            let table = unsafe { &*(table_addr.as_u64() as *const PageTable) };
            let entry = table.entry(indices[level]);

            if !entry.is_present() {
                return None;
            }

            // Check for huge page
            if level >= 1 && entry.is_huge() {
                // 1GB page at PDPT level, 2MB at PD level
                let page_size = if level == 1 { 1 << 30 } else { 1 << 21 };
                let offset_mask = page_size - 1;
                let base = entry.addr().as_u64() & !offset_mask;
                return Some(PhysAddr::new(base | (virt.as_u64() & offset_mask)));
            }

            table_addr = entry.addr();
        }

        // Walk PT level
        let table = unsafe { &*(table_addr.as_u64() as *const PageTable) };
        let entry = table.entry(indices[3]);

        if !entry.is_present() {
            return None;
        }

        let offset = virt.as_u64() & 0xFFF;
        Some(PhysAddr::new(entry.addr().as_u64() | offset))
    }

    /// Get page table indices for a virtual address
    pub fn indices(virt: VirtAddr) -> [usize; 4] {
        let addr = virt.as_u64();
        [
            ((addr >> 39) & LEVEL_MASK) as usize, // PML4
            ((addr >> 30) & LEVEL_MASK) as usize, // PDPT
            ((addr >> 21) & LEVEL_MASK) as usize, // PD
            ((addr >> 12) & LEVEL_MASK) as usize, // PT
        ]
    }
}

/// Page mapper for creating mappings
pub struct PageMapper {
    root: PhysAddr,
}

impl PageMapper {
    /// Create a new mapper with given root
    pub fn new(root: PhysAddr) -> Self {
        Self { root }
    }

    /// Map a 4KB page
    pub fn map_page(
        &mut self,
        virt: VirtAddr,
        phys: PhysAddr,
        flags: PageFlags,
        allocator: &mut impl FnMut() -> Option<PhysAddr>,
    ) -> Result<(), MapError> {
        let indices = PageTableWalker::indices(virt);
        let mut table_addr = self.root;

        // Walk/create PML4 -> PDPT -> PD
        for level in 0..3 {
            let table = unsafe { &mut *(table_addr.as_u64() as *mut PageTable) };
            let entry = table.entry_mut(indices[level]);

            if !entry.is_present() {
                // Allocate new table
                let new_table = allocator().ok_or(MapError::OutOfMemory)?;
                // Zero the new table
                unsafe {
                    let table_ptr = new_table.as_u64() as *mut PageTable;
                    (*table_ptr).zero();
                }
                *entry = PageTableEntry::new(
                    new_table,
                    PageFlags::PRESENT | PageFlags::WRITABLE | PageFlags::USER,
                );
            } else if entry.is_huge() {
                return Err(MapError::HugePageConflict);
            }

            table_addr = entry.addr();
        }

        // Map in PT
        let table = unsafe { &mut *(table_addr.as_u64() as *mut PageTable) };
        let entry = table.entry_mut(indices[3]);

        if entry.is_present() {
            return Err(MapError::AlreadyMapped);
        }

        *entry = PageTableEntry::new(phys, flags | PageFlags::PRESENT);
        Ok(())
    }

    /// Map a 2MB huge page
    pub fn map_huge_page(
        &mut self,
        virt: VirtAddr,
        phys: PhysAddr,
        flags: PageFlags,
        allocator: &mut impl FnMut() -> Option<PhysAddr>,
    ) -> Result<(), MapError> {
        // Verify alignment
        if virt.as_u64() & 0x1F_FFFF != 0 {
            return Err(MapError::MisalignedAddress);
        }
        if phys.as_u64() & 0x1F_FFFF != 0 {
            return Err(MapError::MisalignedAddress);
        }

        let indices = PageTableWalker::indices(virt);
        let mut table_addr = self.root;

        // Walk/create PML4 -> PDPT
        for level in 0..2 {
            let table = unsafe { &mut *(table_addr.as_u64() as *mut PageTable) };
            let entry = table.entry_mut(indices[level]);

            if !entry.is_present() {
                let new_table = allocator().ok_or(MapError::OutOfMemory)?;
                unsafe {
                    let table_ptr = new_table.as_u64() as *mut PageTable;
                    (*table_ptr).zero();
                }
                *entry = PageTableEntry::new(
                    new_table,
                    PageFlags::PRESENT | PageFlags::WRITABLE | PageFlags::USER,
                );
            }

            table_addr = entry.addr();
        }

        // Map huge page in PD
        let table = unsafe { &mut *(table_addr.as_u64() as *mut PageTable) };
        let entry = table.entry_mut(indices[2]);

        if entry.is_present() {
            return Err(MapError::AlreadyMapped);
        }

        *entry = PageTableEntry::huge_page(phys, flags | PageFlags::PRESENT);
        Ok(())
    }

    /// Unmap a page
    pub fn unmap_page(&mut self, virt: VirtAddr) -> Result<PhysAddr, MapError> {
        let indices = PageTableWalker::indices(virt);
        let mut table_addr = self.root;

        // Walk to PT
        for level in 0..3 {
            let table = unsafe { &*(table_addr.as_u64() as *const PageTable) };
            let entry = table.entry(indices[level]);

            if !entry.is_present() {
                return Err(MapError::NotMapped);
            }

            if entry.is_huge() {
                // Handle huge page unmapping
                let table = unsafe { &mut *(table_addr.as_u64() as *mut PageTable) };
                let entry = table.entry_mut(indices[level]);
                let phys = entry.addr();
                *entry = PageTableEntry::empty();
                flush_tlb_page(virt);
                return Ok(phys);
            }

            table_addr = entry.addr();
        }

        // Unmap from PT
        let table = unsafe { &mut *(table_addr.as_u64() as *mut PageTable) };
        let entry = table.entry_mut(indices[3]);

        if !entry.is_present() {
            return Err(MapError::NotMapped);
        }

        let phys = entry.addr();
        *entry = PageTableEntry::empty();
        flush_tlb_page(virt);
        Ok(phys)
    }

    /// Translate virtual address to physical address
    pub fn translate(&self, virt: VirtAddr) -> Option<PhysAddr> {
        let walker = PageTableWalker::new(self.root);
        walker.translate(virt)
    }
}

/// Mapping errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapError {
    /// Page is already mapped
    AlreadyMapped,
    /// Page is not mapped
    NotMapped,
    /// Out of memory for page tables
    OutOfMemory,
    /// Address is misaligned
    MisalignedAddress,
    /// Huge page conflicts with existing mapping
    HugePageConflict,
}

/// Flush TLB for a single page
pub fn flush_tlb_page(virt: VirtAddr) {
    unsafe {
        asm!("invlpg [{}]", in(reg) virt.as_u64(), options(nostack, preserves_flags));
    }
}

/// Flush entire TLB
pub fn flush_tlb_all() {
    unsafe {
        let cr3: u64;
        asm!("mov {}, cr3", out(reg) cr3, options(nostack, preserves_flags));
        asm!("mov cr3, {}", in(reg) cr3, options(nostack, preserves_flags));
    }
}

/// Switch to a new address space
pub fn switch_address_space(root: PhysAddr) {
    unsafe {
        asm!("mov cr3, {}", in(reg) root.as_u64(), options(nostack, preserves_flags));
    }
}

/// Get current CR3 value
pub fn current_cr3() -> PhysAddr {
    let cr3: u64;
    unsafe {
        asm!("mov {}, cr3", out(reg) cr3, options(nostack, preserves_flags));
    }
    PhysAddr::new(cr3 & 0x000F_FFFF_FFFF_F000)
}

/// Kernel virtual address base (higher half)
pub const KERNEL_BASE: u64 = 0xFFFF_8000_0000_0000;

/// Physical memory map base (direct mapping)
pub const PHYS_MAP_BASE: u64 = 0xFFFF_8800_0000_0000;

/// Convert physical address to virtual (assuming direct mapping)
pub fn phys_to_virt(phys: PhysAddr) -> VirtAddr {
    VirtAddr::new(phys.as_u64() + PHYS_MAP_BASE)
}

/// Convert virtual address to physical (assuming direct mapping)
pub fn virt_to_phys(virt: VirtAddr) -> PhysAddr {
    PhysAddr::new(virt.as_u64() - PHYS_MAP_BASE)
}
