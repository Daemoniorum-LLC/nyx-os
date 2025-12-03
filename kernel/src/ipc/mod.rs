//! # Async-First IPC Subsystem
//!
//! io_uring-style completion queue architecture for all inter-process communication.
//!
//! ## Design
//!
//! All IPC operations are asynchronous by default:
//! 1. Userspace submits operations to a submission queue (SQ)
//! 2. Kernel processes operations and posts results to completion queue (CQ)
//! 3. Userspace polls or waits for completions
//!
//! This design enables:
//! - Batching multiple operations in a single syscall
//! - Zero-copy message passing via memory grants
//! - Efficient polling without context switches
//! - Timeout and cancellation support

mod ring;
mod message;
mod endpoint;
pub mod notification;
pub mod shm;

pub use ring::{IpcRing, SqEntry, CqEntry, IpcOpcode, SqFlags, CqFlags, ring_flags};
pub use message::{Message, MessageHeader, MemoryGrant};
pub use endpoint::Endpoint;
pub use notification::Notification;
pub use shm::{SharedRegion, SharedFlags, ShmError};

use crate::cap::{Capability, CapError, ObjectId, ObjectType, Rights};
use spin::RwLock;
use alloc::collections::BTreeMap;

/// Global endpoint registry
static ENDPOINTS: RwLock<BTreeMap<ObjectId, Endpoint>> = RwLock::new(BTreeMap::new());

/// Global notification registry
static NOTIFICATIONS: RwLock<BTreeMap<ObjectId, Notification>> = RwLock::new(BTreeMap::new());

/// Global IPC ring registry
static RINGS: RwLock<BTreeMap<ObjectId, IpcRing>> = RwLock::new(BTreeMap::new());

/// Initialize the IPC subsystem
pub fn init() {
    log::trace!("IPC subsystem initialized");
}

/// Create a new IPC ring for a thread
pub fn create_ring(sq_size: u32, cq_size: u32, _flags: u32) -> Result<Capability, IpcError> {
    // Validate sizes (must be power of 2)
    if !sq_size.is_power_of_two() || !cq_size.is_power_of_two() {
        return Err(IpcError::InvalidSize);
    }

    // Max size limits
    if sq_size > MAX_SQ_SIZE || cq_size > MAX_CQ_SIZE {
        return Err(IpcError::InvalidSize);
    }

    let ring = IpcRing::new(sq_size, cq_size)?;
    let object_id = ObjectId::new(ObjectType::IpcRing);

    // Store ring in registry
    RINGS.write().insert(object_id, ring);

    // SAFETY: Kernel creating initial capability
    let cap = unsafe {
        Capability::new_unchecked(object_id, Rights::IPC_FULL)
    };

    Ok(cap)
}

/// Create a new IPC endpoint
pub fn create_endpoint() -> Result<Capability, IpcError> {
    let endpoint = Endpoint::new();
    let object_id = ObjectId::new(ObjectType::Endpoint);

    ENDPOINTS.write().insert(object_id, endpoint);

    // SAFETY: Kernel creating initial capability
    let cap = unsafe {
        Capability::new_unchecked(object_id, Rights::IPC_FULL)
    };

    Ok(cap)
}

/// Create a new notification object
pub fn create_notification() -> Result<Capability, IpcError> {
    let notification = Notification::new();
    let object_id = ObjectId::new(ObjectType::Notification);

    NOTIFICATIONS.write().insert(object_id, notification);

    let cap = unsafe {
        Capability::new_unchecked(
            object_id,
            Rights::SIGNAL | Rights::WAIT | Rights::POLL | Rights::GRANT,
        )
    };

    Ok(cap)
}

/// Maximum submission queue size
const MAX_SQ_SIZE: u32 = 32768;

/// Maximum completion queue size
const MAX_CQ_SIZE: u32 = 65536;

/// IPC errors
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IpcError {
    /// Invalid ring size
    InvalidSize,
    /// Queue is full
    QueueFull,
    /// Queue is empty
    QueueEmpty,
    /// Operation timed out
    Timeout,
    /// Operation was cancelled
    Cancelled,
    /// Invalid endpoint
    InvalidEndpoint,
    /// Capability error
    Capability(CapError),
    /// Message too large
    MessageTooLarge,
    /// Would block (for non-blocking operations)
    WouldBlock,
    /// Peer disconnected
    Disconnected,
    /// Invalid operation for this object type
    InvalidOperation,
    /// Internal error
    InternalError,
}

