//! Scheduler subsystem
//!
//! ## Features
//!
//! - Heterogeneous scheduling (CPU + GPU + NPU)
//! - CFS-style fair scheduling for normal tasks
//! - Real-time deadline scheduling (SCHED_DEADLINE)
//! - Energy-aware scheduling (big.LITTLE / P-core/E-core)
//! - Priority inheritance for mutex holders

mod cfs;
mod deadline;
mod energy;
mod thread;

pub use thread::{BlockReason, RegisterState, Thread, ThreadId, ThreadState};

use crate::arch::BootInfo;
use crate::cap::Capability;
use alloc::collections::{BTreeMap, BinaryHeap};
use core::arch::asm;
use core::cmp::Ordering as CmpOrdering;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::RwLock;

/// Global thread registry
pub static THREADS: RwLock<BTreeMap<ThreadId, Thread>> = RwLock::new(BTreeMap::new());

/// Per-CPU scheduler state
pub static PER_CPU: RwLock<alloc::vec::Vec<CpuScheduler>> = RwLock::new(alloc::vec::Vec::new());

/// Need reschedule flag (per-CPU, but simplified for now)
static NEED_RESCHED: AtomicBool = AtomicBool::new(false);

/// Current thread ID (per-CPU, simplified)
static CURRENT_THREAD: AtomicU64 = AtomicU64::new(0);

/// Timer tick counter
static TICK_COUNT: AtomicU64 = AtomicU64::new(0);

/// Time slice in timer ticks
const TIME_SLICE_TICKS: u64 = 10;

/// Initialize the scheduler
pub fn init(boot_info: &BootInfo) {
    log::debug!("Initializing scheduler for {} CPUs", boot_info.cpu_count);

    let mut per_cpu = PER_CPU.write();
    for cpu_id in 0..boot_info.cpu_count {
        per_cpu.push(CpuScheduler::new(cpu_id));
    }

    log::debug!("Scheduler initialized");
}

/// Start the scheduler (never returns)
pub fn start(init_cap: Capability) -> ! {
    log::info!("Starting scheduler with init process");

    // Create init thread
    let init_thread = Thread::new_init(init_cap);
    let init_id = init_thread.id;

    THREADS.write().insert(init_id, init_thread);

    // Add to run queue
    if let Some(cpu_sched) = PER_CPU.write().first_mut() {
        cpu_sched.enqueue(init_id);
    }

    // Set as current thread
    CURRENT_THREAD.store(init_id.0, Ordering::SeqCst);

    // Enter scheduling loop
    schedule_loop()
}

/// Main scheduling loop
fn schedule_loop() -> ! {
    loop {
        let cpu_id = current_cpu_id();

        // Get next thread to run
        let next = {
            let mut per_cpu = PER_CPU.write();
            if let Some(cpu_sched) = per_cpu.get_mut(cpu_id as usize) {
                cpu_sched.pick_next()
            } else {
                None
            }
        };

        match next {
            Some(thread_id) => {
                // Context switch to thread
                switch_to(thread_id);
            }
            None => {
                // No runnable threads, idle
                idle();
            }
        }
    }
}

/// Switch to a specific thread
fn switch_to(next_id: ThreadId) {
    let current_id = ThreadId(CURRENT_THREAD.load(Ordering::SeqCst));

    if current_id == next_id {
        return; // Already running
    }

    // Save current thread state and restore next thread state
    let switch_info = {
        let mut threads = THREADS.write();

        // Save current thread's state
        if let Some(current) = threads.get_mut(&current_id) {
            current.state = ThreadState::Ready;
        }

        // Get next thread's state
        let next = threads.get_mut(&next_id);
        if let Some(next) = next {
            next.state = ThreadState::Running;
            let regs = next.registers;
            let page_table_root = next.address_space.page_table_root();
            Some((regs, page_table_root))
        } else {
            None
        }
    };

    // Update current thread and perform context switch
    if let Some((next_regs, page_table_root)) = switch_info {
        CURRENT_THREAD.store(next_id.0, Ordering::SeqCst);

        // Perform actual context switch with address space switch
        // SAFETY: next_regs is valid, page_table_root points to valid page tables
        unsafe {
            context_switch(&next_regs, page_table_root);
        }
    }
}

