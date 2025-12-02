//! Memory management
//!
//! Functions for mapping, unmapping, and managing virtual memory.

use crate::syscall::{self, nr, Error};

/// Page size (4KB on x86_64)
pub const PAGE_SIZE: u64 = 4096;

/// Memory protection flags
pub mod prot {
    /// No access
    pub const NONE: u32 = 0;
    /// Read permission
    pub const READ: u32 = 1 << 0;
    /// Write permission
    pub const WRITE: u32 = 1 << 1;
    /// Execute permission
    pub const EXEC: u32 = 1 << 2;
    /// Userspace accessible (always set for user mappings)
    pub const USER: u32 = 1 << 3;

    /// Read + Write
    pub const RW: u32 = READ | WRITE;
    /// Read + Execute
    pub const RX: u32 = READ | EXEC;
    /// Read + Write + Execute
    pub const RWX: u32 = READ | WRITE | EXEC;
}

/// Memory mapping flags
pub mod flags {
    /// Anonymous mapping (not backed by file)
    pub const ANONYMOUS: u32 = 1 << 0;
    /// Private mapping (copy-on-write)
    pub const PRIVATE: u32 = 1 << 1;
    /// Shared mapping
    pub const SHARED: u32 = 1 << 2;
    /// Fixed address (fail if can't map at hint)
    pub const FIXED: u32 = 1 << 3;
}

/// Map memory into the address space
///
/// # Arguments
/// * `addr_hint` - Suggested address (0 = kernel chooses)
/// * `length` - Size in bytes (will be rounded up to page size)
/// * `protection` - Protection flags (prot::*)
/// * `flags` - Mapping flags (flags::*)
///
/// # Returns
/// The actual mapped address
///
/// # Example
/// ```no_run
/// // Map 4KB of anonymous read-write memory
/// let addr = mmap(0, 4096, prot::RW, flags::ANONYMOUS)?;
///
/// // Map at specific address
/// let addr = mmap(0x1000_0000, 8192, prot::RX, flags::ANONYMOUS | flags::FIXED)?;
/// ```
pub fn mmap(addr_hint: u64, length: u64, protection: u32, map_flags: u32) -> Result<u64, Error> {
    let result = unsafe {
        syscall::syscall4(nr::MEM_MAP, addr_hint, length, protection as u64, map_flags as u64)
    };
    Error::from_raw(result)
}

/// Unmap memory from the address space
///
/// # Arguments
/// * `addr` - Start address (must be page-aligned)
/// * `length` - Size in bytes
///
/// # Example
/// ```no_run
/// let addr = mmap(0, 4096, prot::RW, flags::ANONYMOUS)?;
/// // ... use memory ...
/// munmap(addr, 4096)?;
/// ```
pub fn munmap(addr: u64, length: u64) -> Result<(), Error> {
    let result = unsafe { syscall::syscall2(nr::MEM_UNMAP, addr, length) };
    Error::from_raw(result).map(|_| ())
}

/// Change memory protection
///
/// # Arguments
/// * `addr` - Start address (must be page-aligned)
/// * `length` - Size in bytes
/// * `protection` - New protection flags
///
/// # Example
/// ```no_run
/// // Make memory read-only
/// mprotect(addr, 4096, prot::READ)?;
///
/// // Make memory executable
/// mprotect(code_addr, code_len, prot::RX)?;
/// ```
pub fn mprotect(addr: u64, length: u64, protection: u32) -> Result<(), Error> {
    let result = unsafe { syscall::syscall3(nr::MEM_PROTECT, addr, length, protection as u64) };
    Error::from_raw(result).map(|_| ())
}

/// Allocate physical memory
///
/// This is a low-level function that allocates contiguous physical frames.
/// Most applications should use `mmap` instead.
///
/// # Arguments
/// * `size` - Size in bytes
/// * `flags` - Allocation flags (reserved, pass 0)
///
/// # Returns
/// Physical address of allocated memory
///
/// # Safety
/// This function allocates physical memory which must be mapped before use.
pub unsafe fn alloc_phys(size: u64, flags: u32) -> Result<u64, Error> {
    let result = syscall::syscall2(nr::MEM_ALLOC, size, flags as u64);
    Error::from_raw(result)
}

/// Free physical memory
///
/// # Arguments
/// * `addr` - Physical address returned by `alloc_phys`
/// * `size` - Size that was allocated
///
/// # Safety
/// The caller must ensure the memory is no longer mapped or in use.
pub unsafe fn free_phys(addr: u64, size: u64) -> Result<(), Error> {
    let result = syscall::syscall2(nr::MEM_FREE, addr, size);
    Error::from_raw(result).map(|_| ())
}

// ============================================================================
// Convenience functions
// ============================================================================

/// Allocate anonymous read-write memory
///
/// This is a convenience wrapper around `mmap`.
///
/// # Arguments
/// * `size` - Size in bytes
///
/// # Example
/// ```no_run
/// let ptr = alloc(4096)? as *mut u8;
/// unsafe { *ptr = 42; }
/// ```
pub fn alloc(size: u64) -> Result<u64, Error> {
    mmap(0, size, prot::RW | prot::USER, flags::ANONYMOUS)
}

/// Free memory allocated with `alloc`
pub fn free(addr: u64, size: u64) -> Result<(), Error> {
    munmap(addr, size)
}

/// Allocate a page of memory
pub fn alloc_page() -> Result<u64, Error> {
    alloc(PAGE_SIZE)
}

/// Allocate multiple pages of memory
pub fn alloc_pages(count: usize) -> Result<u64, Error> {
    alloc(count as u64 * PAGE_SIZE)
}
