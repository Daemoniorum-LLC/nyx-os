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
//!
//! # Performance Optimizations
//!
//! For high-frequency IPC, this module provides:
//! - `Message::new_uninit()` - Skip zero-init when you'll overwrite the buffer
//! - `MessagePool` - Pre-allocated message pool to avoid repeated allocations
//! - `IpcRing::submit_batch()` - Batch multiple operations in one syscall

use crate::cap::Capability;
use crate::syscall::{self, nr, Error};
use core::mem::MaybeUninit;

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
    /// Create a new empty message (zero-initialized)
    ///
    /// This is safe but involves zeroing 4KB. For performance-critical code,
    /// use `new_uninit()` when you'll immediately overwrite the buffer.
    pub const fn new() -> Self {
        Self {
            tag: 0,
            length: 0,
            data: [0; MAX_MESSAGE_SIZE],
        }
    }

    /// Create an uninitialized message (fast path)
    ///
    /// This skips zero-initialization of the 4KB buffer, providing ~50ns savings.
    /// The returned message has tag=0, length=0, and uninitialized data.
    ///
    /// # Safety
    /// The data buffer contains uninitialized memory. You MUST call `set_data()`
    /// or `write_data()` before reading from the message. Only the bytes up to
    /// `length` are considered valid.
    ///
    /// # Example
    /// ```no_run
    /// let mut msg = unsafe { Message::new_uninit() };
    /// msg.tag = 1;
    /// msg.set_data(b"Hello"); // Now safe to use
    /// ```
    #[inline]
    pub unsafe fn new_uninit() -> Self {
        let mut msg = MaybeUninit::<Self>::uninit();
        let ptr = msg.as_mut_ptr();
        // Only initialize the header, leave data buffer uninitialized
        (*ptr).tag = 0;
        (*ptr).length = 0;
        msg.assume_init()
    }

    /// Create a message with the given tag and data
    pub fn with_data(tag: u32, data: &[u8]) -> Self {
        let mut msg = Self::new();
        msg.tag = tag;
        msg.set_data(data);
        msg
    }

    /// Create a message with the given tag and data (fast path)
    ///
    /// Uses uninitialized memory and only writes the necessary bytes.
    /// ~50ns faster than `with_data()` for small payloads.
    #[inline]
    pub fn with_data_fast(tag: u32, data: &[u8]) -> Self {
        assert!(data.len() <= MAX_MESSAGE_SIZE);
        // SAFETY: We immediately initialize tag, length, and the used portion of data
        let mut msg = unsafe { Self::new_uninit() };
        msg.tag = tag;
        msg.length = data.len() as u32;
        msg.data[..data.len()].copy_from_slice(data);
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

    /// Write data directly into the message buffer
    ///
    /// Returns a mutable slice of exactly `len` bytes that the caller can write to.
    /// This is useful for zero-copy operations where you want to write directly
    /// into the message buffer.
    ///
    /// # Panics
    /// Panics if len exceeds MAX_MESSAGE_SIZE
    #[inline]
    pub fn write_data(&mut self, tag: u32, len: usize) -> &mut [u8] {
        assert!(len <= MAX_MESSAGE_SIZE);
        self.tag = tag;
        self.length = len as u32;
        &mut self.data[..len]
    }

    /// Get the message data as a slice
    #[inline]
    pub fn as_slice(&self) -> &[u8] {
        &self.data[..self.length as usize]
    }

    /// Get the message data as a mutable slice
    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.data[..self.length as usize]
    }

    /// Get the raw data buffer for direct writes
    ///
    /// # Safety
    /// Caller must ensure `length` is set correctly after writing.
    #[inline]
    pub fn data_mut(&mut self) -> &mut [u8; MAX_MESSAGE_SIZE] {
        &mut self.data
    }

    /// Clear the message (fast - doesn't zero buffer)
    #[inline]
    pub fn clear(&mut self) {
        self.tag = 0;
        self.length = 0;
    }

    /// Returns the capacity (MAX_MESSAGE_SIZE)
    #[inline]
    pub const fn capacity(&self) -> usize {
        MAX_MESSAGE_SIZE
    }

    /// Returns true if the message has no data
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.length == 0
    }

    /// Returns the length of the message data
    #[inline]
    pub const fn len(&self) -> usize {
        self.length as usize
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

// ============================================================================
// High-Performance Message Pool
// ============================================================================

/// Default pool size (16 messages = 64KB)
pub const DEFAULT_POOL_SIZE: usize = 16;

/// Pre-allocated message pool for high-frequency IPC
///
/// Avoids repeated allocation/deallocation overhead by maintaining a pool
/// of reusable message buffers. Messages are tracked with a simple bitmap.
///
/// # Example
/// ```no_run
/// let mut pool = MessagePool::<16>::new();
///
/// // Acquire a message from the pool
/// if let Some(idx) = pool.acquire() {
///     let msg = pool.get_mut(idx);
///     msg.tag = 1;
///     msg.set_data(b"Hello");
///
///     // Use the message...
///     send(endpoint, msg.as_slice(), None)?;
///
///     // Return to pool
///     pool.release(idx);
/// }
/// ```
pub struct MessagePool<const N: usize = DEFAULT_POOL_SIZE> {
    /// Pre-allocated message buffers
    messages: [Message; N],
    /// Bitmap tracking which slots are in use (1 = in use, 0 = free)
    in_use: u64,
    /// Number of messages currently in use
    used_count: usize,
}

impl<const N: usize> MessagePool<N> {
    /// Create a new message pool
    ///
    /// All messages are zero-initialized. For faster initialization,
    /// use `new_uninit()`.
    pub fn new() -> Self {
        assert!(N <= 64, "MessagePool supports max 64 messages");
        Self {
            messages: core::array::from_fn(|_| Message::new()),
            in_use: 0,
            used_count: 0,
        }
    }

    /// Create a new message pool with uninitialized messages
    ///
    /// # Safety
    /// Messages contain uninitialized data buffers. Each message must have
    /// `set_data()` called before its data is read.
    pub unsafe fn new_uninit() -> Self {
        assert!(N <= 64, "MessagePool supports max 64 messages");
        Self {
            messages: core::array::from_fn(|_| Message::new_uninit()),
            in_use: 0,
            used_count: 0,
        }
    }

    /// Acquire a message from the pool
    ///
    /// Returns the index of the acquired message, or None if pool is exhausted.
    #[inline]
    pub fn acquire(&mut self) -> Option<usize> {
        if self.used_count >= N {
            return None;
        }

        // Find first free slot (first zero bit)
        let free_mask = !self.in_use;
        if free_mask == 0 {
            return None;
        }

        let idx = free_mask.trailing_zeros() as usize;
        if idx >= N {
            return None;
        }

        self.in_use |= 1 << idx;
        self.used_count += 1;

        // Clear the message for reuse
        self.messages[idx].clear();

        Some(idx)
    }

    /// Release a message back to the pool
    ///
    /// # Panics
    /// Panics if the index is out of bounds or the slot was not in use.
    #[inline]
    pub fn release(&mut self, idx: usize) {
        assert!(idx < N, "Index out of bounds");
        assert!(self.in_use & (1 << idx) != 0, "Double release");

        self.in_use &= !(1 << idx);
        self.used_count -= 1;
    }

    /// Get a reference to a message by index
    ///
    /// # Panics
    /// Panics if index is out of bounds.
    #[inline]
    pub fn get(&self, idx: usize) -> &Message {
        assert!(idx < N, "Index out of bounds");
        &self.messages[idx]
    }

    /// Get a mutable reference to a message by index
    ///
    /// # Panics
    /// Panics if index is out of bounds.
    #[inline]
    pub fn get_mut(&mut self, idx: usize) -> &mut Message {
        assert!(idx < N, "Index out of bounds");
        &mut self.messages[idx]
    }

    /// Returns the number of messages currently in use
    #[inline]
    pub const fn used(&self) -> usize {
        self.used_count
    }

    /// Returns the number of free messages
    #[inline]
    pub const fn available(&self) -> usize {
        N - self.used_count
    }

    /// Returns the pool capacity
    #[inline]
    pub const fn capacity(&self) -> usize {
        N
    }

    /// Returns true if the pool is empty (all messages available)
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.used_count == 0
    }

    /// Returns true if the pool is full (no messages available)
    #[inline]
    pub const fn is_full(&self) -> bool {
        self.used_count == N
    }
}

