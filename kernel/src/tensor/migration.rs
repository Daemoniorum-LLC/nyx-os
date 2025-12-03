//! Tensor migration between devices

use crate::cap::ObjectId;
use alloc::collections::VecDeque;

/// Tensor migration scheduler
pub struct MigrationScheduler {
    /// Pending migrations
    pending: VecDeque<MigrationJob>,
}

/// Migration job
#[derive(Clone, Debug)]
pub struct MigrationJob {
    /// Tensor to migrate
    pub tensor_id: ObjectId,
    /// Source device
    pub src_device: u32,
    /// Target device
    pub dst_device: u32,
    /// Priority (higher = sooner)
    pub priority: i32,
    /// Status
    pub status: MigrationStatus,
}

/// Migration status
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MigrationStatus {
    /// Queued
    Queued,
    /// In progress
    InProgress,
    /// Completed
    Completed,
    /// Failed
    Failed,
}

impl MigrationScheduler {
    /// Create a new migration scheduler
    pub fn new() -> Self {
        Self {
            pending: VecDeque::new(),
        }
    }

    /// Create a new migration scheduler in const context
    pub const fn new_const() -> Self {
        Self {
            pending: VecDeque::new(),
        }
    }

    /// Schedule a migration
    pub fn schedule(
        &mut self,
        tensor_id: ObjectId,
        src_device: u32,
        dst_device: u32,
    ) -> u64 {
        let job = MigrationJob {
            tensor_id,
            src_device,
            dst_device,
            priority: 0,
            status: MigrationStatus::Queued,
        };

        self.pending.push_back(job);

        // Return job ID (just use index for now)
        self.pending.len() as u64 - 1
    }

    /// Get next job to process
    pub fn next(&mut self) -> Option<MigrationJob> {
        self.pending.pop_front()
    }

    /// Get pending count
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }
}

impl Default for MigrationScheduler {
    fn default() -> Self {
        Self::new()
    }
}

/// Migration strategy
#[derive(Clone, Copy, Debug)]
pub enum MigrationStrategy {
    /// Synchronous copy
    Sync,
    /// Asynchronous copy with DMA
    Async,
    /// Peer-to-peer (GPU to GPU)
    P2P,
    /// Through host memory
    Staged,
}

/// Choose migration strategy based on device types
pub fn choose_strategy(src_device: u32, dst_device: u32) -> MigrationStrategy {
    // For now, always use staged
    // TODO: Detect P2P capability
    MigrationStrategy::Staged
}
