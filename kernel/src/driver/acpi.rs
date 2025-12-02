//! ACPI (Advanced Configuration and Power Interface) support
//!
//! Provides access to ACPI tables for hardware discovery, power management,
//! and system configuration.

use alloc::string::String;
use alloc::vec::Vec;
use spin::{Lazy, RwLock};

/// ACPI tables cache
static ACPI_TABLES: Lazy<RwLock<Option<AcpiTables>>> = Lazy::new(|| RwLock::new(None));

/// ACPI tables container
#[derive(Clone)]
pub struct AcpiTables {
    /// RSDP (Root System Description Pointer) address
    pub rsdp_addr: u64,
    /// List of discovered ACPI devices
    pub devices: Vec<AcpiDevice>,
    /// MADT (Multiple APIC Description Table) if present
    pub madt: Option<MadtInfo>,
    /// FADT (Fixed ACPI Description Table) if present
    pub fadt: Option<FadtInfo>,
}

impl AcpiTables {
    /// Find device by Hardware ID (HID)
    pub fn find_device_by_hid(&self, hids: &[&str]) -> Option<AcpiDevice> {
        for device in &self.devices {
            for hid in hids {
                if device.hid == *hid {
                    return Some(device.clone());
                }
            }
        }
        None
    }

    /// Find all devices matching a HID pattern
    pub fn find_devices_by_hid_prefix(&self, prefix: &str) -> Vec<AcpiDevice> {
        self.devices
            .iter()
            .filter(|d| d.hid.starts_with(prefix))
            .cloned()
            .collect()
    }

    /// Get all ACPI devices
    pub fn all_devices(&self) -> &[AcpiDevice] {
        &self.devices
    }
}

/// ACPI device information
#[derive(Clone, Debug)]
pub struct AcpiDevice {
    /// Hardware ID (e.g., "PNP0C0F", "INTC1040")
    pub hid: String,
    /// Unique ID
    pub uid: u64,
    /// Compatible IDs
    pub cids: Vec<String>,
    /// Device status
    pub status: u32,
    /// Current resource settings (memory, IRQ, etc.)
    pub resources: Vec<AcpiResource>,
}

impl AcpiDevice {
    /// Check if device is present and functioning
    pub fn is_present(&self) -> bool {
        (self.status & 0x01) != 0
    }

    /// Check if device is enabled
    pub fn is_enabled(&self) -> bool {
        (self.status & 0x02) != 0
    }
}

/// ACPI resource descriptor
#[derive(Clone, Debug)]
pub enum AcpiResource {
    /// Memory region
    Memory {
        base: u64,
        length: u64,
        writable: bool,
    },
    /// I/O port range
    Io {
        base: u16,
        length: u16,
    },
    /// IRQ
    Irq {
        irq: u32,
        trigger: IrqTrigger,
        polarity: IrqPolarity,
    },
    /// DMA channel
    Dma {
        channel: u8,
    },
}

/// IRQ trigger mode
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IrqTrigger {
    Edge,
    Level,
}

/// IRQ polarity
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IrqPolarity {
    ActiveHigh,
    ActiveLow,
}

/// MADT (Multiple APIC Description Table) information
#[derive(Clone, Debug)]
pub struct MadtInfo {
    /// Local APIC address
    pub local_apic_addr: u64,
    /// Processor local APICs
    pub local_apics: Vec<LocalApicInfo>,
    /// I/O APICs
    pub io_apics: Vec<IoApicInfo>,
    /// Interrupt source overrides
    pub overrides: Vec<InterruptOverride>,
}

/// Local APIC information
#[derive(Clone, Debug)]
pub struct LocalApicInfo {
    pub acpi_processor_uid: u8,
    pub apic_id: u8,
    pub enabled: bool,
}

/// I/O APIC information
#[derive(Clone, Debug)]
pub struct IoApicInfo {
    pub io_apic_id: u8,
    pub io_apic_addr: u32,
    pub gsi_base: u32,
}

