//! IPC Ring Buffer Implementation
//!
//! Lock-free ring buffers for submission and completion queues.

use core::sync::atomic::{AtomicU32, Ordering};
use alloc::vec::Vec;

use super::IpcError;

/// IPC ring structure shared between kernel and userspace
pub struct IpcRing {
    /// Submission queue
    pub sq: SubmissionQueue,
    /// Completion queue
    pub cq: CompletionQueue,
    /// Ring flags (for coordination)
    pub flags: AtomicU32,
}

/// Submission queue
pub struct SubmissionQueue {
    /// Head index (kernel reads, increments after processing)
    pub head: AtomicU32,
    /// Tail index (userspace writes, increments after adding)
    pub tail: AtomicU32,
    /// Ring mask (size - 1)
    pub mask: u32,
    /// Entry array
    pub entries: Vec<SqEntry>,
}

/// Completion queue
pub struct CompletionQueue {
    /// Head index (userspace reads, increments after consuming)
    pub head: AtomicU32,
    /// Tail index (kernel writes, increments after adding)
    pub tail: AtomicU32,
    /// Ring mask
    pub mask: u32,
    /// Entry array
    pub entries: Vec<CqEntry>,
}

/// Submission queue entry - what userspace submits
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct SqEntry {
    /// Operation code
    pub opcode: IpcOpcode,
    /// Flags
    pub flags: SqFlags,
    /// Capability slot for the operation
    pub cap_slot: u32,
    /// Reserved for alignment
    pub _reserved: u32,
    /// Operation-specific parameters
    pub params: [u64; 4],
    /// User data (returned in completion)
    pub user_data: u64,
}

/// Completion queue entry - what kernel returns
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct CqEntry {
    /// User data from submission
    pub user_data: u64,
    /// Result code (0 = success, negative = error)
    pub result: i64,
    /// Operation-specific return data
    pub data: [u64; 2],
    /// Flags
    pub flags: CqFlags,
    /// Reserved for alignment
    pub _reserved: u32,
}

/// IPC operation codes
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum IpcOpcode {
    /// No operation
    #[default]
    Nop = 0,

    // === Message Passing (1-15) ===

    /// Send message to endpoint
    Send = 1,
    /// Receive from endpoint
    Receive = 2,
    /// Send + receive reply (RPC)
    Call = 3,
    /// Reply to a Call
    Reply = 4,

    // === Notifications (16-31) ===

    /// Set notification bits
    Signal = 16,
    /// Wait for notification bits
    Wait = 17,
    /// Non-blocking poll
    Poll = 18,

    // === Memory Operations (32-47) ===

    /// Map memory region
    Map = 32,
    /// Unmap memory region
    Unmap = 33,
    /// Grant memory to another process
    Grant = 34,

    // === Capability Operations (48-63) ===

    /// Create derived capability
    Derive = 48,
    /// Revoke capability tree
    Revoke = 49,
    /// Get capability metadata
    Identify = 50,

    // === AI Operations (64-79) ===

    /// Allocate tensor buffer
    TensorAlloc = 64,
    /// Free tensor buffer
    TensorFree = 65,
    /// Migrate tensor to different device
    TensorMigrate = 66,
    /// Submit inference request
    Inference = 67,
    /// Submit GPU compute job
    ComputeSubmit = 68,

    // === Time-Travel (80-95) ===

    /// Create execution checkpoint
    Checkpoint = 80,
    /// Restore to checkpoint
    Restore = 81,
    /// Start recording
    RecordStart = 82,
    /// Stop recording
    RecordStop = 83,

    // === Cancel/Timeout (96-111) ===

    /// Cancel pending operation
    Cancel = 96,
    /// Add timeout to operation
    Timeout = 97,
    /// Link timeout to next operation
    LinkTimeout = 98,
}

bitflags::bitflags! {
    /// Submission queue entry flags
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct SqFlags: u32 {
        /// Chain with next entry (atomic batch)
        const CHAIN = 1 << 0;
        /// Don't generate completion (fire-and-forget)
        const NO_CQE = 1 << 1;
        /// Use fixed buffer (zero-copy)
        const FIXED_BUFFER = 1 << 2;
        /// Drain queue before this op
        const DRAIN = 1 << 3;
        /// This is a linked timeout
        const LINK_TIMEOUT = 1 << 4;
        /// Async operation (don't wait)
        const ASYNC = 1 << 5;
    }
}