impl From<CapError> for IpcError {
    fn from(err: CapError) -> Self {
        IpcError::Capability(err)
    }
}

/// Process a submission queue entry
pub fn process_sq_entry(
    entry: &SqEntry,
    ring: &mut IpcRing,
) -> Result<(), IpcError> {
    match entry.opcode {
        IpcOpcode::Send => process_send(entry, ring),
        IpcOpcode::Receive => process_receive(entry, ring),
        IpcOpcode::Call => process_call(entry, ring),
        IpcOpcode::Reply => process_reply(entry, ring),
        IpcOpcode::Signal => process_signal(entry, ring),
        IpcOpcode::Wait => process_wait(entry, ring),
        IpcOpcode::Poll => process_poll(entry, ring),
        IpcOpcode::TensorAlloc => process_tensor_alloc(entry, ring),
        IpcOpcode::Inference => process_inference(entry, ring),
        _ => Err(IpcError::InvalidOperation),
    }
}

/// Process Send operation
/// params[0] = message data pointer
/// params[1] = message length
/// params[2] = flags/tag
fn process_send(entry: &SqEntry, ring: &mut IpcRing) -> Result<(), IpcError> {
    let endpoint_id = ObjectId::from_raw(entry.cap_slot as u64);

    // Look up the endpoint
    let endpoints = ENDPOINTS.read();
    let endpoint = endpoints
        .get(&endpoint_id)
        .ok_or(IpcError::InvalidEndpoint)?;

    // Build message from params
    let msg = Message::simple(entry.params[2] as u32, &[]);

    // Try non-blocking send
    match endpoint.send(msg) {
        Ok(()) => {
            // Post completion
            if !entry.flags.contains(SqFlags::NO_CQE) {
                let cqe = CqEntry {
                    user_data: entry.user_data,
                    result: 0,
                    data: [0; 2],
                    flags: CqFlags::empty(),
                    _reserved: 0,
                };
                ring.push_cq(cqe)?;
            }
            Ok(())
        }
        Err(e) => {
            if !entry.flags.contains(SqFlags::NO_CQE) {
                let cqe = CqEntry {
                    user_data: entry.user_data,
                    result: error_to_code(&e),
                    data: [0; 2],
                    flags: CqFlags::empty(),
                    _reserved: 0,
                };
                ring.push_cq(cqe)?;
            }
            Err(e)
        }
    }
}

/// Process Receive operation
/// params[0] = buffer pointer
/// params[1] = buffer length
/// params[2] = timeout (0 = non-blocking, u64::MAX = blocking)
fn process_receive(entry: &SqEntry, ring: &mut IpcRing) -> Result<(), IpcError> {
    let endpoint_id = ObjectId::from_raw(entry.cap_slot as u64);
    let timeout = entry.params[2];

    // Look up the endpoint
    let endpoints = ENDPOINTS.read();
    let endpoint = endpoints
        .get(&endpoint_id)
        .ok_or(IpcError::InvalidEndpoint)?;

    // Try to receive based on timeout
    let result = if timeout == 0 {
        // Non-blocking
        endpoint.try_receive().ok_or(IpcError::WouldBlock)
    } else if timeout == u64::MAX {
        // Blocking
        drop(endpoints);
        let endpoints = ENDPOINTS.read();
        let endpoint = endpoints.get(&endpoint_id).ok_or(IpcError::InvalidEndpoint)?;
        endpoint.receive()
    } else {
        // Timeout
        drop(endpoints);
        let endpoints = ENDPOINTS.read();
        let endpoint = endpoints.get(&endpoint_id).ok_or(IpcError::InvalidEndpoint)?;
        endpoint.receive_timeout(timeout)
    };

    match result {
        Ok(msg) => {
            if !entry.flags.contains(SqFlags::NO_CQE) {
                let cqe = CqEntry {
                    user_data: entry.user_data,
                    result: msg.header.length as i64,
                    data: [msg.header.tag as u64, msg.header.cap_count as u64],
                    flags: CqFlags::empty(),
                    _reserved: 0,
                };
                ring.push_cq(cqe)?;
            }
            Ok(())
        }
        Err(e) => {
            if !entry.flags.contains(SqFlags::NO_CQE) {
                let cqe = CqEntry {
                    user_data: entry.user_data,
                    result: error_to_code(&e),
                    data: [0; 2],
                    flags: CqFlags::empty(),
                    _reserved: 0,
                };
                ring.push_cq(cqe)?;
            }
            Err(e)
        }
    }
}