/// Interrupt source override
#[derive(Clone, Debug)]
pub struct InterruptOverride {
    pub bus_source: u8,
    pub irq_source: u8,
    pub gsi: u32,
    pub flags: u16,
}

/// FADT (Fixed ACPI Description Table) information
#[derive(Clone, Debug)]
pub struct FadtInfo {
    pub sci_interrupt: u16,
    pub pm1a_evt_blk: u32,
    pub pm1a_cnt_blk: u32,
    pub pm_timer_blk: u32,
    pub pm_timer_length: u8,
    pub reset_reg: Option<u64>,
    pub reset_value: u8,
    pub century: u8,
    pub boot_arch_flags: u16,
}

/// Initialize ACPI subsystem
pub fn init() {
    log::info!("Initializing ACPI subsystem");

    // Search for RSDP in standard locations
    let rsdp = find_rsdp();

    if let Some(rsdp_addr) = rsdp {
        log::debug!("Found RSDP at {:#x}", rsdp_addr);

        // Parse ACPI tables
        if let Some(tables) = parse_acpi_tables(rsdp_addr) {
            log::info!("ACPI: Found {} devices", tables.devices.len());
            *ACPI_TABLES.write() = Some(tables);
        }
    } else {
        log::warn!("ACPI: RSDP not found");
    }
}

/// Get ACPI tables
pub fn get_acpi_tables() -> Option<AcpiTables> {
    ACPI_TABLES.read().clone()
}

/// Search for RSDP in standard memory locations
fn find_rsdp() -> Option<u64> {
    // Search EBDA (Extended BIOS Data Area)
    // Typically at 0x9FC00 - 0xA0000
    if let Some(addr) = search_rsdp_signature(0x9FC00, 0x400) {
        return Some(addr);
    }

    // Search BIOS ROM area
    // 0xE0000 - 0xFFFFF
    if let Some(addr) = search_rsdp_signature(0xE0000, 0x20000) {
        return Some(addr);
    }

    None
}

/// Search for "RSD PTR " signature
fn search_rsdp_signature(start: u64, length: u64) -> Option<u64> {
    const SIGNATURE: &[u8; 8] = b"RSD PTR ";

    let start_ptr = crate::arch::x86_64::paging::phys_to_virt(
        crate::mem::PhysAddr::new(start)
    ).as_u64() as *const u8;

    for offset in (0..length).step_by(16) {
        let ptr = unsafe { start_ptr.add(offset as usize) };
        let sig = unsafe { core::slice::from_raw_parts(ptr, 8) };

        if sig == SIGNATURE {
            // Verify checksum
            let rsdp = unsafe { core::slice::from_raw_parts(ptr, 20) };
            if checksum(rsdp) == 0 {
                return Some(start + offset);
            }
        }
    }

    None
}

/// Calculate ACPI table checksum
fn checksum(data: &[u8]) -> u8 {
    data.iter().fold(0u8, |acc, &b| acc.wrapping_add(b))
}

/// Parse ACPI tables starting from RSDP
fn parse_acpi_tables(rsdp_addr: u64) -> Option<AcpiTables> {
    // Read RSDP to find XSDT/RSDT
    let rsdp_virt = crate::arch::x86_64::paging::phys_to_virt(
        crate::mem::PhysAddr::new(rsdp_addr)
    ).as_u64() as *const u8;

    // Check revision (0 = ACPI 1.0, 2+ = ACPI 2.0+)
    let revision = unsafe { *rsdp_virt.add(15) };

    let sdt_addr = if revision >= 2 {
        // ACPI 2.0+ - use XSDT (64-bit addresses)
        unsafe {
            let xsdt_ptr = rsdp_virt.add(24) as *const u64;
            *xsdt_ptr
        }
    } else {
        // ACPI 1.0 - use RSDT (32-bit addresses)
        unsafe {
            let rsdt_ptr = rsdp_virt.add(16) as *const u32;
            *rsdt_ptr as u64
        }
    };

    log::trace!("ACPI SDT at {:#x}", sdt_addr);

    // Parse system description tables
    let mut devices = Vec::new();
    let mut madt = None;
    let mut fadt = None;

    // For now, return a stub implementation
    // Full implementation would parse DSDT/SSDT for devices

    Some(AcpiTables {
        rsdp_addr,
        devices,
        madt,
        fadt,
    })
}

