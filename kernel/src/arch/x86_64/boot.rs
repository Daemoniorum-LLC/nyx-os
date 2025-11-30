//! x86_64 Boot Entry Point
//!
//! This module contains the _start entry point that's called by the bootloader.
//! It sets up initial state and jumps to kernel_main.

use core::arch::asm;

use crate::arch::BootInfo;

/// Kernel virtual base address (higher-half)
pub const KERNEL_VIRT_BASE: u64 = 0xFFFF_FFFF_8000_0000;

/// Physical memory map base
pub const PHYS_MAP_BASE: u64 = 0xFFFF_8800_0000_0000;

// External symbols from linker script
extern "C" {
    static _kernel_start: u8;
    static _kernel_end: u8;
    static _bss_start: u8;
    static _bss_end: u8;
    static _boot_stack_top: u8;
}

/// Boot information passed from bootloader
#[repr(C)]
#[derive(Debug)]
pub struct MultibootInfo {
    pub flags: u32,
    pub mem_lower: u32,
    pub mem_upper: u32,
    pub boot_device: u32,
    pub cmdline: u32,
    pub mods_count: u32,
    pub mods_addr: u32,
    // ... more fields
}

/// Early boot page tables (identity + higher-half mapping)
/// These are set up before paging is enabled
#[repr(C, align(4096))]
struct BootPageTables {
    pml4: [u64; 512],
    pdpt_low: [u64; 512],   // Identity mapping (0-512GB)
    pdpt_high: [u64; 512],  // Higher-half mapping
    pd: [u64; 512],         // Shared PD for first 1GB
}

#[link_section = ".data"]
static mut BOOT_PAGE_TABLES: BootPageTables = BootPageTables {
    pml4: [0; 512],
    pdpt_low: [0; 512],
    pdpt_high: [0; 512],
    pd: [0; 512],
};

/// Multiboot2 header (in separate section for proper placement)
#[cfg(feature = "multiboot2")]
#[link_section = ".multiboot.header"]
#[used]
static MULTIBOOT2_HEADER: [u32; 6] = [
    0xE85250D6,        // Magic
    0,                 // Architecture (i386)
    24,                // Header length
    0u32.wrapping_sub(0xE85250D6 + 0 + 24), // Checksum
    0,                 // End tag type
    8,                 // End tag size
];

/// Multiboot1 header (fallback)
#[link_section = ".multiboot.header"]
#[used]
static MULTIBOOT_HEADER: [u32; 3] = [
    0x1BADB002,        // Magic
    0x00000003,        // Flags (align modules, provide memory map)
    0u32.wrapping_sub(0x1BADB002 + 0x00000003), // Checksum
];

/// The entry point from the bootloader
///
/// When the bootloader jumps here:
/// - We're in 32-bit protected mode (no paging, or identity-mapped)
/// - EAX contains the multiboot magic
/// - EBX contains the multiboot info pointer
///
/// We need to:
/// 1. Set up 64-bit long mode
/// 2. Set up initial page tables
/// 3. Set up the stack
/// 4. Jump to kernel_main
#[no_mangle]
#[link_section = ".text.boot"]
pub unsafe extern "C" fn _start() -> ! {
    // We expect to be called in 64-bit mode by a Limine/UEFI bootloader
    // or in 32-bit mode by GRUB/Multiboot

    // For now, assume 64-bit mode (modern bootloader)
    asm!(
        // Clear direction flag
        "cld",

        // Set up stack (linker provides _boot_stack_top)
        "lea rsp, [rip + {stack_top}]",

        // Zero BSS section
        "lea rdi, [rip + {bss_start}]",
        "lea rcx, [rip + {bss_end}]",
        "sub rcx, rdi",
        "shr rcx, 3",  // Convert to qwords
        "xor eax, eax",
        "rep stosq",

        // Jump to Rust boot code
        "jmp {boot_rust}",

        stack_top = sym _boot_stack_top,
        bss_start = sym _bss_start,
        bss_end = sym _bss_end,
        boot_rust = sym boot_stage2,
        options(noreturn)
    );
}

/// Second stage boot (in Rust)
///
/// At this point:
/// - We're in 64-bit mode
/// - Stack is set up
/// - BSS is zeroed
unsafe extern "C" fn boot_stage2() -> ! {
    // Initialize serial console FIRST for early debugging
    crate::arch::x86_64::serial::init();
    crate::serial_println!("\n[BOOT] Nyx Kernel starting...");

    // Initialize architecture
    crate::serial_println!("[BOOT] Initializing x86_64 architecture");
    super::init();

    // Build boot info structure
    let boot_info = build_boot_info();

    crate::serial_println!("[BOOT] Jumping to kernel_main");

    // Call kernel main
    crate::kernel_main(&boot_info)
}

/// Build boot information from detected hardware
unsafe fn build_boot_info() -> BootInfo {
    // For now, return minimal boot info
    // A real implementation would parse multiboot/UEFI info

    let kernel_start = &_kernel_start as *const u8 as u64;
    let kernel_end = &_kernel_end as *const u8 as u64;

    crate::serial_println!(
        "[BOOT] Kernel: {:#x} - {:#x} ({} KB)",
        kernel_start,
        kernel_end,
        (kernel_end - kernel_start) / 1024
    );

    BootInfo {
        kernel_phys_start: kernel_start.wrapping_sub(KERNEL_VIRT_BASE),
        kernel_phys_end: kernel_end.wrapping_sub(KERNEL_VIRT_BASE),
        memory_map: &[],
        initrd: None,
        cmdline: None,
        framebuffer: None,
        rsdp_addr: None,
        cpu_count: 1,
    }
}

/// Halt the CPU in a loop (for error conditions)
#[inline(never)]
pub fn halt_loop() -> ! {
    loop {
        unsafe {
            asm!("cli; hlt", options(nomem, nostack));
        }
    }
}

/// Early panic before logging is available
#[cold]
pub fn early_panic(msg: &str) -> ! {
    // Try to print to serial
    unsafe {
        use core::fmt::Write;
        let mut serial = crate::arch::x86_64::serial::SerialPort::new(0x3F8);
        let _ = serial.init();
        let _ = writeln!(serial, "\n!!! EARLY PANIC: {}", msg);
    }
    halt_loop()
}
