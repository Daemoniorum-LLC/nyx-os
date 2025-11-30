//! Execution recording and replay
//!
//! Records non-deterministic events during execution to enable
//! deterministic replay for debugging.
//!
//! ## Recorded Events
//!
//! - Syscall arguments and return values
//! - Scheduling decisions (which thread runs when)
//! - Timer interrupts and timestamps
//! - I/O data (reads from devices/files)
//! - Random number generation
//! - Signal delivery
//! - Tensor inference results

use super::{CheckpointId, RecordingId, TimeTravelError};
use crate::process::ProcessId;
use crate::sched::ThreadId;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::Mutex;

/// Recording session configuration
#[derive(Debug, Clone)]
pub struct RecordingConfig {
    /// Record syscall arguments and results
    pub capture_syscalls: bool,
    /// Record memory accesses (expensive)
    pub capture_memory: bool,
    /// Record scheduling decisions
    pub capture_scheduler: bool,
    /// Record tensor operations
    pub capture_tensors: bool,
    /// Maximum number of events
    pub max_events: usize,
    /// Buffer size in bytes
    pub buffer_size: usize,
}

impl Default for RecordingConfig {
    fn default() -> Self {
        Self {
            capture_syscalls: true,
            capture_memory: false,
            capture_scheduler: true,
            capture_tensors: true,
            max_events: 1_000_000,
            buffer_size: 64 * 1024 * 1024,
        }
    }
}

/// Active recording session
#[derive(Debug)]
pub struct RecordingSession {
    /// Recording ID
    pub id: RecordingId,
    /// Process being recorded
    pub process_id: ProcessId,
    /// Configuration
    pub config: RecordingConfig,
    /// Initial checkpoint (start state)
    pub initial_checkpoint: CheckpointId,
    /// Recorded events
    events: Mutex<Vec<RecordEvent>>,
    /// Event counter
    event_count: AtomicU64,
    /// Is recording active
    active: AtomicBool,
    /// Start timestamp
    start_time: u64,
}

impl RecordingSession {
    /// Create a new recording session
    pub fn new(
        id: RecordingId,
        process_id: ProcessId,
        config: RecordingConfig,
    ) -> Result<Self, TimeTravelError> {
        // Create initial checkpoint
        let checkpoint_cap = super::create_checkpoint(
            process_id,
            Some(alloc::format!("recording_{}", id.0)),
            config.capture_tensors,
        )?;

        // Extract checkpoint ID from capability (simplified)
        let initial_checkpoint = CheckpointId(id.0);

        Ok(Self {
            id,
            process_id,
            config,
            initial_checkpoint,
            events: Mutex::new(Vec::with_capacity(1024)),
            event_count: AtomicU64::new(0),
            active: AtomicBool::new(true),
            start_time: crate::now_ns(),
        })
    }

    /// Check if recording is active
    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }

    /// Record an event
    pub fn record(&self, event: RecordEvent) -> Result<(), TimeTravelError> {
        if !self.is_active() {
            return Err(TimeTravelError::NotRecording);
        }

        let count = self.event_count.fetch_add(1, Ordering::SeqCst);
        if count as usize >= self.config.max_events {
            return Err(TimeTravelError::BufferFull);
        }

        let mut events = self.events.lock();
        events.push(event);

        Ok(())
    }

    /// Finalize recording and return trace
    pub fn finalize(&mut self) -> Result<RecordingTrace, TimeTravelError> {
        self.active.store(false, Ordering::SeqCst);

        let events = core::mem::take(&mut *self.events.lock());
        let end_time = crate::now_ns();

        Ok(RecordingTrace {
            recording_id: self.id,
            process_id: self.process_id,
            initial_checkpoint: self.initial_checkpoint,
            events,
            start_time: self.start_time,
            end_time,
            config: self.config.clone(),
        })
    }
}

/// A recorded event
#[derive(Debug, Clone)]
pub struct RecordEvent {
    /// Sequence number
    pub sequence: u64,
    /// Timestamp (nanoseconds since recording start)
    pub timestamp: u64,
    /// Thread that generated the event
    pub thread_id: ThreadId,
    /// Event type and data
    pub kind: RecordEventKind,
}

/// Types of recorded events
#[derive(Debug, Clone)]
pub enum RecordEventKind {
    /// Syscall entry with arguments
    SyscallEntry {
        syscall_num: u64,
        args: [u64; 6],
    },

    /// Syscall exit with return value
    SyscallExit {
        result: i64,
        /// Any data returned (e.g., read buffer contents)
        data: Option<Vec<u8>>,
    },

    /// Thread was scheduled to run
    ThreadScheduled {
        cpu_id: u32,
        previous_thread: Option<ThreadId>,
    },

    /// Thread was preempted
    ThreadPreempted {
        reason: PreemptReason,
    },

    /// Timer interrupt
    TimerTick {
        tick_count: u64,
    },

    /// Signal delivered
    SignalDelivered {
        signal: u32,
        handler: u64,
    },

