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

// ============================================================================
// Lock-Free Atomic Message Pool
// ============================================================================

use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use core::cell::UnsafeCell;

/// Thread-safe, lock-free message pool
///
/// Uses atomic operations for allocation tracking, allowing concurrent
/// access from multiple threads without locks.
///
/// # Example
/// ```no_run
/// use std::sync::Arc;
///
/// let pool = Arc::new(AtomicMessagePool::<16>::new());
///
/// // Can be used from multiple threads
/// let pool_clone = pool.clone();
/// std::thread::spawn(move || {
///     if let Some(idx) = pool_clone.acquire() {
///         // Use message...
///         pool_clone.release(idx);
///     }
/// });
/// ```
pub struct AtomicMessagePool<const N: usize = DEFAULT_POOL_SIZE> {
    /// Pre-allocated message buffers (UnsafeCell for interior mutability)
    messages: [UnsafeCell<Message>; N],
    /// Atomic bitmap tracking which slots are in use
    in_use: AtomicU64,
    /// Atomic count of used messages (for fast full check)
    used_count: AtomicUsize,
}

impl<const N: usize> AtomicMessagePool<N> {
    /// Create a new atomic message pool
    pub fn new() -> Self {
        assert!(N <= 64, "AtomicMessagePool supports max 64 messages");
        Self {
            messages: core::array::from_fn(|_| UnsafeCell::new(Message::new())),
            in_use: AtomicU64::new(0),
            used_count: AtomicUsize::new(0),
        }
    }

    /// Acquire a message from the pool (lock-free)
    ///
    /// Returns the index of the acquired message, or None if pool is exhausted.
    /// This operation is wait-free for the common case.
    #[inline]
    pub fn acquire(&self) -> Option<usize> {
        loop {
            let current = self.in_use.load(Ordering::Acquire);
            let free_mask = !current;

            if free_mask == 0 {
                return None; // Pool exhausted
            }

            let idx = free_mask.trailing_zeros() as usize;
            if idx >= N {
                return None;
            }

            let new_mask = current | (1 << idx);

            // Try to atomically claim this slot
            match self.in_use.compare_exchange_weak(
                current,
                new_mask,
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    self.used_count.fetch_add(1, Ordering::Relaxed);
                    return Some(idx);
                }
                Err(_) => continue, // Retry on contention
            }
        }
    }

    /// Release a message back to the pool (lock-free)
    ///
    /// # Safety
    /// The caller must ensure no other thread is using this message.
    #[inline]
    pub fn release(&self, idx: usize) {
        debug_assert!(idx < N, "Index out of bounds");

        let mask = 1u64 << idx;
        let prev = self.in_use.fetch_and(!mask, Ordering::Release);

        debug_assert!(prev & mask != 0, "Double release detected");
        self.used_count.fetch_sub(1, Ordering::Relaxed);
    }

    /// Get a mutable reference to a message by index
    ///
    /// # Safety
    /// Caller must ensure exclusive access to this index (i.e., they acquired it).
    #[inline]
    pub unsafe fn get_mut(&self, idx: usize) -> &mut Message {
        debug_assert!(idx < N, "Index out of bounds");
        // SAFETY: Caller guarantees exclusive access via acquire()
        &mut *self.messages[idx].get()
    }

    /// Get a reference to a message by index
    ///
    /// # Safety
    /// Caller must ensure they have acquired this index.
    #[inline]
    pub unsafe fn get(&self, idx: usize) -> &Message {
        debug_assert!(idx < N, "Index out of bounds");
        &*self.messages[idx].get()
    }

    /// Returns the approximate number of messages in use
    #[inline]
    pub fn used(&self) -> usize {
        self.used_count.load(Ordering::Relaxed)
    }

    /// Returns the approximate number of free messages
    #[inline]
    pub fn available(&self) -> usize {
        N - self.used()
    }

    /// Returns the pool capacity
    #[inline]
    pub const fn capacity(&self) -> usize {
        N
    }
}