/// Perform context switch (restore registers and jump)
///
/// # Safety
///
/// - `regs` must point to a valid RegisterState
/// - `page_table_root` must be a valid physical address of a page table
/// - Must be called from kernel mode
unsafe fn context_switch(regs: &RegisterState, page_table_root: crate::mem::PhysAddr) {
    // Switch address space by loading new CR3
    // This is essential for process isolation - each process has its own
    // page tables and must see its own memory view
    let cr3_value = page_table_root.as_u64();

    // Restore registers and return to thread
    // SAFETY: regs pointer is valid, we're in kernel mode, page_table_root is valid
    unsafe {
        asm!(
            // Switch address space - load new CR3
            // This invalidates TLB entries for the old address space
            "mov cr3, {cr3}",
            // Restore general purpose registers
            "mov r15, [{regs} + 0x00]",
            "mov r14, [{regs} + 0x08]",
            "mov r13, [{regs} + 0x10]",
            "mov r12, [{regs} + 0x18]",
            "mov r11, [{regs} + 0x20]",
            "mov r10, [{regs} + 0x28]",
            "mov r9,  [{regs} + 0x30]",
            "mov r8,  [{regs} + 0x38]",
            "mov rbp, [{regs} + 0x40]",
            "mov rdi, [{regs} + 0x48]",
            "mov rsi, [{regs} + 0x50]",
            "mov rdx, [{regs} + 0x58]",
            "mov rcx, [{regs} + 0x60]",
            "mov rbx, [{regs} + 0x68]",
            // rax is last since we need it for the jump
            "mov rsp, [{regs} + 0x78]",  // RSP
            // Set up iret frame
            "push [{regs} + 0x98]",      // SS
            "push [{regs} + 0x78]",      // RSP
            "push [{regs} + 0x88]",      // RFLAGS
            "push [{regs} + 0x90]",      // CS
            "push [{regs} + 0x80]",      // RIP
            "mov rax, [{regs} + 0x70]",  // RAX
            "iretq",
            cr3 = in(reg) cr3_value,
            regs = in(reg) regs as *const RegisterState,
            options(noreturn)
        );
    }
}

/// Run a thread until it yields or is preempted
fn run_thread(_thread_id: ThreadId) {
    // This is handled by context_switch now
}

/// Get current CPU ID
pub fn current_cpu_id() -> u32 {
    crate::arch::x86_64::smp::current_cpu_id()
}

/// Yield current thread
pub fn yield_now() {
    NEED_RESCHED.store(true, Ordering::SeqCst);

    let current_id = ThreadId(CURRENT_THREAD.load(Ordering::SeqCst));
    let cpu_id = current_cpu_id();

    // Re-enqueue current thread
    {
        let mut per_cpu = PER_CPU.write();
        if let Some(cpu_sched) = per_cpu.get_mut(cpu_id as usize) {
            cpu_sched.enqueue(current_id);
        }
    }

    // Trigger reschedule
    schedule();
}

/// Sleep current thread for duration
pub fn sleep(duration: core::time::Duration) {
    let current_id = ThreadId(CURRENT_THREAD.load(Ordering::SeqCst));
    let wake_tick = TICK_COUNT.load(Ordering::SeqCst) + (duration.as_millis() as u64 / 10); // Assuming 100Hz timer

    // Mark thread as sleeping
    {
        let mut threads = THREADS.write();
        if let Some(thread) = threads.get_mut(&current_id) {
            thread.state = ThreadState::Blocked(BlockReason::Sleep);
            thread.wake_tick = wake_tick;
        }
    }

    // Add to timer queue
    {
        let cpu_id = current_cpu_id();
        let mut per_cpu = PER_CPU.write();
        if let Some(cpu_sched) = per_cpu.get_mut(cpu_id as usize) {
            cpu_sched.add_to_timer_queue(current_id, wake_tick);
        }
    }

    // Trigger reschedule
    schedule();
}

/// Timer tick handler (called from IRQ)
pub fn timer_tick() {
    let tick = TICK_COUNT.fetch_add(1, Ordering::SeqCst) + 1;

    // Check for threads to wake
    {
        let cpu_id = current_cpu_id();
        let mut per_cpu = PER_CPU.write();
        if let Some(cpu_sched) = per_cpu.get_mut(cpu_id as usize) {
            let woken = cpu_sched.check_timer_queue(tick);

            // Wake threads
            let mut threads = THREADS.write();
            for thread_id in woken {
                if let Some(thread) = threads.get_mut(&thread_id) {
                    thread.state = ThreadState::Ready;
                }
                cpu_sched.enqueue(thread_id);
            }
        }
    }

    // Check if current thread's time slice expired
    let current_id = ThreadId(CURRENT_THREAD.load(Ordering::SeqCst));
    {
        let threads = THREADS.read();
        if let Some(_thread) = threads.get(&current_id) {
            if tick % TIME_SLICE_TICKS == 0 {
                NEED_RESCHED.store(true, Ordering::SeqCst);
            }
        }
    }

    // Periodic load balancing (only on CPU 0 to avoid thundering herd)
    if current_cpu_id() == 0 {
        periodic_load_balance(tick);
    }
}