impl<const N: usize> Default for MessagePool<N> {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Batch Submission Support
// ============================================================================

/// Operation type for batch submissions
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpType {
    /// Send a message
    Send = 0,
    /// Receive a message
    Receive = 1,
    /// RPC call
    Call = 2,
    /// RPC reply
    Reply = 3,
    /// Signal notification
    Signal = 4,
    /// Wait for notification
    Wait = 5,
    /// Poll notification
    Poll = 6,
}

/// Submission queue entry for batch operations
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SubmissionEntry {
    /// Operation type
    pub op: OpType,
    /// Flags (reserved)
    pub flags: u8,
    /// Reserved padding
    _reserved: u16,
    /// User data (returned in completion)
    pub user_data: u32,
    /// Target capability
    pub cap: u64,
    /// Data pointer or value
    pub addr: u64,
    /// Length or additional parameter
    pub len: u32,
    /// Timeout or flags
    pub param: u32,
}

impl SubmissionEntry {
    /// Create a send operation
    #[inline]
    pub fn send(cap: Capability, data: &[u8], user_data: u32) -> Self {
        Self {
            op: OpType::Send,
            flags: 0,
            _reserved: 0,
            user_data,
            cap: cap.as_raw(),
            addr: data.as_ptr() as u64,
            len: data.len() as u32,
            param: 0,
        }
    }