/// Evaluate an ACPI method (AML interpreter)
pub fn evaluate_method(_path: &str) -> Option<AcpiValue> {
    // AML interpreter not implemented yet
    None
}

/// ACPI value types
#[derive(Clone, Debug)]
pub enum AcpiValue {
    Integer(u64),
    String(String),
    Buffer(Vec<u8>),
    Package(Vec<AcpiValue>),
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // AcpiDevice Tests
    // =========================================================================

    #[test]
    fn test_device_is_present() {
        let device = AcpiDevice {
            hid: "PNP0C0F".to_string(),
            uid: 0,
            cids: Vec::new(),
            status: 0x0F, // Present, enabled, functioning, etc.
            resources: Vec::new(),
        };

        assert!(device.is_present());

        let absent_device = AcpiDevice {
            hid: "PNP0C0F".to_string(),
            uid: 0,
            cids: Vec::new(),
            status: 0x0E, // Not present (bit 0 = 0)
            resources: Vec::new(),
        };

        assert!(!absent_device.is_present());
    }

    #[test]
    fn test_device_is_enabled() {
        let device = AcpiDevice {
            hid: "INTC1040".to_string(),
            uid: 0,
            cids: Vec::new(),
            status: 0x0F, // Present and enabled
            resources: Vec::new(),
        };

        assert!(device.is_enabled());

        let disabled_device = AcpiDevice {
            hid: "INTC1040".to_string(),
            uid: 0,
            cids: Vec::new(),
            status: 0x0D, // Present but not enabled (bit 1 = 0)
            resources: Vec::new(),
        };

        assert!(!disabled_device.is_enabled());
    }

    // =========================================================================
    // AcpiTables Tests
    // =========================================================================

    #[test]
    fn test_find_device_by_hid() {
        let tables = AcpiTables {
            rsdp_addr: 0xE0000,
            devices: vec![
                AcpiDevice {
                    hid: "PNP0C0F".to_string(),
                    uid: 0,
                    cids: Vec::new(),
                    status: 0x0F,
                    resources: Vec::new(),
                },
                AcpiDevice {
                    hid: "INTC1040".to_string(),
                    uid: 0,
                    cids: Vec::new(),
                    status: 0x0F,
                    resources: Vec::new(),
                },
            ],
            madt: None,
            fadt: None,
        };

        // Single HID search
        let found = tables.find_device_by_hid(&["PNP0C0F"]);
        assert!(found.is_some());
        assert_eq!(found.unwrap().hid, "PNP0C0F");

        // Multiple HIDs search
        let found = tables.find_device_by_hid(&["NOTEXIST", "INTC1040"]);
        assert!(found.is_some());
        assert_eq!(found.unwrap().hid, "INTC1040");

        // Not found
        let found = tables.find_device_by_hid(&["NOTEXIST"]);
        assert!(found.is_none());
    }

    #[test]
    fn test_find_devices_by_hid_prefix() {
        let tables = AcpiTables {
            rsdp_addr: 0xE0000,
            devices: vec![
                AcpiDevice {
                    hid: "PNP0C0F".to_string(),
                    uid: 0,
                    cids: Vec::new(),
                    status: 0x0F,
                    resources: Vec::new(),
                },
                AcpiDevice {
                    hid: "PNP0C0D".to_string(),
                    uid: 0,
                    cids: Vec::new(),
                    status: 0x0F,
                    resources: Vec::new(),
                },
                AcpiDevice {
                    hid: "INTC1040".to_string(),
                    uid: 0,
                    cids: Vec::new(),
                    status: 0x0F,
                    resources: Vec::new(),
                },
            ],
            madt: None,
            fadt: None,
        };

        let pnp_devices = tables.find_devices_by_hid_prefix("PNP");
        assert_eq!(pnp_devices.len(), 2);

        let intc_devices = tables.find_devices_by_hid_prefix("INTC");
        assert_eq!(intc_devices.len(), 1);

        let none_devices = tables.find_devices_by_hid_prefix("XYZ");
        assert!(none_devices.is_empty());
    }

