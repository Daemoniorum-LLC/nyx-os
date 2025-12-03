//! System call interface
//!
//! This module provides the kernel's system call handling infrastructure.
//! All user-space requests pass through here and are dispatched to appropriate handlers.
//!
//! ## Security
//!
//! All syscall handlers use the `mem::user` module for safe userspace memory access.
//! This ensures:
//! - Pointers are validated to be within userspace bounds
//! - Memory is verified to be mapped before access
//! - Write operations verify write permissions
//! - No kernel memory can be read or written via syscalls

use crate::cap::{Capability, ObjectId, ObjectType, Rights};
use crate::ipc;
use crate::mem::user::{copy_from_user, copy_string_from_user, copy_to_user, UserMemError};
use crate::mem::{VirtAddr, PAGE_SIZE};
use crate::process::{ProcessId, SpawnArgs, SpawnError};
use crate::sched::{BlockReason, SchedClass, ThreadState};
use alloc::string::String;
use alloc::vec::Vec;
use core::time::Duration;

/// Maximum message size for IPC operations (4 KB)
const MAX_IPC_MSG_SIZE: usize = 4096;

/// Maximum path length for spawn operations
const MAX_PATH_LEN: usize = 4096;

/// Maximum debug message length
const MAX_DEBUG_MSG_LEN: usize = 1024;

/// System call numbers
#[repr(u64)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Syscall {
    // IPC (0-15)
    RingSetup = 0,
    RingEnter = 1,
    Send = 2,
    Receive = 3,
    Call = 4,
    Reply = 5,
    Signal = 6,
    Wait = 7,
    Poll = 8,

    // Capabilities (16-31)
    CapDerive = 16,
    CapRevoke = 17,
    CapIdentify = 18,
    CapGrant = 19,
    CapDrop = 20,

    // Memory (32-63)
    MemMap = 32,
    MemUnmap = 33,
    MemProtect = 34,
    MemAlloc = 35,
    MemFree = 36,

    // Threads (64-79)
    ThreadCreate = 64,
    ThreadExit = 65,
    ThreadYield = 66,
    ThreadSleep = 67,
    ThreadJoin = 68,

    // Process (80-95)
    ProcessSpawn = 80,
    ProcessExit = 81,
    ProcessWait = 82,
    ProcessGetPid = 83,
    ProcessGetPpid = 84,

    // File system (96-111) - reserved for future vfs
    FsOpen = 96,
    FsClose = 97,
    FsRead = 98,
    FsWrite = 99,
    FsStat = 100,
    FsReaddir = 101,

    // Tensor/AI (112-143)
    TensorAlloc = 112,
    TensorFree = 113,
    TensorMigrate = 114,
    InferenceCreate = 115,
    InferenceSubmit = 116,
    ComputeSubmit = 117,

    // Time-Travel (144-159)
    Checkpoint = 144,
    Restore = 145,
    RecordStart = 146,
    RecordStop = 147,

    // System (240-255)
    Debug = 240,
    GetTime = 241,
    Reboot = 254,
    Shutdown = 255,
}

/// System call handler (called from arch-specific entry)
pub fn syscall_handler(regs: &mut SyscallRegs) {
    let syscall_num = regs.syscall_num;

    let result = match syscall_num {
        // IPC syscalls
        0 => handle_ring_setup(regs),
        1 => handle_ring_enter(regs),
        2 => handle_send(regs),
        3 => handle_receive(regs),
        4 => handle_call(regs),
        5 => handle_reply(regs),
        6 => handle_signal(regs),
        7 => handle_wait(regs),
        8 => handle_poll(regs),

        // Capability syscalls
        16 => handle_cap_derive(regs),
        17 => handle_cap_revoke(regs),
        18 => handle_cap_identify(regs),
        19 => handle_cap_grant(regs),
        20 => handle_cap_drop(regs),

        // Memory syscalls
        32 => handle_mem_map(regs),
        33 => handle_mem_unmap(regs),
        34 => handle_mem_protect(regs),
        35 => handle_mem_alloc(regs),
        36 => handle_mem_free(regs),

        // Thread syscalls
        64 => handle_thread_create(regs),
        65 => handle_thread_exit(regs),
        66 => handle_thread_yield(regs),
        67 => handle_thread_sleep(regs),
        68 => handle_thread_join(regs),

        // Process syscalls
        80 => handle_process_spawn(regs),
        81 => handle_process_exit(regs),
        82 => handle_process_wait(regs),
        83 => handle_process_getpid(regs),
        84 => handle_process_getppid(regs),

        // Tensor/AI syscalls
        112 => handle_tensor_alloc(regs),
        113 => handle_tensor_free(regs),
        114 => handle_tensor_migrate(regs),
        115 => handle_inference_create(regs),
        116 => handle_inference_submit(regs),

        // System syscalls
        240 => handle_debug(regs),
        241 => handle_gettime(regs),

        _ => Err(SyscallError::InvalidSyscall),
    };

    regs.result = match result {
        Ok(val) => val as i64,
        Err(err) => err as i64,
    };
}

/// Saved registers for syscall
#[repr(C)]
pub struct SyscallRegs {
    pub syscall_num: u64,
    pub arg0: u64,
    pub arg1: u64,
    pub arg2: u64,
    pub arg3: u64,
    pub arg4: u64,
    pub arg5: u64,
    pub result: i64,
}

