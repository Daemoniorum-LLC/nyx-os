//! Thread management

use crate::cap::{Capability, ObjectId, ObjectType};
use crate::mem::AddressSpace;
use core::sync::atomic::{AtomicU64, Ordering};

static NEXT_THREAD_ID: AtomicU64 = AtomicU64::new(1);

/// Thread identifier
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ThreadId(pub u64);

impl ThreadId {
    pub fn new() -> Self {
        Self(NEXT_THREAD_ID.fetch_add(1, Ordering::Relaxed))
    }
}

impl Default for ThreadId {
    fn default() -> Self {
        Self::new()
    }
}

/// Thread state
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThreadState {
    /// Ready to run
    Ready,
    /// Currently running
    Running,
    /// Blocked waiting for something
    Blocked(BlockReason),
    /// Terminated
    Terminated,
}

/// Reason for blocking
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockReason {
    /// Waiting for IPC
    Ipc,
    /// Waiting for notification
    Notification,
    /// Sleeping
    Sleep,
    /// Waiting for child
    WaitChild,
    /// Waiting for I/O
    Io,
}

/// Thread control block
pub struct Thread {
    /// Thread ID
    pub id: ThreadId,
    /// Object ID (for capability system)
    pub object_id: ObjectId,
    /// Current state
    pub state: ThreadState,
    /// Address space
    pub address_space: AddressSpace,
    /// Scheduling class
    pub sched_class: super::SchedClass,
    /// Priority (higher = more important)
    pub priority: i32,
    /// Virtual runtime (for CFS)
    pub vruntime: u64,
    /// CPU affinity mask
    pub affinity: u64,
    /// Saved register state
    pub registers: RegisterState,
}

/// Saved CPU register state
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct RegisterState {
    // General purpose registers
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rbp: u64,
    pub rsp: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,

    // Instruction pointer and flags
    pub rip: u64,
    pub rflags: u64,

    // Segment registers
    pub cs: u64,
    pub ss: u64,
}

impl Thread {
    /// Create the init thread
    pub fn new_init(init_cap: Capability) -> Self {
        Self {
            id: ThreadId::new(),
            object_id: ObjectId::new(ObjectType::Thread),
            state: ThreadState::Ready,
            address_space: AddressSpace::new(),
            sched_class: super::SchedClass::Normal,
            priority: 0,
            vruntime: 0,
            affinity: u64::MAX, // Can run on any CPU
            registers: RegisterState::default(),
        }
    }

    /// Create a new user thread
    pub fn new_user(entry: u64, stack: u64, address_space: AddressSpace) -> Self {
        let mut regs = RegisterState::default();
        regs.rip = entry;
        regs.rsp = stack;
        regs.rflags = 0x202; // IF flag set
        regs.cs = 0x23;      // User code segment
        regs.ss = 0x1b;      // User data segment

        Self {
            id: ThreadId::new(),
            object_id: ObjectId::new(ObjectType::Thread),
            state: ThreadState::Ready,
            address_space,
            sched_class: super::SchedClass::Normal,
            priority: 0,
            vruntime: 0,
            affinity: u64::MAX,
            registers: regs,
        }
    }

    /// Save registers from current execution
    pub fn save_registers(&mut self) -> RegisterState {
        self.registers
    }

    /// Restore registers for execution
    pub fn restore_registers(&mut self, regs: &RegisterState) {
        self.registers = *regs;
    }
}
