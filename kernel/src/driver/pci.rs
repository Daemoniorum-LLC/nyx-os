//! PCI/PCIe bus enumeration and management
//!
//! Provides PCI configuration space access, device enumeration, and
//! resource management.

use super::device::{Bar, DeviceType, PciInfo};
use super::{register_device, DeviceId, DriverError};
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::arch::asm;
use spin::RwLock;

/// PCI configuration address port
const PCI_CONFIG_ADDRESS: u16 = 0xCF8;
/// PCI configuration data port
const PCI_CONFIG_DATA: u16 = 0xCFC;

/// Known PCI devices
static PCI_DEVICES: RwLock<BTreeMap<(u8, u8, u8), PciDevice>> = RwLock::new(BTreeMap::new());

/// PCI device structure
#[derive(Clone, Debug)]
pub struct PciDevice {
    /// Device ID in our system
    pub device_id: DeviceId,
    /// PCI information
    pub info: PciInfo,
    /// Associated driver
    pub driver: Option<String>,
}

/// Initialize PCI subsystem
pub fn init() {
    log::info!("Initializing PCI subsystem");

    // Enumerate PCI devices
    enumerate_devices();

    let count = PCI_DEVICES.read().len();
    log::info!("PCI: Found {} devices", count);
}

/// Enumerate all PCI devices
fn enumerate_devices() {
    // Scan all buses
    for bus in 0..=255u8 {
        scan_bus(bus);
    }
}

/// Scan a single PCI bus
fn scan_bus(bus: u8) {
    for device in 0..32u8 {
        scan_device(bus, device);
    }
}

/// Scan a single PCI device
fn scan_device(bus: u8, device: u8) {
    // Read vendor ID from function 0
    let vendor = config_read_u16(bus, device, 0, 0x00);
    if vendor == 0xFFFF {
        return; // No device
    }

    // Check if multi-function device
    let header_type = config_read_u8(bus, device, 0, 0x0E);
    let is_multifunction = (header_type & 0x80) != 0;

    // Scan function 0
    scan_function(bus, device, 0);

    // Scan other functions if multi-function
    if is_multifunction {
        for function in 1..8u8 {
            let vendor = config_read_u16(bus, device, function, 0x00);
            if vendor != 0xFFFF {
                scan_function(bus, device, function);
            }
        }
    }
}

/// Scan a single PCI function
fn scan_function(bus: u8, device: u8, function: u8) {
    let vendor_id = config_read_u16(bus, device, function, 0x00);
    let device_id = config_read_u16(bus, device, function, 0x02);
    let class = config_read_u8(bus, device, function, 0x0B);
    let subclass = config_read_u8(bus, device, function, 0x0A);
    let prog_if = config_read_u8(bus, device, function, 0x09);
    let revision = config_read_u8(bus, device, function, 0x08);
    let header_type = config_read_u8(bus, device, function, 0x0E) & 0x7F;
    let interrupt_line = config_read_u8(bus, device, function, 0x3C);
    let interrupt_pin = config_read_u8(bus, device, function, 0x3D);

    // Only handle standard devices (type 0)
    let bars = if header_type == 0 {
        read_bars(bus, device, function)
    } else {
        [Bar::default(); 6]
    };

    // Check for MSI/MSI-X capability
    let (msi_capable, msix_capable) = check_msi_capability(bus, device, function);

    let pci_info = PciInfo {
        bus,
        device,
        function,
        vendor_id,
        device_id,
        class,
        subclass,
        prog_if,
        revision,
        bars,
        interrupt_line,
        interrupt_pin,
        msi_capable,
        msix_capable,
    };

    // Determine device type and register
    let device_type = DeviceType::from_pci_class(class, subclass);
    let name = alloc::format!(
        "pci:{:04x}:{:04x} {:02x}:{:02x}.{}",
        vendor_id, device_id, bus, device, function
    );

    if let Ok(dev_id) = register_device(
        name.clone(),
        device_type,
        Some(super::device::BusInfo::Pci(pci_info.clone())),
    ) {
        let pci_device = PciDevice {
            device_id: dev_id,
            info: pci_info,
            driver: None,
        };

        PCI_DEVICES
            .write()
            .insert((bus, device, function), pci_device);

        log::debug!(
            "PCI: {:02x}:{:02x}.{} {:04x}:{:04x} class={:02x}{:02x} - {:?}",
            bus,
            device,
            function,
            vendor_id,
            device_id,
            class,
            subclass,
            device_type
        );
    }

    // If this is a PCI bridge, scan the secondary bus
    if class == 0x06 && subclass == 0x04 {
        let secondary_bus = config_read_u8(bus, device, function, 0x19);
        if secondary_bus > 0 {
            scan_bus(secondary_bus);
        }
    }
}