    #[test]
    fn test_all_devices() {
        let tables = AcpiTables {
            rsdp_addr: 0xE0000,
            devices: vec![
                AcpiDevice {
                    hid: "DEV1".to_string(),
                    uid: 0,
                    cids: Vec::new(),
                    status: 0x0F,
                    resources: Vec::new(),
                },
                AcpiDevice {
                    hid: "DEV2".to_string(),
                    uid: 0,
                    cids: Vec::new(),
                    status: 0x0F,
                    resources: Vec::new(),
                },
            ],
            madt: None,
            fadt: None,
        };

        let all = tables.all_devices();
        assert_eq!(all.len(), 2);
    }

    // =========================================================================
    // AcpiResource Tests
    // =========================================================================

    #[test]
    fn test_memory_resource() {
        let resource = AcpiResource::Memory {
            base: 0xFE00_0000,
            length: 0x1000,
            writable: true,
        };

        if let AcpiResource::Memory { base, length, writable } = resource {
            assert_eq!(base, 0xFE00_0000);
            assert_eq!(length, 0x1000);
            assert!(writable);
        } else {
            panic!("Expected Memory resource");
        }
    }

    #[test]
    fn test_io_resource() {
        let resource = AcpiResource::Io {
            base: 0x3F8,
            length: 8,
        };

        if let AcpiResource::Io { base, length } = resource {
            assert_eq!(base, 0x3F8);
            assert_eq!(length, 8);
        } else {
            panic!("Expected Io resource");
        }
    }

    #[test]
    fn test_irq_resource() {
        let resource = AcpiResource::Irq {
            irq: 10,
            trigger: IrqTrigger::Level,
            polarity: IrqPolarity::ActiveLow,
        };

        if let AcpiResource::Irq { irq, trigger, polarity } = resource {
            assert_eq!(irq, 10);
            assert_eq!(trigger, IrqTrigger::Level);
            assert_eq!(polarity, IrqPolarity::ActiveLow);
        } else {
            panic!("Expected Irq resource");
        }
    }

    #[test]
    fn test_dma_resource() {
        let resource = AcpiResource::Dma { channel: 3 };

        if let AcpiResource::Dma { channel } = resource {
            assert_eq!(channel, 3);
        } else {
            panic!("Expected Dma resource");
        }
    }

    // =========================================================================
    // IRQ Types Tests
    // =========================================================================

    #[test]
    fn test_irq_trigger_variants() {
        assert_ne!(IrqTrigger::Edge, IrqTrigger::Level);
        assert_eq!(IrqTrigger::Edge, IrqTrigger::Edge);
    }

    #[test]
    fn test_irq_polarity_variants() {
        assert_ne!(IrqPolarity::ActiveHigh, IrqPolarity::ActiveLow);
        assert_eq!(IrqPolarity::ActiveHigh, IrqPolarity::ActiveHigh);
    }

    // =========================================================================
    // MADT Tests
    // =========================================================================

    #[test]
    fn test_madt_info() {
        let madt = MadtInfo {
            local_apic_addr: 0xFEE0_0000,
            local_apics: vec![
                LocalApicInfo {
                    acpi_processor_uid: 0,
                    apic_id: 0,
                    enabled: true,
                },
                LocalApicInfo {
                    acpi_processor_uid: 1,
                    apic_id: 1,
                    enabled: true,
                },
            ],
            io_apics: vec![IoApicInfo {
                io_apic_id: 0,
                io_apic_addr: 0xFEC0_0000,
                gsi_base: 0,
            }],
            overrides: Vec::new(),
        };

        assert_eq!(madt.local_apic_addr, 0xFEE0_0000);
        assert_eq!(madt.local_apics.len(), 2);
        assert_eq!(madt.io_apics.len(), 1);
    }

