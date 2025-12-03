//! # Time-Travel Debugging
//!
//! Provides userspace access to the kernel's checkpoint/restore and execution
//! recording facilities for deterministic debugging.
//!
//! ## Features
//!
//! - **Checkpoints**: Save and restore process state snapshots
//! - **Recording**: Capture execution traces for deterministic replay
//! - **Replay**: Re-execute recorded traces for debugging
//!
//! ## Example: Checkpoint/Restore
//!
//! ```no_run
//! use libnyx::timetravel::{self, CheckpointFlags};
//!
//! // Create a checkpoint
//! let checkpoint = timetravel::checkpoint(CheckpointFlags::empty())?;
//!
//! // ... do some work ...
//!
//! // Restore to the checkpoint
//! timetravel::restore(checkpoint, RestoreFlags::empty())?;
//! // Execution continues from checkpoint point
//! ```
//!
//! ## Example: Execution Recording
//!
//! ```no_run
//! use libnyx::timetravel::{self, RecordFlags};
//!
//! // Start recording
//! let session = timetravel::record_start(RecordFlags::SYSCALLS | RecordFlags::SCHEDULER)?;
//!
//! // ... execute code to record ...
//!
//! // Stop recording
//! let events = timetravel::record_stop(session)?;
//! println!("Recorded {} events", events);
//! ```

use crate::syscall::{self, nr, Error};

/// Checkpoint identifier
///
/// Represents a saved process state that can be restored later.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CheckpointId(u64);

impl CheckpointId {
    /// Create a CheckpointId from a raw value
    pub const fn from_raw(val: u64) -> Self {
        Self(val)
    }

    /// Get the raw value
    pub const fn as_raw(self) -> u64 {
        self.0
    }
}

/// Recording session identifier
///
/// Represents an active or completed recording session.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RecordingId(u64);

impl RecordingId {
    /// Create a RecordingId from a raw value
    pub const fn from_raw(val: u64) -> Self {
        Self(val)
    }

    /// Get the raw value
    pub const fn as_raw(self) -> u64 {
        self.0
    }
}

bitflags::bitflags! {
    /// Flags for checkpoint creation
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct CheckpointFlags: u32 {
        /// Include tensor buffer state in checkpoint
        const INCLUDE_TENSORS = 1 << 0;
        /// Compress checkpoint data
        const COMPRESS = 1 << 1;
        /// Create an incremental checkpoint (diff from previous)
        const INCREMENTAL = 1 << 2;
    }
}

bitflags::bitflags! {
    /// Flags for checkpoint restoration
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct RestoreFlags: u32 {
        /// Restore to current process (default: fork new process)
        const IN_PLACE = 1 << 0;
        /// Fork a new process for the restore
        const FORK = 1 << 1;
    }
}

bitflags::bitflags! {
    /// Flags for execution recording
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct RecordFlags: u32 {
        /// Record syscall arguments and return values
        const SYSCALLS = 1 << 0;
        /// Record memory accesses (expensive!)
        const MEMORY = 1 << 1;
        /// Record scheduling decisions
        const SCHEDULER = 1 << 2;
        /// Record tensor operations
        const TENSORS = 1 << 3;
        /// Record all non-deterministic events
        const ALL = Self::SYSCALLS.bits() | Self::SCHEDULER.bits() | Self::TENSORS.bits();
    }
}

/// Create a checkpoint of the current process state
///
/// Captures a snapshot of:
/// - All memory mappings and contents
/// - Register state for all threads
/// - Capability space
/// - Open file handles
/// - Optionally, tensor buffer contents
///
/// # Arguments
///
/// * `flags` - Checkpoint configuration flags
///
/// # Returns
///
/// * `Ok(CheckpointId)` - The ID of the created checkpoint
/// * `Err(Error::OutOfMemory)` - Not enough memory for snapshot
/// * `Err(Error::PermissionDenied)` - Process lacks checkpoint rights
///
/// # Example
///
/// ```no_run
/// use libnyx::timetravel::{self, CheckpointFlags};
///
/// // Create a basic checkpoint
/// let cp = timetravel::checkpoint(CheckpointFlags::empty())?;
///
/// // Create a checkpoint with tensor state
/// let cp_with_tensors = timetravel::checkpoint(CheckpointFlags::INCLUDE_TENSORS)?;
/// ```
pub fn checkpoint(flags: CheckpointFlags) -> Result<CheckpointId, Error> {
    let ret = unsafe { syscall::syscall1(nr::CHECKPOINT, flags.bits() as u64) };

    Error::from_raw(ret).map(CheckpointId)
}

