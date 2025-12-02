//! Thread management
//!
//! Functions for creating and managing threads within a process.

use crate::syscall::{self, nr, Error};

/// Thread ID
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct ThreadId(pub u64);

impl ThreadId {
    /// Create from raw value
    pub const fn from_raw(id: u64) -> Self {
        Self(id)
    }

    /// Get raw value
    pub const fn as_raw(&self) -> u64 {
        self.0
    }
}

/// Thread entry point function type
pub type ThreadEntry = extern "C" fn(arg: u64) -> i32;

/// Create a new thread
///
/// # Arguments
/// * `entry` - Thread entry point
/// * `stack` - Stack pointer (top of stack)
/// * `arg` - Argument passed to entry function
///
/// # Returns
/// The new thread's ID
///
/// # Safety
/// The caller must ensure:
/// - `entry` is a valid function pointer
/// - `stack` points to valid, properly aligned memory
/// - The stack has sufficient size for the thread's execution
///
/// # Example
/// ```no_run
/// extern "C" fn worker(arg: u64) -> i32 {
///     println!("Worker thread started with arg: {}", arg);
///     0
/// }
///
/// // Allocate stack (in real code, use proper allocation)
/// let stack_top = allocate_stack(8192);
/// let tid = thread_create(worker, stack_top, 42)?;
/// ```
pub unsafe fn thread_create(entry: ThreadEntry, stack: u64, arg: u64) -> Result<ThreadId, Error> {
    let result = syscall::syscall3(nr::THREAD_CREATE, entry as u64, stack, arg);
    Error::from_raw(result).map(ThreadId)
}

/// Exit the current thread
///
/// This function does not return.
///
/// # Arguments
/// * `exit_code` - Exit code (returned to join)
pub fn thread_exit(exit_code: i32) -> ! {
    unsafe {
        syscall::syscall1(nr::THREAD_EXIT, exit_code as u64);
    }
    loop {
        core::hint::spin_loop();
    }
}

/// Yield the current thread's timeslice
///
/// Voluntarily gives up the CPU to allow other threads to run.
/// The thread remains runnable and will be rescheduled.
pub fn thread_yield() {
    unsafe {
        syscall::syscall0(nr::THREAD_YIELD);
    }
}

/// Sleep for a duration
///
/// # Arguments
/// * `duration_ns` - Sleep duration in nanoseconds (max 1 hour)
///
/// # Example
/// ```no_run
/// // Sleep for 1 second
/// thread_sleep(1_000_000_000)?;
///
/// // Sleep for 100 milliseconds
/// thread_sleep(100_000_000)?;
/// ```
pub fn thread_sleep(duration_ns: u64) -> Result<(), Error> {
    // Max 1 hour to prevent DoS
    const MAX_SLEEP_NS: u64 = 3600 * 1_000_000_000;
    if duration_ns > MAX_SLEEP_NS {
        return Err(Error::InvalidArgument);
    }

    let result = unsafe { syscall::syscall1(nr::THREAD_SLEEP, duration_ns) };
    Error::from_raw(result).map(|_| ())
}

/// Wait for a thread to exit
///
/// Blocks until the specified thread exits.
///
/// # Arguments
/// * `tid` - Thread ID to wait for
///
/// # Returns
/// The thread's exit code
///
/// # Example
/// ```no_run
/// let tid = thread_create(worker, stack, 0)?;
/// // ... do other work ...
/// let exit_code = thread_join(tid)?;
/// ```
pub fn thread_join(tid: ThreadId) -> Result<i32, Error> {
    let result = unsafe { syscall::syscall1(nr::THREAD_JOIN, tid.0) };
    Error::from_raw(result).map(|code| code as i32)
}

// ============================================================================
// Convenience functions
// ============================================================================

/// Sleep for milliseconds
pub fn sleep_ms(ms: u64) -> Result<(), Error> {
    thread_sleep(ms * 1_000_000)
}

/// Sleep for seconds
pub fn sleep_secs(secs: u64) -> Result<(), Error> {
    thread_sleep(secs * 1_000_000_000)
}
