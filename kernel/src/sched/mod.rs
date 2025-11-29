//! Scheduler subsystem
//!
//! ## Features
//!
//! - Heterogeneous scheduling (CPU + GPU + NPU)
//! - CFS-style fair scheduling for normal tasks
//! - Real-time deadline scheduling (SCHED_DEADLINE)
//! - Energy-aware scheduling (big.LITTLE / P-core/E-core)
//! - Priority inheritance for mutex holders

mod thread;
mod cfs;
mod deadline;
mod energy;

pub use thread::{Thread, ThreadId, ThreadState};

use crate::arch::BootInfo;
use crate::cap::Capability;
use alloc::collections::BTreeMap;
use spin::RwLock;

/// Global thread registry
static THREADS: RwLock<BTreeMap<ThreadId, Thread>> = RwLock::new(BTreeMap::new());

/// Per-CPU scheduler state
static PER_CPU: RwLock<alloc::vec::Vec<CpuScheduler>> = RwLock::new(alloc::vec::Vec::new());

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
                run_thread(thread_id);
            }
            None => {
                // No runnable threads, idle
                crate::arch::halt();
            }
        }
    }
}

/// Run a thread until it yields or is preempted
fn run_thread(_thread_id: ThreadId) {
    // TODO: Implement context switch
}

/// Get current CPU ID
fn current_cpu_id() -> u32 {
    // TODO: Read from per-CPU data or APIC ID
    0
}

/// Yield current thread
pub fn yield_now() {
    // TODO: Trigger reschedule
}

/// Sleep for duration
pub fn sleep(_duration: core::time::Duration) {
    // TODO: Add to timer queue
}

/// Per-CPU scheduler state
struct CpuScheduler {
    cpu_id: u32,
    /// Currently running thread
    current: Option<ThreadId>,
    /// CFS run queue (normal priority)
    cfs_queue: cfs::CfsQueue,
    /// Deadline queue (real-time)
    deadline_queue: deadline::DeadlineQueue,
    /// Idle thread ID
    idle_thread: Option<ThreadId>,
}

impl CpuScheduler {
    fn new(cpu_id: u32) -> Self {
        Self {
            cpu_id,
            current: None,
            cfs_queue: cfs::CfsQueue::new(),
            deadline_queue: deadline::DeadlineQueue::new(),
            idle_thread: None,
        }
    }

    fn enqueue(&mut self, thread_id: ThreadId) {
        // TODO: Check thread scheduling class
        self.cfs_queue.enqueue(thread_id);
    }

    fn pick_next(&mut self) -> Option<ThreadId> {
        // 1. Check deadline tasks first
        if let Some(dl) = self.deadline_queue.pick_next() {
            return Some(dl);
        }

        // 2. CFS queue
        if let Some(cfs) = self.cfs_queue.pick_next() {
            return Some(cfs);
        }

        // 3. Idle thread
        self.idle_thread
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