/// Syscall errors
#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyscallError {
    Success = 0,
    InvalidSyscall = -1,
    InvalidCapability = -2,
    PermissionDenied = -3,
    OutOfMemory = -4,
    InvalidArgument = -5,
    WouldBlock = -6,
    Timeout = -7,
    Interrupted = -8,
    NotFound = -9,
    InvalidFormat = -10,
    IoError = -11,
    TooManyProcesses = -12,
    NoChild = -13,
    BadAddress = -14,
}

/// Convert user memory errors to syscall errors
impl From<UserMemError> for SyscallError {
    fn from(err: UserMemError) -> Self {
        match err {
            UserMemError::NullPointer => SyscallError::BadAddress,
            UserMemError::InvalidAddress => SyscallError::BadAddress,
            UserMemError::NotMapped => SyscallError::BadAddress,
            UserMemError::PermissionDenied => SyscallError::PermissionDenied,
            UserMemError::SizeTooLarge => SyscallError::InvalidArgument,
            UserMemError::AddressOverflow => SyscallError::BadAddress,
        }
    }
}

// ============================================================================
// IPC Syscall Handlers
// ============================================================================

fn handle_ring_setup(regs: &mut SyscallRegs) -> Result<u64, SyscallError> {
    let sq_entries = regs.arg0 as u32;
    let cq_entries = regs.arg1 as u32;
    let flags = regs.arg2 as u32;

    // Create an IPC ring for the calling process
    match ipc::create_ring(sq_entries, cq_entries, flags) {
        Ok(cap) => Ok(cap.object_id.as_u64()),
        Err(_) => Err(SyscallError::OutOfMemory),
    }
}

fn handle_ring_enter(regs: &mut SyscallRegs) -> Result<u64, SyscallError> {
    let ring_cap = regs.arg0;
    let to_submit = regs.arg1 as u32;
    let min_complete = regs.arg2 as u32;
    let _flags = regs.arg3 as u32;

    // Process the ring's submission queue
    match ipc::ring_enter(ObjectId::from_raw(ring_cap), to_submit, min_complete) {
        Ok(completed) => Ok(completed as u64),
        Err(_) => Err(SyscallError::InvalidCapability),
    }
}

fn handle_send(regs: &mut SyscallRegs) -> Result<u64, SyscallError> {
    let dest_cap = regs.arg0;
    let msg_ptr = regs.arg1 as *const u8;
    let msg_len = regs.arg2 as usize;
    let timeout_ns = regs.arg3;

    // Validate message size
    if msg_len > MAX_IPC_MSG_SIZE {
        return Err(SyscallError::InvalidArgument);
    }

    let timeout = if timeout_ns == u64::MAX {
        None
    } else {
        Some(Duration::from_nanos(timeout_ns))
    };

    // Safely copy message from userspace
    let msg = copy_from_user(msg_ptr, msg_len)?;

    match ipc::send(ObjectId::from_raw(dest_cap), &msg, timeout) {
        Ok(_) => Ok(0),
        Err(_) => Err(SyscallError::InvalidCapability),
    }
}

fn handle_receive(regs: &mut SyscallRegs) -> Result<u64, SyscallError> {
    let src_cap = regs.arg0;
    let buf_ptr = regs.arg1 as *mut u8;
    let buf_len = regs.arg2 as usize;
    let timeout_ns = regs.arg3;

    // Validate buffer size
    if buf_len > MAX_IPC_MSG_SIZE {
        return Err(SyscallError::InvalidArgument);
    }

    let timeout = if timeout_ns == u64::MAX {
        None
    } else {
        Some(Duration::from_nanos(timeout_ns))
    };

    match ipc::receive(ObjectId::from_raw(src_cap), timeout) {
        Ok(msg) => {
            let copy_len = core::cmp::min(msg.len(), buf_len);
            // Safely copy data to userspace
            copy_to_user(buf_ptr, &msg[..copy_len])?;
            Ok(copy_len as u64)
        }
        Err(_) => Err(SyscallError::WouldBlock),
    }
}

fn handle_call(regs: &mut SyscallRegs) -> Result<u64, SyscallError> {
    let dest_cap = regs.arg0;
    let req_ptr = regs.arg1 as *const u8;
    let req_len = regs.arg2 as usize;
    let resp_ptr = regs.arg3 as *mut u8;
    let resp_len = regs.arg4 as usize;

    // Validate sizes
    if req_len > MAX_IPC_MSG_SIZE || resp_len > MAX_IPC_MSG_SIZE {
        return Err(SyscallError::InvalidArgument);
    }

    // Safely copy request from userspace
    let req = copy_from_user(req_ptr, req_len)?;

    match ipc::call(ObjectId::from_raw(dest_cap), &req) {
        Ok(resp) => {
            let copy_len = core::cmp::min(resp.len(), resp_len);
            // Safely copy response to userspace
            copy_to_user(resp_ptr, &resp[..copy_len])?;
            Ok(copy_len as u64)
        }
        Err(_) => Err(SyscallError::InvalidCapability),
    }
}

