//! Architecture-specific code
//!
//! Each architecture provides:
//! - Boot code and early initialization
//! - Interrupt/exception handling
//! - Page table management
//! - Context switching
//! - CPU feature detection

#[cfg(feature = "arch-x86_64")]
pub mod x86_64;

#[cfg(feature = "arch-x86_64")]
pub use x86_64::*;

#[cfg(feature = "arch-aarch64")]
pub mod aarch64;

#[cfg(feature = "arch-aarch64")]
pub use aarch64::*;

/// Boot information passed from bootloader
#[derive(Debug)]
pub struct BootInfo {
    /// Physical memory map
    pub memory_map: &'static [MemoryRegion],
    /// Initial ramdisk (initrd) location
    pub initrd: Option<&'static [u8]>,
    /// Kernel command line
    pub cmdline: &'static str,
    /// ACPI RSDP physical address
    pub acpi_rsdp: Option<u64>,
    /// Framebuffer info (if available)
    pub framebuffer: Option<FramebufferInfo>,
    /// Number of CPUs detected
    pub cpu_count: u32,
}

/// Memory region from bootloader
#[derive(Debug, Clone, Copy)]
pub struct MemoryRegion {
    /// Physical start address
    pub start: u64,
    /// Region size in bytes
    pub size: u64,
    /// Region type
    pub region_type: MemoryRegionType,
}

/// Memory region types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryRegionType {
    /// Usable RAM
    Usable,
    /// Reserved by firmware
    Reserved,
    /// ACPI reclaimable
    AcpiReclaimable,
    /// ACPI NVS
    AcpiNvs,
    /// Bad memory
    BadMemory,
    /// Bootloader reclaimable
    BootloaderReclaimable,
    /// Kernel and modules
    KernelAndModules,
    /// Framebuffer
    Framebuffer,
}

/// Framebuffer information
#[derive(Debug, Clone, Copy)]
pub struct FramebufferInfo {
    /// Physical address
    pub address: u64,
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
    /// Bytes per pixel
    pub bpp: u8,
    /// Pitch (bytes per row)
    pub pitch: u32,
}

/// Start secondary CPUs (after main init)
pub fn start_secondary_cpus() {
    #[cfg(feature = "arch-x86_64")]
    x86_64::smp::start_aps();
}