impl<const N: usize> Default for AtomicMessagePool<N> {
    fn default() -> Self {
        Self::new()
    }
}

// SAFETY: The pool uses atomic operations for thread safety
// and UnsafeCell for interior mutability
unsafe impl<const N: usize> Sync for AtomicMessagePool<N> {}
unsafe impl<const N: usize> Send for AtomicMessagePool<N> {}

// ============================================================================
// Zero-Copy Shared Memory IPC
// ============================================================================

/// Protection flags for shared memory regions
pub mod shm_prot {
    /// Region is readable
    pub const READ: u32 = 1 << 0;
    /// Region is writable
    pub const WRITE: u32 = 1 << 1;
    /// Region is executable
    pub const EXEC: u32 = 1 << 2;
}

/// A shared memory region for zero-copy IPC
///
/// Allows multiple processes to access the same physical memory,
/// eliminating copy overhead for large data transfers.
///
/// # Example
/// ```no_run
/// // Create a 1MB shared region
/// let region = SharedRegion::new(1 << 20)?;
///
/// // Write data directly
/// region.as_mut_slice()[..4].copy_from_slice(b"test");
///
/// // Grant read access to another process
/// let view = region.grant(other_cap, shm_prot::READ)?;
/// send(other_process, &view.to_bytes(), None)?;
/// ```
pub struct SharedRegion {
    /// Capability to the shared memory object
    cap: Capability,
    /// Base address of the mapping
    base: *mut u8,
    /// Size of the region
    size: usize,
}

impl SharedRegion {
    /// Create a new shared memory region
    ///
    /// # Arguments
    /// * `size` - Size in bytes (will be rounded up to page size)
    pub fn new(size: usize) -> Result<Self, Error> {
        let result = unsafe {
            syscall::syscall2(nr::SHM_CREATE, size as u64, 0)
        };

        let cap_id = Error::from_raw(result)?;
        let cap = Capability::from_raw(cap_id);

        // Map the region into our address space
        let map_result = unsafe {
            syscall::syscall4(
                nr::SHM_MAP,
                cap.as_raw(),
                0, // Let kernel choose address
                size as u64,
                (shm_prot::READ | shm_prot::WRITE) as u64,
            )
        };

        let base = Error::from_raw(map_result)? as *mut u8;

        Ok(Self { cap, base, size })
    }

    /// Grant access to another process
    ///
    /// # Arguments
    /// * `target` - Capability to the target process/endpoint
    /// * `protection` - Access rights (shm_prot::READ, shm_prot::WRITE, etc.)
    ///
    /// # Returns
    /// A `SharedView` that can be sent to the other process
    pub fn grant(&self, target: Capability, protection: u32) -> Result<SharedView, Error> {
        let result = unsafe {
            syscall::syscall3(
                nr::SHM_GRANT,
                self.cap.as_raw(),
                target.as_raw(),
                protection as u64,
            )
        };

        let view_cap = Error::from_raw(result)?;

        Ok(SharedView {
            cap: Capability::from_raw(view_cap),
            size: self.size,
            protection,
        })
    }

    /// Get the region as a byte slice
    #[inline]
    pub fn as_slice(&self) -> &[u8] {
        unsafe { core::slice::from_raw_parts(self.base, self.size) }
    }

    /// Get the region as a mutable byte slice
    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self.base, self.size) }
    }

    /// Get the base address
    #[inline]
    pub fn as_ptr(&self) -> *const u8 {
        self.base
    }

    /// Get the mutable base address
    #[inline]
    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.base
    }

    /// Get the size of the region
    #[inline]
    pub fn size(&self) -> usize {
        self.size
    }

    /// Get the capability handle
    #[inline]
    pub fn capability(&self) -> Capability {
        self.cap
    }
}

