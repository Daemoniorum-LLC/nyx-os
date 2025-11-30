//! IPC Endpoint implementation
//!
//! Endpoints provide message-based inter-process communication.
//! They support both synchronous (blocking) and asynchronous operations.

use super::{Message, IpcError};
use crate::sched::{self, ThreadId, BlockReason};
use alloc::collections::VecDeque;
use spin::Mutex;

/// IPC Endpoint - message queue for inter-process communication
pub struct Endpoint {
    /// Message queue
    queue: Mutex<VecDeque<Message>>,
    /// Maximum queue depth
    max_depth: usize,
    /// Threads waiting to receive
    recv_waiters: Mutex<VecDeque<ThreadId>>,
    /// Threads waiting to send (when queue is full)
    send_waiters: Mutex<VecDeque<ThreadId>>,
}

impl Endpoint {
    /// Create a new endpoint
    pub fn new() -> Self {
        Self::with_depth(128)
    }

    /// Create an endpoint with specific queue depth
    pub fn with_depth(max_depth: usize) -> Self {
        Self {
            queue: Mutex::new(VecDeque::with_capacity(max_depth.min(16))),
            max_depth,
            recv_waiters: Mutex::new(VecDeque::new()),
            send_waiters: Mutex::new(VecDeque::new()),
        }
    }

    /// Send a message to this endpoint (non-blocking)
    pub fn send(&self, msg: Message) -> Result<(), IpcError> {
        let mut queue = self.queue.lock();

        if queue.len() >= self.max_depth {
            return Err(IpcError::QueueFull);
        }

        queue.push_back(msg);

        // Wake up a waiter if any
        let waiter = self.recv_waiters.lock().pop_front();
        if let Some(thread_id) = waiter {
            drop(queue); // Release lock before waking
            sched::wake(thread_id);
        }

        Ok(())
    }

    /// Send a message, blocking if queue is full
    pub fn send_blocking(&self, msg: Message) -> Result<(), IpcError> {
        loop {
            {
                let mut queue = self.queue.lock();
                if queue.len() < self.max_depth {
                    queue.push_back(msg);

                    // Wake up a waiter if any
                    let waiter = self.recv_waiters.lock().pop_front();
                    if let Some(thread_id) = waiter {
                        drop(queue);
                        sched::wake(thread_id);
                    }

                    return Ok(());
                }

                // Queue full, add ourselves to waiters
                let current = sched::current_thread_id();
                self.send_waiters.lock().push_back(current);
            }

            // Block until space is available
            sched::block(BlockReason::Ipc);
        }
    }

    /// Receive a message from this endpoint (blocking)
    pub fn receive(&self) -> Result<Message, IpcError> {
        loop {
            // Try to get a message
            {
                let mut queue = self.queue.lock();
                if let Some(msg) = queue.pop_front() {
                    // Wake up a sender if any (queue has space now)
                    let waiter = self.send_waiters.lock().pop_front();
                    if let Some(thread_id) = waiter {
                        drop(queue);
                        sched::wake(thread_id);
                    }
                    return Ok(msg);
                }

                // No message, add ourselves to waiters
                let current = sched::current_thread_id();
                self.recv_waiters.lock().push_back(current);
            }

            // Block until message arrives
            sched::block(BlockReason::Ipc);
        }
    }

    /// Receive with timeout (in milliseconds)
    pub fn receive_timeout(&self, timeout_ms: u64) -> Result<Message, IpcError> {
        // Try immediate receive first
        if let Some(msg) = self.try_receive() {
            return Ok(msg);
        }

        // Set up timeout
        let start = crate::arch::x86_64::rdtsc();
        let timeout_ticks = timeout_ms * 1_000_000; // Approximate conversion

        loop {
            // Check timeout
            if crate::arch::x86_64::rdtsc() - start > timeout_ticks {
                // Remove ourselves from waiters
                let current = sched::current_thread_id();
                self.recv_waiters.lock().retain(|&id| id != current);
                return Err(IpcError::Timeout);
            }

            // Try to receive
            {
                let mut queue = self.queue.lock();
                if let Some(msg) = queue.pop_front() {
                    let waiter = self.send_waiters.lock().pop_front();
                    if let Some(thread_id) = waiter {
                        drop(queue);
                        sched::wake(thread_id);
                    }
                    return Ok(msg);
                }

                // Add to waiters if not already
                let current = sched::current_thread_id();
                let mut waiters = self.recv_waiters.lock();
                if !waiters.contains(&current) {
                    waiters.push_back(current);
                }
            }

            // Yield and retry
            sched::yield_now();
        }
    }

