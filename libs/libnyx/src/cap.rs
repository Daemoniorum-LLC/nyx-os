//! Capability management
//!
//! Capabilities are unforgeable tokens that grant rights to kernel objects.
//! They are the foundation of Nyx's security model.

pub use bitflags::bitflags;

use crate::syscall::{self, nr, Error};

bitflags! {
    /// Rights that can be granted via capabilities
    ///
    /// These are organized into categories:
    /// - Universal (0-7): Apply to all object types
    /// - Memory (8-15): Memory region specific
    /// - IPC (16-23): IPC endpoint specific
    /// - Process (24-31): Process/thread specific
    /// - Hardware (32-39): Hardware access
    /// - AI/Tensor (40-47): AI acceleration specific
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct Rights: u64 {
        // === Universal Rights (bits 0-7) ===

        /// Read data/state from object
        const READ = 1 << 0;
        /// Write/modify data/state
        const WRITE = 1 << 1;
        /// Execute code or invoke operations
        const EXECUTE = 1 << 2;
        /// Derive new capabilities (delegation)
        const GRANT = 1 << 3;
        /// Revoke derived capabilities
        const REVOKE = 1 << 4;
        /// Duplicate the capability itself
        const DUPLICATE = 1 << 5;
        /// Transfer capability to another CSpace
        const TRANSFER = 1 << 6;
        /// Inspect object metadata
        const INSPECT = 1 << 7;

        // === Memory Rights (bits 8-15) ===

        /// Map memory region into address space
        const MAP = 1 << 8;
        /// Unmap memory region
        const UNMAP = 1 << 9;
        /// Access as device memory (uncached)
        const DEVICE_MEM = 1 << 10;
        /// Lock memory (prevent swapping)
        const LOCK = 1 << 11;
        /// Share memory with other processes
        const SHARE = 1 << 12;
        /// Use huge pages
        const HUGE_PAGES = 1 << 13;
        /// Memory is persistent (survives power loss)
        const PERSISTENT = 1 << 14;

        // === IPC Rights (bits 16-23) ===

        /// Send messages to endpoint
        const SEND = 1 << 16;
        /// Receive messages from endpoint
        const RECEIVE = 1 << 17;
        /// Perform synchronous call (send + wait)
        const CALL = 1 << 18;
        /// Reply to a call
        const REPLY = 1 << 19;
        /// Signal notification bits
        const SIGNAL = 1 << 20;
        /// Wait on notification bits
        const WAIT = 1 << 21;
        /// Poll without blocking
        const POLL = 1 << 22;

        // === Process Rights (bits 24-31) ===

        /// Fork/spawn child processes
        const FORK = 1 << 24;
        /// Terminate process/thread
        const KILL = 1 << 25;
        /// Debug/trace execution
        const TRACE = 1 << 26;
        /// Record execution (time-travel)
        const RECORD = 1 << 27;
        /// Suspend execution
        const SUSPEND = 1 << 28;
        /// Resume execution
        const RESUME = 1 << 29;
        /// Modify scheduling parameters
        const SCHEDULE = 1 << 30;

        // === Hardware Rights (bits 32-39) ===

        /// Handle interrupts
        const IRQ = 1 << 32;
        /// Perform DMA operations
        const DMA = 1 << 33;
        /// Access memory-mapped I/O
        const MMIO = 1 << 34;
        /// Access I/O ports (x86)
        const IOPORT = 1 << 35;
        /// Access GPU resources
        const GPU = 1 << 36;
        /// Access NPU resources
        const NPU = 1 << 37;
        /// Access sensors (camera, mic, etc.)
        const SENSOR = 1 << 38;

        // === AI/Tensor Rights (bits 40-47) ===

        /// Allocate tensor buffers
        const TENSOR_ALLOC = 1 << 40;
        /// Free tensor buffers
        const TENSOR_FREE = 1 << 41;
        /// Submit inference requests
        const INFERENCE = 1 << 42;
        /// Submit GPU compute jobs
        const GPU_COMPUTE = 1 << 43;
        /// Access NPU for inference
        const NPU_ACCESS = 1 << 44;
        /// Migrate tensors between devices
        const TENSOR_MIGRATE = 1 << 45;
        /// Access model weights
        const MODEL_ACCESS = 1 << 46;

        // === Common Combinations ===

        /// Full memory access
        const MEMORY_FULL = Self::READ.bits() | Self::WRITE.bits() |
                           Self::MAP.bits() | Self::UNMAP.bits() |
                           Self::SHARE.bits() | Self::GRANT.bits();

        /// Read-only memory access
        const MEMORY_READ = Self::READ.bits() | Self::MAP.bits();

        /// Full IPC access
        const IPC_FULL = Self::SEND.bits() | Self::RECEIVE.bits() |
                        Self::CALL.bits() | Self::REPLY.bits() |
                        Self::SIGNAL.bits() | Self::WAIT.bits() |
                        Self::GRANT.bits();

        /// Client-side IPC (can call services)
        const IPC_CLIENT = Self::SEND.bits() | Self::CALL.bits() | Self::WAIT.bits();

        /// Server-side IPC (can receive and reply)
        const IPC_SERVER = Self::RECEIVE.bits() | Self::REPLY.bits() | Self::WAIT.bits();

        /// Full AI/inference access
        const AI_FULL = Self::TENSOR_ALLOC.bits() | Self::TENSOR_FREE.bits() |
                       Self::INFERENCE.bits() | Self::GPU_COMPUTE.bits() |
                       Self::NPU_ACCESS.bits() | Self::TENSOR_MIGRATE.bits() |
                       Self::MODEL_ACCESS.bits() | Self::GRANT.bits();

        /// Inference-only access (no model modification)
        const AI_INFERENCE = Self::TENSOR_ALLOC.bits() | Self::TENSOR_FREE.bits() |
                            Self::INFERENCE.bits() | Self::TENSOR_MIGRATE.bits();
    }
}

