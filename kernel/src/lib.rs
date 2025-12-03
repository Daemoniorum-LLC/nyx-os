//! # Nyx Microkernel
//!
//! A capability-based microkernel with AI-native syscalls.
//!
//! ## Design Principles
//!
//! - **Zero Ambient Authority**: Pure capability-based security
//! - **Memory Safety**: Rust everywhere except hardware interfaces
//! - **Async-First**: io_uring-style completion queues for all IPC
//! - **AI-Native**: First-class tensor operations and inference syscalls
//! - **Rigorously Tested**: Comprehensive test suite with property-based testing
//!
//! ## Lock Ordering
//!
//! To prevent deadlocks, locks must be acquired in the following order.
//! Acquiring locks out of order is a bug and may cause deadlocks.
//!
//! ```text
//! Lock Hierarchy (acquire in this order, never reverse):
//!
//! Level 0 (outermost - acquire first):
//!   - PROCESSES         (process table)
//!   - TENSORS           (tensor buffer registry)
//!
//! Level 1:
//!   - THREADS           (thread table)
//!   - ENDPOINTS         (IPC endpoint registry)
//!   - RINGS             (IPC ring registry)
//!
//! Level 2:
//!   - PER_CPU           (per-CPU scheduler state)
//!   - NOTIFICATIONS     (notification registry)
//!
//! Level 3 (innermost - acquire last):
//!   - Individual Process.address_space
//!   - Individual Thread fields
//! ```
//!
//! ### Rules
//!
//! 1. Never hold a lower-level lock while acquiring a higher-level lock
//! 2. Prefer read locks over write locks when possible
//! 3. Hold locks for the minimum duration necessary
//! 4. When acquiring multiple locks at the same level, use a consistent ordering
//!    (e.g., by ProcessId or ThreadId to avoid ABBA deadlocks)

#![cfg_attr(not(test), no_std)]
#![cfg_attr(not(test), no_main)]
#![feature(
    abi_x86_interrupt,
    allocator_api,
    naked_functions,
    asm_const,
    const_mut_refs,
    inline_const,
    slice_ptr_get,
    strict_provenance,
)]
#![deny(unsafe_op_in_unsafe_fn)]
#![allow(missing_docs)]

#[cfg(not(test))]
extern crate alloc;
#[cfg(test)]
extern crate std as alloc;

pub mod arch;
pub mod cap;
pub mod signal;
pub mod sync;
pub mod traits;

// Test-compatible modules (data structures and pure logic)
#[cfg(test)]
pub mod mem {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
    pub struct PhysAddr(pub u64);
    impl PhysAddr {
        pub const fn new(addr: u64) -> Self { Self(addr) }
        pub const fn as_u64(self) -> u64 { self.0 }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
    pub struct VirtAddr(pub u64);
    impl VirtAddr {
        pub const fn new(addr: u64) -> Self { Self(addr) }
        pub const fn as_u64(self) -> u64 { self.0 }
    }

    pub const PAGE_SIZE: u64 = 4096;

    pub mod virt {
        bitflags::bitflags! {
            #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
            pub struct Protection: u8 {
                const READ = 1 << 0;
                const WRITE = 1 << 1;
                const EXECUTE = 1 << 2;
                const USER = 1 << 3;
            }
        }
    }

    pub fn phys_to_virt(phys: PhysAddr) -> u64 {
        phys.as_u64() + 0xFFFF_8000_0000_0000
    }
}

// Full modules only in non-test builds
#[cfg(not(test))]
pub mod driver;
#[cfg(not(test))]
pub mod fs;
#[cfg(not(test))]
pub mod ipc;
#[cfg(not(test))]
pub mod mem;
#[cfg(not(test))]
pub mod net;
#[cfg(not(test))]
pub mod process;
#[cfg(not(test))]
pub mod sched;
#[cfg(not(test))]
pub mod tensor;
#[cfg(not(test))]
pub mod timetravel;

#[cfg(not(test))]
mod panic;
#[cfg(not(test))]
mod syscall;

// Test-only stubs for gated modules
#[cfg(test)]
pub mod process {
    use core::sync::atomic::{AtomicU64, Ordering};

    static NEXT_PID: AtomicU64 = AtomicU64::new(1);

    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
    pub struct ProcessId(pub u64);

    impl ProcessId {
        pub fn new() -> Self {
            Self(NEXT_PID.fetch_add(1, Ordering::Relaxed))
        }

        pub fn from_raw(val: u64) -> Self {
            Self(val)
        }

        pub fn raw(&self) -> u64 { self.0 }
    }

    impl Default for ProcessId {
        fn default() -> Self {
            Self::new()
        }
    }

    pub fn current_pid() -> Option<ProcessId> {
        Some(ProcessId(1))
    }

    pub fn current_process_id() -> Option<ProcessId> {
        current_pid()
    }

    pub fn terminate(_pid: ProcessId, _exit_code: i32) {}
    pub fn stop(_pid: ProcessId) {}
    pub fn resume(_pid: ProcessId) {}
}

#[cfg(test)]
pub mod sched {
    use core::time::Duration;

    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
    pub struct ThreadId(pub u64);