impl Drop for SharedRegion {
    fn drop(&mut self) {
        unsafe {
            // Unmap the region
            let _ = syscall::syscall2(nr::SHM_UNMAP, self.base as u64, self.size as u64);
            // Release the capability
            let _ = syscall::syscall1(nr::CAP_DROP, self.cap.as_raw());
        }
    }
}

/// A view into a shared region (for sending to other processes)
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SharedView {
    /// Capability to map this view
    cap: Capability,
    /// Size of the shared region
    size: usize,
    /// Protection flags
    protection: u32,
}

impl SharedView {
    /// Map this view into the current process
    pub fn map(&self) -> Result<MappedView, Error> {
        let result = unsafe {
            syscall::syscall4(
                nr::SHM_MAP,
                self.cap.as_raw(),
                0,
                self.size as u64,
                self.protection as u64,
            )
        };

        let base = Error::from_raw(result)? as *mut u8;

        Ok(MappedView {
            base,
            size: self.size,
            protection: self.protection,
        })
    }

    /// Serialize to bytes for IPC transfer
    #[inline]
    pub fn to_bytes(&self) -> [u8; 24] {
        let mut buf = [0u8; 24];
        buf[0..8].copy_from_slice(&self.cap.as_raw().to_le_bytes());
        buf[8..16].copy_from_slice(&(self.size as u64).to_le_bytes());
        buf[16..20].copy_from_slice(&self.protection.to_le_bytes());
        buf
    }

    /// Deserialize from bytes
    #[inline]
    pub fn from_bytes(bytes: &[u8; 24]) -> Self {
        Self {
            cap: Capability::from_raw(u64::from_le_bytes(bytes[0..8].try_into().unwrap())),
            size: u64::from_le_bytes(bytes[8..16].try_into().unwrap()) as usize,
            protection: u32::from_le_bytes(bytes[16..20].try_into().unwrap()),
        }
    }
}

/// A mapped view of shared memory
pub struct MappedView {
    base: *mut u8,
    size: usize,
    protection: u32,
}

impl MappedView {
    /// Get as a slice (requires READ permission)
    #[inline]
    pub fn as_slice(&self) -> &[u8] {
        debug_assert!(self.protection & shm_prot::READ != 0);
        unsafe { core::slice::from_raw_parts(self.base, self.size) }
    }

    /// Get as a mutable slice (requires WRITE permission)
    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        debug_assert!(self.protection & shm_prot::WRITE != 0);
        unsafe { core::slice::from_raw_parts_mut(self.base, self.size) }
    }

    /// Get the size
    #[inline]
    pub fn size(&self) -> usize {
        self.size
    }
}

impl Drop for MappedView {
    fn drop(&mut self) {
        unsafe {
            let _ = syscall::syscall2(nr::SHM_UNMAP, self.base as u64, self.size as u64);
        }
    }
}

// ============================================================================
// SIMD-Optimized Aligned Message
// ============================================================================

/// Cache line size for alignment
pub const CACHE_LINE_SIZE: usize = 64;

/// A cache-line aligned message for optimal SIMD performance
///
/// Alignment to 64 bytes ensures:
/// - No false sharing between CPU cores
/// - Optimal SIMD load/store operations
/// - Better cache utilization
#[repr(C, align(64))]
pub struct AlignedMessage {
    /// Message tag
    pub tag: u32,
    /// Data length
    pub length: u32,
    /// Padding to align data to cache line
    _pad: [u8; 56],
    /// Aligned data buffer
    pub data: AlignedBuffer,
}

/// 64-byte aligned buffer for SIMD operations
#[repr(C, align(64))]
pub struct AlignedBuffer {
    bytes: [u8; MAX_MESSAGE_SIZE],
}

impl AlignedMessage {
    /// Create a new aligned message
    #[inline]
    pub const fn new() -> Self {
        Self {
            tag: 0,
            length: 0,
            _pad: [0; 56],
            data: AlignedBuffer {
                bytes: [0; MAX_MESSAGE_SIZE],
            },
        }
    }