/// Read PCI Base Address Registers
fn read_bars(bus: u8, device: u8, function: u8) -> [Bar; 6] {
    let mut bars = [Bar::default(); 6];
    let mut i = 0;

    while i < 6 {
        let bar_offset = 0x10 + (i as u8) * 4;
        let bar_value = config_read_u32(bus, device, function, bar_offset);

        if bar_value == 0 {
            i += 1;
            continue;
        }

        let is_memory = (bar_value & 1) == 0;

        if is_memory {
            let is_64bit = ((bar_value >> 1) & 3) == 2;
            let prefetchable = (bar_value & 0x08) != 0;

            // Determine size by writing all 1s and reading back
            config_write_u32(bus, device, function, bar_offset, 0xFFFF_FFFF);
            let size_mask = config_read_u32(bus, device, function, bar_offset);
            config_write_u32(bus, device, function, bar_offset, bar_value);

            let size = if size_mask != 0 {
                (!(size_mask & 0xFFFF_FFF0) + 1) as u64
            } else {
                0
            };

            let address = if is_64bit && i < 5 {
                let high = config_read_u32(bus, device, function, bar_offset + 4) as u64;
                ((high << 32) | (bar_value & 0xFFFF_FFF0) as u64)
            } else {
                (bar_value & 0xFFFF_FFF0) as u64
            };

            bars[i] = Bar {
                present: true,
                is_memory: true,
                is_64bit,
                prefetchable,
                address,
                size,
            };

            if is_64bit {
                i += 2; // Skip next BAR slot
            } else {
                i += 1;
            }
        } else {
            // I/O space
            let address = (bar_value & 0xFFFF_FFFC) as u64;

            config_write_u32(bus, device, function, bar_offset, 0xFFFF_FFFF);
            let size_mask = config_read_u32(bus, device, function, bar_offset);
            config_write_u32(bus, device, function, bar_offset, bar_value);

            let size = (!(size_mask & 0xFFFF_FFFC) + 1) as u64;

            bars[i] = Bar {
                present: true,
                is_memory: false,
                is_64bit: false,
                prefetchable: false,
                address,
                size,
            };

            i += 1;
        }
    }

    bars
}

/// Check for MSI/MSI-X capability
fn check_msi_capability(bus: u8, device: u8, function: u8) -> (bool, bool) {
    let status = config_read_u16(bus, device, function, 0x06);

    // Check if device has capability list
    if (status & 0x10) == 0 {
        return (false, false);
    }

    let mut cap_ptr = config_read_u8(bus, device, function, 0x34) & 0xFC;
    let mut msi = false;
    let mut msix = false;

    while cap_ptr != 0 {
        let cap_id = config_read_u8(bus, device, function, cap_ptr);

        match cap_id {
            0x05 => msi = true,   // MSI
            0x11 => msix = true,  // MSI-X
            _ => {}
        }

        cap_ptr = config_read_u8(bus, device, function, cap_ptr + 1) & 0xFC;
    }

    (msi, msix)
}

// ============================================================================
// Configuration Space Access (via I/O ports)
// ============================================================================