    /// Create a receive operation
    #[inline]
    pub fn receive(cap: Capability, buffer: &mut [u8], user_data: u32) -> Self {
        Self {
            op: OpType::Receive,
            flags: 0,
            _reserved: 0,
            user_data,
            cap: cap.as_raw(),
            addr: buffer.as_mut_ptr() as u64,
            len: buffer.len() as u32,
            param: 0,
        }
    }

    /// Create a signal operation
    #[inline]
    pub fn signal(cap: Capability, bits: u64, user_data: u32) -> Self {
        Self {
            op: OpType::Signal,
            flags: 0,
            _reserved: 0,
            user_data,
            cap: cap.as_raw(),
            addr: bits,
            len: 0,
            param: 0,
        }
    }

    /// Create a poll operation
    #[inline]
    pub fn poll(cap: Capability, mask: u64, user_data: u32) -> Self {
        Self {
            op: OpType::Poll,
            flags: 0,
            _reserved: 0,
            user_data,
            cap: cap.as_raw(),
            addr: mask,
            len: 0,
            param: 0,
        }
    }
}

/// Completion queue entry
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct CompletionEntry {
    /// User data from submission
    pub user_data: u32,
    /// Result code (negative = error)
    pub result: i32,
    /// Additional flags
    pub flags: u32,
    /// Reserved
    _reserved: u32,
}

impl CompletionEntry {
    /// Check if the operation succeeded
    #[inline]
    pub fn is_ok(&self) -> bool {
        self.result >= 0
    }

    /// Convert result to Error type
    #[inline]
    pub fn to_result(&self) -> Result<u64, Error> {
        Error::from_raw(self.result as i64)
    }
}

/// Batch submission builder
///
/// Allows building a batch of IPC operations to submit in one syscall.
///
/// # Example
/// ```no_run
/// let mut batch = SubmissionBatch::<8>::new();
///
/// batch.push_send(endpoint1, b"msg1", 1);
/// batch.push_send(endpoint2, b"msg2", 2);
/// batch.push_signal(notif, 0x1, 3);
///
/// ring.submit_batch(&batch)?;
/// ```
pub struct SubmissionBatch<const N: usize = 16> {
    entries: [MaybeUninit<SubmissionEntry>; N],
    count: usize,
}

impl<const N: usize> SubmissionBatch<N> {
    /// Create a new empty batch
    #[inline]
    pub const fn new() -> Self {
        Self {
            // SAFETY: MaybeUninit doesn't require initialization
            entries: unsafe { MaybeUninit::uninit().assume_init() },
            count: 0,
        }
    }

    /// Push a submission entry
    ///
    /// Returns false if the batch is full.
    #[inline]
    pub fn push(&mut self, entry: SubmissionEntry) -> bool {
        if self.count >= N {
            return false;
        }
        self.entries[self.count].write(entry);
        self.count += 1;
        true
    }

    /// Push a send operation
    #[inline]
    pub fn push_send(&mut self, cap: Capability, data: &[u8], user_data: u32) -> bool {
        self.push(SubmissionEntry::send(cap, data, user_data))
    }

    /// Push a signal operation
    #[inline]
    pub fn push_signal(&mut self, cap: Capability, bits: u64, user_data: u32) -> bool {
        self.push(SubmissionEntry::signal(cap, bits, user_data))
    }

    /// Push a poll operation
    #[inline]
    pub fn push_poll(&mut self, cap: Capability, mask: u64, user_data: u32) -> bool {
        self.push(SubmissionEntry::poll(cap, mask, user_data))
    }

    /// Get the number of entries
    #[inline]
    pub const fn len(&self) -> usize {
        self.count
    }

    /// Check if empty
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Check if full
    #[inline]
    pub const fn is_full(&self) -> bool {
        self.count == N
    }

    /// Clear all entries
    #[inline]
    pub fn clear(&mut self) {
        self.count = 0;
    }

    /// Get entries as a slice
    #[inline]
    pub fn as_slice(&self) -> &[SubmissionEntry] {
        // SAFETY: entries[0..count] are initialized
        unsafe {
            core::slice::from_raw_parts(self.entries.as_ptr() as *const SubmissionEntry, self.count)
        }
    }
}

impl<const N: usize> Default for SubmissionBatch<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl IpcRing {
    /// Submit a batch of operations
    ///
    /// This is more efficient than individual syscalls when you have multiple
    /// operations to perform.
    ///
    /// # Arguments
    /// * `batch` - The batch of operations to submit
    ///
    /// # Returns
    /// Number of operations successfully submitted
    pub fn submit_batch<const N: usize>(&self, batch: &SubmissionBatch<N>) -> Result<u32, Error> {
        if batch.is_empty() {
            return Ok(0);
        }

        // Submit all entries and enter the ring
        let result = unsafe {
            syscall::syscall4(
                nr::RING_ENTER,
                self.handle.as_raw(),
                batch.as_slice().as_ptr() as u64,
                batch.len() as u64,
                0, // flags
            )
        };

        Error::from_raw(result).map(|n| n as u32)
    }
}