    /// Create without zero-initialization (fast path)
    #[inline]
    pub unsafe fn new_uninit() -> Self {
        let mut msg = MaybeUninit::<Self>::uninit();
        let ptr = msg.as_mut_ptr();
        (*ptr).tag = 0;
        (*ptr).length = 0;
        msg.assume_init()
    }

    /// Set data with explicit SIMD hints
    #[inline]
    pub fn set_data(&mut self, data: &[u8]) {
        assert!(data.len() <= MAX_MESSAGE_SIZE);
        self.length = data.len() as u32;

        // Use copy_nonoverlapping for best autovectorization
        unsafe {
            core::ptr::copy_nonoverlapping(
                data.as_ptr(),
                self.data.bytes.as_mut_ptr(),
                data.len(),
            );
        }
    }

    /// Copy data using explicit 64-byte chunks (AVX-512 friendly)
    #[inline]
    pub fn set_data_chunked(&mut self, data: &[u8]) {
        assert!(data.len() <= MAX_MESSAGE_SIZE);
        self.length = data.len() as u32;

        let chunks = data.len() / 64;
        let remainder = data.len() % 64;

        // Copy 64-byte aligned chunks
        for i in 0..chunks {
            let src = unsafe { data.as_ptr().add(i * 64) };
            let dst = unsafe { self.data.bytes.as_mut_ptr().add(i * 64) };
            unsafe {
                core::ptr::copy_nonoverlapping(src, dst, 64);
            }
        }

        // Copy remainder
        if remainder > 0 {
            let offset = chunks * 64;
            self.data.bytes[offset..offset + remainder]
                .copy_from_slice(&data[offset..]);
        }
    }

    /// Get data as slice
    #[inline]
    pub fn as_slice(&self) -> &[u8] {
        &self.data.bytes[..self.length as usize]
    }

    /// Get the raw buffer
    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.data.bytes[..self.length as usize]
    }

    /// Clear the message
    #[inline]
    pub fn clear(&mut self) {
        self.tag = 0;
        self.length = 0;
    }
}