/// Process Call operation (synchronous RPC: send + receive reply)
/// params[0] = request data pointer
/// params[1] = request length
/// params[2] = reply buffer pointer
/// params[3] = reply buffer length
fn process_call(entry: &SqEntry, ring: &mut IpcRing) -> Result<(), IpcError> {
    let endpoint_id = ObjectId::from_raw(entry.cap_slot as u64);

    // Build request message
    let request = Message::simple(entry.params[1] as u32, &[]);

    // Look up the endpoint and perform call
    let endpoints = ENDPOINTS.read();
    let endpoint = endpoints
        .get(&endpoint_id)
        .ok_or(IpcError::InvalidEndpoint)?;

    // Send request
    endpoint.send(request)?;
    drop(endpoints);

    // Wait for reply
    let endpoints = ENDPOINTS.read();
    let endpoint = endpoints.get(&endpoint_id).ok_or(IpcError::InvalidEndpoint)?;
    let reply = endpoint.receive()?;

    if !entry.flags.contains(SqFlags::NO_CQE) {
        let cqe = CqEntry {
            user_data: entry.user_data,
            result: reply.header.length as i64,
            data: [reply.header.tag as u64, 0],
            flags: CqFlags::empty(),
            _reserved: 0,
        };
        ring.push_cq(cqe)?;
    }

    Ok(())
}

/// Process Reply operation (respond to a Call)
/// params[0] = reply data pointer
/// params[1] = reply length
/// params[2] = caller token
fn process_reply(entry: &SqEntry, ring: &mut IpcRing) -> Result<(), IpcError> {
    let endpoint_id = ObjectId::from_raw(entry.cap_slot as u64);
    let caller_token = entry.params[2];

    // Build reply message
    let reply = Message::simple(entry.params[1] as u32, &[]);

    // Look up the endpoint
    let endpoints = ENDPOINTS.read();
    let endpoint = endpoints
        .get(&endpoint_id)
        .ok_or(IpcError::InvalidEndpoint)?;

    // Send reply
    endpoint.send(reply)?;

    if !entry.flags.contains(SqFlags::NO_CQE) {
        let cqe = CqEntry {
            user_data: entry.user_data,
            result: 0,
            data: [caller_token, 0],
            flags: CqFlags::empty(),
            _reserved: 0,
        };
        ring.push_cq(cqe)?;
    }

    Ok(())
}

/// Process Signal operation (set notification bits)
/// params[0] = bits to signal
fn process_signal(entry: &SqEntry, ring: &mut IpcRing) -> Result<(), IpcError> {
    let notif_id = ObjectId::from_raw(entry.cap_slot as u64);
    let bits = entry.params[0];

    // Look up the notification object
    let notifications = NOTIFICATIONS.read();
    let notification = notifications
        .get(&notif_id)
        .ok_or(IpcError::InvalidEndpoint)?;

    // Signal the bits
    notification.signal(bits);

    if !entry.flags.contains(SqFlags::NO_CQE) {
        let cqe = CqEntry {
            user_data: entry.user_data,
            result: 0,
            data: [bits, notification.peek()],
            flags: CqFlags::empty(),
            _reserved: 0,
        };
        ring.push_cq(cqe)?;
    }

    Ok(())
}