    impl ThreadId {
        pub fn new(id: u64) -> Self { Self(id) }
        pub fn raw(&self) -> u64 { self.0 }
    }

    impl Default for ThreadId {
        fn default() -> Self {
            Self(1)
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum BlockReason {
        Sleep,
        IpcReceive,
        IpcSend,
        Mutex,
        Semaphore,
        Futex,
        Signal,
        Join(ThreadId),
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum ThreadState {
        Ready,
        Running,
        Blocked(BlockReason),
        Terminated,
    }

    pub fn timer_tick() {}
    pub fn wake(_tid: ThreadId) {}
    pub fn block(_reason: BlockReason) {}
    pub fn current_thread_id() -> ThreadId { ThreadId(1) }
    pub fn yield_now() {}
    pub fn sleep(_duration: Duration) {}
}

use core::sync::atomic::{AtomicU64, Ordering};

/// Kernel version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Build timestamp
pub const BUILD_TIME: &str = match option_env!("BUILD_TIMESTAMP") {
    Some(t) => t,
    None => "unknown",
};

/// Global tick counter (nanoseconds since boot)
static TICK_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Get current timestamp in nanoseconds
#[inline]
pub fn now_ns() -> u64 {
    TICK_COUNTER.load(Ordering::Relaxed)
}

/// Kernel entry point (called from arch-specific boot code)
///
/// # Safety
///
/// Must only be called once during boot, after arch-specific initialization.
#[cfg(not(test))]
pub unsafe fn kernel_main(boot_info: &arch::BootInfo) -> ! {
    log::info!("Nyx Kernel v{VERSION} starting...");

    // Phase 1: Memory initialization
    log::debug!("Initializing memory subsystem");
    mem::init(boot_info);

    // Phase 2: Capability system initialization
    log::debug!("Initializing capability system");
    cap::init();

    // Phase 3: IPC subsystem initialization
    log::debug!("Initializing IPC subsystem");
    ipc::init();

    // Phase 4: Filesystem initialization
    log::debug!("Initializing filesystem subsystem");
    fs::init();

    // Phase 5: Process subsystem initialization
    log::debug!("Initializing process subsystem");
    process::init();

    // Phase 6: Scheduler initialization
    log::debug!("Initializing scheduler");
    sched::init(boot_info);

    // Phase 7: Tensor runtime initialization (if hardware available)
    if tensor::has_accelerator() {
        log::debug!("Initializing tensor runtime");
        tensor::init();
    }

    // Phase 8: Time-travel subsystem
    log::debug!("Initializing time-travel subsystem");
    timetravel::init();

    // Phase 9: Device driver framework
    log::debug!("Initializing device driver framework");
    driver::init();

    // Phase 10: Network stack
    log::debug!("Initializing network stack");
    net::init();

    // Phase 11: Signal subsystem
    log::debug!("Initializing signal subsystem");
    signal::init();

    // Phase 12: Load initrd
    if let Some(initrd) = boot_info.initrd {
        log::info!("Loading initrd ({} bytes)", initrd.len());
        let initrd_phys = mem::PhysAddr::new(initrd.as_ptr() as u64);
        fs::init_initrd(initrd_phys, initrd.len());
    }

    // Phase 13: Start secondary CPUs
    log::debug!("Starting secondary CPUs");
    arch::start_secondary_cpus();

    // Phase 14: Load init process
    log::info!("Loading init process");
    let init_cap = load_init_process(boot_info);

    // Phase 15: Start scheduler - never returns
    log::info!("Starting scheduler");
    sched::start(init_cap)
}

/// Load the init process from initrd
#[cfg(not(test))]
fn load_init_process(_boot_info: &arch::BootInfo) -> cap::Capability {
    // Try to spawn /init or /sbin/init
    let init_paths = ["/init", "/sbin/init", "/bin/init"];

    for path in &init_paths {
        if fs::exists(path) {
            log::info!("Found init at {}", path);

            let args = process::SpawnArgs {
                path: alloc::string::String::from(*path),
                args: alloc::vec![alloc::string::String::from(*path)],
                env: alloc::vec![
                    (alloc::string::String::from("PATH"), alloc::string::String::from("/bin:/sbin:/usr/bin")),
                ],
                caps: alloc::vec![],
                sched_class: sched::SchedClass::Normal,
                priority: 0,
                cwd: Some(alloc::string::String::from("/")),
                uid: 0,
                gid: 0,
            };

            match process::spawn(args) {
                Ok(pid) => {
                    log::info!("Init process spawned with PID {}", pid.raw());
                    // Return a capability for the init process
                    return unsafe {
                        cap::Capability::new_unchecked(
                            cap::ObjectId::new(cap::ObjectType::Process),
                            cap::Rights::PROCESS_FULL,
                        )
                    };
                }
                Err(e) => {
                    log::error!("Failed to spawn init: {:?}", e);
                }
            }
        }
    }

    // No init found - create a minimal kernel thread
    log::warn!("No init binary found, starting kernel shell");

    // Create a dummy capability for scheduler to work with
    unsafe {
        cap::Capability::new_unchecked(
            cap::ObjectId::new(cap::ObjectType::Process),
            cap::Rights::PROCESS_FULL,
        )
    }
}
