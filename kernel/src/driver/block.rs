//! Block device interface
//!
//! Provides a common interface for block storage devices (NVMe, AHCI, virtio-blk).

use super::{DeviceId, DriverError};
use crate::cap::{Capability, ObjectId, ObjectType, Rights};
use crate::mem::PhysAddr;
use crate::process::ProcessId;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::RwLock;

/// Block device registry
static BLOCK_DEVICES: RwLock<BTreeMap<BlockDeviceId, BlockDevice>> = RwLock::new(BTreeMap::new());

/// Next block device ID
static NEXT_BLOCK_ID: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(1);

/// Block device identifier
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlockDeviceId(pub u64);

impl BlockDeviceId {
    pub fn new() -> Self {
        Self(NEXT_BLOCK_ID.fetch_add(1, core::sync::atomic::Ordering::SeqCst))
    }
}

impl Default for BlockDeviceId {
    fn default() -> Self {
        Self::new()
    }
}

/// Block device information
#[derive(Clone, Debug)]
pub struct BlockDevice {
    /// Block device ID
    pub id: BlockDeviceId,
    /// Underlying device ID
    pub device_id: DeviceId,
    /// Device name (e.g., "nvme0n1", "sda")
    pub name: String,
    /// Block device type
    pub device_type: BlockDeviceType,
    /// Block size in bytes
    pub block_size: u32,
    /// Total number of blocks
    pub num_blocks: u64,
    /// Current state
    pub state: BlockDeviceState,
    /// Capabilities
    pub capabilities: BlockCapabilities,
    /// Partition table
    pub partitions: Vec<Partition>,
    /// Driver process
    pub driver: Option<ProcessId>,
}

/// Block device type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockDeviceType {
    /// NVMe SSD
    Nvme,
    /// AHCI/SATA device
    Ahci,
    /// VirtIO block device
    VirtioBlk,
    /// USB mass storage
    UsbMass,
    /// Ramdisk
    Ramdisk,
    /// Loop device
    Loop,
    /// Unknown
    Unknown,
}

/// Block device state
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockDeviceState {
    /// Device discovered but not ready
    Discovered,
    /// Device is initializing
    Initializing,
    /// Device is ready
    Ready,
    /// Device is suspended
    Suspended,
    /// Device has error
    Error,
    /// Device removed
    Removed,
}

/// Block device capabilities
#[derive(Clone, Debug, Default)]
pub struct BlockCapabilities {
    /// Supports discard/trim
    pub discard: bool,
    /// Supports secure erase
    pub secure_erase: bool,
    /// Supports write zeros
    pub write_zeros: bool,
    /// Supports volatile write cache
    pub volatile_cache: bool,
    /// Supports FUA (Force Unit Access)
    pub fua: bool,
    /// Maximum transfer size in blocks
    pub max_transfer_blocks: u32,
    /// Maximum segments per request
    pub max_segments: u16,
    /// Optimal I/O size in bytes
    pub optimal_io_size: u32,
    /// Physical block size (may differ from logical)
    pub physical_block_size: u32,
    /// Minimum I/O size
    pub min_io_size: u32,
}

/// Disk partition
#[derive(Clone, Debug)]
pub struct Partition {
    /// Partition number
    pub number: u32,
    /// Start block
    pub start_block: u64,
    /// Number of blocks
    pub num_blocks: u64,
    /// Partition type GUID
    pub type_guid: [u8; 16],
    /// Partition GUID
    pub partition_guid: [u8; 16],
    /// Partition name
    pub name: String,
    /// Partition attributes
    pub attributes: u64,
}

/// Block I/O request
#[derive(Clone, Debug)]
pub struct BlockRequest {
    /// Request ID
    pub id: u64,
    /// Operation type
    pub operation: BlockOperation,
    /// Starting block
    pub start_block: u64,
    /// Number of blocks
    pub num_blocks: u32,
    /// Buffer physical address
    pub buffer: PhysAddr,
    /// Request flags
    pub flags: BlockRequestFlags,
}

/// Block operation type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockOperation {
    /// Read blocks
    Read,
    /// Write blocks
    Write,
    /// Flush cache
    Flush,
    /// Discard/trim blocks
    Discard,
    /// Write zeros
    WriteZeros,
    /// Secure erase
    SecureErase,
}

bitflags::bitflags! {
    /// Block request flags
    #[derive(Clone, Copy, Debug, Default)]
    pub struct BlockRequestFlags: u32 {
        /// Force unit access (bypass cache)
        const FUA = 1 << 0;
        /// High priority
        const HIGH_PRIORITY = 1 << 1;
        /// Preflush (flush before operation)
        const PREFLUSH = 1 << 2;
        /// No wait (return immediately if queue full)
        const NOWAIT = 1 << 3;
    }
}

