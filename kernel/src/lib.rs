//! # Nyx Microkernel
//!
//! A formally-verified, capability-based microkernel with AI-native syscalls.
//!
//! ## Design Principles
//!
//! - **Zero Ambient Authority**: Pure capability-based security
//! - **Memory Safety**: Rust everywhere except hardware interfaces
//! - **Async-First**: io_uring-style completion queues for all IPC
//! - **AI-Native**: First-class tensor operations and inference syscalls
//! - **Formally Verified**: Core components proven in Lean 4

#![no_std]
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
#![warn(missing_docs, rust_2024_compatibility)]

extern crate alloc;

pub mod arch;
pub mod cap;
pub mod fs;
pub mod ipc;
pub mod mem;
pub mod process;
pub mod sched;
pub mod tensor;

#[cfg(feature = "time-travel")]
pub mod timetravel;

mod panic;
mod syscall;

use core::sync::atomic::{AtomicU64, Ordering};

/// Kernel version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Build timestamp
pub const BUILD_TIME: &str = env!("BUILD_TIMESTAMP", "unknown");

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

    // Phase 8: Time-travel subsystem (if enabled)
    #[cfg(feature = "time-travel")]
    {
        log::debug!("Initializing time-travel subsystem");
        timetravel::init();
    }

    // Phase 9: Load initrd
    if let Some(initrd) = boot_info.initrd {
        log::info!("Loading initrd ({} bytes)", initrd.len());
        let initrd_phys = mem::PhysAddr::new(initrd.as_ptr() as u64);
        fs::init_initrd(initrd_phys, initrd.len());
    }

    // Phase 10: Start secondary CPUs
    log::debug!("Starting secondary CPUs");
    arch::start_secondary_cpus();

    // Phase 11: Load init process
    log::info!("Loading init process");
    let init_cap = load_init_process(boot_info);

    // Phase 12: Start scheduler - never returns
    log::info!("Starting scheduler");
    sched::start(init_cap)
}

/// Load the init process from initrd
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