bitflags::bitflags! {
    /// Completion queue entry flags
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct CqFlags: u32 {
        /// More completions available
        const MORE = 1 << 0;
        /// Buffer has been consumed
        const BUFFER = 1 << 1;
        /// Operation was cancelled
        const CANCELLED = 1 << 2;
    }
}

impl IpcRing {
    /// Create a new IPC ring
    pub fn new(sq_size: u32, cq_size: u32) -> Result<Self, IpcError> {
        Ok(Self {
            sq: SubmissionQueue {
                head: AtomicU32::new(0),
                tail: AtomicU32::new(0),
                mask: sq_size - 1,
                entries: alloc::vec![SqEntry::default(); sq_size as usize],
            },
            cq: CompletionQueue {
                head: AtomicU32::new(0),
                tail: AtomicU32::new(0),
                mask: cq_size - 1,
                entries: alloc::vec![CqEntry::default(); cq_size as usize],
            },
            flags: AtomicU32::new(0),
        })
    }

    /// Get number of pending submissions
    pub fn sq_pending(&self) -> u32 {
        let head = self.sq.head.load(Ordering::Acquire);
        let tail = self.sq.tail.load(Ordering::Acquire);
        tail.wrapping_sub(head)
    }

    /// Get number of pending completions
    pub fn cq_pending(&self) -> u32 {
        let head = self.cq.head.load(Ordering::Acquire);
        let tail = self.cq.tail.load(Ordering::Acquire);
        tail.wrapping_sub(head)
    }

    /// Submit entries for processing (userspace side)
    pub fn submit(&self, count: u32) -> u32 {
        // Memory barrier to ensure entries are visible
        core::sync::atomic::fence(Ordering::Release);

        // Update tail
        self.sq.tail.fetch_add(count, Ordering::Release);

        count
    }

    /// Pop a submission entry (kernel side)
    pub fn pop_sq(&mut self) -> Option<SqEntry> {
        let head = self.sq.head.load(Ordering::Relaxed);
        let tail = self.sq.tail.load(Ordering::Acquire);

        if head == tail {
            return None;
        }

        let idx = (head & self.sq.mask) as usize;
        let entry = self.sq.entries[idx];

        self.sq.head.store(head.wrapping_add(1), Ordering::Release);

        Some(entry)
    }

    /// Push a completion entry (kernel side)
    pub fn push_cq(&mut self, entry: CqEntry) -> Result<(), IpcError> {
        let head = self.cq.head.load(Ordering::Acquire);
        let tail = self.cq.tail.load(Ordering::Relaxed);

        // Check if queue is full
        if tail.wrapping_sub(head) > self.cq.mask {
            return Err(IpcError::QueueFull);
        }

        let idx = (tail & self.cq.mask) as usize;
        self.cq.entries[idx] = entry;

        // Memory barrier before updating tail
        core::sync::atomic::fence(Ordering::Release);

        self.cq.tail.store(tail.wrapping_add(1), Ordering::Release);

        Ok(())
    }

    /// Pop a completion entry (userspace side)
    pub fn pop_cq(&mut self) -> Option<CqEntry> {
        let head = self.cq.head.load(Ordering::Relaxed);
        let tail = self.cq.tail.load(Ordering::Acquire);

        if head == tail {
            return None;
        }

        let idx = (head & self.cq.mask) as usize;
        let entry = self.cq.entries[idx];

        self.cq.head.store(head.wrapping_add(1), Ordering::Release);

        Some(entry)
    }
}

/// Ring flags
pub mod ring_flags {
    /// Kernel needs wakeup (userspace should call ring_enter)
    pub const NEED_WAKEUP: u32 = 1 << 0;
    /// CQ overflow occurred (completions were dropped)
    pub const CQ_OVERFLOW: u32 = 1 << 1;
}
