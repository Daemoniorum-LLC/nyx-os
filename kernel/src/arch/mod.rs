//! Architecture-specific code
//!
//! Each architecture provides:
//! - Boot code and early initialization
//! - Interrupt/exception handling
//! - Page table management
//! - Context switching
//! - CPU feature detection

#[cfg(all(feature = "arch-x86_64", not(test)))]
pub mod x86_64;

#[cfg(all(feature = "arch-x86_64", not(test)))]
pub use x86_64::*;

#[cfg(all(feature = "arch-aarch64", not(test)))]
pub mod aarch64;

#[cfg(all(feature = "arch-aarch64", not(test)))]
pub use aarch64::*;

// Test stubs for architecture-dependent functions
#[cfg(test)]
pub mod x86_64 {
    pub mod paging {
        pub use crate::arch::paging::*;

        pub enum MapError {
            AlreadyMapped,
            OutOfMemory,
            NotMapped,
        }

        pub fn switch_address_space(_root: crate::mem::PhysAddr) {}
        pub fn phys_to_virt(addr: crate::mem::PhysAddr) -> u64 {
            addr.as_u64() + 0xFFFF_8000_0000_0000
        }
    }

    pub mod idt {
        #[repr(C)]
        #[derive(Debug, Default)]
        pub struct ExceptionFrame {
            pub r15: u64, pub r14: u64, pub r13: u64, pub r12: u64,
            pub r11: u64, pub r10: u64, pub r9: u64, pub r8: u64,
            pub rbp: u64, pub rdi: u64, pub rsi: u64, pub rdx: u64,
            pub rcx: u64, pub rbx: u64, pub rax: u64,
            pub exception_number: u64, pub error_code: u64,
            pub rip: u64, pub cs: u64, pub rflags: u64, pub rsp: u64, pub ss: u64,
        }
    }

    pub mod smp {
        use core::sync::atomic::{AtomicU32, Ordering};
        static CPU_COUNT: AtomicU32 = AtomicU32::new(1);

        pub fn current_cpu_id() -> u32 { 0 }
        pub fn cpu_count() -> u32 { CPU_COUNT.load(Ordering::Relaxed) }
        pub fn send_ipi_to(_cpu: u32, _vector: u8) {}
        pub fn init_apic_timer(_hz: u32) {}
        pub fn send_eoi() {}
    }

    pub mod serial {
        pub fn write_fmt(_args: core::fmt::Arguments) {}
    }

    pub fn rdtsc() -> u64 { 0 }
}

#[cfg(test)]
pub mod paging {
    use crate::mem::PhysAddr;

    pub fn get_kernel_page_table() -> PhysAddr {
        PhysAddr::new(0x1000)
    }

    pub fn init() {}

    pub fn flush_tlb_page(_addr: crate::mem::VirtAddr) {}

    bitflags::bitflags! {
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
        pub struct PageFlags: u64 {
            const PRESENT = 1 << 0;
            const WRITABLE = 1 << 1;
            const USER = 1 << 2;
            const NO_CACHE = 1 << 4;
            const WRITE_THROUGH = 1 << 3;
            const NO_EXECUTE = 1 << 63;
        }
    }

    pub struct PageMapper {
        _root: PhysAddr,
    }

    impl PageMapper {
        pub fn new(root: PhysAddr) -> Self { Self { _root: root } }

        pub fn map_page(
            &mut self,
            _virt: crate::mem::VirtAddr,
            _phys: PhysAddr,
            _flags: PageFlags
        ) -> Result<(), MapError> {
            Ok(())
        }

        pub fn unmap_page(&mut self, _virt: crate::mem::VirtAddr) -> Result<(), MapError> {
            Ok(())
        }
    }

    pub enum MapError {
        AlreadyMapped,
        OutOfMemory,
        NotMapped,
    }
}

#[cfg(test)]
pub fn halt() {}

#[cfg(test)]
pub fn disable_interrupts() {}

#[cfg(test)]
pub fn enable_interrupts() {}

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
    #[cfg(all(feature = "arch-x86_64", not(test)))]
    x86_64::smp::start_aps();
}
