//! Notification objects for lightweight signaling

use core::sync::atomic::{AtomicU64, Ordering};
use alloc::collections::VecDeque;
use spin::Mutex;

/// Notification - lightweight async signaling
///
/// Unlike endpoints which transfer messages, notifications just transfer
/// a bitmap of signal bits. This is useful for:
/// - Event notification
/// - Interrupt delivery
/// - Semaphore-like synchronization
pub struct Notification {
    /// Signal bits
    bits: AtomicU64,
    /// Threads waiting for signals
    waiters: Mutex<VecDeque<Waiter>>,
}

struct Waiter {
    thread_id: crate::sched::ThreadId,
    mask: u64,
}

impl Notification {
    /// Create a new notification
    pub fn new() -> Self {
        Self {
            bits: AtomicU64::new(0),
            waiters: Mutex::new(VecDeque::new()),
        }
    }

    /// Signal bits (OR with existing)
    pub fn signal(&self, bits: u64) {
        let old = self.bits.fetch_or(bits, Ordering::SeqCst);
        let new = old | bits;

        // Check if any waiters should be woken
        let mut waiters = self.waiters.lock();
        waiters.retain(|waiter| {
            if (new & waiter.mask) != 0 {
                // TODO: Wake up waiter.thread_id
                false // Remove from list
            } else {
                true // Keep waiting
            }
        });
    }

    /// Wait for any of the specified bits to be set
    pub fn wait(&self, mask: u64) -> u64 {
        loop {
            let bits = self.bits.load(Ordering::SeqCst);

            if (bits & mask) != 0 {
                // Clear and return the bits we waited for
                let result = bits & mask;
                self.bits.fetch_and(!result, Ordering::SeqCst);
                return result;
            }

            // Add ourselves to waiters
            // TODO: Get current thread ID and add to waiters
            // TODO: Block current thread

            // For now, busy-wait (not ideal)
            core::hint::spin_loop();
        }
    }

    /// Poll for bits without blocking
    pub fn poll(&self, mask: u64) -> u64 {
        let bits = self.bits.load(Ordering::SeqCst);
        let result = bits & mask;

        if result != 0 {
            self.bits.fetch_and(!result, Ordering::SeqCst);
        }

        result
    }

    /// Clear specific bits
    pub fn clear(&self, bits: u64) {
        self.bits.fetch_and(!bits, Ordering::SeqCst);
    }

    /// Get current bits (without clearing)
    pub fn peek(&self) -> u64 {
        self.bits.load(Ordering::SeqCst)
    }
}

impl Default for Notification {
    fn default() -> Self {
        Self::new()
    }
}