impl Default for AlignedMessage {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// CPU Affinity Hints
// ============================================================================

/// CPU affinity hint for IPC optimization
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AffinityHint {
    /// No preference (default scheduling)
    None = 0,
    /// Prefer same core as target (lowest latency)
    SameCore = 1,
    /// Prefer same L2 cache (good latency, less contention)
    SameL2 = 2,
    /// Prefer same NUMA node (good for large data)
    SameNuma = 3,
    /// Prefer different core (for parallel workloads)
    DifferentCore = 4,
}

/// Set CPU affinity hint for the current thread's IPC
///
/// This is a hint to the kernel scheduler to optimize placement
/// of communicating threads.
///
/// # Example
/// ```no_run
/// // Request same-core placement for latency-sensitive IPC
/// set_affinity_hint(endpoint, AffinityHint::SameCore)?;
/// ```
pub fn set_affinity_hint(endpoint: Capability, hint: AffinityHint) -> Result<(), Error> {
    let result = unsafe {
        syscall::syscall2(nr::IPC_AFFINITY, endpoint.as_raw(), hint as u64)
    };
    Error::from_raw(result).map(|_| ())
}

/// Get the recommended affinity for communicating with an endpoint
pub fn get_affinity_hint(endpoint: Capability) -> Result<AffinityHint, Error> {
    let result = unsafe {
        syscall::syscall1(nr::IPC_GET_AFFINITY, endpoint.as_raw())
    };
    let hint = Error::from_raw(result)? as u8;
    Ok(match hint {
        0 => AffinityHint::None,
        1 => AffinityHint::SameCore,
        2 => AffinityHint::SameL2,
        3 => AffinityHint::SameNuma,
        4 => AffinityHint::DifferentCore,
        _ => AffinityHint::None,
    })
}

// ============================================================================
// Completion Polling Mode
// ============================================================================

/// Flags for IPC ring setup
pub mod ring_flags {
    /// Enable kernel-side polling (eliminates syscalls for submissions)
    pub const SQPOLL: u32 = 1 << 0;
    /// Use single issuer mode (no locking needed)
    pub const SINGLE_ISSUER: u32 = 1 << 1;
    /// Defer task work to submission time
    pub const DEFER_TASKRUN: u32 = 1 << 2;
    /// Use cooperative task running
    pub const COOP_TASKRUN: u32 = 1 << 3;
}

/// A polling-mode IPC ring for ultra-low latency
///
/// When the kernel polling thread is active, submissions don't
/// require syscalls - just write to shared memory.
///
/// # Example
/// ```no_run
/// let ring = PollingRing::new(256, 512, 1000)?; // 1ms idle timeout
///
/// // Submit without syscall when kernel thread is polling
/// ring.submit_nosyscall(&batch);
///
/// // Read completions from shared memory
/// while let Some(cqe) = ring.poll_completion() {
///     handle_completion(cqe);
/// }
/// ```
///
/// # Safety Guarantees
///
/// The raw pointers in this struct point to kernel-mapped shared memory.
/// The memory is guaranteed to remain valid as long as:
///
/// 1. The `handle` capability is valid (not revoked)
/// 2. The kernel ring hasn't been destroyed
///
/// The kernel guarantees that the mapped memory region lives as long as the
/// capability, so dropping this struct (which closes the capability) is the
/// only way the pointers can become invalid.
///
/// # Thread Safety
///
/// This struct is explicitly `!Sync` because the ring is designed for
/// single-issuer use. Only one thread should submit at a time. Multiple
/// threads may safely move the ring between them (hence `Send`).
pub struct PollingRing {
    /// Ring capability - dropping this invalidates all pointers
    handle: Capability,
    /// Submission queue head (shared with kernel, read-only for userspace)
    sq_head: *const AtomicU64,
    /// Submission queue tail (userspace owned, write for userspace)
    sq_tail: *mut AtomicU64,
    /// Completion queue head (userspace owned, write for userspace)
    cq_head: *mut AtomicU64,
    /// Completion queue tail (shared with kernel, read-only for userspace)
    cq_tail: *const AtomicU64,
    /// Submission queue entries array
    sq_entries: *mut SubmissionEntry,
    /// Completion queue entries array
    cq_entries: *const CompletionEntry,
    /// Queue size mask (size - 1)
    sq_mask: u32,
    cq_mask: u32,
    /// Marker to prevent Sync (only Send is safe)
    _not_sync: core::marker::PhantomData<*mut ()>,
}

impl PollingRing {
    /// Create a new polling-mode IPC ring
    ///
    /// # Arguments
    /// * `sq_size` - Submission queue size (power of 2)
    /// * `cq_size` - Completion queue size (power of 2)
    /// * `idle_timeout_ms` - Time before kernel stops polling (0 = never stop)
    pub fn new(sq_size: u32, cq_size: u32, idle_timeout_ms: u32) -> Result<Self, Error> {
        assert!(sq_size.is_power_of_two());
        assert!(cq_size.is_power_of_two());

        let flags = ring_flags::SQPOLL | ring_flags::SINGLE_ISSUER;

        let result = unsafe {
            syscall::syscall5(
                nr::RING_SETUP,
                sq_size as u64,
                cq_size as u64,
                flags as u64,
                idle_timeout_ms as u64,
                0,
            )
        };

        let ring_id = Error::from_raw(result)?;
        let handle = Capability::from_raw(ring_id);

        // Get ring memory mapping info
        let mut ring_info = RingMmapInfo::default();
        let info_result = unsafe {
            syscall::syscall2(
                nr::RING_MMAP_INFO,
                handle.as_raw(),
                &mut ring_info as *mut _ as u64,
            )
        };
        Error::from_raw(info_result)?;

        Ok(Self {
            handle,
            sq_head: ring_info.sq_head as *const AtomicU64,
            sq_tail: ring_info.sq_tail as *mut AtomicU64,
            cq_head: ring_info.cq_head as *mut AtomicU64,
            cq_tail: ring_info.cq_tail as *const AtomicU64,
            sq_entries: ring_info.sq_entries as *mut SubmissionEntry,
            cq_entries: ring_info.cq_entries as *const CompletionEntry,
            sq_mask: sq_size - 1,
            cq_mask: cq_size - 1,
            _not_sync: core::marker::PhantomData,
        })
    }