/// Trigger reschedule (called from various places)
pub fn schedule() {
    if !NEED_RESCHED.swap(false, Ordering::SeqCst) {
        return;
    }

    let cpu_id = current_cpu_id();
    let current_id = ThreadId(CURRENT_THREAD.load(Ordering::SeqCst));

    // Get next thread
    let next = {
        let mut per_cpu = PER_CPU.write();
        if let Some(cpu_sched) = per_cpu.get_mut(cpu_id as usize) {
            cpu_sched.pick_next()
        } else {
            None
        }
    };

    if let Some(next_id) = next {
        if next_id != current_id {
            switch_to(next_id);
        }
    }
}

/// Block current thread
pub fn block(reason: BlockReason) {
    let current_id = ThreadId(CURRENT_THREAD.load(Ordering::SeqCst));

    {
        let mut threads = THREADS.write();
        if let Some(thread) = threads.get_mut(&current_id) {
            thread.state = ThreadState::Blocked(reason);
        }
    }

    NEED_RESCHED.store(true, Ordering::SeqCst);
    schedule();
}

/// Wake a blocked thread
pub fn wake(thread_id: ThreadId) {
    let cpu_id = current_cpu_id();

    {
        let mut threads = THREADS.write();
        if let Some(thread) = threads.get_mut(&thread_id) {
            if matches!(thread.state, ThreadState::Blocked(_)) {
                thread.state = ThreadState::Ready;
            } else {
                return; // Already ready or terminated
            }
        } else {
            return;
        }
    }

    // Add to run queue
    {
        let mut per_cpu = PER_CPU.write();
        if let Some(cpu_sched) = per_cpu.get_mut(cpu_id as usize) {
            cpu_sched.enqueue(thread_id);
        }
    }
}

/// Get current thread ID
pub fn current_thread_id() -> ThreadId {
    ThreadId(CURRENT_THREAD.load(Ordering::SeqCst))
}

/// Get current tick count
pub fn get_tick_count() -> u64 {
    TICK_COUNT.load(Ordering::SeqCst)
}

/// Idle when no threads are runnable
fn idle() {
    // Try to steal work from other CPUs before going idle
    if let Some(thread_id) = try_steal_work() {
        // Found work, enqueue it locally and reschedule
        let cpu_id = current_cpu_id() as usize;
        {
            let mut per_cpu = PER_CPU.write();
            if let Some(cpu_sched) = per_cpu.get_mut(cpu_id) {
                cpu_sched.enqueue(thread_id);
            }
        }
        NEED_RESCHED.store(true, Ordering::SeqCst);
        return;
    }

    // No work available, halt until next interrupt
    unsafe {
        asm!("hlt", options(nomem, nostack));
    }
}

/// Timer queue entry for sleeping threads
///
/// Implements Ord to work with BinaryHeap - note that BinaryHeap is a max-heap,
/// so we reverse the comparison to get min-heap behavior (earliest wake time first).
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
struct TimerEntry {
    thread_id: ThreadId,
    wake_tick: u64,
}

impl Ord for TimerEntry {
    fn cmp(&self, other: &Self) -> CmpOrdering {
        // Reverse comparison for min-heap behavior (earliest first)
        other
            .wake_tick
            .cmp(&self.wake_tick)
            .then_with(|| other.thread_id.0.cmp(&self.thread_id.0))
    }
}

impl PartialOrd for TimerEntry {
    fn partial_cmp(&self, other: &Self) -> Option<CmpOrdering> {
        Some(self.cmp(other))
    }
}

/// Per-CPU scheduler state
pub struct CpuScheduler {
    cpu_id: u32,
    /// Currently running thread
    current: Option<ThreadId>,
    /// CFS run queue (normal priority)
    cfs_queue: cfs::CfsQueue,
    /// Deadline queue (real-time)
    deadline_queue: deadline::DeadlineQueue,
    /// Idle thread ID
    idle_thread: Option<ThreadId>,
    /// Timer queue for sleeping threads (min-heap by wake_tick)
    timer_queue: BinaryHeap<TimerEntry>,
}