/// Block I/O completion
#[derive(Clone, Debug)]
pub struct BlockCompletion {
    /// Request ID
    pub request_id: u64,
    /// Result (0 = success, negative = error)
    pub result: i32,
    /// Bytes transferred
    pub bytes_transferred: u64,
}

// ============================================================================
// Block Device Management
// ============================================================================

/// Register a new block device
pub fn register_block_device(
    device_id: DeviceId,
    name: String,
    device_type: BlockDeviceType,
    block_size: u32,
    num_blocks: u64,
    capabilities: BlockCapabilities,
) -> Result<BlockDeviceId, DriverError> {
    let id = BlockDeviceId::new();

    let block_device = BlockDevice {
        id,
        device_id,
        name: name.clone(),
        device_type,
        block_size,
        num_blocks,
        state: BlockDeviceState::Discovered,
        capabilities,
        partitions: Vec::new(),
        driver: None,
    };

    BLOCK_DEVICES.write().insert(id, block_device);

    log::info!(
        "Registered block device {} ({:?}, {} blocks of {} bytes)",
        name,
        device_type,
        num_blocks,
        block_size
    );

    Ok(id)
}

/// Unregister a block device
pub fn unregister_block_device(id: BlockDeviceId) -> Result<(), DriverError> {
    BLOCK_DEVICES
        .write()
        .remove(&id)
        .ok_or(DriverError::DeviceNotFound)?;

    Ok(())
}

/// Get block device info
pub fn get_block_device(id: BlockDeviceId) -> Option<BlockDevice> {
    BLOCK_DEVICES.read().get(&id).cloned()
}

/// Get all block devices
pub fn list_block_devices() -> Vec<BlockDeviceId> {
    BLOCK_DEVICES.read().keys().cloned().collect()
}

/// Find block device by name
pub fn find_by_name(name: &str) -> Option<BlockDeviceId> {
    BLOCK_DEVICES
        .read()
        .values()
        .find(|d| d.name == name)
        .map(|d| d.id)
}

/// Update block device state
pub fn set_device_state(id: BlockDeviceId, state: BlockDeviceState) -> Result<(), DriverError> {
    let mut devices = BLOCK_DEVICES.write();
    let device = devices.get_mut(&id).ok_or(DriverError::DeviceNotFound)?;
    device.state = state;
    Ok(())
}

/// Add partitions to a block device
pub fn set_partitions(id: BlockDeviceId, partitions: Vec<Partition>) -> Result<(), DriverError> {
    let mut devices = BLOCK_DEVICES.write();
    let device = devices.get_mut(&id).ok_or(DriverError::DeviceNotFound)?;
    device.partitions = partitions;
    Ok(())
}

// ============================================================================
// Block I/O Capability Granting
// ============================================================================

/// Grant block device access capability
pub fn grant_block_capability(
    process_id: ProcessId,
    device_id: BlockDeviceId,
    write: bool,
) -> Result<Capability, DriverError> {
    // Verify device exists
    BLOCK_DEVICES
        .read()
        .get(&device_id)
        .ok_or(DriverError::DeviceNotFound)?;

    let mut rights = Rights::READ | Rights::POLL;
    if write {
        rights |= Rights::WRITE;
    }

    let cap = unsafe {
        Capability::new_unchecked(ObjectId::new(ObjectType::BlockDevice), rights)
    };

    log::debug!(
        "Granted block device {:?} capability to process {:?}",
        device_id,
        process_id
    );

    Ok(cap)
}

// ============================================================================
// Block I/O Interface (for user-space drivers)
// ============================================================================

/// Submit block I/O request
///
/// This is called by user-space drivers through a syscall.
/// The actual I/O is handled by the device-specific driver.
pub fn submit_request(
    device_id: BlockDeviceId,
    request: BlockRequest,
) -> Result<(), DriverError> {
    let devices = BLOCK_DEVICES.read();
    let device = devices.get(&device_id).ok_or(DriverError::DeviceNotFound)?;

    // Validate request
    if device.state != BlockDeviceState::Ready {
        return Err(DriverError::HardwareError);
    }

    // Check bounds
    let end_block = request.start_block + request.num_blocks as u64;
    if end_block > device.num_blocks {
        return Err(DriverError::InvalidConfig);
    }

    // Check capabilities
    match request.operation {
        BlockOperation::Discard if !device.capabilities.discard => {
            return Err(DriverError::InvalidConfig);
        }
        BlockOperation::SecureErase if !device.capabilities.secure_erase => {
            return Err(DriverError::InvalidConfig);
        }
        BlockOperation::WriteZeros if !device.capabilities.write_zeros => {
            return Err(DriverError::InvalidConfig);
        }
        _ => {}
    }

    // In a real implementation, this would queue the request to the
    // device-specific driver process via IPC

    Ok(())
}