/// Restore process state from a checkpoint
///
/// Restores the process to a previously saved state. By default, this creates
/// a new process; use `RestoreFlags::IN_PLACE` to restore the current process.
///
/// **Warning**: In-place restore will destroy the current process state and
/// execution will continue from the checkpoint point.
///
/// # Arguments
///
/// * `checkpoint` - The checkpoint to restore from
/// * `flags` - Restore configuration flags
///
/// # Returns
///
/// * `Ok(pid)` - The process ID where state was restored (may be current or new)
/// * `Err(Error::NotFound)` - Checkpoint does not exist
/// * `Err(Error::InvalidArgument)` - Checkpoint is corrupted
/// * `Err(Error::OutOfMemory)` - Not enough memory for restore
///
/// # Example
///
/// ```no_run
/// use libnyx::timetravel::{self, RestoreFlags};
///
/// // Restore to a new forked process
/// let new_pid = timetravel::restore(checkpoint, RestoreFlags::FORK)?;
///
/// // Restore in-place (does not return on success!)
/// timetravel::restore(checkpoint, RestoreFlags::IN_PLACE)?;
/// ```
pub fn restore(checkpoint: CheckpointId, flags: RestoreFlags) -> Result<u64, Error> {
    let ret = unsafe { syscall::syscall2(nr::RESTORE, checkpoint.0, flags.bits() as u64) };

    Error::from_raw(ret)
}

/// Delete a checkpoint and free its resources
///
/// # Arguments
///
/// * `checkpoint` - The checkpoint to delete
///
/// # Returns
///
/// * `Ok(())` - Checkpoint was deleted
/// * `Err(Error::NotFound)` - Checkpoint does not exist
pub fn delete_checkpoint(checkpoint: CheckpointId) -> Result<(), Error> {
    // Note: This would need a separate syscall in a full implementation
    // For now, checkpoints are reference-counted and freed when no longer used
    let _ = checkpoint;
    Ok(())
}

/// Start recording execution for deterministic replay
///
/// Begins capturing non-deterministic events (syscall results, scheduling
/// decisions, I/O data) to enable later replay debugging.
///
/// Only one recording session can be active per process.
///
/// # Arguments
///
/// * `flags` - What to record (syscalls, memory, scheduler, tensors)
///
/// # Returns
///
/// * `Ok(RecordingId)` - The recording session ID
/// * `Err(Error::InvalidArgument)` - Already recording this process
/// * `Err(Error::OutOfMemory)` - Cannot allocate recording buffers
///
/// # Example
///
/// ```no_run
/// use libnyx::timetravel::{self, RecordFlags};
///
/// // Record syscalls and scheduling only
/// let session = timetravel::record_start(RecordFlags::SYSCALLS | RecordFlags::SCHEDULER)?;
///
/// // ... code to record ...
///
/// let events = timetravel::record_stop(session)?;
/// ```
pub fn record_start(flags: RecordFlags) -> Result<RecordingId, Error> {
    let ret = unsafe { syscall::syscall1(nr::RECORD_START, flags.bits() as u64) };

    Error::from_raw(ret).map(RecordingId)
}

/// Stop recording and finalize the trace
///
/// Ends the recording session and returns the number of events captured.
/// The trace can then be used for replay debugging.
///
/// # Arguments
///
/// * `session` - The recording session to stop
///
/// # Returns
///
/// * `Ok(event_count)` - Number of events recorded
/// * `Err(Error::NotFound)` - Recording session does not exist
/// * `Err(Error::InvalidArgument)` - Recording was not active
///
/// # Example
///
/// ```no_run
/// use libnyx::timetravel;
///
/// let events = timetravel::record_stop(session)?;
/// println!("Captured {} events for replay", events);
/// ```
pub fn record_stop(session: RecordingId) -> Result<u64, Error> {
    let ret = unsafe { syscall::syscall1(nr::RECORD_STOP, session.0) };

    Error::from_raw(ret)
}

/// Check if the current process is being recorded
///
/// # Returns
///
/// `true` if there's an active recording session for this process
pub fn is_recording() -> bool {
    // This would need a separate syscall in a full implementation
    // For now, return false as a stub
    false
}

/// Check if the current process is in replay mode
///
/// When in replay mode, non-deterministic events return recorded values
/// instead of live values.
///
/// # Returns
///
/// `true` if the process is replaying a recorded trace
pub fn is_replaying() -> bool {
    // This would need a separate syscall in a full implementation
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checkpoint_flags() {
        let flags = CheckpointFlags::INCLUDE_TENSORS | CheckpointFlags::COMPRESS;
        assert!(flags.contains(CheckpointFlags::INCLUDE_TENSORS));
        assert!(flags.contains(CheckpointFlags::COMPRESS));
        assert!(!flags.contains(CheckpointFlags::INCREMENTAL));
    }

    #[test]
    fn test_record_flags() {
        let flags = RecordFlags::ALL;
        assert!(flags.contains(RecordFlags::SYSCALLS));
        assert!(flags.contains(RecordFlags::SCHEDULER));
        assert!(flags.contains(RecordFlags::TENSORS));
        // MEMORY is expensive and not included in ALL
        assert!(!flags.contains(RecordFlags::MEMORY));
    }

    #[test]
    fn test_checkpoint_id() {
        let id = CheckpointId::from_raw(42);
        assert_eq!(id.as_raw(), 42);
    }

    #[test]
    fn test_recording_id() {
        let id = RecordingId::from_raw(123);
        assert_eq!(id.as_raw(), 123);
    }
}