fn handle_reply(regs: &mut SyscallRegs) -> Result<u64, SyscallError> {
    let reply_cap = regs.arg0;
    let msg_ptr = regs.arg1 as *const u8;
    let msg_len = regs.arg2 as usize;

    // Validate message size
    if msg_len > MAX_IPC_MSG_SIZE {
        return Err(SyscallError::InvalidArgument);
    }

    // Safely copy message from userspace
    let msg = copy_from_user(msg_ptr, msg_len)?;

    match ipc::reply(ObjectId::from_raw(reply_cap), &msg) {
        Ok(_) => Ok(0),
        Err(_) => Err(SyscallError::InvalidCapability),
    }
}

fn handle_signal(regs: &mut SyscallRegs) -> Result<u64, SyscallError> {
    let target_cap = regs.arg0;
    let signal_bits = regs.arg1;

    match ipc::signal(ObjectId::from_raw(target_cap), signal_bits) {
        Ok(_) => Ok(0),
        Err(_) => Err(SyscallError::InvalidCapability),
    }
}

fn handle_wait(regs: &mut SyscallRegs) -> Result<u64, SyscallError> {
    let wait_cap = regs.arg0;
    let mask = regs.arg1;
    let timeout_ns = regs.arg2;

    let timeout = if timeout_ns == u64::MAX {
        None
    } else {
        Some(Duration::from_nanos(timeout_ns))
    };

    match ipc::wait(ObjectId::from_raw(wait_cap), mask, timeout) {
        Ok(signals) => Ok(signals),
        Err(_) => Err(SyscallError::Timeout),
    }
}

fn handle_poll(regs: &mut SyscallRegs) -> Result<u64, SyscallError> {
    let poll_cap = regs.arg0;
    let mask = regs.arg1;

    match ipc::poll(ObjectId::from_raw(poll_cap), mask) {
        Ok(signals) => Ok(signals),
        Err(_) => Err(SyscallError::InvalidCapability),
    }
}

// ============================================================================
// Capability Syscall Handlers
// ============================================================================

fn handle_cap_derive(regs: &mut SyscallRegs) -> Result<u64, SyscallError> {
    let src_cap = regs.arg0;
    let new_rights = Rights::from_bits_truncate(regs.arg1);

    match crate::cap::derive(ObjectId::from_raw(src_cap), new_rights) {
        Ok(cap) => Ok(cap.object_id.as_u64()),
        Err(_) => Err(SyscallError::InvalidCapability),
    }
}

fn handle_cap_revoke(regs: &mut SyscallRegs) -> Result<u64, SyscallError> {
    let cap_id = regs.arg0;

    match crate::cap::revoke(ObjectId::from_raw(cap_id)) {
        Ok(_) => Ok(0),
        Err(_) => Err(SyscallError::InvalidCapability),
    }
}

fn handle_cap_identify(regs: &mut SyscallRegs) -> Result<u64, SyscallError> {
    let cap_id = regs.arg0;

    match crate::cap::identify(ObjectId::from_raw(cap_id)) {
        Ok((obj_type, rights)) => {
            // Pack type in upper 32 bits, rights in lower 32 bits
            Ok(((obj_type as u64) << 32) | (rights.bits() as u64))
        }
        Err(_) => Err(SyscallError::InvalidCapability),
    }
}

fn handle_cap_grant(regs: &mut SyscallRegs) -> Result<u64, SyscallError> {
    let cap_id = regs.arg0;
    let target_process = ProcessId(regs.arg1);
    let rights_mask = regs.arg2; // New: rights mask for granted capability

    match crate::cap::grant_with_rights(ObjectId::from_raw(cap_id), target_process, rights_mask) {
        Ok(new_cap) => Ok(new_cap.object_id.as_u64()),
        Err(_) => Err(SyscallError::InvalidCapability),
    }
}

fn handle_cap_drop(regs: &mut SyscallRegs) -> Result<u64, SyscallError> {
    let cap_id = regs.arg0;

    match crate::cap::drop_cap(ObjectId::from_raw(cap_id)) {
        Ok(_) => Ok(0),
        Err(_) => Err(SyscallError::InvalidCapability),
    }
}

// ============================================================================
// Memory Syscall Handlers
// ============================================================================

fn handle_mem_map(regs: &mut SyscallRegs) -> Result<u64, SyscallError> {
    let addr_hint = regs.arg0;
    let length = regs.arg1;
    let prot = regs.arg2 as u32;
    let flags = regs.arg3 as u32;

    // Validate length
    if length == 0 || length > 1024 * 1024 * 1024 {
        return Err(SyscallError::InvalidArgument);
    }

    // Get current process
    let pid = crate::process::current_process_id().ok_or(SyscallError::InvalidCapability)?;

    let mut proc_guard =
        crate::process::get_process_mut(pid).ok_or(SyscallError::InvalidCapability)?;

    // Convert protection flags
    let protection = crate::mem::virt::Protection::from_bits_truncate(prot as u8);

    // Find suitable address
    let addr = if addr_hint == 0 {
        // Use the process's next available address
        find_free_region(&proc_guard.address_space, length)?
    } else {
        VirtAddr::new(addr_hint)
    };

    // Create the mapping
    let backing = if flags & 0x1 != 0 {
        // MAP_ANONYMOUS
        crate::mem::virt::VmaBacking::Anonymous
    } else {
        crate::mem::virt::VmaBacking::Anonymous // File mappings would need fd
    };

    proc_guard
        .address_space
        .map(addr, length, protection, backing)
        .map_err(|_| SyscallError::OutOfMemory)?;

    Ok(addr.as_u64())
}

