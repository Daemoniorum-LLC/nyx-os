//! System call interface
//!
//! This module provides the kernel's system call handling infrastructure.
//! All user-space requests pass through here and are dispatched to appropriate handlers.

use crate::cap::{Capability, ObjectId, ObjectType, Rights};
use crate::ipc;
use crate::mem::{VirtAddr, PAGE_SIZE};
use crate::process::{ProcessId, SpawnArgs, SpawnError};
use crate::sched::{SchedClass, ThreadState, BlockReason};
use alloc::string::String;
use alloc::vec::Vec;
use core::time::Duration;

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

        // Process syscalls
        80 => handle_process_spawn(regs),
        81 => handle_process_exit(regs),
        82 => handle_process_wait(regs),
        83 => handle_process_getpid(regs),
        84 => handle_process_getppid(regs),

        // Tensor/AI syscalls
        112 => handle_tensor_alloc(regs),
        113 => handle_tensor_free(regs),
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

    let timeout = if timeout_ns == u64::MAX {
        None
    } else {
        Some(Duration::from_nanos(timeout_ns))
    };

    // Copy message from userspace
    let msg = unsafe {
        if msg_len > 4096 {
            return Err(SyscallError::InvalidArgument);
        }
        core::slice::from_raw_parts(msg_ptr, msg_len).to_vec()
    };

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

    let timeout = if timeout_ns == u64::MAX {
        None
    } else {
        Some(Duration::from_nanos(timeout_ns))
    };

    match ipc::receive(ObjectId::from_raw(src_cap), timeout) {
        Ok(msg) => {
            let copy_len = core::cmp::min(msg.len(), buf_len);
            unsafe {
                core::ptr::copy_nonoverlapping(msg.as_ptr(), buf_ptr, copy_len);
            }
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

    // Copy request from userspace
    let req = unsafe {
        if req_len > 4096 {
            return Err(SyscallError::InvalidArgument);
        }
        core::slice::from_raw_parts(req_ptr, req_len).to_vec()
    };

    match ipc::call(ObjectId::from_raw(dest_cap), &req) {
        Ok(resp) => {
            let copy_len = core::cmp::min(resp.len(), resp_len);
            unsafe {
                core::ptr::copy_nonoverlapping(resp.as_ptr(), resp_ptr, copy_len);
            }
            Ok(copy_len as u64)
        }
        Err(_) => Err(SyscallError::InvalidCapability),
    }
}

fn handle_reply(regs: &mut SyscallRegs) -> Result<u64, SyscallError> {
    let reply_cap = regs.arg0;
    let msg_ptr = regs.arg1 as *const u8;
    let msg_len = regs.arg2 as usize;

    let msg = unsafe {
        if msg_len > 4096 {
            return Err(SyscallError::InvalidArgument);
        }
        core::slice::from_raw_parts(msg_ptr, msg_len).to_vec()
    };

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
    let new_rights = Rights::from_bits_truncate(regs.arg1 as u32);

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

    match crate::cap::grant(ObjectId::from_raw(cap_id), target_process) {
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

    // Get current process's address space
    let pid = crate::process::current_process_id()
        .ok_or(SyscallError::InvalidCapability)?;

    let proc = crate::process::get_process(pid)
        .ok_or(SyscallError::InvalidCapability)?;

    // Convert protection flags
    let protection = crate::mem::virt::Protection::from_bits_truncate(prot as u8);

    // Find suitable address if hint is 0
    let addr = if addr_hint == 0 {
        // Simple allocator: use fixed base + offset
        VirtAddr::new(0x0000_1000_0000_0000 + length)
    } else {
        VirtAddr::new(addr_hint)
    };

    // For now, just return the address (actual mapping happens on fault)
    Ok(addr.as_u64())
}

fn handle_mem_unmap(regs: &mut SyscallRegs) -> Result<u64, SyscallError> {
    let addr = VirtAddr::new(regs.arg0);
    let length = regs.arg1;

    // Get current process's address space
    let pid = crate::process::current_process_id()
        .ok_or(SyscallError::InvalidCapability)?;

    // Unmap the region
    // (simplified - actual implementation would need process write access)
    Ok(0)
}

fn handle_mem_protect(regs: &mut SyscallRegs) -> Result<u64, SyscallError> {
    let addr = VirtAddr::new(regs.arg0);
    let length = regs.arg1;
    let prot = regs.arg2 as u32;

    // Change protection on memory region
    // (simplified)
    Ok(0)
}

fn handle_mem_alloc(regs: &mut SyscallRegs) -> Result<u64, SyscallError> {
    let size = regs.arg1;
    let flags = regs.arg2 as u32;

    // Allocate physical frames
    let pages = (size + PAGE_SIZE - 1) / PAGE_SIZE;

    let frame = crate::mem::alloc_frame()
        .ok_or(SyscallError::OutOfMemory)?;

    Ok(frame.as_u64())
}

fn handle_mem_free(regs: &mut SyscallRegs) -> Result<u64, SyscallError> {
    let addr = regs.arg0;

    // Free physical frame
    // (simplified - need proper tracking)
    Ok(0)
}

// ============================================================================
// Thread Syscall Handlers
// ============================================================================

fn handle_thread_create(regs: &mut SyscallRegs) -> Result<u64, SyscallError> {
    let entry = regs.arg0;
    let stack = regs.arg1;
    let arg = regs.arg2;

    // Get current process
    let pid = crate::process::current_process_id()
        .ok_or(SyscallError::InvalidCapability)?;

    let mut proc = crate::process::get_process(pid)
        .ok_or(SyscallError::InvalidCapability)?;

    // Create new thread
    let thread = crate::sched::Thread::new_user(
        entry,
        stack,
        proc.address_space.clone(),
    );
    let thread_id = thread.id;

    // Register thread
    crate::sched::THREADS.write().insert(thread_id, thread);

    // Add to process
    // (Note: Would need mutable access to registered process)

    // Enqueue for scheduling
    let cpu_id = crate::sched::current_cpu_id();
    {
        let mut per_cpu = crate::sched::PER_CPU.write();
        if let Some(cpu_sched) = per_cpu.get_mut(cpu_id as usize) {
            cpu_sched.enqueue(thread_id);
        }
    }

    Ok(thread_id.0)
}

fn handle_thread_exit(regs: &mut SyscallRegs) -> Result<u64, SyscallError> {
    let exit_code = regs.arg0 as i32;

    let thread_id = crate::sched::current_thread_id();

    // Mark thread as terminated
    {
        let mut threads = crate::sched::THREADS.write();
        if let Some(thread) = threads.get_mut(&thread_id) {
            thread.state = ThreadState::Terminated;
        }
    }

    // Trigger reschedule
    crate::sched::schedule();

    Ok(0)
}

fn handle_thread_yield(_regs: &mut SyscallRegs) -> Result<u64, SyscallError> {
    crate::sched::yield_now();
    Ok(0)
}

fn handle_thread_sleep(regs: &mut SyscallRegs) -> Result<u64, SyscallError> {
    let duration_ns = regs.arg0;
    let duration = Duration::from_nanos(duration_ns);

    crate::sched::sleep(duration);
    Ok(0)
}

// ============================================================================
// Process Syscall Handlers
// ============================================================================

fn handle_process_spawn(regs: &mut SyscallRegs) -> Result<u64, SyscallError> {
    let path_ptr = regs.arg0 as *const u8;
    let path_len = regs.arg1 as usize;
    let args_ptr = regs.arg2 as *const u8;
    let args_len = regs.arg3 as usize;
    let flags = regs.arg4 as u32;

    // Copy path from userspace
    let path = unsafe {
        if path_len > 4096 {
            return Err(SyscallError::InvalidArgument);
        }
        let slice = core::slice::from_raw_parts(path_ptr, path_len);
        String::from_utf8_lossy(slice).to_string()
    };

    // Create spawn args
    let args = SpawnArgs {
        path: path.clone(),
        args: alloc::vec![path],
        env: alloc::vec![],
        caps: alloc::vec![],
        sched_class: SchedClass::Normal,
        priority: 0,
        cwd: Some(String::from("/")),
        uid: 0,
        gid: 0,
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
    let pid = crate::process::current_process_id()
        .ok_or(SyscallError::InvalidCapability)?;

    let proc = crate::process::get_process(pid)
        .ok_or(SyscallError::InvalidCapability)?;

    Ok(proc.parent.map(|p| p.0).unwrap_or(0))
}

// ============================================================================
// Tensor/AI Syscall Handlers
// ============================================================================

fn handle_tensor_alloc(regs: &mut SyscallRegs) -> Result<u64, SyscallError> {
    let size = regs.arg0;
    let device_type = regs.arg1 as u32;
    let alignment = regs.arg2;

    match crate::tensor::allocate_buffer(size, device_type, alignment) {
        Ok((buffer_id, _phys_addr)) => Ok(buffer_id),
        Err(_) => Err(SyscallError::OutOfMemory),
    }
}

fn handle_tensor_free(regs: &mut SyscallRegs) -> Result<u64, SyscallError> {
    let tensor_cap = regs.arg0;

    // Create capability from raw ID
    let cap = unsafe {
        Capability::new_unchecked(
            ObjectId::from_raw(tensor_cap),
            Rights::TENSOR_FREE,
        )
    };

    match crate::tensor::tensor_free(cap) {
        Ok(_) => Ok(0),
        Err(_) => Err(SyscallError::InvalidCapability),
    }
}

fn handle_inference_create(regs: &mut SyscallRegs) -> Result<u64, SyscallError> {
    let model_cap = regs.arg0;
    let config_ptr = regs.arg1 as *const u8;
    let config_len = regs.arg2 as usize;

    // Create capability from raw model ID
    let cap = unsafe {
        Capability::new_unchecked(
            ObjectId::from_raw(model_cap),
            Rights::MODEL_ACCESS,
        )
    };

    // Default config for now
    let config = crate::tensor::InferenceConfig::default();

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

    // Copy message from userspace and log it
    let msg = unsafe {
        if msg_len > 1024 {
            return Err(SyscallError::InvalidArgument);
        }
        let slice = core::slice::from_raw_parts(msg_ptr, msg_len);
        String::from_utf8_lossy(slice).to_string()
    };

    log::debug!("[userspace] {}", msg);
    Ok(0)
}

fn handle_gettime(_regs: &mut SyscallRegs) -> Result<u64, SyscallError> {
    Ok(crate::now_ns())
}