    /// Random number generated
    RandomValue {
        value: u64,
    },

    /// I/O read completed
    IoRead {
        fd: u32,
        data: Vec<u8>,
    },

    /// Memory access (if capture_memory enabled)
    MemoryAccess {
        address: u64,
        size: u32,
        is_write: bool,
        value: u64,
    },

    /// Tensor operation
    TensorOp {
        op_type: TensorOpType,
        tensor_id: u64,
        result: Option<Vec<u8>>,
    },

    /// IPC message received
    IpcReceive {
        endpoint_id: u64,
        message: Vec<u8>,
    },

    /// Lock acquired
    LockAcquire {
        lock_addr: u64,
    },

    /// Lock released
    LockRelease {
        lock_addr: u64,
    },

    /// Context switch details
    ContextSwitch {
        from_thread: ThreadId,
        to_thread: ThreadId,
        reason: SwitchReason,
    },
}

/// Reason for preemption
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreemptReason {
    /// Timer tick
    TimerExpired,
    /// Higher priority thread ready
    HigherPriority,
    /// Yielded voluntarily
    Yield,
    /// Blocked on I/O or lock
    Blocked,
}

/// Reason for context switch
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwitchReason {
    /// Normal scheduling
    Scheduled,
    /// Thread blocked
    Blocked,
    /// Thread exited
    Exit,
    /// Preempted
    Preempt,
}

/// Type of tensor operation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TensorOpType {
    /// Inference forward pass
    Inference,
    /// Tensor allocation
    Alloc,
    /// Tensor free
    Free,
    /// Data copy
    Copy,
    /// Custom compute kernel
    Compute,
}

/// Complete recording trace
#[derive(Debug)]
pub struct RecordingTrace {
    /// Recording ID
    pub recording_id: RecordingId,
    /// Original process
    pub process_id: ProcessId,
    /// Initial state checkpoint
    pub initial_checkpoint: CheckpointId,
    /// All recorded events
    pub events: Vec<RecordEvent>,
    /// Start timestamp
    pub start_time: u64,
    /// End timestamp
    pub end_time: u64,
    /// Configuration used
    pub config: RecordingConfig,
}

impl RecordingTrace {
    /// Get duration in nanoseconds
    pub fn duration_ns(&self) -> u64 {
        self.end_time - self.start_time
    }

    /// Get event count
    pub fn event_count(&self) -> usize {
        self.events.len()
    }

    /// Serialize to bytes
    pub fn serialize(&self) -> Vec<u8> {
        // Simple serialization format
        let mut data = Vec::new();

        // Header
        data.extend_from_slice(b"NYXREC01"); // Magic + version
        data.extend_from_slice(&self.recording_id.0.to_le_bytes());
        data.extend_from_slice(&self.process_id.0.to_le_bytes());
        data.extend_from_slice(&self.initial_checkpoint.0.to_le_bytes());
        data.extend_from_slice(&self.start_time.to_le_bytes());
        data.extend_from_slice(&self.end_time.to_le_bytes());
        data.extend_from_slice(&(self.events.len() as u64).to_le_bytes());

        // Events would be serialized here
        // (simplified - real impl would use proper serialization)

        data
    }

    /// Deserialize from bytes
    pub fn deserialize(data: &[u8]) -> Result<Self, TimeTravelError> {
        if data.len() < 56 || &data[0..8] != b"NYXREC01" {
            return Err(TimeTravelError::InvalidCheckpoint);
        }

        let recording_id = RecordingId(u64::from_le_bytes(data[8..16].try_into().unwrap()));
        let process_id = ProcessId(u64::from_le_bytes(data[16..24].try_into().unwrap()));
        let initial_checkpoint = CheckpointId(u64::from_le_bytes(data[24..32].try_into().unwrap()));
        let start_time = u64::from_le_bytes(data[32..40].try_into().unwrap());
        let end_time = u64::from_le_bytes(data[40..48].try_into().unwrap());
        let _event_count = u64::from_le_bytes(data[48..56].try_into().unwrap());

        Ok(Self {
            recording_id,
            process_id,
            initial_checkpoint,
            events: Vec::new(), // Would parse events here
            start_time,
            end_time,
            config: RecordingConfig::default(),
        })
    }
}

// ============================================================================
// Replay Engine
// ============================================================================

/// Per-process replay state
static REPLAY_STATE: spin::RwLock<BTreeMap<ProcessId, ReplayState>> =
    spin::RwLock::new(BTreeMap::new());

/// Replay state for a process
#[derive(Debug)]
pub struct ReplayState {
    /// Recording trace being replayed
    trace: RecordingTrace,
    /// Current event index
    event_index: AtomicU64,
    /// Is replay active
    active: AtomicBool,
    /// Divergence detected
    diverged: AtomicBool,
}

