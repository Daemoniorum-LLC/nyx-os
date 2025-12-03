//! # Kernel Abstraction Traits
//!
//! This module provides traits for kernel services that can be mocked during testing.
//! Using trait-based dependency injection allows for proper unit testing without
//! requiring the full kernel infrastructure.
//!
//! ## Testing Strategy
//!
//! In production, the real implementations are used. During testing, mock
//! implementations can be injected to control behavior and verify interactions.

use crate::sched::ThreadId;
use core::time::Duration;

// Re-export IDs that are needed across both test and production
#[cfg(test)]
pub use crate::process::ProcessId;
#[cfg(not(test))]
pub use crate::process::ProcessId;

/// Process management operations
///
/// This trait abstracts process lifecycle management for testability.
pub trait ProcessOps {
    /// Get the current process ID
    fn current_pid(&self) -> Option<ProcessId>;

    /// Terminate a process with an exit code
    fn terminate(&self, pid: ProcessId, exit_code: i32);

    /// Stop (pause) a process
    fn stop(&self, pid: ProcessId);

    /// Resume a stopped process
    fn resume(&self, pid: ProcessId);
}

/// Scheduler operations
///
/// This trait abstracts thread scheduling for testability.
pub trait SchedulerOps {
    /// Get the current thread ID
    fn current_thread(&self) -> ThreadId;

    /// Wake a blocked thread
    fn wake(&self, tid: ThreadId);

    /// Block the current thread
    fn block(&self, reason: BlockReason);

    /// Yield the current thread's timeslice
    fn yield_now(&self);

    /// Sleep for a duration
    fn sleep(&self, duration: Duration);

    /// Process a timer tick
    fn timer_tick(&self);
}

/// Reason for blocking a thread
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockReason {
    /// Sleeping for a duration
    Sleep,
    /// Waiting to receive an IPC message
    IpcReceive,
    /// Waiting to send an IPC message
    IpcSend,
    /// Waiting on a mutex
    Mutex,
    /// Waiting on a semaphore
    Semaphore,
    /// Waiting on a futex
    Futex,
    /// Waiting for a signal
    Signal,
    /// Waiting for another thread to exit
    Join(ThreadId),
}

// ============================================================================
// Production Implementations
// ============================================================================

#[cfg(not(test))]
mod production {
    use super::*;
    use crate::process;
    use crate::sched;

    /// Production process operations
    pub struct RealProcessOps;

    impl ProcessOps for RealProcessOps {
        fn current_pid(&self) -> Option<ProcessId> {
            process::current_process_id()
        }

        fn terminate(&self, pid: ProcessId, exit_code: i32) {
            process::terminate(pid, exit_code);
        }

        fn stop(&self, pid: ProcessId) {
            process::stop(pid);
        }

        fn resume(&self, pid: ProcessId) {
            process::resume(pid);
        }
    }

    /// Production scheduler operations
    pub struct RealSchedulerOps;

    impl SchedulerOps for RealSchedulerOps {
        fn current_thread(&self) -> ThreadId {
            sched::current_thread_id()
        }

        fn wake(&self, tid: ThreadId) {
            sched::wake(tid);
        }

        fn block(&self, reason: BlockReason) {
            let sched_reason = match reason {
                BlockReason::Sleep => sched::BlockReason::Sleep,
                BlockReason::IpcReceive => sched::BlockReason::IpcReceive,
                BlockReason::IpcSend => sched::BlockReason::IpcSend,
                BlockReason::Mutex => sched::BlockReason::Mutex,
                BlockReason::Semaphore => sched::BlockReason::Semaphore,
                BlockReason::Futex => sched::BlockReason::Futex,
                BlockReason::Signal => sched::BlockReason::Signal,
                BlockReason::Join(tid) => sched::BlockReason::Join(tid),
            };
            sched::block(sched_reason);
        }

        fn yield_now(&self) {
            sched::yield_now();
        }

