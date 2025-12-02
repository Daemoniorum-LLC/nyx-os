//! Raw syscall interface
//!
//! This module provides the low-level syscall primitives for Nyx OS.
//! Most users should use the higher-level wrappers in other modules.

use core::arch::asm;

/// Perform a syscall with no arguments
#[inline]
pub unsafe fn syscall0(num: u64) -> i64 {
    let ret: i64;
    asm!(
        "syscall",
        inout("rax") num => ret,
        out("rcx") _,
        out("r11") _,
        options(nostack),
    );
    ret
}

/// Perform a syscall with 1 argument
#[inline]
pub unsafe fn syscall1(num: u64, arg0: u64) -> i64 {
    let ret: i64;
    asm!(
        "syscall",
        inout("rax") num => ret,
        in("rdi") arg0,
        out("rcx") _,
        out("r11") _,
        options(nostack),
    );
    ret
}

/// Perform a syscall with 2 arguments
#[inline]
pub unsafe fn syscall2(num: u64, arg0: u64, arg1: u64) -> i64 {
    let ret: i64;
    asm!(
        "syscall",
        inout("rax") num => ret,
        in("rdi") arg0,
        in("rsi") arg1,
        out("rcx") _,
        out("r11") _,
        options(nostack),
    );
    ret
}

/// Perform a syscall with 3 arguments
#[inline]
pub unsafe fn syscall3(num: u64, arg0: u64, arg1: u64, arg2: u64) -> i64 {
    let ret: i64;
    asm!(
        "syscall",
        inout("rax") num => ret,
        in("rdi") arg0,
        in("rsi") arg1,
        in("rdx") arg2,
        out("rcx") _,
        out("r11") _,
        options(nostack),
    );
    ret
}

/// Perform a syscall with 4 arguments
#[inline]
pub unsafe fn syscall4(num: u64, arg0: u64, arg1: u64, arg2: u64, arg3: u64) -> i64 {
    let ret: i64;
    asm!(
        "syscall",
        inout("rax") num => ret,
        in("rdi") arg0,
        in("rsi") arg1,
        in("rdx") arg2,
        in("r10") arg3,
        out("rcx") _,
        out("r11") _,
        options(nostack),
    );
    ret
}

/// Perform a syscall with 5 arguments
#[inline]
pub unsafe fn syscall5(num: u64, arg0: u64, arg1: u64, arg2: u64, arg3: u64, arg4: u64) -> i64 {
    let ret: i64;
    asm!(
        "syscall",
        inout("rax") num => ret,
        in("rdi") arg0,
        in("rsi") arg1,
        in("rdx") arg2,
        in("r10") arg3,
        in("r8") arg4,
        out("rcx") _,
        out("r11") _,
        options(nostack),
    );
    ret
}

/// Perform a syscall with 6 arguments
#[inline]
pub unsafe fn syscall6(
    num: u64,
    arg0: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    arg4: u64,
    arg5: u64,
) -> i64 {
    let ret: i64;
    asm!(
        "syscall",
        inout("rax") num => ret,
        in("rdi") arg0,
        in("rsi") arg1,
        in("rdx") arg2,
        in("r10") arg3,
        in("r8") arg4,
        in("r9") arg5,
        out("rcx") _,
        out("r11") _,
        options(nostack),
    );
    ret
}

/// System call numbers
///
/// These MUST match the kernel's syscall numbers in kernel/src/syscall.rs.
/// Any mismatch will cause undefined behavior at runtime.
pub mod nr {
    // ========================================================================
    // IPC (0-15)
    // ========================================================================

    /// Set up an IPC ring for async operations
    /// Args: sq_entries, cq_entries, flags
    /// Returns: ring capability ID or negative error
    pub const RING_SETUP: u64 = 0;

    /// Enter the IPC ring to submit/complete operations
    /// Args: ring_cap, to_submit, min_complete, flags
    /// Returns: number of completions or negative error
    pub const RING_ENTER: u64 = 1;

    /// Send a message to an endpoint
    /// Args: dest_cap, msg_ptr, msg_len, timeout_ns
    pub const SEND: u64 = 2;

    /// Receive a message from an endpoint
    /// Args: src_cap, buf_ptr, buf_len, timeout_ns
    /// Returns: bytes received or negative error
    pub const RECEIVE: u64 = 3;

    /// Synchronous call: send request and wait for reply
    /// Args: dest_cap, req_ptr, req_len, resp_ptr, resp_len
    /// Returns: response length or negative error
    pub const CALL: u64 = 4;

    /// Reply to an incoming call
    /// Args: reply_cap, msg_ptr, msg_len
    pub const REPLY: u64 = 5;

    /// Signal notification bits
    /// Args: target_cap, signal_bits
    pub const SIGNAL: u64 = 6;

    /// Wait for notification bits
    /// Args: wait_cap, mask, timeout_ns
    /// Returns: signaled bits or negative error
    pub const WAIT: u64 = 7;

    /// Poll notification bits (non-blocking)
    /// Args: poll_cap, mask
    /// Returns: current bits
    pub const POLL: u64 = 8;