impl CpuScheduler {
    fn new(cpu_id: u32) -> Self {
        Self {
            cpu_id,
            current: None,
            cfs_queue: cfs::CfsQueue::new(),
            deadline_queue: deadline::DeadlineQueue::new(),
            idle_thread: None,
            timer_queue: BinaryHeap::new(),
        }
    }

    pub fn enqueue(&mut self, thread_id: ThreadId) {
        // Check thread scheduling class
        let threads = THREADS.read();
        if let Some(thread) = threads.get(&thread_id) {
            match thread.sched_class {
                SchedClass::Deadline => {
                    // For deadline tasks, create a default entry
                    // Real implementation would get deadline params from thread
                    let entry = deadline::DeadlineEntry {
                        thread_id,
                        deadline: 0, // Would be set based on thread params
                        runtime_remaining: 0,
                        period: 0,
                    };
                    self.deadline_queue.enqueue(entry);
                }
                SchedClass::RtFifo | SchedClass::RtRr => self.cfs_queue.enqueue(thread_id), // Use CFS for now
                _ => self.cfs_queue.enqueue(thread_id),
            }
        } else {
            self.cfs_queue.enqueue(thread_id);
        }
    }

    fn pick_next(&mut self) -> Option<ThreadId> {
        // 1. Check deadline tasks first
        if let Some(dl) = self.deadline_queue.pick_next() {
            self.current = Some(dl);
            return Some(dl);
        }

        // 2. CFS queue
        if let Some(cfs) = self.cfs_queue.pick_next() {
            self.current = Some(cfs);
            return Some(cfs);
        }

        // 3. Idle thread
        self.current = self.idle_thread;
        self.idle_thread
    }

    /// Add a thread to the timer queue
    ///
    /// Complexity: O(log n) - much better than O(n log n) with Vec + sort
    pub fn add_to_timer_queue(&mut self, thread_id: ThreadId, wake_tick: u64) {
        self.timer_queue.push(TimerEntry {
            thread_id,
            wake_tick,
        });
    }

    /// Check timer queue and return threads that should be woken
    ///
    /// Complexity: O(k log n) where k is the number of expired timers
    /// This is much better than O(n) with Vec::remove(0)
    fn check_timer_queue(&mut self, current_tick: u64) -> alloc::vec::Vec<ThreadId> {
        let mut woken = alloc::vec::Vec::new();

        // Pop all expired timers (BinaryHeap with reversed Ord gives us min-heap)
        while let Some(entry) = self.timer_queue.peek() {
            if entry.wake_tick <= current_tick {
                // Timer expired, remove and add to woken list
                let entry = self.timer_queue.pop().unwrap();
                woken.push(entry.thread_id);
            } else {
                // No more expired timers (they're in sorted order)
                break;
            }
        }

        woken
    }

    /// Get queue length for load balancing
    pub fn queue_len(&self) -> usize {
        self.cfs_queue.len() + if self.deadline_queue.is_empty() { 0 } else { 1 }
    }

    /// Steal a thread from this CPU (for work stealing)
    pub fn steal_thread(&mut self) -> Option<ThreadId> {
        // Only steal from CFS queue (don't touch deadline tasks)
        self.cfs_queue.pick_next()
    }

    /// Check if this CPU is idle
    pub fn is_idle(&self) -> bool {
        self.cfs_queue.is_empty() && self.deadline_queue.is_empty()
    }

    /// Get CPU ID
    pub fn cpu_id(&self) -> u32 {
        self.cpu_id
    }
}

/// Scheduling class
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SchedClass {
    /// Real-time deadline scheduling
    Deadline,
    /// Real-time FIFO
    RtFifo,
    /// Real-time round-robin
    RtRr,
    /// Normal CFS scheduling
    Normal,
    /// Batch processing (lower priority)
    Batch,
    /// Idle (lowest priority)
    Idle,
}

// ============================================================================
// Multi-core Load Balancing
// ============================================================================

/// Load balancing interval in timer ticks
const LOAD_BALANCE_INTERVAL: u64 = 100;

/// Load imbalance threshold for triggering migration
const LOAD_IMBALANCE_THRESHOLD: usize = 2;