    /// Submit entries without a syscall (polling mode)
    ///
    /// Returns the number of entries submitted. Only works when
    /// the kernel polling thread is active.
    #[inline]
    pub fn submit_nosyscall<const N: usize>(&self, batch: &SubmissionBatch<N>) -> u32 {
        let entries = batch.as_slice();
        if entries.is_empty() {
            return 0;
        }

        unsafe {
            let tail = (*self.sq_tail).load(Ordering::Relaxed) as u32;
            let head = (*self.sq_head).load(Ordering::Acquire) as u32;
            let available = self.sq_mask + 1 - (tail.wrapping_sub(head));

            let to_submit = (entries.len() as u32).min(available);

            for i in 0..to_submit {
                let idx = (tail + i) & self.sq_mask;
                core::ptr::write(
                    self.sq_entries.add(idx as usize),
                    entries[i as usize],
                );
            }

            // Memory barrier before updating tail
            (*self.sq_tail).store((tail + to_submit) as u64, Ordering::Release);

            to_submit
        }
    }

    /// Poll for a completion without syscall
    #[inline]
    pub fn poll_completion(&self) -> Option<CompletionEntry> {
        unsafe {
            let head = (*self.cq_head).load(Ordering::Relaxed) as u32;
            let tail = (*self.cq_tail).load(Ordering::Acquire) as u32;

            if head == tail {
                return None;
            }

            let idx = head & self.cq_mask;
            let entry = core::ptr::read(self.cq_entries.add(idx as usize));

            (*self.cq_head).store((head + 1) as u64, Ordering::Release);

            Some(entry)
        }
    }

    /// Wake up the kernel polling thread if it's idle
    pub fn wake_poller(&self) -> Result<(), Error> {
        let result = unsafe {
            syscall::syscall1(nr::RING_WAKE, self.handle.as_raw())
        };
        Error::from_raw(result).map(|_| ())
    }

