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
        write!(
            f,
            "ObjectId({:?}:{})",
            self.object_type(),
            self.0 & 0x00FFFFFFFFFFFFFF
        )
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
            Self::Interrupt | Self::MmioRegion => {
                Rights::IRQ | Rights::MMIO | Rights::READ | Rights::WRITE
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_object_id_new() {
        let id1 = ObjectId::new(ObjectType::Process);
        let id2 = ObjectId::new(ObjectType::Process);
        assert_ne!(id1, id2); // Each ID should be unique
    }

    #[test]
    fn test_object_id_object_type() {
        let id = ObjectId::new(ObjectType::Thread);
        assert_eq!(id.object_type(), ObjectType::Thread);

        let id2 = ObjectId::new(ObjectType::Endpoint);
        assert_eq!(id2.object_type(), ObjectType::Endpoint);
    }

    #[test]
    fn test_object_id_from_raw() {
        let original = ObjectId::new(ObjectType::MemoryRegion);
        let raw = original.raw();
        let restored = ObjectId::from_raw(raw);
        assert_eq!(original, restored);
    }

    #[test]
    fn test_object_id_as_u64() {
        let id = ObjectId::new(ObjectType::Process);
        assert_eq!(id.as_u64(), id.raw());
    }

    #[test]
    fn test_object_id_ordering() {
        let id1 = ObjectId::new(ObjectType::Process);
        let id2 = ObjectId::new(ObjectType::Process);
        // IDs created later should be greater
        assert!(id2 > id1);
    }

    #[test]
    fn test_object_id_hash() {
        use std::collections::HashSet;

        let mut set = HashSet::new();
        let id1 = ObjectId::new(ObjectType::Process);
        let id2 = ObjectId::new(ObjectType::Process);

        set.insert(id1);
        set.insert(id2);
        set.insert(id1); // Duplicate

        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_object_id_debug() {
        let id = ObjectId::new(ObjectType::Endpoint);
        let debug_str = format!("{:?}", id);
        assert!(debug_str.contains("ObjectId"));
        assert!(debug_str.contains("Endpoint"));
    }

    #[test]
    fn test_object_type_from_u8_valid() {
        assert_eq!(ObjectType::from_u8(0), Some(ObjectType::Unknown));
        assert_eq!(ObjectType::from_u8(1), Some(ObjectType::Endpoint));
        assert_eq!(ObjectType::from_u8(2), Some(ObjectType::Notification));
        assert_eq!(ObjectType::from_u8(3), Some(ObjectType::MemoryRegion));
        assert_eq!(ObjectType::from_u8(4), Some(ObjectType::AddressSpace));
        assert_eq!(ObjectType::from_u8(5), Some(ObjectType::Thread));
        assert_eq!(ObjectType::from_u8(6), Some(ObjectType::Process));
        assert_eq!(ObjectType::from_u8(7), Some(ObjectType::SchedulerContext));
        assert_eq!(ObjectType::from_u8(8), Some(ObjectType::IpcRing));
    }

    #[test]
    fn test_object_type_from_u8_hardware() {
        assert_eq!(ObjectType::from_u8(32), Some(ObjectType::Interrupt));
        assert_eq!(ObjectType::from_u8(33), Some(ObjectType::IoPort));
        assert_eq!(ObjectType::from_u8(34), Some(ObjectType::MmioRegion));
        assert_eq!(ObjectType::from_u8(35), Some(ObjectType::DmaBuffer));
        assert_eq!(ObjectType::from_u8(36), Some(ObjectType::GpuDevice));
        assert_eq!(ObjectType::from_u8(37), Some(ObjectType::NpuDevice));
        assert_eq!(ObjectType::from_u8(38), Some(ObjectType::BlockDevice));
    }

    #[test]
    fn test_object_type_from_u8_tensor() {
        assert_eq!(ObjectType::from_u8(64), Some(ObjectType::TensorBuffer));
        assert_eq!(ObjectType::from_u8(65), Some(ObjectType::InferenceContext));
        assert_eq!(ObjectType::from_u8(66), Some(ObjectType::ComputeQueue));
        assert_eq!(ObjectType::from_u8(67), Some(ObjectType::ModelHandle));
        assert_eq!(ObjectType::from_u8(68), Some(ObjectType::TensorView));
    }

    #[test]
    fn test_object_type_from_u8_fs() {
        assert_eq!(ObjectType::from_u8(96), Some(ObjectType::File));
        assert_eq!(ObjectType::from_u8(97), Some(ObjectType::Directory));
        assert_eq!(ObjectType::from_u8(98), Some(ObjectType::Mount));
        assert_eq!(ObjectType::from_u8(99), Some(ObjectType::PersistentRegion));
    }

    #[test]
    fn test_object_type_from_u8_timetravel() {
        assert_eq!(ObjectType::from_u8(128), Some(ObjectType::Checkpoint));
        assert_eq!(ObjectType::from_u8(129), Some(ObjectType::RecordingSession));
    }

    #[test]
    fn test_object_type_from_u8_network() {
        assert_eq!(ObjectType::from_u8(160), Some(ObjectType::Socket));
        assert_eq!(ObjectType::from_u8(161), Some(ObjectType::NetworkInterface));
    }

    #[test]
    fn test_object_type_from_u8_invalid() {
        assert_eq!(ObjectType::from_u8(9), None);
        assert_eq!(ObjectType::from_u8(31), None);
        assert_eq!(ObjectType::from_u8(63), None);
        assert_eq!(ObjectType::from_u8(200), None);
        assert_eq!(ObjectType::from_u8(255), None);
    }

    #[test]
    fn test_object_type_default_rights_memory() {
        use super::super::Rights;
        let rights = ObjectType::MemoryRegion.default_rights();
        assert!(rights.contains(Rights::READ));
        assert!(rights.contains(Rights::WRITE));
        assert!(rights.contains(Rights::MAP));
    }

    #[test]
    fn test_object_type_default_rights_endpoint() {
        use super::super::Rights;
        let rights = ObjectType::Endpoint.default_rights();
        assert!(rights.contains(Rights::SEND));
        assert!(rights.contains(Rights::RECEIVE));
    }

    #[test]
    fn test_object_type_default_rights_process() {
        use super::super::Rights;
        let rights = ObjectType::Process.default_rights();
        assert!(rights.contains(Rights::FORK));
        assert!(rights.contains(Rights::KILL));

        let rights2 = ObjectType::Thread.default_rights();
        assert_eq!(rights, rights2);
    }

    #[test]
    fn test_object_type_default_rights_tensor() {
        use super::super::Rights;
        let rights = ObjectType::TensorBuffer.default_rights();
        assert!(rights.contains(Rights::TENSOR_ALLOC));
        assert!(rights.contains(Rights::INFERENCE));

        let rights2 = ObjectType::InferenceContext.default_rights();
        assert_eq!(rights, rights2);
    }

    #[test]
    fn test_object_type_default_rights_interrupt() {
        use super::super::Rights;
        let rights = ObjectType::Interrupt.default_rights();
        assert!(rights.contains(Rights::IRQ));
        assert!(rights.contains(Rights::READ));
    }

    #[test]
    fn test_object_type_requires_privilege() {
        assert!(ObjectType::Interrupt.requires_privilege());
        assert!(ObjectType::IoPort.requires_privilege());
        assert!(ObjectType::MmioRegion.requires_privilege());
        assert!(ObjectType::DmaBuffer.requires_privilege());
        assert!(ObjectType::GpuDevice.requires_privilege());
        assert!(ObjectType::NpuDevice.requires_privilege());
    }

    #[test]
    fn test_object_type_does_not_require_privilege() {
        assert!(!ObjectType::Process.requires_privilege());
        assert!(!ObjectType::Thread.requires_privilege());
        assert!(!ObjectType::MemoryRegion.requires_privilege());
        assert!(!ObjectType::Endpoint.requires_privilege());
        assert!(!ObjectType::File.requires_privilege());
        assert!(!ObjectType::Socket.requires_privilege());
    }

    #[test]
    fn test_object_id_type_tag_preserved() {
        // Test that object type is correctly encoded in the ID
        let types = [
            ObjectType::Process,
            ObjectType::Thread,
            ObjectType::Endpoint,
            ObjectType::MemoryRegion,
            ObjectType::TensorBuffer,
            ObjectType::Socket,
        ];

        for obj_type in types {
            let id = ObjectId::new(obj_type);
            assert_eq!(
                id.object_type(),
                obj_type,
                "Type mismatch for {:?}",
                obj_type
            );
        }
    }

    #[cfg(test)]
    #[test]
    fn test_object_id_new_test() {
        let id = ObjectId::new_test(42);
        assert_eq!(id.raw(), 42);
    }
}