/// Set up replay for a process
pub fn setup_replay(pid: ProcessId, trace: RecordingTrace) -> Result<(), TimeTravelError> {
    let state = ReplayState {
        trace,
        event_index: AtomicU64::new(0),
        active: AtomicBool::new(true),
        diverged: AtomicBool::new(false),
    };

    REPLAY_STATE.write().insert(pid, state);
    Ok(())
}

/// Get next expected event during replay
pub fn next_replay_event(pid: ProcessId) -> Option<RecordEvent> {
    let states = REPLAY_STATE.read();
    let state = states.get(&pid)?;

    if !state.active.load(Ordering::SeqCst) {
        return None;
    }

    let index = state.event_index.fetch_add(1, Ordering::SeqCst) as usize;
    state.trace.events.get(index).cloned()
}

/// Check if replay should provide a specific value
pub fn replay_syscall_result(pid: ProcessId, syscall_num: u64) -> Option<(i64, Option<Vec<u8>>)> {
    let states = REPLAY_STATE.read();
    let state = states.get(&pid)?;

    if !state.active.load(Ordering::SeqCst) {
        return None;
    }

    let index = state.event_index.load(Ordering::SeqCst) as usize;

    // Look for matching syscall exit
    for event in state.trace.events.iter().skip(index) {
        if let RecordEventKind::SyscallExit { result, data } = &event.kind {
            return Some((*result, data.clone()));
        }
    }

    None
}

/// Signal replay divergence
pub fn signal_divergence(pid: ProcessId, reason: &str) {
    log::warn!("Replay diverged for process {:?}: {}", pid, reason);

    if let Some(state) = REPLAY_STATE.write().get_mut(&pid) {
        state.diverged.store(true, Ordering::SeqCst);
        state.active.store(false, Ordering::SeqCst);
    }
}

/// Check if process is in replay mode
pub fn is_replaying(pid: ProcessId) -> bool {
    REPLAY_STATE
        .read()
        .get(&pid)
        .is_some_and(|s| s.active.load(Ordering::SeqCst))
}

/// Stop replay for a process
pub fn stop_replay(pid: ProcessId) {
    if let Some(state) = REPLAY_STATE.write().get_mut(&pid) {
        state.active.store(false, Ordering::SeqCst);
    }
}

// ============================================================================
// Recording Hooks (called from various kernel subsystems)
// ============================================================================

/// Record syscall entry
pub fn record_syscall_entry(pid: ProcessId, thread_id: ThreadId, num: u64, args: [u64; 6]) {
    let event = RecordEvent {
        sequence: 0, // Will be set by session
        timestamp: crate::now_ns(),
        thread_id,
        kind: RecordEventKind::SyscallEntry {
            syscall_num: num,
            args,
        },
    };
    let _ = super::record_event(pid, event);
}

/// Record syscall exit
pub fn record_syscall_exit(pid: ProcessId, thread_id: ThreadId, result: i64, data: Option<Vec<u8>>) {
    let event = RecordEvent {
        sequence: 0,
        timestamp: crate::now_ns(),
        thread_id,
        kind: RecordEventKind::SyscallExit { result, data },
    };
    let _ = super::record_event(pid, event);
}

/// Record context switch
pub fn record_context_switch(from: ThreadId, to: ThreadId, reason: SwitchReason) {
    // Get process ID from thread
    let threads = crate::sched::THREADS.read();
    if let Some(thread) = threads.get(&from) {
        let event = RecordEvent {
            sequence: 0,
            timestamp: crate::now_ns(),
            thread_id: from,
            kind: RecordEventKind::ContextSwitch {
                from_thread: from,
                to_thread: to,
                reason,
            },
        };
        // Would need process ID from thread
        // super::record_event(thread.process_id, event);
    }
}

/// Record timer tick
pub fn record_timer_tick(tick_count: u64) {
    // Record for all active recordings
    // (simplified - real impl would iterate recordings)
}

/// Record random value
pub fn record_random(pid: ProcessId, thread_id: ThreadId, value: u64) {
    let event = RecordEvent {
        sequence: 0,
        timestamp: crate::now_ns(),
        thread_id,
        kind: RecordEventKind::RandomValue { value },
    };
    let _ = super::record_event(pid, event);
}

/// Record I/O read
pub fn record_io_read(pid: ProcessId, thread_id: ThreadId, fd: u32, data: Vec<u8>) {
    let event = RecordEvent {
        sequence: 0,
        timestamp: crate::now_ns(),
        thread_id,
        kind: RecordEventKind::IoRead { fd, data },
    };
    let _ = super::record_event(pid, event);
}

/// Record tensor operation
pub fn record_tensor_op(
    pid: ProcessId,
    thread_id: ThreadId,
    op_type: TensorOpType,
    tensor_id: u64,
    result: Option<Vec<u8>>,
) {
    let event = RecordEvent {
        sequence: 0,
        timestamp: crate::now_ns(),
        thread_id,
        kind: RecordEventKind::TensorOp {
            op_type,
            tensor_id,
            result,
        },
    };
    let _ = super::record_event(pid, event);
}