/// Build PCI configuration address
fn config_address(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
    0x8000_0000
        | ((bus as u32) << 16)
        | ((device as u32) << 11)
        | ((function as u32) << 8)
        | ((offset as u32) & 0xFC)
}

/// Read from PCI configuration space
pub fn config_read(bus: u8, device: u8, function: u8, offset: u8, size: u8) -> u32 {
    match size {
        1 => config_read_u8(bus, device, function, offset) as u32,
        2 => config_read_u16(bus, device, function, offset) as u32,
        4 => config_read_u32(bus, device, function, offset),
        _ => 0xFFFF_FFFF,
    }
}

/// Write to PCI configuration space
pub fn config_write(bus: u8, device: u8, function: u8, offset: u8, size: u8, value: u32) {
    match size {
        1 => config_write_u8(bus, device, function, offset, value as u8),
        2 => config_write_u16(bus, device, function, offset, value as u16),
        4 => config_write_u32(bus, device, function, offset, value),
        _ => {}
    }
}

/// Read u8 from PCI config space
fn config_read_u8(bus: u8, device: u8, function: u8, offset: u8) -> u8 {
    let addr = config_address(bus, device, function, offset);
    let shift = (offset & 3) * 8;

    unsafe {
        outl(PCI_CONFIG_ADDRESS, addr);
        ((inl(PCI_CONFIG_DATA) >> shift) & 0xFF) as u8
    }
}

/// Read u16 from PCI config space
fn config_read_u16(bus: u8, device: u8, function: u8, offset: u8) -> u16 {
    let addr = config_address(bus, device, function, offset);
    let shift = (offset & 2) * 8;

    unsafe {
        outl(PCI_CONFIG_ADDRESS, addr);
        ((inl(PCI_CONFIG_DATA) >> shift) & 0xFFFF) as u16
    }
}

/// Read u32 from PCI config space
fn config_read_u32(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
    let addr = config_address(bus, device, function, offset);

    unsafe {
        outl(PCI_CONFIG_ADDRESS, addr);
        inl(PCI_CONFIG_DATA)
    }
}

/// Write u8 to PCI config space
fn config_write_u8(bus: u8, device: u8, function: u8, offset: u8, value: u8) {
    let addr = config_address(bus, device, function, offset);
    let shift = (offset & 3) * 8;

    unsafe {
        outl(PCI_CONFIG_ADDRESS, addr);
        let old = inl(PCI_CONFIG_DATA);
        let new = (old & !(0xFF << shift)) | ((value as u32) << shift);
        outl(PCI_CONFIG_DATA, new);
    }
}

/// Write u16 to PCI config space
fn config_write_u16(bus: u8, device: u8, function: u8, offset: u8, value: u16) {
    let addr = config_address(bus, device, function, offset);
    let shift = (offset & 2) * 8;

    unsafe {
        outl(PCI_CONFIG_ADDRESS, addr);
        let old = inl(PCI_CONFIG_DATA);
        let new = (old & !(0xFFFF << shift)) | ((value as u32) << shift);
        outl(PCI_CONFIG_DATA, new);
    }
}

/// Write u32 to PCI config space
fn config_write_u32(bus: u8, device: u8, function: u8, offset: u8, value: u32) {
    let addr = config_address(bus, device, function, offset);

    unsafe {
        outl(PCI_CONFIG_ADDRESS, addr);
        outl(PCI_CONFIG_DATA, value);
    }
}

// ============================================================================
// I/O Port Access
// ============================================================================

unsafe fn outl(port: u16, value: u32) {
    // SAFETY: Caller ensures valid PCI configuration port
    unsafe {
        asm!(
            "out dx, eax",
            in("dx") port,
            in("eax") value,
            options(nostack, preserves_flags)
        );
    }
}

unsafe fn inl(port: u16) -> u32 {
    let value: u32;
    // SAFETY: Caller ensures valid PCI configuration port
    unsafe {
        asm!(
            "in eax, dx",
            out("eax") value,
            in("dx") port,
            options(nostack, preserves_flags)
        );
    }
    value
}