/// Poll for completed requests
pub fn poll_completions(device_id: BlockDeviceId) -> Result<Vec<BlockCompletion>, DriverError> {
    let _devices = BLOCK_DEVICES.read();
    // In a real implementation, this would retrieve completions from the
    // device-specific driver

    Ok(Vec::new())
}

// ============================================================================
// Partition Table Parsing
// ============================================================================

/// Parse GPT partition table
pub fn parse_gpt(device_id: BlockDeviceId, buffer: &[u8]) -> Result<Vec<Partition>, DriverError> {
    if buffer.len() < 512 {
        return Err(DriverError::InvalidConfig);
    }

    // Check GPT signature "EFI PART"
    if &buffer[0..8] != b"EFI PART" {
        return Err(DriverError::InvalidConfig);
    }

    let num_entries = u32::from_le_bytes([buffer[80], buffer[81], buffer[82], buffer[83]]);
    let entry_size = u32::from_le_bytes([buffer[84], buffer[85], buffer[86], buffer[87]]);
    let _first_entry_lba = u64::from_le_bytes([
        buffer[72], buffer[73], buffer[74], buffer[75],
        buffer[76], buffer[77], buffer[78], buffer[79],
    ]);

    let mut partitions = Vec::new();

    // Parse partition entries (would need to read additional sectors)
    // This is a simplified version - real implementation would read
    // the partition entry array from the appropriate LBA

    for i in 0..num_entries.min(128) {
        let entry_offset = (i as usize) * (entry_size as usize);
        if entry_offset + entry_size as usize > buffer.len() {
            break;
        }

        // Skip if type GUID is zero (unused entry)
        let type_guid = &buffer[entry_offset..entry_offset + 16];
        if type_guid.iter().all(|&b| b == 0) {
            continue;
        }

        let mut type_guid_arr = [0u8; 16];
        let mut partition_guid_arr = [0u8; 16];
        type_guid_arr.copy_from_slice(type_guid);
        partition_guid_arr.copy_from_slice(&buffer[entry_offset + 16..entry_offset + 32]);

        let start_lba = u64::from_le_bytes([
            buffer[entry_offset + 32], buffer[entry_offset + 33],
            buffer[entry_offset + 34], buffer[entry_offset + 35],
            buffer[entry_offset + 36], buffer[entry_offset + 37],
            buffer[entry_offset + 38], buffer[entry_offset + 39],
        ]);

        let end_lba = u64::from_le_bytes([
            buffer[entry_offset + 40], buffer[entry_offset + 41],
            buffer[entry_offset + 42], buffer[entry_offset + 43],
            buffer[entry_offset + 44], buffer[entry_offset + 45],
            buffer[entry_offset + 46], buffer[entry_offset + 47],
        ]);

        let attributes = u64::from_le_bytes([
            buffer[entry_offset + 48], buffer[entry_offset + 49],
            buffer[entry_offset + 50], buffer[entry_offset + 51],
            buffer[entry_offset + 52], buffer[entry_offset + 53],
            buffer[entry_offset + 54], buffer[entry_offset + 55],
        ]);

        // Parse UTF-16LE name
        let name_bytes = &buffer[entry_offset + 56..entry_offset + 128];
        let name = String::from_utf16_lossy(
            &name_bytes
                .chunks(2)
                .map(|c| u16::from_le_bytes([c[0], c.get(1).copied().unwrap_or(0)]))
                .take_while(|&c| c != 0)
                .collect::<Vec<_>>(),
        );

        partitions.push(Partition {
            number: (i + 1) as u32,
            start_block: start_lba,
            num_blocks: end_lba - start_lba + 1,
            type_guid: type_guid_arr,
            partition_guid: partition_guid_arr,
            name,
            attributes,
        });
    }

    Ok(partitions)
}

/// Check if partition is a Linux filesystem
pub fn is_linux_partition(type_guid: &[u8; 16]) -> bool {
    // Linux filesystem GUID: 0FC63DAF-8483-4772-8E79-3D69D8477DE4
    const LINUX_FS_GUID: [u8; 16] = [
        0xAF, 0x3D, 0xC6, 0x0F, 0x83, 0x84, 0x72, 0x47,
        0x8E, 0x79, 0x3D, 0x69, 0xD8, 0x47, 0x7D, 0xE4,
    ];
    type_guid == &LINUX_FS_GUID
}

/// Check if partition is EFI System Partition
pub fn is_efi_partition(type_guid: &[u8; 16]) -> bool {
    // EFI System Partition GUID: C12A7328-F81F-11D2-BA4B-00A0C93EC93B
    const EFI_GUID: [u8; 16] = [
        0x28, 0x73, 0x2A, 0xC1, 0x1F, 0xF8, 0xD2, 0x11,
        0xBA, 0x4B, 0x00, 0xA0, 0xC9, 0x3E, 0xC9, 0x3B,
    ];
    type_guid == &EFI_GUID
}
