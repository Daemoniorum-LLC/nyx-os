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
    /// Waiting for mutex
    Mutex,
    /// Waiting for semaphore
    Semaphore,
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
    /// Wake tick for sleeping threads
    pub wake_tick: u64,
    /// Kernel stack pointer
    pub kernel_stack: u64,
    /// User stack pointer
    pub user_stack: u64,
}

/// Saved CPU register state for context switching
/// Layout must match the assembly in context_switch
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct RegisterState {
    // Callee-saved registers (in order for context switch)
    pub r15: u64, // 0x00
    pub r14: u64, // 0x08
    pub r13: u64, // 0x10
    pub r12: u64, // 0x18
    pub r11: u64, // 0x20
    pub r10: u64, // 0x28
    pub r9: u64,  // 0x30
    pub r8: u64,  // 0x38
    pub rbp: u64, // 0x40
    pub rdi: u64, // 0x48
    pub rsi: u64, // 0x50
    pub rdx: u64, // 0x58
    pub rcx: u64, // 0x60
    pub rbx: u64, // 0x68
    pub rax: u64, // 0x70
    pub rsp: u64, // 0x78

    // Instruction pointer and flags
    pub rip: u64,    // 0x80
    pub rflags: u64, // 0x88

    // Segment registers
    pub cs: u64, // 0x90
    pub ss: u64, // 0x98
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
            wake_tick: 0,
            kernel_stack: 0,
            user_stack: 0,
        }
    }

    /// Create a new user thread
    pub fn new_user(entry: u64, stack: u64, address_space: AddressSpace) -> Self {
        let mut regs = RegisterState::default();
        regs.rip = entry;
        regs.rsp = stack;
        regs.rflags = 0x202; // IF flag set
        regs.cs = 0x23; // User code segment (ring 3)
        regs.ss = 0x1b; // User data segment (ring 3)

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
            wake_tick: 0,
            kernel_stack: 0,
            user_stack: stack,
        }
    }

    /// Create a new kernel thread
    pub fn new_kernel(entry: u64, stack: u64) -> Self {
        let mut regs = RegisterState::default();
        regs.rip = entry;
        regs.rsp = stack;
        regs.rflags = 0x202; // IF flag set
        regs.cs = 0x08; // Kernel code segment (ring 0)
        regs.ss = 0x10; // Kernel data segment (ring 0)

        Self {
            id: ThreadId::new(),
            object_id: ObjectId::new(ObjectType::Thread),
            state: ThreadState::Ready,
            address_space: AddressSpace::new(), // Uses kernel address space
            sched_class: super::SchedClass::Normal,
            priority: 0,
            vruntime: 0,
            affinity: u64::MAX,
            registers: regs,
            wake_tick: 0,
            kernel_stack: stack,
            user_stack: 0,
        }
    }

    /// Save registers from interrupt frame
    pub fn save_from_frame(&mut self, frame: &crate::arch::x86_64::idt::ExceptionFrame) {
        self.registers.rax = frame.rax;
        self.registers.rbx = frame.rbx;
        self.registers.rcx = frame.rcx;
        self.registers.rdx = frame.rdx;
        self.registers.rsi = frame.rsi;
        self.registers.rdi = frame.rdi;
        self.registers.rbp = frame.rbp;
        self.registers.r8 = frame.r8;
        self.registers.r9 = frame.r9;
        self.registers.r10 = frame.r10;
        self.registers.r11 = frame.r11;
        self.registers.r12 = frame.r12;
        self.registers.r13 = frame.r13;
        self.registers.r14 = frame.r14;
        self.registers.r15 = frame.r15;
        self.registers.rip = frame.rip;
        self.registers.rsp = frame.rsp;
        self.registers.rflags = frame.rflags;
        self.registers.cs = frame.cs;
        self.registers.ss = frame.ss;
    }

    /// Check if thread is runnable
    pub fn is_runnable(&self) -> bool {
        matches!(self.state, ThreadState::Ready | ThreadState::Running)
    }

    /// Check if thread is blocked
    pub fn is_blocked(&self) -> bool {
        matches!(self.state, ThreadState::Blocked(_))
    }

    /// Check if thread is terminated
    pub fn is_terminated(&self) -> bool {
        matches!(self.state, ThreadState::Terminated)
    }

    /// Set thread priority
    pub fn set_priority(&mut self, priority: i32) {
        self.priority = priority;
    }

    /// Set CPU affinity
    pub fn set_affinity(&mut self, mask: u64) {
        self.affinity = mask;
    }

    /// Check if thread can run on CPU
    pub fn can_run_on(&self, cpu_id: u32) -> bool {
        (self.affinity & (1 << cpu_id)) != 0
    }
}

/// Thread entry function type
pub type ThreadEntry = extern "C" fn(arg: u64);

/// Spawn a new kernel thread
pub fn spawn_kernel_thread(entry: ThreadEntry, arg: u64, stack_size: usize) -> ThreadId {
    // Allocate stack
    let stack = alloc::vec![0u8; stack_size];
    let stack_top = stack.as_ptr() as u64 + stack_size as u64;
    core::mem::forget(stack); // Don't drop the stack

    // Set up initial stack frame
    // Push argument for the entry function
    let stack_top = stack_top - 8;
    unsafe {
        *(stack_top as *mut u64) = arg;
    }

    // Create thread
    let thread = Thread::new_kernel(entry as u64, stack_top);
    let thread_id = thread.id;

    // Register thread
    super::THREADS.write().insert(thread_id, thread);

    // Enqueue for scheduling
    let cpu_id = super::current_cpu_id();
    {
        let mut per_cpu = super::PER_CPU.write();
        if let Some(cpu_sched) = per_cpu.get_mut(cpu_id as usize) {
            cpu_sched.enqueue(thread_id);
        }
    }

    thread_id
}
