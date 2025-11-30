//! Capability object types and identifiers

use core::sync::atomic::{AtomicU64, Ordering};

/// Global object ID counter
static NEXT_OBJECT_ID: AtomicU64 = AtomicU64::new(1);

/// Globally unique object identifier
///
/// Object IDs are 64 bits:
/// - Bits 0-55: Unique counter
/// - Bits 56-63: Object type tag
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ObjectId(u64);

impl ObjectId {
    /// Create a new unique object ID
    pub fn new(object_type: ObjectType) -> Self {
        let counter = NEXT_OBJECT_ID.fetch_add(1, Ordering::Relaxed);
        let type_tag = (object_type as u64) << 56;
        Self(counter | type_tag)
    }

    /// Get the object type from the ID
    pub fn object_type(&self) -> ObjectType {
        let type_tag = (self.0 >> 56) as u8;
        ObjectType::from_u8(type_tag).unwrap_or(ObjectType::Unknown)
    }

    /// Get the raw ID value
    pub fn raw(&self) -> u64 {
        self.0
    }

    /// Get the raw ID value as u64 (alias for raw)
    pub fn as_u64(&self) -> u64 {
        self.0
    }

    /// Create from raw value (for deserialization)
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    /// Create a test object ID (for unit tests only)
    #[cfg(test)]
    pub fn new_test(id: u64) -> Self {
        Self(id)
    }
}

impl core::fmt::Debug for ObjectId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "ObjectId({:?}:{})", self.object_type(), self.0 & 0x00FFFFFFFFFFFFFF)
    }
}

/// Types of kernel objects
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ObjectType {
    /// Unknown or invalid type
    Unknown = 0,

    // === Core Kernel Objects (1-31) ===

    /// IPC endpoint for message passing
    Endpoint = 1,
    /// Async notification (like eventfd)
    Notification = 2,
    /// Physical memory region
    MemoryRegion = 3,
    /// Virtual address space
    AddressSpace = 4,
    /// Execution thread
    Thread = 5,
    /// Process container
    Process = 6,
    /// Scheduling domain
    SchedulerContext = 7,
    /// IPC ring buffer
    IpcRing = 8,

    // === Hardware Objects (32-63) ===

    /// IRQ handler
    Interrupt = 32,
    /// x86 I/O port range
    IoPort = 33,
    /// Memory-mapped I/O region
    MmioRegion = 34,
    /// DMA-capable buffer
    DmaBuffer = 35,
    /// GPU device
    GpuDevice = 36,
    /// NPU device
    NpuDevice = 37,
    /// Block storage device
    BlockDevice = 38,

    // === AI/Tensor Objects (64-95) ===

    /// GPU/NPU tensor memory
    TensorBuffer = 64,
    /// Model execution context
    InferenceContext = 65,
    /// GPU/NPU command queue
    ComputeQueue = 66,
    /// Loaded model reference
    ModelHandle = 67,
    /// Tensor view (slice of tensor)
    TensorView = 68,

    // === File System Objects (96-127) ===

    /// Open file handle
    File = 96,
    /// Directory handle
    Directory = 97,
    /// File system mount
    Mount = 98,
    /// Persistent memory region
    PersistentRegion = 99,

    // === Time-Travel Objects (128-159) ===

    /// Execution checkpoint
    Checkpoint = 128,
    /// Recording session
    RecordingSession = 129,

    // === Network Objects (160-191) ===

    /// Network socket
    Socket = 160,
    /// Network interface
    NetworkInterface = 161,
}

impl ObjectType {
    /// Convert from u8
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Unknown),
            1 => Some(Self::Endpoint),
            2 => Some(Self::Notification),
            3 => Some(Self::MemoryRegion),
            4 => Some(Self::AddressSpace),
            5 => Some(Self::Thread),
            6 => Some(Self::Process),
            7 => Some(Self::SchedulerContext),
            8 => Some(Self::IpcRing),
            32 => Some(Self::Interrupt),
            33 => Some(Self::IoPort),
            34 => Some(Self::MmioRegion),
            35 => Some(Self::DmaBuffer),
            36 => Some(Self::GpuDevice),
            37 => Some(Self::NpuDevice),
            38 => Some(Self::BlockDevice),
            64 => Some(Self::TensorBuffer),
            65 => Some(Self::InferenceContext),
            66 => Some(Self::ComputeQueue),
            67 => Some(Self::ModelHandle),
            68 => Some(Self::TensorView),
            96 => Some(Self::File),
            97 => Some(Self::Directory),
            98 => Some(Self::Mount),
            99 => Some(Self::PersistentRegion),
            128 => Some(Self::Checkpoint),
            129 => Some(Self::RecordingSession),
            160 => Some(Self::Socket),
            161 => Some(Self::NetworkInterface),
            _ => None,
        }
    }

    /// Get default rights for this object type
    pub fn default_rights(&self) -> super::Rights {
        use super::Rights;

        match self {
            Self::MemoryRegion => Rights::MEMORY_FULL,
            Self::Endpoint => Rights::IPC_FULL,
            Self::Thread | Self::Process => Rights::PROCESS_FULL,
            Self::TensorBuffer | Self::InferenceContext => Rights::AI_FULL,
            Self::Interrupt | Self::MmioRegion => Rights::IRQ | Rights::MMIO | Rights::READ | Rights::WRITE,
            _ => Rights::READ | Rights::WRITE | Rights::GRANT,
        }
    }

    /// Check if this object type requires special privileges
    pub fn requires_privilege(&self) -> bool {
        matches!(
            self,
            Self::Interrupt
                | Self::IoPort
                | Self::MmioRegion
                | Self::DmaBuffer
                | Self::GpuDevice
                | Self::NpuDevice
        )
    }
}