    #[test]
    fn test_local_apic_info() {
        let apic = LocalApicInfo {
            acpi_processor_uid: 0,
            apic_id: 2,
            enabled: true,
        };

        assert_eq!(apic.acpi_processor_uid, 0);
        assert_eq!(apic.apic_id, 2);
        assert!(apic.enabled);
    }

    #[test]
    fn test_io_apic_info() {
        let io_apic = IoApicInfo {
            io_apic_id: 0,
            io_apic_addr: 0xFEC0_0000,
            gsi_base: 0,
        };

        assert_eq!(io_apic.io_apic_id, 0);
        assert_eq!(io_apic.io_apic_addr, 0xFEC0_0000);
        assert_eq!(io_apic.gsi_base, 0);
    }

    #[test]
    fn test_interrupt_override() {
        let override_entry = InterruptOverride {
            bus_source: 0,
            irq_source: 0,
            gsi: 2,
            flags: 0,
        };

        assert_eq!(override_entry.bus_source, 0);
        assert_eq!(override_entry.irq_source, 0);
        assert_eq!(override_entry.gsi, 2);
    }

    // =========================================================================
    // FADT Tests
    // =========================================================================

    #[test]
    fn test_fadt_info() {
        let fadt = FadtInfo {
            sci_interrupt: 9,
            pm1a_evt_blk: 0x800,
            pm1a_cnt_blk: 0x804,
            pm_timer_blk: 0x808,
            pm_timer_length: 4,
            reset_reg: Some(0x64),
            reset_value: 0xFE,
            century: 0x32,
            boot_arch_flags: 0x0003,
        };

        assert_eq!(fadt.sci_interrupt, 9);
        assert_eq!(fadt.pm1a_evt_blk, 0x800);
        assert_eq!(fadt.pm_timer_length, 4);
        assert_eq!(fadt.reset_reg, Some(0x64));
        assert_eq!(fadt.boot_arch_flags, 0x0003);
    }

    // =========================================================================
    // Checksum Tests
    // =========================================================================

    #[test]
    fn test_checksum_valid() {
        // A valid checksum should sum to 0
        let data = [0x01, 0x02, 0x03, 0xFA]; // Sum = 0 mod 256
        assert_eq!(checksum(&data), 0);
    }

    #[test]
    fn test_checksum_invalid() {
        let data = [0x01, 0x02, 0x03, 0x04];
        assert_ne!(checksum(&data), 0);
    }

    // =========================================================================
    // AcpiValue Tests
    // =========================================================================

    #[test]
    fn test_acpi_value_integer() {
        let value = AcpiValue::Integer(42);
        if let AcpiValue::Integer(n) = value {
            assert_eq!(n, 42);
        } else {
            panic!("Expected Integer");
        }
    }

    #[test]
    fn test_acpi_value_string() {
        let value = AcpiValue::String("test".to_string());
        if let AcpiValue::String(s) = value {
            assert_eq!(s, "test");
        } else {
            panic!("Expected String");
        }
    }

    #[test]
    fn test_acpi_value_buffer() {
        let value = AcpiValue::Buffer(vec![1, 2, 3, 4]);
        if let AcpiValue::Buffer(b) = value {
            assert_eq!(b, vec![1, 2, 3, 4]);
        } else {
            panic!("Expected Buffer");
        }
    }

    #[test]
    fn test_acpi_value_package() {
        let value = AcpiValue::Package(vec![
            AcpiValue::Integer(1),
            AcpiValue::Integer(2),
        ]);
        if let AcpiValue::Package(p) = value {
            assert_eq!(p.len(), 2);
        } else {
            panic!("Expected Package");
        }
    }
}
