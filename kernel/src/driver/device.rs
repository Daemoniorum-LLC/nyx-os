//! Device types and structures
//!
//! Defines common device abstractions used throughout the driver framework.

use super::DeviceId;
use crate::mem::PhysAddr;
use crate::process::ProcessId;
use alloc::string::String;
use alloc::vec::Vec;

/// Device information
#[derive(Clone, Debug)]
pub struct Device {
    /// Unique device ID
    pub id: DeviceId,
    /// Human-readable name
    pub name: String,
    /// Device type
    pub device_type: DeviceType,
    /// Bus information
    pub bus_info: Option<BusInfo>,
    /// Owning driver process
    pub driver: Option<DriverInfo>,
    /// Assigned IRQs
    pub irqs: Vec<u8>,
    /// MMIO regions
    pub mmio_regions: Vec<MmioRegionInfo>,
    /// DMA regions
    pub dma_regions: Vec<DmaRegionInfo>,
    /// Current state
    pub state: DeviceState,
}

/// Device type classification
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceType {
    /// Block storage device (SSD, HDD, NVMe)
    Block,
    /// Character device (serial, console)
    Character,
    /// Network interface
    Network,
    /// Graphics/display device
    Graphics,
    /// Input device (keyboard, mouse)
    Input,
    /// Sound device
    Audio,
    /// USB host controller
    UsbController,
    /// USB device
    UsbDevice,
    /// PCI bridge
    PciBridge,
    /// ACPI device
    Acpi,
    /// Platform device
    Platform,
    /// AI accelerator (GPU, TPU, NPU)
    AiAccelerator,
    /// Other/unknown
    Other,
}

impl DeviceType {
    /// Get PCI class code for this device type
    pub fn to_pci_class(&self) -> Option<(u8, u8)> {
        match self {
            DeviceType::Block => Some((0x01, 0x08)), // NVMe
            DeviceType::Network => Some((0x02, 0x00)),
            DeviceType::Graphics => Some((0x03, 0x00)),
            DeviceType::Audio => Some((0x04, 0x03)),
            DeviceType::UsbController => Some((0x0C, 0x03)),
            DeviceType::PciBridge => Some((0x06, 0x04)),
            DeviceType::AiAccelerator => Some((0x03, 0x02)), // 3D controller
            _ => None,
        }
    }

    /// Create from PCI class code
    pub fn from_pci_class(class: u8, subclass: u8) -> Self {
        match (class, subclass) {
            (0x01, _) => DeviceType::Block,           // Mass storage
            (0x02, _) => DeviceType::Network,         // Network
            (0x03, _) => DeviceType::Graphics,        // Display
            (0x04, _) => DeviceType::Audio,           // Multimedia
            (0x06, 0x04) => DeviceType::PciBridge,    // PCI bridge
            (0x0C, 0x03) => DeviceType::UsbController, // USB
            (0x12, _) => DeviceType::AiAccelerator,   // Processing accelerators
            _ => DeviceType::Other,
        }
    }
}

/// Bus information
#[derive(Clone, Debug)]
pub enum BusInfo {
    /// PCI/PCIe device
    Pci(PciInfo),
    /// USB device
    Usb(UsbInfo),
    /// Platform device (memory-mapped)
    Platform(PlatformInfo),
    /// ACPI device
    Acpi(AcpiInfo),
}

/// PCI device information
#[derive(Clone, Debug)]
pub struct PciInfo {
    /// Bus number
    pub bus: u8,
    /// Device number
    pub device: u8,
    /// Function number
    pub function: u8,
    /// Vendor ID
    pub vendor_id: u16,
    /// Device ID
    pub device_id: u16,
    /// Class code
    pub class: u8,
    /// Subclass code
    pub subclass: u8,
    /// Programming interface
    pub prog_if: u8,
    /// Revision ID
    pub revision: u8,
    /// Base Address Registers
    pub bars: [Bar; 6],
    /// Interrupt line
    pub interrupt_line: u8,
    /// Interrupt pin
    pub interrupt_pin: u8,
    /// MSI capable
    pub msi_capable: bool,
    /// MSI-X capable
    pub msix_capable: bool,
}

impl PciInfo {
    /// Get segment:bus:device.function string
    pub fn bdf(&self) -> String {
        alloc::format!("{:02x}:{:02x}.{}", self.bus, self.device, self.function)
    }
}

/// PCI Base Address Register
#[derive(Clone, Copy, Debug, Default)]
pub struct Bar {
    /// BAR is present
    pub present: bool,
    /// Memory or I/O space
    pub is_memory: bool,
    /// 64-bit BAR (spans two slots)
    pub is_64bit: bool,
    /// Prefetchable
    pub prefetchable: bool,
    /// Base address
    pub address: u64,
    /// Size in bytes
    pub size: u64,
}