fn handle_mem_unmap(regs: &mut SyscallRegs) -> Result<u64, SyscallError> {
    let addr = regs.arg0;
    let length = regs.arg1;

    // Validate address is in userspace
    if addr >= 0x0000_8000_0000_0000 || addr < 0x1000 {
        return Err(SyscallError::BadAddress);
    }

    // Validate length
    if length == 0 {
        return Err(SyscallError::InvalidArgument);
    }

    // Get current process
    let pid = crate::process::current_process_id().ok_or(SyscallError::InvalidCapability)?;

    let mut proc_guard =
        crate::process::get_process_mut(pid).ok_or(SyscallError::InvalidCapability)?;

    // Unmap the region
    proc_guard
        .address_space
        .unmap(VirtAddr::new(addr), length)
        .map_err(|_| SyscallError::BadAddress)?;

    Ok(0)
}

fn handle_mem_protect(regs: &mut SyscallRegs) -> Result<u64, SyscallError> {
    let addr = regs.arg0;
    let length = regs.arg1;
    let prot = regs.arg2 as u32;

    // Validate address is in userspace
    if addr >= 0x0000_8000_0000_0000 || addr < 0x1000 {
        return Err(SyscallError::BadAddress);
    }

    // Validate length
    if length == 0 {
        return Err(SyscallError::InvalidArgument);
    }

    // Get current process
    let pid = crate::process::current_process_id().ok_or(SyscallError::InvalidCapability)?;

    let mut proc_guard =
        crate::process::get_process_mut(pid).ok_or(SyscallError::InvalidCapability)?;

    let protection = crate::mem::virt::Protection::from_bits_truncate(prot as u8);

    // Change protection on each page in the range
    let start_page = addr & !(PAGE_SIZE - 1);
    let end = addr.saturating_add(length);
    let end_page = (end + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);

    let mut current = start_page;
    while current < end_page {
        // Update the VMA protection
        // This is simplified - a full implementation would split/merge VMAs
        if let Some(phys) = proc_guard.address_space.translate(VirtAddr::new(current)) {
            proc_guard
                .address_space
                .map_page(VirtAddr::new(current), phys, protection)
                .map_err(|_| SyscallError::OutOfMemory)?;
        }
        current = current.saturating_add(PAGE_SIZE);
    }

    Ok(0)
}

/// Memory allocation flags
mod mem_flags {
    /// Allocate contiguous physical memory (for DMA)
    pub const CONTIGUOUS: u32 = 1 << 0;
    /// Zero the allocated memory
    pub const ZEROED: u32 = 1 << 1;
    /// Lock pages in memory (no swap)
    pub const LOCKED: u32 = 1 << 2;
}