// ============================================================================
// Device Lookup
// ============================================================================

/// Find devices by vendor and device ID
pub fn find_devices(vendor_id: u16, device_id: u16) -> Vec<DeviceId> {
    PCI_DEVICES
        .read()
        .values()
        .filter(|d| d.info.vendor_id == vendor_id && d.info.device_id == device_id)
        .map(|d| d.device_id)
        .collect()
}

/// Find devices by class code
pub fn find_devices_by_class(class: u8, subclass: u8) -> Vec<DeviceId> {
    PCI_DEVICES
        .read()
        .values()
        .filter(|d| d.info.class == class && d.info.subclass == subclass)
        .map(|d| d.device_id)
        .collect()
}

/// Get PCI device by BDF
pub fn get_device(bus: u8, device: u8, function: u8) -> Option<PciDevice> {
    PCI_DEVICES.read().get(&(bus, device, function)).cloned()
}

/// Get all PCI devices
pub fn get_all_devices() -> Vec<PciDevice> {
    PCI_DEVICES.read().values().cloned().collect()
}

// ============================================================================
// Device Control
// ============================================================================

/// Enable bus mastering for a device
pub fn enable_bus_master(bus: u8, device: u8, function: u8) {
    let command = config_read_u16(bus, device, function, 0x04);
    config_write_u16(bus, device, function, 0x04, command | 0x04);
}

/// Enable memory space access for a device
pub fn enable_memory_space(bus: u8, device: u8, function: u8) {
    let command = config_read_u16(bus, device, function, 0x04);
    config_write_u16(bus, device, function, 0x04, command | 0x02);
}

/// Enable I/O space access for a device
pub fn enable_io_space(bus: u8, device: u8, function: u8) {
    let command = config_read_u16(bus, device, function, 0x04);
    config_write_u16(bus, device, function, 0x04, command | 0x01);
}

/// Disable interrupts for a device
pub fn disable_interrupts(bus: u8, device: u8, function: u8) {
    let command = config_read_u16(bus, device, function, 0x04);
    config_write_u16(bus, device, function, 0x04, command | 0x400);
}

/// Configure MSI for a device
pub fn configure_msi(
    bus: u8,
    device: u8,
    function: u8,
    vector: u8,
    cpu_id: u32,
) -> Result<(), DriverError> {
    // Find MSI capability
    let status = config_read_u16(bus, device, function, 0x06);
    if (status & 0x10) == 0 {
        return Err(DriverError::InvalidConfig);
    }

    let mut cap_ptr = config_read_u8(bus, device, function, 0x34) & 0xFC;

    while cap_ptr != 0 {
        let cap_id = config_read_u8(bus, device, function, cap_ptr);

        if cap_id == 0x05 {
            // MSI capability found
            let msg_ctrl = config_read_u16(bus, device, function, cap_ptr + 2);
            let is_64bit = (msg_ctrl & 0x80) != 0;

            // Message address (APIC)
            let msg_addr = 0xFEE0_0000u32 | ((cpu_id & 0xFF) << 12);
            config_write_u32(bus, device, function, cap_ptr + 4, msg_addr);

            // Message data
            let msg_data = vector as u32;
            if is_64bit {
                config_write_u32(bus, device, function, cap_ptr + 8, 0); // Upper 32 bits
                config_write_u16(bus, device, function, cap_ptr + 12, msg_data as u16);
            } else {
                config_write_u16(bus, device, function, cap_ptr + 8, msg_data as u16);
            }

            // Enable MSI
            let msg_ctrl = config_read_u16(bus, device, function, cap_ptr + 2);
            config_write_u16(bus, device, function, cap_ptr + 2, msg_ctrl | 0x01);

            // Disable legacy interrupts
            disable_interrupts(bus, device, function);

            return Ok(());
        }

        cap_ptr = config_read_u8(bus, device, function, cap_ptr + 1) & 0xFC;
    }

    Err(DriverError::InvalidConfig)
}
