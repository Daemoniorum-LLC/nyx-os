//! # Time-Travel Debugging Subsystem
//!
//! Provides checkpoint/restore and execution recording/replay capabilities
//! for deterministic debugging of concurrent and AI workloads.
//!
//! ## Features
//!
//! - **Checkpoints**: Full process state snapshots (memory, registers, capabilities)
//! - **Recording**: Execution trace capture for replay debugging
//! - **Replay**: Deterministic re-execution from recorded traces
//! - **Tensor Snapshots**: AI model state preservation
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────┐
//! │                 User Space                       │
//! │  ┌─────────────┐  ┌─────────────┐               │
//! │  │ Debugger UI │  │ AI Trainer  │               │
//! │  └──────┬──────┘  └──────┬──────┘               │
//! ├─────────┼────────────────┼──────────────────────┤
//! │         │    Syscalls    │                      │
//! │  ┌──────┴────────────────┴──────┐               │
//! │  │     Time-Travel Subsystem     │               │
//! │  ├───────────────┬───────────────┤               │
//! │  │  Checkpoint   │   Recording   │               │
//! │  │   Manager     │    Engine     │               │
//! │  ├───────────────┼───────────────┤               │
//! │  │     Memory Snapshot Layer     │               │
//! │  └───────────────────────────────┘               │
//! └─────────────────────────────────────────────────┘
//! ```

pub mod checkpoint;
pub mod record;

use crate::cap::{Capability, CapError, ObjectId, ObjectType, Rights};
use crate::process::ProcessId;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::RwLock;

/// Global checkpoint registry
static CHECKPOINTS: RwLock<BTreeMap<CheckpointId, checkpoint::Checkpoint>> =
    RwLock::new(BTreeMap::new());

/// Global recording sessions
static RECORDINGS: RwLock<BTreeMap<RecordingId, record::RecordingSession>> =
    RwLock::new(BTreeMap::new());

/// Next checkpoint ID
static NEXT_CHECKPOINT_ID: AtomicU64 = AtomicU64::new(1);

/// Next recording ID
static NEXT_RECORDING_ID: AtomicU64 = AtomicU64::new(1);

/// Checkpoint identifier
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CheckpointId(pub u64);

impl CheckpointId {
    fn new() -> Self {
        Self(NEXT_CHECKPOINT_ID.fetch_add(1, Ordering::SeqCst))
    }
}

/// Recording session identifier
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RecordingId(pub u64);

impl RecordingId {
    fn new() -> Self {
        Self(NEXT_RECORDING_ID.fetch_add(1, Ordering::SeqCst))
    }
}

/// Time-travel error types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimeTravelError {
    /// Checkpoint not found
    CheckpointNotFound,
    /// Recording not found
    RecordingNotFound,
    /// Out of memory for snapshot
    OutOfMemory,
    /// Process not found
    ProcessNotFound,
    /// Recording already in progress
    AlreadyRecording,
    /// Not currently recording
    NotRecording,
    /// Replay diverged from recording
    ReplayDiverged,
    /// Invalid checkpoint data
    InvalidCheckpoint,
    /// Capability error
    Capability(CapError),
    /// Recording buffer full
    BufferFull,
}

impl From<CapError> for TimeTravelError {
    fn from(err: CapError) -> Self {
        TimeTravelError::Capability(err)
    }
}

/// Initialize time-travel subsystem
pub fn init() {
    log::info!("Time-travel debugging subsystem initialized");
}

// ============================================================================
// Checkpoint API
// ============================================================================

/// Create a checkpoint of a process
pub fn create_checkpoint(
    process_id: ProcessId,
    name: Option<String>,
    include_tensors: bool,
) -> Result<Capability, TimeTravelError> {
    // Get process state
    let process = crate::process::get_process(process_id)
        .ok_or(TimeTravelError::ProcessNotFound)?;

    // Create checkpoint
    let checkpoint_id = CheckpointId::new();
    let checkpoint = checkpoint::Checkpoint::capture(
        checkpoint_id,
        &process,
        name,
        include_tensors,
    )?;

    // Store checkpoint
    CHECKPOINTS.write().insert(checkpoint_id, checkpoint);

    // Create capability for the checkpoint
    let object_id = ObjectId::new(ObjectType::Checkpoint);
    let cap = unsafe {
        Capability::new_unchecked(
            object_id,
            Rights::READ | Rights::WRITE | Rights::GRANT,
        )
    };

    log::debug!("Created checkpoint {:?} for process {:?}", checkpoint_id, process_id);

    Ok(cap)
}

/// Restore a process from a checkpoint
pub fn restore_checkpoint(
    checkpoint_id: CheckpointId,
    target_process: Option<ProcessId>,
) -> Result<ProcessId, TimeTravelError> {
    let checkpoints = CHECKPOINTS.read();
    let checkpoint = checkpoints
        .get(&checkpoint_id)
        .ok_or(TimeTravelError::CheckpointNotFound)?;

    let pid = checkpoint.restore(target_process)?;

    log::debug!("Restored checkpoint {:?} to process {:?}", checkpoint_id, pid);

    Ok(pid)
}

/// Delete a checkpoint
pub fn delete_checkpoint(checkpoint_id: CheckpointId) -> Result<(), TimeTravelError> {
    CHECKPOINTS
        .write()
        .remove(&checkpoint_id)
        .ok_or(TimeTravelError::CheckpointNotFound)?;

    log::debug!("Deleted checkpoint {:?}", checkpoint_id);
    Ok(())
}

