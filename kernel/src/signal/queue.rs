//! Signal queue implementation
//!
//! Maintains a queue of pending signals with their associated siginfo.

use super::{SigInfo, SigSet, Signal, SignalError};
use alloc::collections::VecDeque;

/// Maximum number of queued signals per process/thread
pub const SIGQUEUE_MAX: usize = 32;

/// Signal queue (pending signals)
#[derive(Clone, Debug)]
pub struct SignalQueue {
    /// Standard signals (only one per signal number)
    standard: [Option<SigInfo>; 32],
    /// Real-time signals (can queue multiple)
    realtime: VecDeque<SigInfo>,
    /// Bitmap of pending standard signals
    pending: u32,
}

impl SignalQueue {
    /// Create an empty signal queue
    pub fn new() -> Self {
        Self {
            standard: core::array::from_fn(|_| None),
            realtime: VecDeque::new(),
            pending: 0,
        }
    }

    /// Check if any signals are pending
    pub fn is_empty(&self) -> bool {
        self.pending == 0 && self.realtime.is_empty()
    }

    /// Check if a specific signal is pending
    pub fn is_pending(&self, signum: u8) -> bool {
        if signum > 0 && signum < 32 {
            (self.pending & (1 << signum)) != 0
        } else if signum >= 32 && signum < 64 {
            self.realtime.iter().any(|info| info.signo == signum)
        } else {
            false
        }
    }

    /// Get the set of pending signals
    pub fn pending_set(&self) -> SigSet {
        let mut set = SigSet::from_raw(self.pending as u64);

        // Add real-time signals
        for info in &self.realtime {
            set.add(info.signo);
        }

        set
    }

    /// Enqueue a signal
    pub fn enqueue(&mut self, signum: u8, info: SigInfo) -> Result<(), SignalError> {
        if signum == 0 || signum > 63 {
            return Err(SignalError::InvalidSignal);
        }

        if signum < 32 {
            // Standard signal - at most one pending per signal
            let idx = signum as usize;
            if self.standard[idx].is_none() {
                self.standard[idx] = Some(info);
                self.pending |= 1 << signum;
            }
            // If already pending, just ignore (standard signal behavior)
            Ok(())
        } else {
            // Real-time signal - can queue multiple
            if self.realtime.len() >= SIGQUEUE_MAX {
                return Err(SignalError::QueueFull);
            }
            self.realtime.push_back(info);
            Ok(())
        }
    }

    /// Dequeue a specific signal
    pub fn dequeue(&mut self, signum: u8) -> Option<SigInfo> {
        if signum == 0 || signum > 63 {
            return None;
        }

        if signum < 32 {
            let idx = signum as usize;
            if let Some(info) = self.standard[idx].take() {
                self.pending &= !(1 << signum);
                return Some(info);
            }
        } else {
            // Find and remove first instance of this real-time signal
            if let Some(pos) = self.realtime.iter().position(|i| i.signo == signum) {
                return self.realtime.remove(pos);
            }
        }

        None
    }

    /// Dequeue the highest priority pending signal not in mask
    ///
    /// Lower signal numbers have higher priority.
    /// Real-time signals are delivered FIFO within their priority.
    pub fn dequeue_unblocked(&mut self, mask: &SigSet) -> Option<SigInfo> {
        // First check standard signals (lower numbers first)
        for signum in 1..32u8 {
            if (self.pending & (1 << signum)) != 0 && !mask.contains(signum) {
                return self.dequeue(signum);
            }
        }

        // Then check real-time signals (FIFO within same signal number)
        for signum in 32..64u8 {
            if !mask.contains(signum) {
                if let Some(pos) = self.realtime.iter().position(|i| i.signo == signum) {
                    return self.realtime.remove(pos);
                }
            }
        }

        None
    }

    /// Clear all pending signals
    pub fn clear(&mut self) {
        for slot in &mut self.standard {
            *slot = None;
        }
        self.realtime.clear();
        self.pending = 0;
    }

    /// Clear a specific signal
    pub fn clear_signal(&mut self, signum: u8) {
        if signum > 0 && signum < 32 {
            self.standard[signum as usize] = None;
            self.pending &= !(1 << signum);
        } else if signum >= 32 && signum < 64 {
            self.realtime.retain(|i| i.signo != signum);
        }
    }

    /// Get count of pending signals
    pub fn count(&self) -> usize {
        (self.pending.count_ones() as usize) + self.realtime.len()
    }

    /// Get count of queued real-time signals
    pub fn realtime_count(&self) -> usize {
        self.realtime.len()
    }
}

impl Default for SignalQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_queue() {
        let queue = SignalQueue::new();
        assert!(queue.is_empty());
        assert_eq!(queue.count(), 0);
    }

    #[test]
    fn test_enqueue_dequeue() {
        let mut queue = SignalQueue::new();

        let info = SigInfo::new(Signal::SIGINT);
        queue.enqueue(Signal::SIGINT.as_raw(), info).unwrap();

        assert!(!queue.is_empty());
        assert!(queue.is_pending(Signal::SIGINT.as_raw()));

        let dequeued = queue.dequeue(Signal::SIGINT.as_raw());
        assert!(dequeued.is_some());
        assert!(queue.is_empty());
    }

    #[test]
    fn test_standard_signal_coalescing() {
        let mut queue = SignalQueue::new();

        // Enqueue same signal twice
        queue.enqueue(2, SigInfo::new(Signal::SIGINT)).unwrap();
        queue.enqueue(2, SigInfo::new(Signal::SIGINT)).unwrap();

        // Should only have one pending
        assert_eq!(queue.count(), 1);
    }
}
