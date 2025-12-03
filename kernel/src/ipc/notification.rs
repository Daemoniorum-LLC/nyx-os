//! Notification objects for lightweight signaling
//!
//! Notifications provide lightweight async signaling between threads.
//! Unlike endpoints which transfer messages, notifications just transfer
//! a bitmap of signal bits. This is useful for:
//! - Event notification
//! - Interrupt delivery to userspace
//! - Semaphore-like synchronization
//! - Condition variables

use core::sync::atomic::{AtomicU64, Ordering};
use crate::sched::{self, ThreadId, BlockReason};
use alloc::collections::VecDeque;
use spin::Mutex;

/// Notification - lightweight async signaling
pub struct Notification {
    /// Signal bits (64 independent signals)
    bits: AtomicU64,
    /// Threads waiting for signals
    waiters: Mutex<VecDeque<Waiter>>,
}

/// A waiter waiting for specific bits
struct Waiter {
    thread_id: ThreadId,
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

    /// Create a notification with initial bits set
    pub fn with_bits(bits: u64) -> Self {
        Self {
            bits: AtomicU64::new(bits),
            waiters: Mutex::new(VecDeque::new()),
        }
    }

    /// Signal bits (OR with existing)
    pub fn signal(&self, bits: u64) {
        let old = self.bits.fetch_or(bits, Ordering::SeqCst);
        let new = old | bits;

        // Check if any waiters should be woken
        let mut waiters = self.waiters.lock();
        let mut to_wake = VecDeque::new();

        waiters.retain(|waiter| {
            if (new & waiter.mask) != 0 {
                to_wake.push_back(waiter.thread_id);
                false // Remove from list
            } else {
                true // Keep waiting
            }
        });

        drop(waiters); // Release lock before waking threads

        // Wake threads
        for thread_id in to_wake {
            sched::wake(thread_id);
        }
    }

    /// Signal a single bit
    pub fn signal_bit(&self, bit: usize) {
        if bit < 64 {
            self.signal(1 << bit);
        }
    }

    /// Wait for any of the specified bits to be set (blocking)
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
            let current = sched::current_thread_id();
            self.waiters.lock().push_back(Waiter {
                thread_id: current,
                mask,
            });