/// List all checkpoints for a process
pub fn list_checkpoints(process_id: ProcessId) -> Vec<CheckpointId> {
    CHECKPOINTS
        .read()
        .iter()
        .filter(|(_, cp)| cp.process_id == process_id)
        .map(|(id, _)| *id)
        .collect()
}

// ============================================================================
// Recording API
// ============================================================================

/// Start recording execution of a process
pub fn start_recording(
    process_id: ProcessId,
    config: record::RecordingConfig,
) -> Result<Capability, TimeTravelError> {
    // Check if already recording
    let recordings = RECORDINGS.read();
    for session in recordings.values() {
        if session.process_id == process_id && session.is_active() {
            return Err(TimeTravelError::AlreadyRecording);
        }
    }
    drop(recordings);

    // Create recording session
    let recording_id = RecordingId::new();
    let session = record::RecordingSession::new(recording_id, process_id, config)?;

    // Store session
    RECORDINGS.write().insert(recording_id, session);

    // Create capability
    let object_id = ObjectId::new(ObjectType::RecordingSession);
    let cap = unsafe {
        Capability::new_unchecked(
            object_id,
            Rights::READ | Rights::WRITE | Rights::RECORD | Rights::GRANT,
        )
    };

    log::info!("Started recording session {:?} for process {:?}", recording_id, process_id);

    Ok(cap)
}

/// Stop recording and finalize the trace
pub fn stop_recording(recording_id: RecordingId) -> Result<record::RecordingTrace, TimeTravelError> {
    let mut recordings = RECORDINGS.write();
    let session = recordings
        .get_mut(&recording_id)
        .ok_or(TimeTravelError::RecordingNotFound)?;

    if !session.is_active() {
        return Err(TimeTravelError::NotRecording);
    }

    let trace = session.finalize()?;

    log::info!(
        "Stopped recording {:?}: {} events captured",
        recording_id,
        trace.events.len()
    );

    Ok(trace)
}

/// Record an event during execution
pub fn record_event(
    process_id: ProcessId,
    event: record::RecordEvent,
) -> Result<(), TimeTravelError> {
    let mut recordings = RECORDINGS.write();

    // Find active recording for this process
    for session in recordings.values_mut() {
        if session.process_id == process_id && session.is_active() {
            return session.record(event);
        }
    }

    // No active recording - silently ignore
    Ok(())
}

/// Start replay from a recording trace
pub fn start_replay(
    trace: record::RecordingTrace,
    target_process: Option<ProcessId>,
) -> Result<ProcessId, TimeTravelError> {
    // Create or use target process
    let pid = if let Some(existing) = target_process {
        existing
    } else {
        // Fork from the initial checkpoint in the trace
        restore_checkpoint(trace.initial_checkpoint, None)?
    };

    // Set up replay state
    record::setup_replay(pid, trace)?;

    log::info!("Started replay for process {:?}", pid);

    Ok(pid)
}

// ============================================================================
// Syscall Handlers
// ============================================================================

/// Handle checkpoint syscall
pub fn syscall_checkpoint(
    process_id: ProcessId,
    flags: u32,
) -> Result<u64, TimeTravelError> {
    let include_tensors = (flags & CHECKPOINT_INCLUDE_TENSORS) != 0;

    let cap = create_checkpoint(process_id, None, include_tensors)?;
    Ok(cap.object_id.as_u64())
}

/// Handle restore syscall
pub fn syscall_restore(checkpoint_cap: u64) -> Result<u64, TimeTravelError> {
    // For now, use the cap value as checkpoint ID (simplified)
    let checkpoint_id = CheckpointId(checkpoint_cap & 0xFFFFFFFF);
    let pid = restore_checkpoint(checkpoint_id, None)?;
    Ok(pid.0)
}

/// Handle record start syscall
pub fn syscall_record_start(
    process_id: ProcessId,
    flags: u32,
) -> Result<u64, TimeTravelError> {
    let config = record::RecordingConfig {
        capture_syscalls: (flags & RECORD_SYSCALLS) != 0,
        capture_memory: (flags & RECORD_MEMORY) != 0,
        capture_scheduler: (flags & RECORD_SCHEDULER) != 0,
        capture_tensors: (flags & RECORD_TENSORS) != 0,
        max_events: 1_000_000,
        buffer_size: 64 * 1024 * 1024, // 64MB
    };

    let cap = start_recording(process_id, config)?;
    Ok(cap.object_id.as_u64())
}

/// Handle record stop syscall
pub fn syscall_record_stop(recording_cap: u64) -> Result<u64, TimeTravelError> {
    let recording_id = RecordingId(recording_cap & 0xFFFFFFFF);
    let trace = stop_recording(recording_id)?;
    Ok(trace.events.len() as u64)
}

// Checkpoint flags
const CHECKPOINT_INCLUDE_TENSORS: u32 = 1 << 0;

// Recording flags
const RECORD_SYSCALLS: u32 = 1 << 0;
const RECORD_MEMORY: u32 = 1 << 1;
const RECORD_SCHEDULER: u32 = 1 << 2;
const RECORD_TENSORS: u32 = 1 << 3;