/// Process Wait operation (wait for notification bits)
/// params[0] = mask of bits to wait for
/// params[1] = timeout (0 = non-blocking, u64::MAX = blocking)
fn process_wait(entry: &SqEntry, ring: &mut IpcRing) -> Result<(), IpcError> {
    let notif_id = ObjectId::from_raw(entry.cap_slot as u64);
    let mask = entry.params[0];
    let timeout = entry.params[1];

    // Look up the notification object
    let notifications = NOTIFICATIONS.read();
    let notification = notifications
        .get(&notif_id)
        .ok_or(IpcError::InvalidEndpoint)?;

    // Wait based on timeout
    let result = if timeout == 0 {
        // Non-blocking poll
        let bits = notification.poll(mask);
        if bits != 0 { Ok(bits) } else { Err(IpcError::WouldBlock) }
    } else if timeout == u64::MAX {
        // Blocking wait
        Ok(notification.wait(mask))
    } else {
        // Timeout wait
        let bits = notification.wait_timeout(mask, timeout);
        if bits != 0 { Ok(bits) } else { Err(IpcError::Timeout) }
    };

    match result {
        Ok(bits) => {
            if !entry.flags.contains(SqFlags::NO_CQE) {
                let cqe = CqEntry {
                    user_data: entry.user_data,
                    result: bits as i64,
                    data: [bits, mask],
                    flags: CqFlags::empty(),
                    _reserved: 0,
                };
                ring.push_cq(cqe)?;
            }
            Ok(())
        }
        Err(e) => {
            if !entry.flags.contains(SqFlags::NO_CQE) {
                let cqe = CqEntry {
                    user_data: entry.user_data,
                    result: error_to_code(&e),
                    data: [0, mask],
                    flags: CqFlags::empty(),
                    _reserved: 0,
                };
                ring.push_cq(cqe)?;
            }
            Err(e)
        }
    }
}

/// Process Poll operation (non-blocking check for notification bits)
/// params[0] = mask of bits to check
fn process_poll(entry: &SqEntry, ring: &mut IpcRing) -> Result<(), IpcError> {
    let notif_id = ObjectId::from_raw(entry.cap_slot as u64);
    let mask = entry.params[0];

    // Look up the notification object
    let notifications = NOTIFICATIONS.read();
    let notification = notifications
        .get(&notif_id)
        .ok_or(IpcError::InvalidEndpoint)?;

    // Non-blocking poll
    let bits = notification.poll(mask);

    if !entry.flags.contains(SqFlags::NO_CQE) {
        let cqe = CqEntry {
            user_data: entry.user_data,
            result: bits as i64,
            data: [bits, notification.peek()],
            flags: CqFlags::empty(),
            _reserved: 0,
        };
        ring.push_cq(cqe)?;
    }

    Ok(())
}

/// Process TensorAlloc operation (allocate tensor buffer)
/// params[0] = size in bytes
/// params[1] = device type (0=CPU, 1=GPU, 2=NPU)
/// params[2] = alignment
fn process_tensor_alloc(entry: &SqEntry, ring: &mut IpcRing) -> Result<(), IpcError> {
    let size = entry.params[0];
    let device_type = entry.params[1] as u32;
    let alignment = entry.params[2];

    // Delegate to tensor subsystem
    let result = crate::tensor::allocate_buffer(size, device_type, alignment);

    match result {
        Ok((buffer_id, phys_addr)) => {
            if !entry.flags.contains(SqFlags::NO_CQE) {
                let cqe = CqEntry {
                    user_data: entry.user_data,
                    result: 0,
                    data: [buffer_id, phys_addr],
                    flags: CqFlags::empty(),
                    _reserved: 0,
                };
                ring.push_cq(cqe)?;
            }
            Ok(())
        }
        Err(_) => {
            if !entry.flags.contains(SqFlags::NO_CQE) {
                let cqe = CqEntry {
                    user_data: entry.user_data,
                    result: -1,
                    data: [0; 2],
                    flags: CqFlags::empty(),
                    _reserved: 0,
                };
                ring.push_cq(cqe)?;
            }
            Err(IpcError::InvalidOperation)
        }
    }
}

