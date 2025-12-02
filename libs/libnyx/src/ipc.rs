//! IPC (Inter-Process Communication) interface
//!
//! Nyx uses an io_uring-style async IPC model:
//! 1. Create an IPC ring with `IpcRing::new()`
//! 2. Submit operations to the submission queue
//! 3. Call `ring.enter()` to process submissions
//! 4. Read completions from the completion queue
//!
//! For simple synchronous operations, use the helper functions:
//! - `send()` / `receive()` for one-way messages
//! - `call()` / `reply()` for RPC-style communication
//! - `signal()` / `wait()` for notifications

use crate::cap::Capability;
use crate::syscall::{self, nr, Error};

/// Maximum message size (must match kernel MAX_IPC_MSG_SIZE)
pub const MAX_MESSAGE_SIZE: usize = 4096;

/// IPC Ring for async operations
///
/// The ring provides batched, async IPC similar to Linux's io_uring.
/// Multiple operations can be submitted before entering the kernel,
/// reducing syscall overhead.
pub struct IpcRing {
    /// Ring capability (object ID)
    handle: Capability,
}

impl IpcRing {
    /// Create a new IPC ring
    ///
    /// # Arguments
    /// * `sq_size` - Submission queue size (must be power of 2, max 32768)
    /// * `cq_size` - Completion queue size (must be power of 2, max 65536)
    ///
    /// # Example
    /// ```no_run
    /// let ring = IpcRing::new(256, 512)?;
    /// ```
    pub fn new(sq_size: u32, cq_size: u32) -> Result<Self, Error> {
        let result = unsafe {
            syscall::syscall3(nr::RING_SETUP, sq_size as u64, cq_size as u64, 0)
        };

        Error::from_raw(result).map(|id| Self {
            handle: Capability::from_raw(id),
        })
    }

    /// Get the ring's capability handle
    pub fn handle(&self) -> Capability {
        self.handle
    }

    /// Submit entries and wait for completions
    ///
    /// # Arguments
    /// * `to_submit` - Number of entries to submit from the SQ
    /// * `min_complete` - Minimum completions to wait for (0 = don't wait)
    ///
    /// # Returns
    /// Number of completions available in the CQ
    pub fn enter(&self, to_submit: u32, min_complete: u32) -> Result<u32, Error> {
        let result = unsafe {
            syscall::syscall4(
                nr::RING_ENTER,
                self.handle.as_raw(),
                to_submit as u64,
                min_complete as u64,
                0, // flags
            )
        };

        Error::from_raw(result).map(|n| n as u32)
    }
}

/// IPC Message buffer
///
/// Messages can be up to 4KB and contain arbitrary data.
/// For typed communication, consider serializing with serde.
#[repr(C)]
pub struct Message {
    /// Message tag (application-defined)
    pub tag: u32,
    /// Actual data length
    pub length: u32,
    /// Message data
    pub data: [u8; MAX_MESSAGE_SIZE],
}

impl Message {
    /// Create a new empty message
    pub const fn new() -> Self {
        Self {
            tag: 0,
            length: 0,
            data: [0; MAX_MESSAGE_SIZE],
        }
    }

    /// Create a message with the given tag and data
    pub fn with_data(tag: u32, data: &[u8]) -> Self {
        let mut msg = Self::new();
        msg.tag = tag;
        msg.set_data(data);
        msg
    }

    /// Set the message data
    ///
    /// # Panics
    /// Panics if data exceeds MAX_MESSAGE_SIZE
    pub fn set_data(&mut self, data: &[u8]) {
        assert!(data.len() <= MAX_MESSAGE_SIZE);
        self.length = data.len() as u32;
        self.data[..data.len()].copy_from_slice(data);
    }

    /// Get the message data as a slice
    pub fn as_slice(&self) -> &[u8] {
        &self.data[..self.length as usize]
    }

    /// Get the message data as a mutable slice
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.data[..self.length as usize]
    }

    /// Clear the message
    pub fn clear(&mut self) {
        self.tag = 0;
        self.length = 0;
    }
}