    /// Get ring memory mapping information for polling mode
    /// Args: ring_cap, info_ptr
    /// Returns: 0 on success
    pub const RING_MMAP_INFO: u64 = 9;

    /// Wake the kernel polling thread
    /// Args: ring_cap
    pub const RING_WAKE: u64 = 10;

    /// Set IPC affinity hint for an endpoint
    /// Args: endpoint_cap, hint
    pub const IPC_AFFINITY: u64 = 11;

    /// Get IPC affinity hint for an endpoint
    /// Args: endpoint_cap
    /// Returns: hint value
    pub const IPC_GET_AFFINITY: u64 = 12;

    /// Send from a registered buffer (fast path)
    /// Args: dest_cap, buf_idx, offset, len, flags
    pub const SEND_REGISTERED: u64 = 13;

    /// Receive into a registered buffer (fast path)
    /// Args: src_cap, buf_idx, offset, max_len, flags
    /// Returns: bytes received
    pub const RECV_REGISTERED: u64 = 14;

    // ========================================================================
    // Capabilities (16-31)
    // ========================================================================

    /// Derive a new capability with reduced rights
    /// Args: src_cap, new_rights
    /// Returns: new capability ID or negative error
    pub const CAP_DERIVE: u64 = 16;

    /// Revoke a capability and all its derivatives
    /// Args: cap_id
    pub const CAP_REVOKE: u64 = 17;

    /// Identify a capability's type and rights
    /// Args: cap_id
    /// Returns: (type << 32) | rights
    pub const CAP_IDENTIFY: u64 = 18;

    /// Grant a capability to another process
    /// Args: cap_id, target_process, rights_mask
    /// Returns: new capability ID in target's cspace
    pub const CAP_GRANT: u64 = 19;

    /// Drop (release) a capability
    /// Args: cap_id
    pub const CAP_DROP: u64 = 20;

    // ========================================================================
    // Memory (32-63)
    // ========================================================================

    /// Map memory into address space
    /// Args: addr_hint, length, prot, flags
    /// Returns: mapped address or negative error
    pub const MEM_MAP: u64 = 32;

    /// Unmap memory from address space
    /// Args: addr, length
    pub const MEM_UNMAP: u64 = 33;

    /// Change memory protection
    /// Args: addr, length, prot
    pub const MEM_PROTECT: u64 = 34;

    /// Allocate physical memory
    /// Args: size, flags
    /// Returns: physical address or negative error
    pub const MEM_ALLOC: u64 = 35;

    /// Free physical memory
    /// Args: addr, size
    pub const MEM_FREE: u64 = 36;

    /// Create a shared memory region
    /// Args: size, flags
    /// Returns: capability ID or negative error
    pub const SHM_CREATE: u64 = 40;

    /// Map a shared memory region
    /// Args: shm_cap, addr_hint, size, prot
    /// Returns: mapped address or negative error
    pub const SHM_MAP: u64 = 41;

    /// Unmap a shared memory region
    /// Args: addr, size
    pub const SHM_UNMAP: u64 = 42;

    /// Grant shared memory access to another process
    /// Args: shm_cap, target_cap, prot
    /// Returns: view capability or negative error
    pub const SHM_GRANT: u64 = 43;

    /// Register a buffer with the IPC subsystem
    /// Args: addr, size
    /// Returns: (index << 32) | cap_id
    pub const BUF_REGISTER: u64 = 48;

    /// Unregister a buffer
    /// Args: cap_id
    pub const BUF_UNREGISTER: u64 = 49;

    // ========================================================================
    // Threads (64-79)
    // ========================================================================

    /// Create a new thread
    /// Args: entry_point, stack_ptr, arg
    /// Returns: thread ID or negative error
    pub const THREAD_CREATE: u64 = 64;

    /// Exit the current thread
    /// Args: exit_code
    /// Does not return
    pub const THREAD_EXIT: u64 = 65;

    /// Yield the current thread's timeslice
    pub const THREAD_YIELD: u64 = 66;

    /// Sleep for a duration
    /// Args: duration_ns
    pub const THREAD_SLEEP: u64 = 67;

    /// Wait for a thread to exit
    /// Args: thread_id
    /// Returns: exit code or negative error
    pub const THREAD_JOIN: u64 = 68;

    // ========================================================================
    // Process (80-95)
    // ========================================================================

    /// Spawn a new process
    /// Args: path_ptr, path_len, args_ptr, args_len, flags
    /// Returns: process ID or negative error
    pub const PROCESS_SPAWN: u64 = 80;

    /// Exit the current process
    /// Args: exit_code
    /// Does not return
    pub const PROCESS_EXIT: u64 = 81;

    /// Wait for a child process to exit
    /// Args: pid (0 = any child)
    /// Returns: (exit_code << 32) | child_pid
    pub const PROCESS_WAIT: u64 = 82;

    /// Get current process ID
    /// Returns: pid
    pub const PROCESS_GETPID: u64 = 83;

