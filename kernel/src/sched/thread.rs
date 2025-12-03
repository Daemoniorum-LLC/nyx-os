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
    /// Waiting for another thread to exit (thread join)
    Join(ThreadId),
}

/// Thread control block
pub struct Thread {
    /// Thread ID
    pub id: ThreadId,
    /// Object ID (for capability system)
    pub object_id: ObjectId,
    /// Process ID that owns this thread
    pub process_id: crate::process::ProcessId,
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
    /// Exit code when thread terminates
    pub exit_code: i32,
    /// Thread we're waiting to join (if any)
    pub join_target: Option<ThreadId>,

    // =========================================================================
    // Process Accounting
    // =========================================================================

    /// User time (nanoseconds spent in user mode)
    pub utime_ns: u64,
    /// System time (nanoseconds spent in kernel mode)
    pub stime_ns: u64,
    /// Timestamp when the thread started running in user mode
    pub user_start_ns: u64,
    /// Timestamp when the thread entered kernel mode
    pub kernel_start_ns: u64,
    /// Total context switches (voluntary + involuntary)
    pub context_switches: u64,
    /// Number of voluntary context switches (e.g., blocking on I/O)
    pub voluntary_switches: u64,
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
            process_id: crate::process::ProcessId(1), // Init is always PID 1
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
            exit_code: 0,
            join_target: None,
            // Accounting
            utime_ns: 0,
            stime_ns: 0,
            user_start_ns: 0,
            kernel_start_ns: 0,
            context_switches: 0,
            voluntary_switches: 0,
        }
    }

    /// Create a new user thread
    pub fn new_user(entry: u64, stack: u64, address_space: AddressSpace, process_id: crate::process::ProcessId) -> Self {
        let mut regs = RegisterState::default();
        regs.rip = entry;
        regs.rsp = stack;
        regs.rflags = 0x202; // IF flag set
        regs.cs = 0x23; // User code segment (ring 3)
        regs.ss = 0x1b; // User data segment (ring 3)

        Self {
            id: ThreadId::new(),
            object_id: ObjectId::new(ObjectType::Thread),
            process_id,
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
            exit_code: 0,
            join_target: None,
            // Accounting
            utime_ns: 0,
            stime_ns: 0,
            user_start_ns: 0,
            kernel_start_ns: 0,
            context_switches: 0,
            voluntary_switches: 0,
        }
    }

    /// Account for time spent in user mode
    ///
    /// Called when entering kernel mode (syscall, exception, interrupt)
    pub fn account_user_time(&mut self, now_ns: u64) {
        if self.user_start_ns > 0 {
            self.utime_ns += now_ns.saturating_sub(self.user_start_ns);
            self.user_start_ns = 0;
        }
        self.kernel_start_ns = now_ns;
    }

    /// Account for time spent in kernel mode
    ///
    /// Called when returning to user mode (sysret, iret)
    pub fn account_kernel_time(&mut self, now_ns: u64) {
        if self.kernel_start_ns > 0 {
            self.stime_ns += now_ns.saturating_sub(self.kernel_start_ns);
            self.kernel_start_ns = 0;
        }
        self.user_start_ns = now_ns;
    }

    /// Account for a context switch
    pub fn account_context_switch(&mut self, voluntary: bool) {
        self.context_switches += 1;
        if voluntary {
            self.voluntary_switches += 1;
        }
    }

    /// Get user time in microseconds (for getrusage/times)
    pub fn utime_usec(&self) -> u64 {
        self.utime_ns / 1000
    }

    /// Get system time in microseconds
    pub fn stime_usec(&self) -> u64 {
        self.stime_ns / 1000
    }

    /// Get user time as (seconds, microseconds) tuple
    pub fn utime(&self) -> (u64, u64) {
        let usec = self.utime_usec();
        (usec / 1_000_000, usec % 1_000_000)
    }

    /// Get system time as (seconds, microseconds) tuple
    pub fn stime(&self) -> (u64, u64) {
        let usec = self.stime_usec();
        (usec / 1_000_000, usec % 1_000_000)
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
            process_id: crate::process::ProcessId(0), // Kernel threads belong to PID 0
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
            exit_code: 0,
            join_target: None,
            // Accounting (kernel threads only accumulate stime)
            utime_ns: 0,
            stime_ns: 0,
            user_start_ns: 0,
            kernel_start_ns: 0,
            context_switches: 0,
            voluntary_switches: 0,
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

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // Thread ID Tests
    // =========================================================================

    #[test]
    fn test_thread_id_unique() {
        let id1 = ThreadId::new();
        let id2 = ThreadId::new();
        let id3 = ThreadId::new();

        assert_ne!(id1, id2);
        assert_ne!(id2, id3);
        assert_ne!(id1, id3);
    }

    #[test]
    fn test_thread_id_default() {
        let id1 = ThreadId::default();
        let id2 = ThreadId::default();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_thread_id_ordering() {
        let id1 = ThreadId::new();
        let id2 = ThreadId::new();
        // Later IDs should be greater (monotonically increasing)
        assert!(id2 > id1);
    }

    // =========================================================================
    // Thread State Tests
    // =========================================================================

    #[test]
    fn test_thread_state_variants() {
        let ready = ThreadState::Ready;
        let running = ThreadState::Running;
        let blocked = ThreadState::Blocked(BlockReason::Ipc);
        let terminated = ThreadState::Terminated;

        assert_eq!(ready, ThreadState::Ready);
        assert_eq!(running, ThreadState::Running);
        assert_ne!(ready, running);
        assert_ne!(blocked, terminated);
    }

    #[test]
    fn test_block_reason_variants() {
        let reasons = [
            BlockReason::Ipc,
            BlockReason::Notification,
            BlockReason::Sleep,
            BlockReason::WaitChild,
            BlockReason::Io,
            BlockReason::Mutex,
            BlockReason::Semaphore,
            BlockReason::Join(ThreadId(1)),
        ];

        // All variants should be distinct
        for (i, &a) in reasons.iter().enumerate() {
            for (j, &b) in reasons.iter().enumerate() {
                if i != j {
                    assert_ne!(a, b);
                }
            }
        }
    }

    #[test]
    fn test_block_reason_join_different_threads() {
        let join1 = BlockReason::Join(ThreadId(1));
        let join2 = BlockReason::Join(ThreadId(2));
        assert_ne!(join1, join2);

        let join_same = BlockReason::Join(ThreadId(1));
        assert_eq!(join1, join_same);
    }

    // =========================================================================
    // Process Accounting Tests
    // =========================================================================

    #[test]
    fn test_account_user_time() {
        let mut thread = create_test_thread();

        // Simulate entering user mode at time 1000
        thread.user_start_ns = 1000;

        // Account user time at time 1500 (500ns elapsed)
        thread.account_user_time(1500);

        assert_eq!(thread.utime_ns, 500);
        assert_eq!(thread.user_start_ns, 0); // Reset after accounting
        assert_eq!(thread.kernel_start_ns, 1500); // Now tracking kernel time
    }

    #[test]
    fn test_account_kernel_time() {
        let mut thread = create_test_thread();

        // Simulate entering kernel mode at time 1000
        thread.kernel_start_ns = 1000;

        // Account kernel time at time 1800 (800ns elapsed)
        thread.account_kernel_time(1800);

        assert_eq!(thread.stime_ns, 800);
        assert_eq!(thread.kernel_start_ns, 0); // Reset after accounting
        assert_eq!(thread.user_start_ns, 1800); // Now tracking user time
    }

    #[test]
    fn test_account_user_time_saturating() {
        let mut thread = create_test_thread();

        // Edge case: now_ns < user_start_ns (shouldn't happen but handle gracefully)
        thread.user_start_ns = 1000;
        thread.account_user_time(500); // Earlier timestamp

        // Should use saturating_sub, so utime_ns stays 0
        assert_eq!(thread.utime_ns, 0);
    }

    #[test]
    fn test_account_context_switch() {
        let mut thread = create_test_thread();

        assert_eq!(thread.context_switches, 0);
        assert_eq!(thread.voluntary_switches, 0);

        // Involuntary switch (preemption)
        thread.account_context_switch(false);
        assert_eq!(thread.context_switches, 1);
        assert_eq!(thread.voluntary_switches, 0);

        // Voluntary switch (blocking)
        thread.account_context_switch(true);
        assert_eq!(thread.context_switches, 2);
        assert_eq!(thread.voluntary_switches, 1);

        // Another involuntary
        thread.account_context_switch(false);
        assert_eq!(thread.context_switches, 3);
        assert_eq!(thread.voluntary_switches, 1);
    }

    #[test]
    fn test_utime_usec_conversion() {
        let mut thread = create_test_thread();
        thread.utime_ns = 5_500_000; // 5.5ms = 5500us

        assert_eq!(thread.utime_usec(), 5500);
    }

    #[test]
    fn test_stime_usec_conversion() {
        let mut thread = create_test_thread();
        thread.stime_ns = 2_250_000; // 2.25ms = 2250us

        assert_eq!(thread.stime_usec(), 2250);
    }

    #[test]
    fn test_utime_tuple() {
        let mut thread = create_test_thread();
        thread.utime_ns = 3_500_000_000; // 3.5 seconds

        let (secs, usecs) = thread.utime();
        assert_eq!(secs, 3);
        assert_eq!(usecs, 500_000);
    }

    #[test]
    fn test_stime_tuple() {
        let mut thread = create_test_thread();
        thread.stime_ns = 10_750_000_000; // 10.75 seconds

        let (secs, usecs) = thread.stime();
        assert_eq!(secs, 10);
        assert_eq!(usecs, 750_000);
    }

    #[test]
    fn test_zero_time_accounting() {
        let thread = create_test_thread();

        assert_eq!(thread.utime_ns, 0);
        assert_eq!(thread.stime_ns, 0);
        assert_eq!(thread.utime(), (0, 0));
        assert_eq!(thread.stime(), (0, 0));
    }

    // =========================================================================
    // Thread State Helpers Tests
    // =========================================================================

    #[test]
    fn test_is_runnable() {
        let mut thread = create_test_thread();

        thread.state = ThreadState::Ready;
        assert!(thread.is_runnable());

        thread.state = ThreadState::Running;
        assert!(thread.is_runnable());

        thread.state = ThreadState::Blocked(BlockReason::Sleep);
        assert!(!thread.is_runnable());

        thread.state = ThreadState::Terminated;
        assert!(!thread.is_runnable());
    }

    #[test]
    fn test_is_blocked() {
        let mut thread = create_test_thread();

        thread.state = ThreadState::Ready;
        assert!(!thread.is_blocked());

        thread.state = ThreadState::Blocked(BlockReason::Ipc);
        assert!(thread.is_blocked());

        thread.state = ThreadState::Blocked(BlockReason::Mutex);
        assert!(thread.is_blocked());
    }

    #[test]
    fn test_is_terminated() {
        let mut thread = create_test_thread();

        thread.state = ThreadState::Ready;
        assert!(!thread.is_terminated());

        thread.state = ThreadState::Running;
        assert!(!thread.is_terminated());

        thread.state = ThreadState::Terminated;
        assert!(thread.is_terminated());
    }

    // =========================================================================
    // Priority and Affinity Tests
    // =========================================================================

    #[test]
    fn test_set_priority() {
        let mut thread = create_test_thread();
        assert_eq!(thread.priority, 0);

        thread.set_priority(10);
        assert_eq!(thread.priority, 10);

        thread.set_priority(-5);
        assert_eq!(thread.priority, -5);
    }

    #[test]
    fn test_set_affinity() {
        let mut thread = create_test_thread();
        assert_eq!(thread.affinity, u64::MAX); // Can run on any CPU

        thread.set_affinity(0b0101); // CPUs 0 and 2
        assert_eq!(thread.affinity, 0b0101);
    }

    #[test]
    fn test_can_run_on() {
        let mut thread = create_test_thread();
        thread.affinity = 0b1010; // CPUs 1 and 3

        assert!(!thread.can_run_on(0));
        assert!(thread.can_run_on(1));
        assert!(!thread.can_run_on(2));
        assert!(thread.can_run_on(3));
    }

    #[test]
    fn test_can_run_on_any() {
        let thread = create_test_thread();
        // Default affinity is u64::MAX (all CPUs)
        assert!(thread.can_run_on(0));
        assert!(thread.can_run_on(31));
        assert!(thread.can_run_on(63));
    }

    // =========================================================================
    // Register State Tests
    // =========================================================================

    #[test]
    fn test_register_state_default() {
        let regs = RegisterState::default();

        assert_eq!(regs.rax, 0);
        assert_eq!(regs.rbx, 0);
        assert_eq!(regs.rsp, 0);
        assert_eq!(regs.rip, 0);
        assert_eq!(regs.rflags, 0);
    }

    #[test]
    fn test_register_state_clone() {
        let mut regs = RegisterState::default();
        regs.rax = 0xDEADBEEF;
        regs.rip = 0x1000;

        let cloned = regs;
        assert_eq!(cloned.rax, 0xDEADBEEF);
        assert_eq!(cloned.rip, 0x1000);
    }

    // =========================================================================
    // Helper Functions
    // =========================================================================

    fn create_test_thread() -> Thread {
        Thread {
            id: ThreadId::new(),
            object_id: crate::cap::ObjectId::new(crate::cap::ObjectType::Thread),
            process_id: crate::process::ProcessId(1),
            state: ThreadState::Ready,
            address_space: AddressSpace::new(),
            sched_class: super::super::SchedClass::Normal,
            priority: 0,
            vruntime: 0,
            affinity: u64::MAX,
            registers: RegisterState::default(),
            wake_tick: 0,
            kernel_stack: 0,
            user_stack: 0,
            exit_code: 0,
            join_target: None,
            utime_ns: 0,
            stime_ns: 0,
            user_start_ns: 0,
            kernel_start_ns: 0,
            context_switches: 0,
            voluntary_switches: 0,
        }
    }
}