impl Default for Message {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Synchronous IPC Functions
// ============================================================================

/// Send a message to an endpoint
///
/// # Arguments
/// * `dest` - Destination endpoint capability
/// * `data` - Message data (max 4KB)
/// * `timeout_ns` - Timeout in nanoseconds (None = blocking)
///
/// # Example
/// ```no_run
/// send(endpoint, b"Hello, world!", None)?;
/// ```
pub fn send(dest: Capability, data: &[u8], timeout_ns: Option<u64>) -> Result<(), Error> {
    if data.len() > MAX_MESSAGE_SIZE {
        return Err(Error::InvalidArgument);
    }

    let timeout = timeout_ns.unwrap_or(u64::MAX);
    let result = unsafe {
        syscall::syscall4(
            nr::SEND,
            dest.as_raw(),
            data.as_ptr() as u64,
            data.len() as u64,
            timeout,
        )
    };

    Error::from_raw(result).map(|_| ())
}

/// Receive a message from an endpoint
///
/// # Arguments
/// * `src` - Source endpoint capability
/// * `buffer` - Buffer to receive into
/// * `timeout_ns` - Timeout in nanoseconds (None = blocking)
///
/// # Returns
/// Number of bytes received
///
/// # Example
/// ```no_run
/// let mut buf = [0u8; 4096];
/// let len = receive(endpoint, &mut buf, None)?;
/// println!("Received: {:?}", &buf[..len]);
/// ```
pub fn receive(src: Capability, buffer: &mut [u8], timeout_ns: Option<u64>) -> Result<usize, Error> {
    let timeout = timeout_ns.unwrap_or(u64::MAX);
    let result = unsafe {
        syscall::syscall4(
            nr::RECEIVE,
            src.as_raw(),
            buffer.as_mut_ptr() as u64,
            buffer.len() as u64,
            timeout,
        )
    };

    Error::from_raw(result).map(|n| n as usize)
}

/// Perform a synchronous RPC call
///
/// Sends a request and waits for a reply in a single operation.
///
/// # Arguments
/// * `dest` - Destination endpoint capability
/// * `request` - Request data
/// * `response` - Buffer for response
///
/// # Returns
/// Number of bytes in the response
///
/// # Example
/// ```no_run
/// let request = b"get_status";
/// let mut response = [0u8; 1024];
/// let len = call(service, request, &mut response)?;
/// ```
pub fn call(dest: Capability, request: &[u8], response: &mut [u8]) -> Result<usize, Error> {
    if request.len() > MAX_MESSAGE_SIZE || response.len() > MAX_MESSAGE_SIZE {
        return Err(Error::InvalidArgument);
    }

    let result = unsafe {
        syscall::syscall5(
            nr::CALL,
            dest.as_raw(),
            request.as_ptr() as u64,
            request.len() as u64,
            response.as_mut_ptr() as u64,
            response.len() as u64,
        )
    };

    Error::from_raw(result).map(|n| n as usize)
}

/// Reply to an incoming call
///
/// # Arguments
/// * `reply_cap` - Reply capability (provided with the incoming call)
/// * `data` - Response data
pub fn reply(reply_cap: Capability, data: &[u8]) -> Result<(), Error> {
    if data.len() > MAX_MESSAGE_SIZE {
        return Err(Error::InvalidArgument);
    }

    let result = unsafe {
        syscall::syscall3(
            nr::REPLY,
            reply_cap.as_raw(),
            data.as_ptr() as u64,
            data.len() as u64,
        )
    };

    Error::from_raw(result).map(|_| ())
}

// ============================================================================
// Notification Functions
// ============================================================================

/// Signal notification bits
///
/// Atomically OR the given bits into the notification object.
///
/// # Arguments
/// * `notif` - Notification capability
/// * `bits` - Bits to signal
pub fn signal(notif: Capability, bits: u64) -> Result<(), Error> {
    let result = unsafe { syscall::syscall2(nr::SIGNAL, notif.as_raw(), bits) };
    Error::from_raw(result).map(|_| ())
}

/// Wait for notification bits
///
/// Blocks until any of the masked bits are set.
///
/// # Arguments
/// * `notif` - Notification capability
/// * `mask` - Bits to wait for
/// * `timeout_ns` - Timeout in nanoseconds (None = blocking)
///
/// # Returns
/// The bits that were set (masked by the wait mask)
pub fn wait(notif: Capability, mask: u64, timeout_ns: Option<u64>) -> Result<u64, Error> {
    let timeout = timeout_ns.unwrap_or(u64::MAX);
    let result = unsafe { syscall::syscall3(nr::WAIT, notif.as_raw(), mask, timeout) };
    Error::from_raw(result)
}

/// Poll notification bits (non-blocking)
///
/// # Arguments
/// * `notif` - Notification capability
/// * `mask` - Bits to check
///
/// # Returns
/// The bits that are currently set (masked)
pub fn poll(notif: Capability, mask: u64) -> Result<u64, Error> {
    let result = unsafe { syscall::syscall2(nr::POLL, notif.as_raw(), mask) };
    Error::from_raw(result)
}