        fn sleep(&self, duration: Duration) {
            sched::sleep(duration);
        }

        fn timer_tick(&self) {
            sched::timer_tick();
        }
    }

    /// Get the default process operations implementation
    pub fn process_ops() -> &'static dyn ProcessOps {
        static OPS: RealProcessOps = RealProcessOps;
        &OPS
    }

    /// Get the default scheduler operations implementation
    pub fn scheduler_ops() -> &'static dyn SchedulerOps {
        static OPS: RealSchedulerOps = RealSchedulerOps;
        &OPS
    }
}

#[cfg(not(test))]
pub use production::{process_ops, scheduler_ops, RealProcessOps, RealSchedulerOps};

// ============================================================================
// Test Mock Implementations
// ============================================================================

#[cfg(test)]
pub mod mock {
    use super::*;
    use alloc::vec::Vec;
    use core::sync::atomic::{AtomicU64, Ordering};
    use spin::Mutex;

    /// Thread-safe mock state for testing
    pub struct MockState {
        /// Recorded terminate calls
        pub terminated: Mutex<Vec<(ProcessId, i32)>>,
        /// Recorded stop calls
        pub stopped: Mutex<Vec<ProcessId>>,
        /// Recorded resume calls
        pub resumed: Mutex<Vec<ProcessId>>,
        /// Recorded wake calls
        pub woken: Mutex<Vec<ThreadId>>,
        /// Recorded block calls
        pub blocks: Mutex<Vec<BlockReason>>,
        /// Current mock PID to return
        pub current_pid: AtomicU64,
        /// Current mock thread ID
        pub current_tid: AtomicU64,
        /// Timer tick count
        pub tick_count: AtomicU64,
    }

    impl MockState {
        pub const fn new() -> Self {
            Self {
                terminated: Mutex::new(Vec::new()),
                stopped: Mutex::new(Vec::new()),
                resumed: Mutex::new(Vec::new()),
                woken: Mutex::new(Vec::new()),
                blocks: Mutex::new(Vec::new()),
                current_pid: AtomicU64::new(1),
                current_tid: AtomicU64::new(1),
                tick_count: AtomicU64::new(0),
            }
        }

        pub fn reset(&self) {
            self.terminated.lock().clear();
            self.stopped.lock().clear();
            self.resumed.lock().clear();
            self.woken.lock().clear();
            self.blocks.lock().clear();
            self.current_pid.store(1, Ordering::Relaxed);
            self.current_tid.store(1, Ordering::Relaxed);
            self.tick_count.store(0, Ordering::Relaxed);
        }
    }

    // Global mock state for tests
    pub static MOCK_STATE: MockState = MockState::new();

    /// Mock process operations for testing
    pub struct MockProcessOps;

    impl ProcessOps for MockProcessOps {
        fn current_pid(&self) -> Option<ProcessId> {
            Some(ProcessId::from_raw(MOCK_STATE.current_pid.load(Ordering::Relaxed)))
        }

        fn terminate(&self, pid: ProcessId, exit_code: i32) {
            MOCK_STATE.terminated.lock().push((pid, exit_code));
        }

        fn stop(&self, pid: ProcessId) {
            MOCK_STATE.stopped.lock().push(pid);
        }

        fn resume(&self, pid: ProcessId) {
            MOCK_STATE.resumed.lock().push(pid);
        }
    }

    /// Mock scheduler operations for testing
    pub struct MockSchedulerOps;

    impl SchedulerOps for MockSchedulerOps {
        fn current_thread(&self) -> ThreadId {
            ThreadId::new(MOCK_STATE.current_tid.load(Ordering::Relaxed))
        }

        fn wake(&self, tid: ThreadId) {
            MOCK_STATE.woken.lock().push(tid);
        }

        fn block(&self, reason: BlockReason) {
            MOCK_STATE.blocks.lock().push(reason);
        }