/// Perform load balancing across CPUs
///
/// This function atomically steals work from the busiest CPU and migrates it
/// to the idlest CPU using split borrows to avoid dropping and re-acquiring
/// references (which could cause TOCTOU races if we weren't holding the lock).
pub fn load_balance() {
    let cpu_count = crate::arch::x86_64::smp::cpu_count() as usize;
    if cpu_count <= 1 {
        return; // No balancing needed for single CPU
    }

    let mut per_cpu = PER_CPU.write();

    // Can't balance if we don't have enough CPUs
    if per_cpu.len() < 2 {
        return;
    }

    // Calculate load for each CPU and find busiest/idlest in single pass
    let mut busiest_cpu = 0;
    let mut busiest_load = 0usize;
    let mut idlest_cpu = 0;
    let mut idlest_load = usize::MAX;

    for (i, sched) in per_cpu.iter().enumerate() {
        let load = sched.queue_len();
        if load > busiest_load {
            busiest_load = load;
            busiest_cpu = i;
        }
        if load < idlest_load {
            idlest_load = load;
            idlest_cpu = i;
        }
    }

    // Check if imbalance warrants migration
    if busiest_cpu == idlest_cpu {
        return; // Same CPU, nothing to do
    }

    if busiest_load.saturating_sub(idlest_load) < LOAD_IMBALANCE_THRESHOLD {
        return; // Load is balanced enough
    }

    // Use split_at_mut to get simultaneous mutable access to both CPUs
    // This avoids the TOCTOU issue by ensuring atomic steal+enqueue
    let thread_to_migrate = if busiest_cpu < idlest_cpu {
        let (left, right) = per_cpu.split_at_mut(idlest_cpu);
        let busiest_sched = &mut left[busiest_cpu];
        let idlest_sched = &mut right[0];

        if let Some(thread_id) = busiest_sched.steal_thread() {
            idlest_sched.enqueue(thread_id);
            Some((thread_id, busiest_cpu, idlest_cpu))
        } else {
            None
        }
    } else {
        let (left, right) = per_cpu.split_at_mut(busiest_cpu);
        let idlest_sched = &mut left[idlest_cpu];
        let busiest_sched = &mut right[0];

        if let Some(thread_id) = busiest_sched.steal_thread() {
            idlest_sched.enqueue(thread_id);
            Some((thread_id, busiest_cpu, idlest_cpu))
        } else {
            None
        }
    };

    // Log migration and send IPI outside the borrow scope
    if let Some((thread_id, from_cpu, to_cpu)) = thread_to_migrate {
        log::trace!(
            "Migrated thread {:?} from CPU {} to CPU {}",
            thread_id,
            from_cpu,
            to_cpu
        );

        // Send IPI to wake idle CPU if it was idle
        if idlest_load == 0 {
            // Drop lock before sending IPI to avoid deadlock
            drop(per_cpu);
            crate::arch::x86_64::smp::send_ipi_to(to_cpu as u32, RESCHEDULE_IPI_VECTOR);
        }
    }
}

/// Reschedule IPI vector number
const RESCHEDULE_IPI_VECTOR: u8 = 0xFE;

/// Handle reschedule IPI (called from interrupt handler)
pub fn handle_reschedule_ipi() {
    NEED_RESCHED.store(true, Ordering::SeqCst);
}

/// Try to find work on other CPUs (work stealing from idle CPU)
pub fn try_steal_work() -> Option<ThreadId> {
    let current_cpu = current_cpu_id() as usize;

    let mut per_cpu = PER_CPU.write();
    let cpu_count = per_cpu.len();

    // Try to steal from other CPUs in round-robin fashion
    for offset in 1..cpu_count {
        let target_cpu = (current_cpu + offset) % cpu_count;
        if let Some(target_sched) = per_cpu.get_mut(target_cpu) {
            if target_sched.queue_len() > 1 {
                if let Some(thread_id) = target_sched.steal_thread() {
                    log::trace!(
                        "CPU {} stole thread {:?} from CPU {}",
                        current_cpu,
                        thread_id,
                        target_cpu
                    );
                    return Some(thread_id);
                }
            }
        }
    }

    None
}

/// Periodic load balancing check (called from timer tick)
pub fn periodic_load_balance(tick: u64) {
    if tick % LOAD_BALANCE_INTERVAL == 0 {
        load_balance();
    }
}

/// Enqueue thread on least loaded CPU
pub fn enqueue_on_least_loaded(thread_id: ThreadId) {
    let mut per_cpu = PER_CPU.write();

    // Find CPU with lowest load
    let best_cpu = per_cpu
        .iter()
        .enumerate()
        .min_by_key(|(_, sched)| sched.queue_len())
        .map(|(i, _)| i)
        .unwrap_or(0);

    if let Some(sched) = per_cpu.get_mut(best_cpu) {
        sched.enqueue(thread_id);
    }
}