            // Block until signaled
            sched::block(BlockReason::Notification);
        }
    }

    /// Wait for specific bit to be set
    pub fn wait_bit(&self, bit: usize) -> bool {
        if bit >= 64 {
            return false;
        }
        self.wait(1 << bit) != 0
    }

    /// Wait with timeout (returns 0 on timeout)
    pub fn wait_timeout(&self, mask: u64, timeout_ms: u64) -> u64 {
        // Try immediate poll first
        let result = self.poll(mask);
        if result != 0 {
            return result;
        }

        // Set up timeout
        let start = crate::arch::x86_64::rdtsc();
        let timeout_ticks = timeout_ms * 1_000_000; // Approximate

        loop {
            // Check timeout
            if crate::arch::x86_64::rdtsc() - start > timeout_ticks {
                // Remove ourselves from waiters
                let current = sched::current_thread_id();
                self.waiters.lock().retain(|w| w.thread_id != current);
                return 0;
            }

            let bits = self.bits.load(Ordering::SeqCst);
            if (bits & mask) != 0 {
                let result = bits & mask;
                self.bits.fetch_and(!result, Ordering::SeqCst);
                return result;
            }

            // Add to waiters if not already
            let current = sched::current_thread_id();
            let mut waiters = self.waiters.lock();
            if !waiters.iter().any(|w| w.thread_id == current) {
                waiters.push_back(Waiter {
                    thread_id: current,
                    mask,
                });
            }
            drop(waiters);

            // Yield and retry
            sched::yield_now();
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

    /// Poll a single bit
    pub fn poll_bit(&self, bit: usize) -> bool {
        if bit >= 64 {
            return false;
        }
        self.poll(1 << bit) != 0
    }

    /// Clear specific bits
    pub fn clear(&self, bits: u64) {
        self.bits.fetch_and(!bits, Ordering::SeqCst);
    }

    /// Clear a single bit
    pub fn clear_bit(&self, bit: usize) {
        if bit < 64 {
            self.clear(1 << bit);
        }
    }

    /// Get current bits (without clearing)
    pub fn peek(&self) -> u64 {
        self.bits.load(Ordering::SeqCst)
    }

    /// Check if a specific bit is set (without clearing)
    pub fn is_set(&self, bit: usize) -> bool {
        if bit >= 64 {
            return false;
        }
        (self.peek() & (1 << bit)) != 0
    }

    /// Cancel all waiters (wake them up)
    pub fn cancel_all(&self) {
        let waiters: VecDeque<Waiter> = {
            let mut waiters = self.waiters.lock();
            core::mem::take(&mut *waiters)
        };

        for waiter in waiters {
            sched::wake(waiter.thread_id);
        }
    }

    /// Get number of waiting threads
    pub fn waiter_count(&self) -> usize {
        self.waiters.lock().len()
    }
}

impl Default for Notification {
    fn default() -> Self {
        Self::new()
    }
}

/// Semaphore built on notifications
pub struct Semaphore {
    /// Current count
    count: AtomicU64,
    /// Notification for wake-ups
    notify: Notification,
}

impl Semaphore {
    /// Create a new semaphore with initial count
    pub fn new(initial: u64) -> Self {
        Self {
            count: AtomicU64::new(initial),
            notify: Notification::new(),
        }
    }

    /// Acquire (decrement) the semaphore, blocking if count is 0
    pub fn acquire(&self) {
        loop {
            let count = self.count.load(Ordering::SeqCst);
            if count > 0 {
                if self
                    .count
                    .compare_exchange(count, count - 1, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
                {
                    return;
                }
            } else {
                // Wait for signal
                self.notify.wait(1);
            }
        }
    }

    /// Try to acquire without blocking
    pub fn try_acquire(&self) -> bool {
        loop {
            let count = self.count.load(Ordering::SeqCst);
            if count == 0 {
                return false;
            }
            if self
                .count
                .compare_exchange(count, count - 1, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                return true;
            }
        }
    }

    /// Release (increment) the semaphore
    pub fn release(&self) {
        self.count.fetch_add(1, Ordering::SeqCst);
        self.notify.signal(1);
    }

    /// Get current count
    pub fn count(&self) -> u64 {
        self.count.load(Ordering::SeqCst)
    }
}

/// Event flag group (32 independent events)
pub struct EventGroup {
    /// Event flags
    flags: AtomicU64,
    /// Notification for wake-ups
    notify: Notification,
}

impl EventGroup {
    /// Create a new event group
    pub fn new() -> Self {
        Self {
            flags: AtomicU64::new(0),
            notify: Notification::new(),
        }
    }

    /// Set event flags
    pub fn set(&self, flags: u64) {
        self.flags.fetch_or(flags, Ordering::SeqCst);
        self.notify.signal(flags);
    }

    /// Clear event flags
    pub fn clear(&self, flags: u64) {
        self.flags.fetch_and(!flags, Ordering::SeqCst);
    }

    /// Wait for any of the specified flags
    pub fn wait_any(&self, flags: u64) -> u64 {
        loop {
            let current = self.flags.load(Ordering::SeqCst);
            let matched = current & flags;
            if matched != 0 {
                return matched;
            }
            self.notify.wait(flags);
        }
    }

    /// Wait for all specified flags
    pub fn wait_all(&self, flags: u64) -> u64 {
        loop {
            let current = self.flags.load(Ordering::SeqCst);
            if (current & flags) == flags {
                return flags;
            }
            self.notify.wait(flags);
        }
    }

    /// Get current flags
    pub fn get(&self) -> u64 {
        self.flags.load(Ordering::SeqCst)
    }
}

impl Default for EventGroup {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Module-level functions for IPC integration
// ============================================================================

use crate::cap::ObjectId;
use alloc::collections::BTreeMap;
use spin::RwLock;

/// Global notification registry
static NOTIFICATIONS: RwLock<BTreeMap<ObjectId, Notification>> = RwLock::new(BTreeMap::new());

/// Signal a notification by its ObjectId
///
/// This is called from the network driver notification system and other
/// kernel subsystems that need to signal notifications by ID.
pub fn signal(notif_id: ObjectId, bits: u64) -> Result<(), super::IpcError> {
    let notifications = NOTIFICATIONS.read();
    let notification = notifications
        .get(&notif_id)
        .ok_or(super::IpcError::InvalidEndpoint)?;

    notification.signal(bits);
    Ok(())
}

/// Register a notification in the global registry
pub fn register(id: ObjectId, notification: Notification) {
    NOTIFICATIONS.write().insert(id, notification);
}

/// Unregister a notification from the global registry
pub fn unregister(id: ObjectId) -> Option<Notification> {
    NOTIFICATIONS.write().remove(&id)
}

/// Get a reference to a notification (for internal use)
pub fn get(id: ObjectId) -> Option<NotificationRef> {
    if NOTIFICATIONS.read().contains_key(&id) {
        Some(NotificationRef { id })
    } else {
        None
    }
}

/// Reference to a notification in the registry
pub struct NotificationRef {
    id: ObjectId,
}

impl NotificationRef {
    pub fn signal(&self, bits: u64) -> Result<(), super::IpcError> {
        signal(self.id, bits)
    }

    pub fn peek(&self) -> u64 {
        NOTIFICATIONS.read()
            .get(&self.id)
            .map(|n| n.peek())
            .unwrap_or(0)
    }
}