impl Rights {
    /// Check if this rights set is a subset of another
    #[inline]
    pub fn is_subset_of(self, other: Rights) -> bool {
        (self.bits() & !other.bits()) == 0
    }
}

/// Capability handle (slot in process's capability space)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct Capability(u64);

impl Capability {
    /// Invalid/null capability
    pub const INVALID: Self = Self(u64::MAX);

    /// Create from raw object ID
    pub const fn from_raw(id: u64) -> Self {
        Self(id)
    }

    /// Get raw object ID
    pub const fn as_raw(&self) -> u64 {
        self.0
    }

    /// Check if valid
    pub const fn is_valid(&self) -> bool {
        self.0 != u64::MAX
    }

    /// Derive a new capability with reduced rights
    ///
    /// The new capability will have at most the specified rights,
    /// but cannot exceed the rights of the source capability.
    pub fn derive(&self, new_rights: Rights) -> Result<Capability, Error> {
        let result = unsafe { syscall::syscall2(nr::CAP_DERIVE, self.0, new_rights.bits()) };
        Error::from_raw(result).map(Capability)
    }

    /// Revoke this capability and all capabilities derived from it
    ///
    /// After revocation, any attempt to use this capability or its
    /// derivatives will fail with `InvalidCapability`.
    pub fn revoke(&self) -> Result<(), Error> {
        let result = unsafe { syscall::syscall1(nr::CAP_REVOKE, self.0) };
        Error::from_raw(result).map(|_| ())
    }

    /// Identify this capability's object type and rights
    ///
    /// Returns (object_type, rights) where object_type is:
    /// - 0: Memory region
    /// - 1: IPC endpoint
    /// - 2: Process
    /// - 3: Thread
    /// - 4: Notification
    /// - 5: IPC Ring
    /// - 6: Tensor buffer
    /// - 7: Model
    pub fn identify(&self) -> Result<(u32, Rights), Error> {
        let result = unsafe { syscall::syscall1(nr::CAP_IDENTIFY, self.0) };
        let packed = Error::from_raw(result)?;
        let obj_type = (packed >> 32) as u32;
        let rights = Rights::from_bits_truncate(packed as u64);
        Ok((obj_type, rights))
    }

    /// Grant this capability to another process
    ///
    /// The granted capability will have at most the specified rights mask.
    pub fn grant(&self, target_pid: u64, rights_mask: Rights) -> Result<Capability, Error> {
        let result =
            unsafe { syscall::syscall3(nr::CAP_GRANT, self.0, target_pid, rights_mask.bits()) };
        Error::from_raw(result).map(Capability)
    }

    /// Drop (release) this capability
    ///
    /// After dropping, this capability handle is invalid.
    pub fn drop_cap(&self) -> Result<(), Error> {
        let result = unsafe { syscall::syscall1(nr::CAP_DROP, self.0) };
        Error::from_raw(result).map(|_| ())
    }
}

/// Object types that can be referenced by capabilities
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectType {
    /// Memory region
    Memory = 0,
    /// IPC endpoint
    Endpoint = 1,
    /// Process
    Process = 2,
    /// Thread
    Thread = 3,
    /// Notification object
    Notification = 4,
    /// IPC Ring (io_uring-style)
    IpcRing = 5,
    /// Tensor buffer
    Tensor = 6,
    /// AI Model
    Model = 7,
    /// Unknown type
    Unknown = 255,
}

impl From<u32> for ObjectType {
    fn from(value: u32) -> Self {
        match value {
            0 => Self::Memory,
            1 => Self::Endpoint,
            2 => Self::Process,
            3 => Self::Thread,
            4 => Self::Notification,
            5 => Self::IpcRing,
            6 => Self::Tensor,
            7 => Self::Model,
            _ => Self::Unknown,
        }
    }
}