/// USB device information
#[derive(Clone, Debug)]
pub struct UsbInfo {
    /// USB bus number
    pub bus: u8,
    /// Device address
    pub address: u8,
    /// Parent hub port
    pub port: u8,
    /// USB speed
    pub speed: UsbSpeed,
    /// Vendor ID
    pub vendor_id: u16,
    /// Product ID
    pub product_id: u16,
    /// Device class
    pub class: u8,
    /// Device subclass
    pub subclass: u8,
    /// Protocol
    pub protocol: u8,
}

/// USB speed
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsbSpeed {
    Low,      // 1.5 Mbps (USB 1.0)
    Full,     // 12 Mbps (USB 1.1)
    High,     // 480 Mbps (USB 2.0)
    Super,    // 5 Gbps (USB 3.0)
    SuperPlus, // 10 Gbps (USB 3.1)
    SuperPlus2x2, // 20 Gbps (USB 3.2)
}

/// Platform device information
#[derive(Clone, Debug)]
pub struct PlatformInfo {
    /// Device name/compatible string
    pub compatible: String,
    /// Base address
    pub base_address: PhysAddr,
    /// Size
    pub size: u64,
    /// IRQ numbers
    pub irqs: Vec<u8>,
}

/// ACPI device information
#[derive(Clone, Debug)]
pub struct AcpiInfo {
    /// ACPI path
    pub path: String,
    /// Hardware ID
    pub hid: String,
    /// Unique ID
    pub uid: Option<String>,
    /// Compatible IDs
    pub cids: Vec<String>,
}

/// MMIO region information
#[derive(Clone, Debug)]
pub struct MmioRegionInfo {
    /// Physical address
    pub phys_addr: PhysAddr,
    /// Size in bytes
    pub size: u64,
    /// Flags
    pub flags: MmioFlags,
}

bitflags::bitflags! {
    /// MMIO region flags
    #[derive(Clone, Copy, Debug, Default)]
    pub struct MmioFlags: u32 {
        /// Write-combining
        const WRITE_COMBINE = 1 << 0;
        /// Uncacheable
        const UNCACHED = 1 << 1;
        /// Prefetchable
        const PREFETCHABLE = 1 << 2;
    }
}

/// DMA region information
#[derive(Clone, Debug)]
pub struct DmaRegionInfo {
    /// Physical address
    pub phys_addr: PhysAddr,
    /// Size in bytes
    pub size: u64,
    /// Is coherent DMA
    pub coherent: bool,
}

/// Driver process information
#[derive(Clone, Debug)]
pub struct DriverInfo {
    /// Driver process ID
    pub process_id: ProcessId,
    /// Driver name
    pub name: String,
    /// Driver version
    pub version: String,
}

/// Device state
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceState {
    /// Device discovered but not initialized
    Uninitialized,
    /// Device is being initialized
    Initializing,
    /// Device is ready for use
    Ready,
    /// Device is suspended (power management)
    Suspended,
    /// Device has encountered an error
    Error,
    /// Device is being removed
    Removing,
    /// Device has been removed
    Removed,
}

impl DeviceState {
    /// Check if device is operational
    pub fn is_operational(&self) -> bool {
        matches!(self, DeviceState::Ready)
    }

    /// Check if device can be used
    pub fn is_usable(&self) -> bool {
        matches!(self, DeviceState::Ready | DeviceState::Suspended)
    }
}

/// Device capabilities
#[derive(Clone, Debug, Default)]
pub struct DeviceCapabilities {
    /// Device supports DMA
    pub dma: bool,
    /// Device supports MSI
    pub msi: bool,
    /// Device supports MSI-X
    pub msix: bool,
    /// Device supports power management
    pub power_management: bool,
    /// Device supports hotplug
    pub hotplug: bool,
    /// Device supports SR-IOV
    pub sriov: bool,
    /// Maximum DMA address
    pub dma_mask: u64,
}

/// Device power state
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerState {
    D0, // Fully on
    D1, // Light sleep
    D2, // Deeper sleep
    D3Hot, // Sleeping but powered
    D3Cold, // Off
}

/// Device event
#[derive(Clone, Debug)]
pub enum DeviceEvent {
    /// Device added
    Added(DeviceId),
    /// Device removed
    Removed(DeviceId),
    /// Device state changed
    StateChanged(DeviceId, DeviceState),
    /// Device error
    Error(DeviceId, String),
    /// Power state changed
    PowerStateChanged(DeviceId, PowerState),
}