    /// Try to receive without blocking
    pub fn try_receive(&self) -> Option<Message> {
        let mut queue = self.queue.lock();
        let msg = queue.pop_front();

        if msg.is_some() {
            // Wake up a sender if any
            let waiter = self.send_waiters.lock().pop_front();
            if let Some(thread_id) = waiter {
                drop(queue);
                sched::wake(thread_id);
            }
        }

        msg
    }

    /// Get queue depth
    pub fn depth(&self) -> usize {
        self.queue.lock().len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.queue.lock().is_empty()
    }

    /// Check if full
    pub fn is_full(&self) -> bool {
        self.queue.lock().len() >= self.max_depth
    }

    /// Get number of threads waiting to receive
    pub fn recv_waiter_count(&self) -> usize {
        self.recv_waiters.lock().len()
    }

    /// Get number of threads waiting to send
    pub fn send_waiter_count(&self) -> usize {
        self.send_waiters.lock().len()
    }

    /// Cancel all pending operations (wake all waiters with error)
    pub fn close(&self) {
        // Wake all receive waiters
        let recv_waiters: VecDeque<ThreadId> = {
            let mut waiters = self.recv_waiters.lock();
            core::mem::take(&mut *waiters)
        };

        for thread_id in recv_waiters {
            sched::wake(thread_id);
        }

        // Wake all send waiters
        let send_waiters: VecDeque<ThreadId> = {
            let mut waiters = self.send_waiters.lock();
            core::mem::take(&mut *waiters)
        };

        for thread_id in send_waiters {
            sched::wake(thread_id);
        }
    }
}

impl Default for Endpoint {
    fn default() -> Self {
        Self::new()
    }
}

/// Call endpoint - for synchronous request/response
pub struct CallEndpoint {
    /// Request endpoint
    request: Endpoint,
    /// Response endpoint (per-thread)
    responses: Mutex<alloc::collections::BTreeMap<ThreadId, Endpoint>>,
}

impl CallEndpoint {
    /// Create a new call endpoint
    pub fn new() -> Self {
        Self {
            request: Endpoint::new(),
            responses: Mutex::new(alloc::collections::BTreeMap::new()),
        }
    }

    /// Make a synchronous call (client side)
    pub fn call(&self, request: Message) -> Result<Message, IpcError> {
        let thread_id = sched::current_thread_id();

        // Ensure we have a response endpoint
        {
            let mut responses = self.responses.lock();
            responses.entry(thread_id).or_insert_with(Endpoint::new);
        }

        // Send request
        self.request.send(request)?;

        // Wait for response
        let responses = self.responses.lock();
        if let Some(resp_endpoint) = responses.get(&thread_id) {
            drop(responses);
            // Need to get it again without holding the lock
            let responses = self.responses.lock();
            let resp_endpoint = responses.get(&thread_id).unwrap();
            return resp_endpoint.receive();
        }

        Err(IpcError::InternalError)
    }

    /// Receive a request (server side)
    pub fn recv_request(&self) -> Result<(ThreadId, Message), IpcError> {
        let msg = self.request.receive()?;
        // The sender's thread ID should be embedded in the message
        // For now, return current thread as placeholder
        Ok((sched::current_thread_id(), msg))
    }

    /// Send a response (server side)
    pub fn send_response(&self, client: ThreadId, response: Message) -> Result<(), IpcError> {
        let responses = self.responses.lock();
        if let Some(endpoint) = responses.get(&client) {
            return endpoint.send(response);
        }
        Err(IpcError::InvalidEndpoint)
    }
}

impl Default for CallEndpoint {
    fn default() -> Self {
        Self::new()
    }
}
