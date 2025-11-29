//! Deadline scheduling (SCHED_DEADLINE)

use super::ThreadId;
use alloc::collections::BinaryHeap;
use core::cmp::Ordering;

/// Deadline queue (earliest deadline first)
pub struct DeadlineQueue {
    heap: BinaryHeap<DeadlineEntry>,
}

/// Entry in deadline queue
#[derive(Clone, Copy, Debug)]
pub struct DeadlineEntry {
    /// Thread ID
    pub thread_id: ThreadId,
    /// Absolute deadline (nanoseconds since boot)
    pub deadline: u64,
    /// Runtime remaining in this period
    pub runtime_remaining: u64,
    /// Period length
    pub period: u64,
}

// Reverse ordering for min-heap (earliest deadline first)
impl PartialEq for DeadlineEntry {
    fn eq(&self, other: &Self) -> bool {
        self.deadline == other.deadline
    }
}

impl Eq for DeadlineEntry {}

impl PartialOrd for DeadlineEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for DeadlineEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse for min-heap
        other.deadline.cmp(&self.deadline)
    }
}

impl DeadlineQueue {
    /// Create a new deadline queue
    pub fn new() -> Self {
        Self {
            heap: BinaryHeap::new(),
        }
    }

    /// Add thread with deadline
    pub fn enqueue(&mut self, entry: DeadlineEntry) {
        self.heap.push(entry);
    }

    /// Pick next thread (earliest deadline)
    pub fn pick_next(&mut self) -> Option<ThreadId> {
        self.heap.pop().map(|e| e.thread_id)
    }

    /// Peek at earliest deadline
    pub fn peek(&self) -> Option<&DeadlineEntry> {
        self.heap.peek()
    }

    /// Check if queue is empty
    pub fn is_empty(&self) -> bool {
        self.heap.is_empty()
    }
}

impl Default for DeadlineQueue {
    fn default() -> Self {
        Self::new()
    }
}
