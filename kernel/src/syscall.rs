//! System call interface

use crate::cap::Capability;
use crate::ipc::{SqEntry, IpcRing};

/// System call numbers
#[repr(u64)]
pub enum Syscall {
    // IPC (0-15)
    RingSetup = 0,
    RingEnter = 1,
    Send = 2,
    Receive = 3,
    Call = 4,
    Reply = 5,

    // Capabilities (16-31)
    CapDerive = 16,
    CapRevoke = 17,
    CapIdentify = 18,
    CapGrant = 19,

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

    // Process (80-95)
    ProcessSpawn = 80,
    ProcessExit = 81,
    ProcessWait = 82,

    // Tensor/AI (96-127)
    TensorAlloc = 96,
    TensorFree = 97,
    TensorMigrate = 98,
    InferenceCreate = 99,
    InferenceSubmit = 100,
    ComputeSubmit = 101,

    // Time-Travel (128-143)
    Checkpoint = 128,
    Restore = 129,
    RecordStart = 130,
    RecordStop = 131,

    // System (240-255)
    Debug = 240,
    Reboot = 254,
    Shutdown = 255,
}

/// System call handler (called from arch-specific entry)
pub fn syscall_handler(regs: &mut SyscallRegs) {
    let syscall_num = regs.syscall_num;

    let result = match syscall_num {
        0 => handle_ring_setup(regs),
        1 => handle_ring_enter(regs),
        // ... more syscalls
        _ => Err(SyscallError::InvalidSyscall),
    };

    regs.result = match result {
        Ok(val) => val as i64,
        Err(err) => -(err as i64),
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
}

fn handle_ring_setup(_regs: &mut SyscallRegs) -> Result<u64, SyscallError> {
    todo!("Implement ring setup")
}

fn handle_ring_enter(_regs: &mut SyscallRegs) -> Result<u64, SyscallError> {
    todo!("Implement ring enter")
}