        fn yield_now(&self) {
            // No-op in tests
        }

        fn sleep(&self, _duration: Duration) {
            MOCK_STATE.blocks.lock().push(BlockReason::Sleep);
        }

        fn timer_tick(&self) {
            MOCK_STATE.tick_count.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Get mock process operations
    pub fn process_ops() -> &'static dyn ProcessOps {
        static OPS: MockProcessOps = MockProcessOps;
        &OPS
    }

    /// Get mock scheduler operations
    pub fn scheduler_ops() -> &'static dyn SchedulerOps {
        static OPS: MockSchedulerOps = MockSchedulerOps;
        &OPS
    }

    /// Reset mock state before each test
    pub fn reset_mocks() {
        MOCK_STATE.reset();
    }

    /// Verify that terminate was called with expected arguments
    pub fn verify_terminated(expected: &[(ProcessId, i32)]) -> bool {
        let actual = MOCK_STATE.terminated.lock();
        actual.as_slice() == expected
    }

    /// Verify that stop was called with expected PIDs
    pub fn verify_stopped(expected: &[ProcessId]) -> bool {
        let actual = MOCK_STATE.stopped.lock();
        actual.as_slice() == expected
    }

    /// Verify that resume was called with expected PIDs
    pub fn verify_resumed(expected: &[ProcessId]) -> bool {
        let actual = MOCK_STATE.resumed.lock();
        actual.as_slice() == expected
    }

    /// Verify wake calls
    pub fn verify_woken(expected: &[ThreadId]) -> bool {
        let actual = MOCK_STATE.woken.lock();
        actual.as_slice() == expected
    }

    /// Get count of timer ticks
    pub fn get_tick_count() -> u64 {
        MOCK_STATE.tick_count.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
pub use mock::{process_ops, scheduler_ops, MockProcessOps, MockSchedulerOps};

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use super::mock::*;

    fn setup() {
        reset_mocks();
    }

    #[test]
    fn test_mock_process_ops() {
        setup();

        let ops = process_ops();

        // Test current_pid
        MOCK_STATE.current_pid.store(42, core::sync::atomic::Ordering::Relaxed);
        assert_eq!(ops.current_pid().map(|p| p.raw()), Some(42));

        // Test terminate
        ops.terminate(ProcessId::from_raw(1), -9);
        ops.terminate(ProcessId::from_raw(2), 0);
        assert!(verify_terminated(&[
            (ProcessId::from_raw(1), -9),
            (ProcessId::from_raw(2), 0),
        ]));

        // Test stop
        ops.stop(ProcessId::from_raw(10));
        assert!(verify_stopped(&[ProcessId::from_raw(10)]));

        // Test resume
        ops.resume(ProcessId::from_raw(10));
        assert!(verify_resumed(&[ProcessId::from_raw(10)]));
    }

    #[test]
    fn test_mock_scheduler_ops() {
        setup();

        let ops = scheduler_ops();

        // Test current_thread
        MOCK_STATE.current_tid.store(100, core::sync::atomic::Ordering::Relaxed);
        assert_eq!(ops.current_thread().raw(), 100);

        // Test wake
        ops.wake(ThreadId::new(5));
        ops.wake(ThreadId::new(10));
        assert!(verify_woken(&[ThreadId::new(5), ThreadId::new(10)]));

        // Test timer_tick
        assert_eq!(get_tick_count(), 0);
        ops.timer_tick();
        ops.timer_tick();
        assert_eq!(get_tick_count(), 2);
    }

    #[test]
    fn test_mock_reset() {
        setup();

        let ops = process_ops();
        ops.terminate(ProcessId::from_raw(1), 0);

        // Verify something was recorded
        assert!(!MOCK_STATE.terminated.borrow().is_empty());

        // Reset and verify cleared
        reset_mocks();
        assert!(MOCK_STATE.terminated.borrow().is_empty());
    }
}