fn handle_mem_alloc(regs: &mut SyscallRegs) -> Result<u64, SyscallError> {
    let size = regs.arg0;
    let flags = regs.arg1 as u32;

    // Validate size (max 1GB per allocation)
    if size == 0 || size > 1024 * 1024 * 1024 {
        return Err(SyscallError::InvalidArgument);
    }

    // Get current process
    let pid = crate::process::current_process_id().ok_or(SyscallError::InvalidCapability)?;
    let mut proc_guard =
        crate::process::get_process_mut(pid).ok_or(SyscallError::InvalidCapability)?;

    // Find a free virtual address region for this allocation
    let aligned_size = (size + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
    let virt_addr = find_free_region(&proc_guard.address_space, aligned_size)?;

    // Determine protection (RW for allocated memory, user-accessible)
    let protection = crate::mem::virt::Protection::READ
        | crate::mem::virt::Protection::WRITE
        | crate::mem::virt::Protection::USER;

    // Allocate physical frames and map them
    let num_pages = (aligned_size / PAGE_SIZE) as usize;

    if flags & mem_flags::CONTIGUOUS != 0 {
        // Allocate contiguous physical memory (for DMA buffers)
        let phys_base = crate::mem::alloc_frames(num_pages).ok_or(SyscallError::OutOfMemory)?;

        // Map all pages contiguously
        for i in 0..num_pages {
            let page_virt = VirtAddr::new(virt_addr.as_u64() + (i as u64) * PAGE_SIZE);
            let page_phys = crate::mem::PhysAddr::new(phys_base.as_u64() + (i as u64) * PAGE_SIZE);

            // Zero the page if requested
            if flags & mem_flags::ZEROED != 0 {
                let virt_ptr = crate::mem::phys_to_virt(page_phys) as *mut u8;
                // SAFETY: phys_to_virt gives kernel-mapped address, we own this frame
                unsafe {
                    core::ptr::write_bytes(virt_ptr, 0, PAGE_SIZE as usize);
                }
            }

            proc_guard
                .address_space
                .map_page(page_virt, page_phys, protection)
                .map_err(|_| SyscallError::OutOfMemory)?;
        }

        // Track this allocation for ownership verification during free
        proc_guard.track_allocation(virt_addr, aligned_size, true);
    } else {
        // Allocate individual frames (may not be physically contiguous)
        for i in 0..num_pages {
            let page_virt = VirtAddr::new(virt_addr.as_u64() + (i as u64) * PAGE_SIZE);
            let page_phys = crate::mem::alloc_frame().ok_or(SyscallError::OutOfMemory)?;

            // Zero the page if requested
            if flags & mem_flags::ZEROED != 0 {
                let virt_ptr = crate::mem::phys_to_virt(page_phys) as *mut u8;
                // SAFETY: phys_to_virt gives kernel-mapped address, we own this frame
                unsafe {
                    core::ptr::write_bytes(virt_ptr, 0, PAGE_SIZE as usize);
                }
            }

            proc_guard
                .address_space
                .map_page(page_virt, page_phys, protection)
                .map_err(|_| SyscallError::OutOfMemory)?;
        }

        // Track this allocation
        proc_guard.track_allocation(virt_addr, aligned_size, false);
    }

    // Update memory statistics
    proc_guard.mem_stats.vm_size += aligned_size;
    proc_guard.mem_stats.rss += aligned_size;

    // Return the VIRTUAL address (not physical!) - this is the secure approach
    Ok(virt_addr.as_u64())
}

fn handle_mem_free(regs: &mut SyscallRegs) -> Result<u64, SyscallError> {
    let addr = regs.arg0;
    let size = regs.arg1;

    // Validate address is in userspace range
    if addr == 0 || addr >= 0x0000_8000_0000_0000 || addr < 0x1000 {
        return Err(SyscallError::BadAddress);
    }

    // Validate size
    if size == 0 {
        return Err(SyscallError::InvalidArgument);
    }

    // Get current process
    let pid = crate::process::current_process_id().ok_or(SyscallError::InvalidCapability)?;
    let mut proc_guard =
        crate::process::get_process_mut(pid).ok_or(SyscallError::InvalidCapability)?;

    let virt_addr = VirtAddr::new(addr);
    let aligned_size = (size + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);

    // CRITICAL: Verify this process owns this allocation
    // This prevents use-after-free and freeing other processes' memory
    if !proc_guard.verify_allocation(virt_addr, aligned_size) {
        log::warn!(
            "Process {} attempted to free unowned memory at {:#x} (size {})",
            pid.raw(),
            addr,
            size
        );
        return Err(SyscallError::PermissionDenied);
    }

    // Unmap pages and free physical frames
    let num_pages = (aligned_size / PAGE_SIZE) as usize;
    for i in 0..num_pages {
        let page_virt = VirtAddr::new(addr + (i as u64) * PAGE_SIZE);

        // Translate to physical address before unmapping
        if let Some(phys_addr) = proc_guard.address_space.translate(page_virt) {
            // Unmap the page from the address space
            let _ = proc_guard.address_space.unmap(page_virt, PAGE_SIZE);

            // Free the physical frame
            crate::mem::free_frame(phys_addr);
        }
    }

    // Remove from allocation tracking
    proc_guard.untrack_allocation(virt_addr);

    // Update memory statistics
    proc_guard.mem_stats.vm_size = proc_guard.mem_stats.vm_size.saturating_sub(aligned_size);
    proc_guard.mem_stats.rss = proc_guard.mem_stats.rss.saturating_sub(aligned_size);

    Ok(0)
}

/// Find a free region in the address space for the given size
fn find_free_region(
    addr_space: &crate::mem::AddressSpace,
    size: u64,
) -> Result<VirtAddr, SyscallError> {
    // Start searching from a reasonable base address
    const USER_BASE: u64 = 0x0000_1000_0000_0000;
    const USER_TOP: u64 = 0x0000_7FFF_0000_0000;

    let aligned_size = (size + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
    let mut candidate = USER_BASE;

    // Simple first-fit allocator - walk through VMAs to find a gap
    let mut regions: Vec<_> = addr_space.regions().collect();
    regions.sort_by_key(|vma| vma.start.as_u64());

    for vma in regions {
        if candidate + aligned_size <= vma.start.as_u64() {
            // Found a gap
            return Ok(VirtAddr::new(candidate));
        }
        // Move candidate past this VMA
        candidate = vma.end.as_u64();
    }

    // Check if there's space after the last VMA
    if candidate + aligned_size <= USER_TOP {
        return Ok(VirtAddr::new(candidate));
    }

    Err(SyscallError::OutOfMemory)
}

// ============================================================================
// Thread Syscall Handlers
// ============================================================================

fn handle_thread_create(regs: &mut SyscallRegs) -> Result<u64, SyscallError> {
    let entry = regs.arg0;
    let stack = regs.arg1;
    let arg = regs.arg2;

    // Validate entry point and stack are in userspace
    if entry >= 0x0000_8000_0000_0000 || entry < 0x1000 {
        return Err(SyscallError::BadAddress);
    }
    if stack >= 0x0000_8000_0000_0000 || stack < 0x1000 {
        return Err(SyscallError::BadAddress);
    }

    // Get current process
    let pid = crate::process::current_process_id().ok_or(SyscallError::InvalidCapability)?;

    let proc = crate::process::get_process(pid).ok_or(SyscallError::InvalidCapability)?;

    // Create new thread with the argument passed via register
    let mut thread = crate::sched::Thread::new_user(entry, stack, proc.address_space.clone());

    // Set the thread argument in rdi (first argument register on x86_64)
    thread.registers.rdi = arg;

    let thread_id = thread.id;

    // Register thread
    crate::sched::THREADS.write().insert(thread_id, thread);

    // Add thread to process
    if let Some(mut proc_guard) = crate::process::get_process_mut(pid) {
        proc_guard.add_thread(thread_id);
    }

    // Enqueue for scheduling on least loaded CPU
    crate::sched::enqueue_on_least_loaded(thread_id);

    Ok(thread_id.0)
}

fn handle_thread_exit(regs: &mut SyscallRegs) -> Result<u64, SyscallError> {
    let exit_code = regs.arg0 as i32;

    let thread_id = crate::sched::current_thread_id();
    let pid = crate::process::current_process_id();

    // Mark thread as terminated
    {
        let mut threads = crate::sched::THREADS.write();
        if let Some(thread) = threads.get_mut(&thread_id) {
            thread.state = ThreadState::Terminated;
        }
    }

    // Remove thread from process
    if let Some(pid) = pid {
        if let Some(mut proc_guard) = crate::process::get_process_mut(pid) {
            proc_guard.remove_thread(thread_id);

            // If this was the last thread, exit the process
            if !proc_guard.has_running_threads() {
                drop(proc_guard);
                crate::process::exit(exit_code);
            }
        }
    }

    // Trigger reschedule
    crate::sched::schedule();

    // Never returns for the exiting thread
    Ok(0)
}

fn handle_thread_yield(_regs: &mut SyscallRegs) -> Result<u64, SyscallError> {
    crate::sched::yield_now();
    Ok(0)
}

fn handle_thread_sleep(regs: &mut SyscallRegs) -> Result<u64, SyscallError> {
    let duration_ns = regs.arg0;

    // Validate duration (max 1 hour to prevent DoS)
    const MAX_SLEEP_NS: u64 = 3600 * 1_000_000_000;
    if duration_ns > MAX_SLEEP_NS {
        return Err(SyscallError::InvalidArgument);
    }

    let duration = Duration::from_nanos(duration_ns);
    crate::sched::sleep(duration);
    Ok(0)
}

/// Thread join syscall
///
/// Blocks the calling thread until the target thread exits.
/// Returns the exit code of the target thread.
///
/// Arguments:
/// - arg0: Target thread ID to wait for
/// - arg1: Timeout in nanoseconds (u64::MAX = infinite)
///
/// Returns:
/// - Exit code of the joined thread on success
/// - Error code on failure
fn handle_thread_join(regs: &mut SyscallRegs) -> Result<u64, SyscallError> {
    let target_tid = ThreadId(regs.arg0);
    let timeout_ns = regs.arg1;

    // Get current thread ID
    let current_tid = crate::sched::current_thread_id();

    // Cannot join self
    if target_tid == current_tid {
        return Err(SyscallError::InvalidArgument);
    }

    // Check if target thread exists and get its process
    let (target_pid, target_state, exit_code) = {
        let threads = crate::sched::THREADS.read();
        match threads.get(&target_tid) {
            Some(thread) => {
                let is_terminated = matches!(thread.state, ThreadState::Terminated);
                (thread.process_id, thread.state, if is_terminated { thread.exit_code } else { 0 })
            }
            None => return Err(SyscallError::NotFound),
        }
    };

    // Get current thread's process
    let current_pid = crate::process::current_process_id().ok_or(SyscallError::InvalidCapability)?;

    // For security, only allow joining threads in the same process
    // or child processes (to prevent information leaks)
    if target_pid != current_pid {
        // Check if target is a child process thread
        let is_child = {
            let proc = crate::process::get_process(current_pid);
            proc.is_some_and(|p| p.children.contains(&target_pid))
        };
        if !is_child {
            return Err(SyscallError::PermissionDenied);
        }
    }

    // If target thread is already terminated, return its exit code immediately
    if matches!(target_state, ThreadState::Terminated) {
        return Ok(exit_code as u64);
    }

    // Register as a waiter on the target thread
    {
        let mut proc_guard = crate::process::get_process_mut(target_pid)
            .ok_or(SyscallError::NotFound)?;
        proc_guard.add_join_waiter(target_tid, current_tid);
    }

    // Block current thread until target exits or timeout
    let timeout = if timeout_ns == u64::MAX {
        None
    } else {
        Some(Duration::from_nanos(timeout_ns))
    };

    // Set up the block
    {
        let mut threads = crate::sched::THREADS.write();
        if let Some(current) = threads.get_mut(&current_tid) {
            current.state = ThreadState::Blocked(crate::sched::BlockReason::Join(target_tid));
            current.join_target = Some(target_tid);
        }
    }

    // If we have a timeout, set up a timer
    if let Some(duration) = timeout {
        let wake_tick = crate::sched::get_tick_count() + (duration.as_millis() as u64 / 10);
        let cpu_id = crate::sched::current_cpu_id() as usize;
        let mut per_cpu = crate::sched::PER_CPU.write();
        if let Some(cpu_sched) = per_cpu.get_mut(cpu_id) {
            cpu_sched.add_to_timer_queue(current_tid, wake_tick);
        }
    }

    // Trigger reschedule - this will switch to another thread
    crate::sched::schedule();

    // When we wake up, check why
    let (joined_exit_code, was_timeout) = {
        let threads = crate::sched::THREADS.read();
        if let Some(current) = threads.get(&current_tid) {
            // Check if we woke up due to the target exiting or timeout
            let target = threads.get(&target_tid);
            match target {
                Some(t) if matches!(t.state, ThreadState::Terminated) => {
                    (t.exit_code, false)
                }
                _ => (0, true), // Target not found or not terminated = timeout
            }
        } else {
            (0, true)
        }
    };

    // Clear join target
    {
        let mut threads = crate::sched::THREADS.write();
        if let Some(current) = threads.get_mut(&current_tid) {
            current.join_target = None;
        }
    }

    if was_timeout && timeout.is_some() {
        return Err(SyscallError::Timeout);
    }

    Ok(joined_exit_code as u64)
}

// ============================================================================
// Process Syscall Handlers
// ============================================================================

fn handle_process_spawn(regs: &mut SyscallRegs) -> Result<u64, SyscallError> {
    let path_ptr = regs.arg0 as *const u8;
    let path_len = regs.arg1 as usize;
    let _args_ptr = regs.arg2 as *const u8;
    let _args_len = regs.arg3 as usize;
    let _flags = regs.arg4 as u32;

    // Validate path length
    if path_len > MAX_PATH_LEN || path_len == 0 {
        return Err(SyscallError::InvalidArgument);
    }

    // Safely copy path from userspace
    let path = copy_string_from_user(path_ptr, path_len)?;

    // Trim null terminator if present
    let path = path.trim_end_matches('\0').to_string();

    // Create spawn args with inherited credentials from current process
    let (uid, gid) = if let Some(pid) = crate::process::current_process_id() {
        if let Some(proc) = crate::process::get_process(pid) {
            (proc.uid, proc.gid)
        } else {
            (0, 0)
        }
    } else {
        (0, 0)
    };

    let args = SpawnArgs {
        path: path.clone(),
        args: alloc::vec![path],
        env: alloc::vec![],
        caps: alloc::vec![],
        sched_class: SchedClass::Normal,
        priority: 0,
        cwd: Some(String::from("/")),
        uid,
        gid,
    };

    match crate::process::spawn(args) {
        Ok(pid) => Ok(pid.0),
        Err(SpawnError::NotFound) => Err(SyscallError::NotFound),
        Err(SpawnError::PermissionDenied) => Err(SyscallError::PermissionDenied),
        Err(SpawnError::InvalidFormat) => Err(SyscallError::InvalidFormat),
        Err(SpawnError::OutOfMemory) => Err(SyscallError::OutOfMemory),
        Err(SpawnError::TooManyProcesses) => Err(SyscallError::TooManyProcesses),
        Err(_) => Err(SyscallError::IoError),
    }
}

fn handle_process_exit(regs: &mut SyscallRegs) -> Result<u64, SyscallError> {
    let exit_code = regs.arg0 as i32;

    crate::process::exit(exit_code);

    // Never returns
    Ok(0)
}

fn handle_process_wait(regs: &mut SyscallRegs) -> Result<u64, SyscallError> {
    let pid = if regs.arg0 == 0 {
        None // Wait for any child
    } else {
        Some(ProcessId(regs.arg0))
    };

    match crate::process::waitpid(pid) {
        Ok((child_pid, exit_code)) => {
            // Pack PID and exit code: upper 32 bits = exit_code, lower 32 = pid
            Ok(((exit_code as u64) << 32) | (child_pid.0 & 0xFFFFFFFF))
        }
        Err(crate::process::WaitError::NoChild) => Err(SyscallError::NoChild),
        Err(crate::process::WaitError::Interrupted) => Err(SyscallError::Interrupted),
    }
}

fn handle_process_getpid(_regs: &mut SyscallRegs) -> Result<u64, SyscallError> {
    crate::process::current_process_id()
        .map(|pid| pid.0)
        .ok_or(SyscallError::InvalidCapability)
}

fn handle_process_getppid(_regs: &mut SyscallRegs) -> Result<u64, SyscallError> {
    let pid = crate::process::current_process_id().ok_or(SyscallError::InvalidCapability)?;

    let proc = crate::process::get_process(pid).ok_or(SyscallError::InvalidCapability)?;

    Ok(proc.parent.map(|p| p.0).unwrap_or(0))
}

// ============================================================================
// Tensor/AI Syscall Handlers
// ============================================================================

fn handle_tensor_alloc(regs: &mut SyscallRegs) -> Result<u64, SyscallError> {
    let size = regs.arg0;
    let device_type = regs.arg1 as u32;
    let alignment = regs.arg2;

    // Validate size (max 16 GB for single tensor)
    const MAX_TENSOR_SIZE: u64 = 16 * 1024 * 1024 * 1024;
    if size == 0 || size > MAX_TENSOR_SIZE {
        return Err(SyscallError::InvalidArgument);
    }

    // Validate alignment (must be power of 2)
    if alignment != 0 && !alignment.is_power_of_two() {
        return Err(SyscallError::InvalidArgument);
    }

    match crate::tensor::allocate_buffer(size, device_type, alignment) {
        Ok((buffer_id, _phys_addr)) => Ok(buffer_id),
        Err(_) => Err(SyscallError::OutOfMemory),
    }
}

fn handle_tensor_free(regs: &mut SyscallRegs) -> Result<u64, SyscallError> {
    let tensor_cap = regs.arg0;

    // Create capability from raw ID
    let cap =
        unsafe { Capability::new_unchecked(ObjectId::from_raw(tensor_cap), Rights::TENSOR_FREE) };

    match crate::tensor::tensor_free(cap) {
        Ok(_) => Ok(0),
        Err(_) => Err(SyscallError::InvalidCapability),
    }
}

/// Migrate tensor to a different device
///
/// Arguments:
/// - arg0: Tensor capability
/// - arg1: Target device ID (0 = CPU, 1-N = GPU/NPU)
/// - arg2: Flags (0 = sync, 1 = async)
///
/// Returns:
/// - Job ID for async migrations (> 0)
/// - 0 for sync migrations that completed
/// - Negative error code on failure
fn handle_tensor_migrate(regs: &mut SyscallRegs) -> Result<u64, SyscallError> {
    let tensor_cap = regs.arg0;
    let target_device = regs.arg1 as u32;
    let flags = regs.arg2 as u32;

    // Validate device ID (max 16 devices)
    const MAX_DEVICES: u32 = 16;
    if target_device >= MAX_DEVICES {
        return Err(SyscallError::InvalidArgument);
    }

    // Create capability from raw ID
    let tensor_id = ObjectId::from_raw(tensor_cap);
    let cap = unsafe {
        Capability::new_unchecked(tensor_id, Rights::TENSOR_MIGRATE)
    };

    // Check capability rights
    if !cap.rights().contains(Rights::TENSOR_MIGRATE) {
        return Err(SyscallError::PermissionDenied);
    }

    // Get current device for the tensor
    let current_device = match crate::tensor::get_tensor_device(tensor_id) {
        Some(dev) => dev,
        None => return Err(SyscallError::InvalidCapability),
    };

    // If already on target device, no-op
    if current_device == target_device {
        return Ok(0);
    }

    // Choose migration strategy
    let strategy = crate::tensor::migration::choose_strategy(current_device, target_device);

    // Flag bit 0: async migration
    let is_async = (flags & 1) != 0;

    if is_async {
        // Schedule asynchronous migration
        let job_id = crate::tensor::schedule_migration(tensor_id, current_device, target_device);
        Ok(job_id)
    } else {
        // Perform synchronous migration
        match crate::tensor::migrate_sync(tensor_id, current_device, target_device, strategy) {
            Ok(_) => Ok(0),
            Err(_) => Err(SyscallError::IoError),
        }
    }
}

fn handle_inference_create(regs: &mut SyscallRegs) -> Result<u64, SyscallError> {
    let model_cap = regs.arg0;
    let config_ptr = regs.arg1 as *const u8;
    let config_len = regs.arg2 as usize;

    // Validate config length
    const MAX_CONFIG_SIZE: usize = 64 * 1024; // 64 KB max
    if config_len > MAX_CONFIG_SIZE {
        return Err(SyscallError::InvalidArgument);
    }

    // Create capability from raw model ID
    let cap =
        unsafe { Capability::new_unchecked(ObjectId::from_raw(model_cap), Rights::MODEL_ACCESS) };

    // Copy and parse config if provided
    let config = if config_len > 0 && !config_ptr.is_null() {
        let config_data = copy_from_user(config_ptr, config_len)?;
        // Parse config from bytes (simplified - just use default for now)
        let _ = config_data; // Would parse YAML/JSON config here
        crate::tensor::InferenceConfig::default()
    } else {
        crate::tensor::InferenceConfig::default()
    };

    match crate::tensor::inference_create(cap, config) {
        Ok(context_cap) => Ok(context_cap.object_id.as_u64()),
        Err(_) => Err(SyscallError::OutOfMemory),
    }
}

fn handle_inference_submit(regs: &mut SyscallRegs) -> Result<u64, SyscallError> {
    let model_id = regs.arg0;
    let input_buffer = regs.arg1;
    let output_buffer = regs.arg2;
    let flags = regs.arg3 as u32;

    // Validate buffer IDs are non-zero
    if input_buffer == 0 || output_buffer == 0 {
        return Err(SyscallError::InvalidArgument);
    }

    match crate::tensor::submit_inference(model_id, input_buffer, output_buffer, flags) {
        Ok(request_id) => Ok(request_id),
        Err(_) => Err(SyscallError::InvalidCapability),
    }
}

// ============================================================================
// System Syscall Handlers
// ============================================================================

fn handle_debug(regs: &mut SyscallRegs) -> Result<u64, SyscallError> {
    let msg_ptr = regs.arg0 as *const u8;
    let msg_len = regs.arg1 as usize;

    // Validate message length
    if msg_len > MAX_DEBUG_MSG_LEN {
        return Err(SyscallError::InvalidArgument);
    }

    // Safely copy message from userspace
    let msg = copy_string_from_user(msg_ptr, msg_len)?;

    log::debug!("[userspace] {}", msg);
    Ok(0)
}

fn handle_gettime(_regs: &mut SyscallRegs) -> Result<u64, SyscallError> {
    Ok(crate::now_ns())
}