    /// Get parent process ID
    /// Returns: ppid
    pub const PROCESS_GETPPID: u64 = 84;

    // ========================================================================
    // File System (96-111) - Reserved for future VFS
    // ========================================================================

    pub const FS_OPEN: u64 = 96;
    pub const FS_CLOSE: u64 = 97;
    pub const FS_READ: u64 = 98;
    pub const FS_WRITE: u64 = 99;
    pub const FS_STAT: u64 = 100;
    pub const FS_READDIR: u64 = 101;

    // ========================================================================
    // Tensor/AI (112-143)
    // ========================================================================

    /// Allocate a tensor buffer
    /// Args: size, device_type, alignment
    /// Returns: buffer_id or negative error
    pub const TENSOR_ALLOC: u64 = 112;

    /// Free a tensor buffer
    /// Args: tensor_cap
    pub const TENSOR_FREE: u64 = 113;

    /// Migrate tensor between devices
    /// Args: tensor_cap, target_device
    pub const TENSOR_MIGRATE: u64 = 114;

    /// Create an inference context
    /// Args: model_cap, config_ptr, config_len
    /// Returns: context capability or negative error
    pub const INFERENCE_CREATE: u64 = 115;

    /// Submit an inference request
    /// Args: model_id, input_buffer, output_buffer, flags
    /// Returns: request_id or negative error
    pub const INFERENCE_SUBMIT: u64 = 116;

    /// Submit a compute operation
    /// Args: varies by operation
    pub const COMPUTE_SUBMIT: u64 = 117;

    // ========================================================================
    // Time-Travel (144-159)
    // ========================================================================

    /// Create a checkpoint of the current process state
    /// Returns: checkpoint ID or negative error
    pub const CHECKPOINT: u64 = 144;

    /// Restore to a previous checkpoint
    /// Args: checkpoint_id
    /// Does not return on success (jumps to checkpoint)
    pub const RESTORE: u64 = 145;

    /// Start recording execution for replay
    pub const RECORD_START: u64 = 146;

    /// Stop recording execution
    pub const RECORD_STOP: u64 = 147;

    // ========================================================================
    // System (240-255)
    // ========================================================================

    /// Print a debug message (development only)
    /// Args: msg_ptr, msg_len
    pub const DEBUG: u64 = 240;

    /// Get current time in nanoseconds since boot
    /// Returns: nanoseconds
    pub const GET_TIME: u64 = 241;

    /// Reboot the system (requires privilege)
    pub const REBOOT: u64 = 254;

    /// Shutdown the system (requires privilege)
    pub const SHUTDOWN: u64 = 255;
}

/// Syscall error codes (matches kernel SyscallError)
#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// Success (not an error)
    Success = 0,
    /// Invalid syscall number
    InvalidSyscall = -1,
    /// Invalid or revoked capability
    InvalidCapability = -2,
    /// Operation not permitted
    PermissionDenied = -3,
    /// Out of memory
    OutOfMemory = -4,
    /// Invalid argument
    InvalidArgument = -5,
    /// Operation would block
    WouldBlock = -6,
    /// Operation timed out
    Timeout = -7,
    /// Operation interrupted
    Interrupted = -8,
    /// Resource not found
    NotFound = -9,
    /// Invalid format (e.g., bad ELF)
    InvalidFormat = -10,
    /// I/O error
    IoError = -11,
    /// Too many processes
    TooManyProcesses = -12,
    /// No child processes
    NoChild = -13,
    /// Bad memory address
    BadAddress = -14,
}

impl Error {
    /// Convert from raw syscall return value
    pub fn from_raw(value: i64) -> Result<u64, Self> {
        if value >= 0 {
            Ok(value as u64)
        } else {
            Err(match value as i32 {
                -1 => Self::InvalidSyscall,
                -2 => Self::InvalidCapability,
                -3 => Self::PermissionDenied,
                -4 => Self::OutOfMemory,
                -5 => Self::InvalidArgument,
                -6 => Self::WouldBlock,
                -7 => Self::Timeout,
                -8 => Self::Interrupted,
                -9 => Self::NotFound,
                -10 => Self::InvalidFormat,
                -11 => Self::IoError,
                -12 => Self::TooManyProcesses,
                -13 => Self::NoChild,
                -14 => Self::BadAddress,
                _ => Self::InvalidSyscall, // Unknown error
            })
        }
    }

    /// Get human-readable error message
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::InvalidSyscall => "invalid syscall number",
            Self::InvalidCapability => "invalid or revoked capability",
            Self::PermissionDenied => "permission denied",
            Self::OutOfMemory => "out of memory",
            Self::InvalidArgument => "invalid argument",
            Self::WouldBlock => "operation would block",
            Self::Timeout => "operation timed out",
            Self::Interrupted => "operation interrupted",
            Self::NotFound => "not found",
            Self::InvalidFormat => "invalid format",
            Self::IoError => "I/O error",
            Self::TooManyProcesses => "too many processes",
            Self::NoChild => "no child processes",
            Self::BadAddress => "bad memory address",
        }
    }
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
