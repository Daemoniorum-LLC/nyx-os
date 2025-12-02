//! Completely Fair Scheduler (CFS) implementation

use super::ThreadId;
use alloc::collections::BTreeMap;

/// CFS run queue
pub struct CfsQueue {
    /// Red-black tree ordered by virtual runtime
    tree: BTreeMap<u64, ThreadId>,
    /// Minimum virtual runtime (for new threads)
    min_vruntime: u64,
}

impl CfsQueue {
    /// Create a new CFS queue
    pub fn new() -> Self {
        Self {
            tree: BTreeMap::new(),
            min_vruntime: 0,
        }
    }

    /// Add thread to queue
    pub fn enqueue(&mut self, thread_id: ThreadId) {
        // New threads start at min_vruntime to avoid starvation
        self.tree.insert(self.min_vruntime, thread_id);
    }

    /// Add thread with specific vruntime
    pub fn enqueue_with_vruntime(&mut self, thread_id: ThreadId, vruntime: u64) {
        self.tree.insert(vruntime, thread_id);
    }

    /// Pick next thread (lowest vruntime)
    pub fn pick_next(&mut self) -> Option<ThreadId> {
        let (vruntime, thread_id) = self.tree.pop_first()?;
        self.min_vruntime = vruntime;
        Some(thread_id)
    }

    /// Check if queue is empty
    pub fn is_empty(&self) -> bool {
        self.tree.is_empty()
    }

    /// Get queue length
    pub fn len(&self) -> usize {
        self.tree.len()
    }
}

impl Default for CfsQueue {
    fn default() -> Self {
        Self::new()
    }
}

/// Calculate weight from nice value (-20 to 19)
pub fn nice_to_weight(nice: i32) -> u32 {
    // Linux CFS weight table (approximate)
    const WEIGHTS: [u32; 40] = [
        88761, 71755, 56483, 46273, 36291, 29154, 23254, 18705, 14949, 11916, 9548, 7620, 6100,
        4904, 3906, 3121, 2501, 1991, 1586, 1277, 1024, 820, 655, 526, 423, 335, 272, 215, 172,
        137, 110, 87, 70, 56, 45, 36, 29, 23, 18, 15,
    ];

    let idx = (nice + 20).clamp(0, 39) as usize;
    WEIGHTS[idx]
}

/// Calculate virtual runtime delta
pub fn calc_vruntime_delta(runtime_ns: u64, weight: u32) -> u64 {
    // vruntime = runtime * NICE_0_WEIGHT / weight
    const NICE_0_WEIGHT: u64 = 1024;
    (runtime_ns * NICE_0_WEIGHT) / weight as u64
}
