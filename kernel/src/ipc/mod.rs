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
mod notification;

pub use ring::{IpcRing, SqEntry, CqEntry, IpcOpcode, SqFlags, CqFlags};
pub use message::{Message, MessageHeader, MemoryGrant};
pub use endpoint::Endpoint;
pub use notification::Notification;

use crate::cap::{Capability, CapError, ObjectId, ObjectType, Rights};
use spin::RwLock;
use alloc::collections::BTreeMap;

/// Global endpoint registry
static ENDPOINTS: RwLock<BTreeMap<ObjectId, Endpoint>> = RwLock::new(BTreeMap::new());

/// Global notification registry
static NOTIFICATIONS: RwLock<BTreeMap<ObjectId, Notification>> = RwLock::new(BTreeMap::new());

/// Initialize the IPC subsystem
pub fn init() {
    log::trace!("IPC subsystem initialized");
}

/// Create a new IPC ring for a thread
pub fn create_ring(sq_size: u32, cq_size: u32) -> Result<IpcRing, IpcError> {
    // Validate sizes (must be power of 2)
    if !sq_size.is_power_of_two() || !cq_size.is_power_of_two() {
        return Err(IpcError::InvalidSize);
    }

    // Max size limits
    if sq_size > MAX_SQ_SIZE || cq_size > MAX_CQ_SIZE {
        return Err(IpcError::InvalidSize);
    }

    IpcRing::new(sq_size, cq_size)
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

fn process_send(_entry: &SqEntry, _ring: &mut IpcRing) -> Result<(), IpcError> {
    todo!("Implement send operation")
}

fn process_receive(_entry: &SqEntry, _ring: &mut IpcRing) -> Result<(), IpcError> {
    todo!("Implement receive operation")
}

fn process_call(_entry: &SqEntry, _ring: &mut IpcRing) -> Result<(), IpcError> {
    todo!("Implement call operation")
}

fn process_reply(_entry: &SqEntry, _ring: &mut IpcRing) -> Result<(), IpcError> {
    todo!("Implement reply operation")
}

fn process_signal(_entry: &SqEntry, _ring: &mut IpcRing) -> Result<(), IpcError> {
    todo!("Implement signal operation")
}

fn process_wait(_entry: &SqEntry, _ring: &mut IpcRing) -> Result<(), IpcError> {
    todo!("Implement wait operation")
}

fn process_poll(_entry: &SqEntry, _ring: &mut IpcRing) -> Result<(), IpcError> {
    todo!("Implement poll operation")
}

fn process_tensor_alloc(_entry: &SqEntry, _ring: &mut IpcRing) -> Result<(), IpcError> {
    todo!("Implement tensor_alloc operation")
}

fn process_inference(_entry: &SqEntry, _ring: &mut IpcRing) -> Result<(), IpcError> {
    todo!("Implement inference operation")
}