/// Process Inference operation (submit inference request)
/// params[0] = model ID
/// params[1] = input tensor buffer ID
/// params[2] = output tensor buffer ID
/// params[3] = flags
fn process_inference(entry: &SqEntry, ring: &mut IpcRing) -> Result<(), IpcError> {
    let model_id = entry.params[0];
    let input_buffer = entry.params[1];
    let output_buffer = entry.params[2];
    let flags = entry.params[3] as u32;

    // Submit inference request to tensor subsystem
    let result = crate::tensor::submit_inference(model_id, input_buffer, output_buffer, flags);

    match result {
        Ok(request_id) => {
            if !entry.flags.contains(SqFlags::NO_CQE) {
                let cqe = CqEntry {
                    user_data: entry.user_data,
                    result: 0,
                    data: [request_id, 0],
                    flags: CqFlags::empty(),
                    _reserved: 0,
                };
                ring.push_cq(cqe)?;
            }
            Ok(())
        }
        Err(_) => {
            if !entry.flags.contains(SqFlags::NO_CQE) {
                let cqe = CqEntry {
                    user_data: entry.user_data,
                    result: -1,
                    data: [0; 2],
                    flags: CqFlags::empty(),
                    _reserved: 0,
                };
                ring.push_cq(cqe)?;
            }
            Err(IpcError::InvalidOperation)
        }
    }
}

/// Convert IpcError to error code for completion entry
fn error_to_code(err: &IpcError) -> i64 {
    match err {
        IpcError::InvalidSize => -1,
        IpcError::QueueFull => -2,
        IpcError::QueueEmpty => -3,
        IpcError::Timeout => -4,
        IpcError::Cancelled => -5,
        IpcError::InvalidEndpoint => -6,
        IpcError::Capability(_) => -7,
        IpcError::MessageTooLarge => -8,
        IpcError::WouldBlock => -9,
        IpcError::Disconnected => -10,
        IpcError::InvalidOperation => -11,
        IpcError::InternalError => -12,
    }
}

// ============================================================================
// High-Level Syscall Interface Functions
// ============================================================================

/// Enter IPC ring and process operations
///
/// This is the main entry point for io_uring-style IPC. It:
/// 1. Processes up to `to_submit` submission queue entries
/// 2. Waits until at least `min_complete` completions are available (if > 0)
/// 3. Returns the number of completions generated
///
/// # Arguments
///
/// * `ring_id` - Object ID of the IPC ring
/// * `to_submit` - Maximum number of submissions to process (0 = none)
/// * `min_complete` - Minimum completions to wait for (0 = don't wait)
///
/// # Returns
///
/// Number of completions generated, or error
pub fn ring_enter(
    ring_id: ObjectId,
    to_submit: u32,
    min_complete: u32,
) -> Result<u32, IpcError> {
    // Get the ring
    let mut rings = RINGS.write();
    let ring = rings.get_mut(&ring_id).ok_or(IpcError::InvalidEndpoint)?;

    let mut completions_generated: u32 = 0;

    // Process submission queue entries
    if to_submit > 0 {
        let mut processed: u32 = 0;

        while processed < to_submit {
            // Try to pop a submission entry
            let entry = match ring.pop_sq() {
                Some(e) => e,
                None => break, // No more entries
            };

            // Process the entry
            let result = process_sq_entry(&entry, ring);

            // Generate completion (unless NO_CQE flag is set)
            if !entry.flags.contains(SqFlags::NO_CQE) {
                let cqe = CqEntry {
                    user_data: entry.user_data,
                    result: match &result {
                        Ok(_) => 0,
                        Err(e) => error_to_code(e),
                    },
                    data: [0; 2],
                    flags: CqFlags::empty(),
                    _reserved: 0,
                };

                // Try to push completion, mark overflow if full
                if ring.push_cq(cqe).is_ok() {
                    completions_generated += 1;
                } else {
                    // Set overflow flag
                    ring.flags.fetch_or(ring_flags::CQ_OVERFLOW, core::sync::atomic::Ordering::SeqCst);
                }
            }

            processed += 1;

            // If this entry is chained and failed, skip the chain
            if entry.flags.contains(SqFlags::CHAIN) && result.is_err() {
                skip_chain(ring);
            }
        }
    }

    // Wait for minimum completions if requested
    if min_complete > 0 && completions_generated < min_complete {
        // In a real implementation, we would block here and wake when completions arrive
        // For now, we'll spin-wait with a yield (not ideal but functional)
        let mut attempts = 0;
        const MAX_ATTEMPTS: u32 = 10000;

        while ring.cq_pending() < min_complete && attempts < MAX_ATTEMPTS {
            // Yield to other threads
            crate::sched::yield_now();
            attempts += 1;
        }

        completions_generated = ring.cq_pending();
    }

    Ok(completions_generated)
}

