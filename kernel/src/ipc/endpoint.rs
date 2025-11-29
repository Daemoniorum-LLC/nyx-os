//! IPC Endpoint implementation

use super::{Message, IpcError};
use alloc::collections::VecDeque;
use spin::Mutex;

/// IPC Endpoint - message queue for inter-process communication
pub struct Endpoint {
    /// Message queue
    queue: Mutex<VecDeque<Message>>,
    /// Maximum queue depth
    max_depth: usize,
    /// Threads waiting to receive
    waiters: Mutex<VecDeque<crate::sched::ThreadId>>,
}

impl Endpoint {
    /// Create a new endpoint
    pub fn new() -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            max_depth: 128,
            waiters: Mutex::new(VecDeque::new()),
        }
    }

    /// Send a message to this endpoint
    pub fn send(&self, msg: Message) -> Result<(), IpcError> {
        let mut queue = self.queue.lock();

        if queue.len() >= self.max_depth {
            return Err(IpcError::QueueFull);
        }

        queue.push_back(msg);

        // Wake up a waiter if any
        if let Some(_waiter) = self.waiters.lock().pop_front() {
            // TODO: Wake up the waiting thread
        }

        Ok(())
    }

    /// Receive a message from this endpoint (blocking)
    pub fn receive(&self) -> Result<Message, IpcError> {
        loop {
            if let Some(msg) = self.queue.lock().pop_front() {
                return Ok(msg);
            }

            // Add ourselves to waiters
            // TODO: Get current thread ID and add to waiters
            // TODO: Block current thread

            // For now, return error
            return Err(IpcError::WouldBlock);
        }
    }

    /// Try to receive without blocking
    pub fn try_receive(&self) -> Option<Message> {
        self.queue.lock().pop_front()
    }

    /// Get queue depth
    pub fn depth(&self) -> usize {
        self.queue.lock().len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.queue.lock().is_empty()
    }
}

impl Default for Endpoint {
    fn default() -> Self {
        Self::new()
    }
}
