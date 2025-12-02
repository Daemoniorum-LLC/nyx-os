//! # Device Driver Framework
//!
//! Provides infrastructure for device drivers in the Nyx microkernel.
//!
//! ## Architecture
//!
//! Unlike traditional monolithic kernels, Nyx runs drivers in user-space
//! with kernel-granted capabilities for hardware access:
//!
//! ```text
//! ┌─────────────────────────────────────────────────────┐
//! │                   User Space                         │
//! │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  │
//! │  │  NVMe Driver │  │  GPU Driver │  │ Net Driver  │  │
//! │  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘  │
//! ├─────────┼────────────────┼───────────────┼──────────┤
//! │         │    Capabilities│               │          │
//! │  ┌──────┴────────────────┴───────────────┴──────┐   │
//! │  │           Driver Framework (Kernel)           │   │
//! │  ├──────────────┬──────────────┬─────────────────┤   │
//! │  │  IRQ Manager │  MMIO Mapper │  DMA Allocator  │   │
//! │  └──────────────┴──────────────┴─────────────────┘   │
//! │  ┌────────────────────────────────────────────────┐  │
//! │  │              Hardware Abstraction              │  │
//! │  └────────────────────────────────────────────────┘  │
//! └─────────────────────────────────────────────────────┘
//! ```

pub mod acpi;
pub mod block;
pub mod device;
pub mod devicetree;
pub mod irq;
pub mod mmio;
pub mod pci;

use crate::cap::{Capability, CapError, ObjectId, ObjectType, Rights};
use crate::mem::PhysAddr;
use crate::process::ProcessId;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::RwLock;

/// Global device registry
static DEVICES: RwLock<BTreeMap<DeviceId, device::Device>> = RwLock::new(BTreeMap::new());

/// Next device ID
static NEXT_DEVICE_ID: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(1);

/// Device identifier
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeviceId(pub u64);

impl DeviceId {
    /// Create a new device ID
    pub fn new() -> Self {
        Self(NEXT_DEVICE_ID.fetch_add(1, core::sync::atomic::Ordering::SeqCst))
    }
}

impl Default for DeviceId {
    fn default() -> Self {
        Self::new()
    }
}

/// Driver framework errors
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriverError {
    /// Device not found
    DeviceNotFound,
    /// IRQ already registered
    IrqAlreadyRegistered,
    /// IRQ not found
    IrqNotFound,
    /// MMIO region conflict
    MmioConflict,
    /// Out of resources
    OutOfResources,
    /// Permission denied
    PermissionDenied,
    /// Invalid configuration
    InvalidConfig,
    /// Hardware error
    HardwareError,
    /// Capability error
    Capability(CapError),
}

impl From<CapError> for DriverError {
    fn from(err: CapError) -> Self {
        DriverError::Capability(err)
    }
}

/// Initialize the driver framework
pub fn init() {
    log::info!("Initializing driver framework");

    // Initialize subsystems
    irq::init();
    pci::init();

    log::info!("Driver framework initialized");
}

// ============================================================================
// Device Management
// ============================================================================

/// Register a new device
pub fn register_device(
    name: String,
    device_type: device::DeviceType,
    bus_info: Option<device::BusInfo>,
) -> Result<DeviceId, DriverError> {
    let device_id = DeviceId::new();

    let device = device::Device {
        id: device_id,
        name,
        device_type,
        bus_info,
        driver: None,
        irqs: Vec::new(),
        mmio_regions: Vec::new(),
        dma_regions: Vec::new(),
        state: device::DeviceState::Uninitialized,
    };

    DEVICES.write().insert(device_id, device);

    log::debug!("Registered device {:?}", device_id);

    Ok(device_id)
}

/// Unregister a device
pub fn unregister_device(device_id: DeviceId) -> Result<(), DriverError> {
    let mut devices = DEVICES.write();

    let device = devices
        .get(&device_id)
        .ok_or(DriverError::DeviceNotFound)?;

    // Release IRQs
    for &irq in &device.irqs {
        irq::unregister_irq(irq)?;
    }

    devices.remove(&device_id);

    log::debug!("Unregistered device {:?}", device_id);

    Ok(())
}

/// Get device information
pub fn get_device(device_id: DeviceId) -> Option<device::Device> {
    DEVICES.read().get(&device_id).cloned()
}

/// List all devices
pub fn list_devices() -> Vec<DeviceId> {
    DEVICES.read().keys().cloned().collect()
}

/// List devices by type
pub fn list_devices_by_type(device_type: device::DeviceType) -> Vec<DeviceId> {
    DEVICES
        .read()
        .iter()
        .filter(|(_, d)| d.device_type == device_type)
        .map(|(id, _)| *id)
        .collect()
}