/// Skip chained entries after a failure
fn skip_chain(ring: &mut IpcRing) {
    loop {
        match ring.pop_sq() {
            Some(entry) => {
                // Generate a cancelled completion for this entry
                if !entry.flags.contains(SqFlags::NO_CQE) {
                    let cqe = CqEntry {
                        user_data: entry.user_data,
                        result: error_to_code(&IpcError::Cancelled),
                        data: [0; 2],
                        flags: CqFlags::CANCELLED,
                        _reserved: 0,
                    };
                    let _ = ring.push_cq(cqe);
                }

                // Continue if still chained
                if !entry.flags.contains(SqFlags::CHAIN) {
                    break;
                }
            }
            None => break,
        }
    }
}

/// Destroy an IPC ring
pub fn destroy_ring(ring_id: ObjectId) -> Result<(), IpcError> {
    RINGS
        .write()
        .remove(&ring_id)
        .map(|_| ())
        .ok_or(IpcError::InvalidEndpoint)
}

/// Send a message to an endpoint
pub fn send(
    dest_id: ObjectId,
    data: &[u8],
    _timeout: Option<core::time::Duration>,
) -> Result<(), IpcError> {
    let endpoints = ENDPOINTS.read();
    let endpoint = endpoints
        .get(&dest_id)
        .ok_or(IpcError::InvalidEndpoint)?;

    let msg = Message::simple(0, data);
    endpoint.send(msg)
}

/// Receive a message from an endpoint
pub fn receive(
    src_id: ObjectId,
    _timeout: Option<core::time::Duration>,
) -> Result<alloc::vec::Vec<u8>, IpcError> {
    let endpoints = ENDPOINTS.read();
    let endpoint = endpoints
        .get(&src_id)
        .ok_or(IpcError::InvalidEndpoint)?;

    let msg = endpoint.receive()?;
    Ok(msg.data().to_vec())
}

/// Synchronous call: send request and wait for reply
pub fn call(
    dest_id: ObjectId,
    request: &[u8],
) -> Result<alloc::vec::Vec<u8>, IpcError> {
    // Send request
    send(dest_id, request, None)?;

    // Wait for reply
    receive(dest_id, None)
}

/// Reply to an incoming call
pub fn reply(
    reply_id: ObjectId,
    data: &[u8],
) -> Result<(), IpcError> {
    send(reply_id, data, None)
}

/// Signal notification bits
pub fn signal(
    notif_id: ObjectId,
    bits: u64,
) -> Result<(), IpcError> {
    let notifications = NOTIFICATIONS.read();
    let notification = notifications
        .get(&notif_id)
        .ok_or(IpcError::InvalidEndpoint)?;

    notification.signal(bits);
    Ok(())
}

/// Wait for notification bits
pub fn wait(
    notif_id: ObjectId,
    mask: u64,
    timeout: Option<core::time::Duration>,
) -> Result<u64, IpcError> {
    let notifications = NOTIFICATIONS.read();
    let notification = notifications
        .get(&notif_id)
        .ok_or(IpcError::InvalidEndpoint)?;

    if let Some(duration) = timeout {
        let timeout_ns = duration.as_nanos() as u64;
        let bits = notification.wait_timeout(mask, timeout_ns);
        if bits != 0 {
            Ok(bits)
        } else {
            Err(IpcError::Timeout)
        }
    } else {
        Ok(notification.wait(mask))
    }
}

/// Poll notification bits (non-blocking)
pub fn poll(
    notif_id: ObjectId,
    mask: u64,
) -> Result<u64, IpcError> {
    let notifications = NOTIFICATIONS.read();
    let notification = notifications
        .get(&notif_id)
        .ok_or(IpcError::InvalidEndpoint)?;

    Ok(notification.poll(mask))
}