    /// Get the underlying capability handle
    #[inline]
    pub fn handle(&self) -> Capability {
        self.handle
    }
}

// SAFETY: PollingRing can be safely sent between threads because:
//
// 1. All pointer targets are in kernel-managed memory that lives as long as the
//    capability handle (which is owned by this struct)
// 2. All shared state is accessed through AtomicU64 with appropriate orderings
// 3. The struct is `!Sync` (enforced by PhantomData<*mut ()>), so no concurrent
//    access from multiple threads is possible
// 4. Moving the ring between threads is safe because the kernel doesn't care
//    which thread issues operations (it's capability-based, not thread-based)
//
// Note: This is NOT Sync because the ring protocol assumes single-issuer semantics.
// Concurrent submissions from multiple threads would corrupt the queue state.
unsafe impl Send for PollingRing {}

impl Drop for PollingRing {
    fn drop(&mut self) {
        // Close the ring capability, which tells the kernel to:
        // 1. Stop the polling thread (if any)
        // 2. Unmap the shared memory region
        // 3. Free kernel resources
        //
        // After this, all pointers in this struct become invalid,
        // but that's okay because the struct is being dropped.
        let _ = unsafe {
            syscall::syscall1(nr::RING_DESTROY, self.handle.as_raw())
        };
    }
}

/// Ring memory mapping information
#[repr(C)]
#[derive(Default)]
struct RingMmapInfo {
    sq_head: u64,
    sq_tail: u64,
    cq_head: u64,
    cq_tail: u64,
    sq_entries: u64,
    cq_entries: u64,
}

// ============================================================================
// Registered Buffers
// ============================================================================

/// A buffer registered with the kernel for fast IPC
///
/// Registered buffers are pinned in memory and pre-mapped in the kernel,
/// eliminating page table walks on each IPC operation.
///
/// # Example
/// ```no_run
/// let mut buf = RegisteredBuffer::new(4096)?;
///
/// // Write data
/// buf.as_mut_slice()[..5].copy_from_slice(b"hello");
///
/// // Send using the registered buffer (faster than regular send)
/// ring.send_registered(&buf, 0, 5, endpoint)?;
/// ```
pub struct RegisteredBuffer {
    /// Capability to the registered buffer
    cap: Capability,
    /// Buffer pointer
    ptr: *mut u8,
    /// Buffer size
    size: usize,
    /// Buffer index (for kernel reference)
    index: u32,
}

impl RegisteredBuffer {
    /// Create and register a new buffer
    pub fn new(size: usize) -> Result<Self, Error> {
        // Allocate page-aligned memory via mmap
        let ptr_result = unsafe {
            syscall::syscall4(
                nr::MEM_MAP,
                0, // Let kernel choose address
                size as u64,
                (shm_prot::READ | shm_prot::WRITE) as u64,
                0, // flags
            )
        };

        let ptr = Error::from_raw(ptr_result)? as *mut u8;

        // Register the buffer with the IPC subsystem
        let reg_result = unsafe {
            syscall::syscall2(nr::BUF_REGISTER, ptr as u64, size as u64)
        };

        let cap_and_idx = Error::from_raw(reg_result)?;
        let cap = Capability::from_raw(cap_and_idx & 0xFFFF_FFFF);
        let index = (cap_and_idx >> 32) as u32;

        Ok(Self { cap, ptr, size, index })
    }

    /// Get the buffer as a slice
    #[inline]
    pub fn as_slice(&self) -> &[u8] {
        unsafe { core::slice::from_raw_parts(self.ptr, self.size) }
    }

    /// Get the buffer as a mutable slice
    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self.ptr, self.size) }
    }

    /// Get the buffer index (for use in submissions)
    #[inline]
    pub fn index(&self) -> u32 {
        self.index
    }

    /// Get the size
    #[inline]
    pub fn size(&self) -> usize {
        self.size
    }

    /// Get the capability
    #[inline]
    pub fn capability(&self) -> Capability {
        self.cap
    }
}

impl Drop for RegisteredBuffer {
    fn drop(&mut self) {
        unsafe {
            // Unregister the buffer
            let _ = syscall::syscall1(nr::BUF_UNREGISTER, self.cap.as_raw());
            // Free the memory
            let _ = syscall::syscall2(nr::MEM_UNMAP, self.ptr as u64, self.size as u64);
        }
    }
}

/// Extension methods for IpcRing to use registered buffers
impl IpcRing {
    /// Send data from a registered buffer (faster path)
    pub fn send_registered(
        &self,
        buf: &RegisteredBuffer,
        offset: usize,
        len: usize,
        dest: Capability,
    ) -> Result<(), Error> {
        assert!(offset + len <= buf.size());

        let result = unsafe {
            syscall::syscall5(
                nr::SEND_REGISTERED,
                dest.as_raw(),
                buf.index() as u64,
                offset as u64,
                len as u64,
                0,
            )
        };

        Error::from_raw(result).map(|_| ())
    }

    /// Receive into a registered buffer (faster path)
    pub fn receive_registered(
        &self,
        buf: &mut RegisteredBuffer,
        offset: usize,
        max_len: usize,
        src: Capability,
    ) -> Result<usize, Error> {
        assert!(offset + max_len <= buf.size());

        let result = unsafe {
            syscall::syscall5(
                nr::RECV_REGISTERED,
                src.as_raw(),
                buf.index() as u64,
                offset as u64,
                max_len as u64,
                0,
            )
        };

        Error::from_raw(result).map(|n| n as usize)
    }
}