// ============================================================================
// Capability Granting
// ============================================================================

/// Grant IRQ capability to a process
pub fn grant_irq_capability(
    process_id: ProcessId,
    irq_number: u8,
) -> Result<Capability, DriverError> {
    // Verify IRQ is valid
    irq::validate_irq(irq_number)?;

    // Create IRQ capability
    let object_id = ObjectId::new(ObjectType::Interrupt);
    let cap = unsafe {
        Capability::new_unchecked(
            object_id,
            Rights::IRQ | Rights::WAIT | Rights::POLL | Rights::GRANT,
        )
    };

    log::debug!(
        "Granted IRQ {} capability to process {:?}",
        irq_number,
        process_id
    );

    Ok(cap)
}

/// Grant MMIO capability to a process
pub fn grant_mmio_capability(
    process_id: ProcessId,
    phys_addr: PhysAddr,
    size: u64,
) -> Result<Capability, DriverError> {
    // Register MMIO region
    mmio::register_region(phys_addr, size)?;

    // Create MMIO capability
    let object_id = ObjectId::new(ObjectType::MmioRegion);
    let cap = unsafe {
        Capability::new_unchecked(
            object_id,
            Rights::MMIO | Rights::READ | Rights::WRITE | Rights::GRANT,
        )
    };

    log::debug!(
        "Granted MMIO {:016x}-{:016x} capability to process {:?}",
        phys_addr.as_u64(),
        phys_addr.as_u64() + size,
        process_id
    );

    Ok(cap)
}

/// Grant DMA buffer capability to a process
pub fn grant_dma_capability(
    process_id: ProcessId,
    size: u64,
) -> Result<(Capability, PhysAddr), DriverError> {
    // Allocate DMA-capable memory
    let phys = crate::mem::alloc_contiguous(size)
        .ok_or(DriverError::OutOfResources)?;

    // Create DMA capability
    let object_id = ObjectId::new(ObjectType::DmaBuffer);
    let cap = unsafe {
        Capability::new_unchecked(
            object_id,
            Rights::DMA | Rights::READ | Rights::WRITE | Rights::GRANT,
        )
    };

    log::debug!(
        "Granted DMA buffer ({} bytes at {:016x}) to process {:?}",
        size,
        phys.as_u64(),
        process_id
    );

    Ok((cap, phys))
}

/// Grant I/O port capability (x86 specific)
pub fn grant_ioport_capability(
    process_id: ProcessId,
    port_start: u16,
    port_count: u16,
) -> Result<Capability, DriverError> {
    // Create I/O port capability
    let object_id = ObjectId::new(ObjectType::IoPort);
    let cap = unsafe {
        Capability::new_unchecked(
            object_id,
            Rights::IOPORT | Rights::READ | Rights::WRITE | Rights::GRANT,
        )
    };

    log::debug!(
        "Granted I/O ports {:04x}-{:04x} to process {:?}",
        port_start,
        port_start + port_count - 1,
        process_id
    );

    Ok(cap)
}

// ============================================================================
// Syscall Interface
// ============================================================================

/// Handle driver syscalls
pub fn handle_syscall(
    syscall_num: u64,
    arg0: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
) -> Result<u64, DriverError> {
    match syscall_num {
        // IRQ wait
        0 => {
            let irq = arg0 as u8;
            irq::wait_irq(irq)?;
            Ok(0)
        }
        // IRQ ack
        1 => {
            let irq = arg0 as u8;
            irq::ack_irq(irq);
            Ok(0)
        }
        // MMIO read
        2 => {
            let addr = PhysAddr::new(arg0);
            let size = arg1 as u8;
            Ok(mmio::read(addr, size)?)
        }
        // MMIO write
        3 => {
            let addr = PhysAddr::new(arg0);
            let size = arg1 as u8;
            let value = arg2;
            mmio::write(addr, size, value)?;
            Ok(0)
        }
        // PCI config read
        4 => {
            let bus = (arg0 >> 16) as u8;
            let device = (arg0 >> 8) as u8;
            let function = arg0 as u8;
            let offset = arg1 as u8;
            let size = arg2 as u8;
            Ok(pci::config_read(bus, device, function, offset, size) as u64)
        }
        // PCI config write
        5 => {
            let bus = (arg0 >> 16) as u8;
            let device = (arg0 >> 8) as u8;
            let function = arg0 as u8;
            let offset = arg1 as u8;
            let size = arg2 as u8;
            let value = arg3 as u32;
            pci::config_write(bus, device, function, offset, size, value);
            Ok(0)
        }
        _ => Err(DriverError::InvalidConfig),
    }
}
