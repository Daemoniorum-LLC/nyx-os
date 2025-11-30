//! x86_64 architecture support

pub mod gdt;
pub mod idt;
pub mod paging;
pub mod serial;
pub mod smp;

use core::arch::asm;

/// Initialize x86_64-specific features
pub fn init() {
    // Initialize serial console first (for early debugging)
    serial::init();
    serial::init_logging();

    // Set up GDT
    gdt::init();

    // Set up IDT
    idt::init();

    // Enable required CPU features
    enable_features();

    log::info!("x86_64 architecture initialized");
}

/// Enable CPU features (SSE, AVX, etc.)
fn enable_features() {
    unsafe {
        // Enable SSE
        let mut cr0: u64;
        asm!("mov {}, cr0", out(reg) cr0);
        cr0 &= !(1 << 2); // Clear EM
        cr0 |= 1 << 1;    // Set MP
        asm!("mov cr0, {}", in(reg) cr0);

        let mut cr4: u64;
        asm!("mov {}, cr4", out(reg) cr4);
        cr4 |= 1 << 9;    // OSFXSR
        cr4 |= 1 << 10;   // OSXMMEXCPT
        asm!("mov cr4, {}", in(reg) cr4);

        // Enable AVX if supported
        if has_avx() {
            cr4 |= 1 << 18; // OSXSAVE
            asm!("mov cr4, {}", in(reg) cr4);

            // Enable AVX in XCR0
            let xcr0 = 0x7u64; // X87 + SSE + AVX
            asm!(
                "xsetbv",
                in("ecx") 0u32,
                in("eax") xcr0 as u32,
                in("edx") (xcr0 >> 32) as u32,
            );
        }
    }
}

/// Check if AVX is supported
fn has_avx() -> bool {
    let (_, _, ecx, _) = cpuid(1);
    (ecx & (1 << 28)) != 0 // AVX bit
}

/// Execute CPUID instruction
fn cpuid(leaf: u32) -> (u32, u32, u32, u32) {
    let (eax, ebx, ecx, edx): (u32, u32, u32, u32);
    unsafe {
        asm!(
            "cpuid",
            inout("eax") leaf => eax,
            out("ebx") ebx,
            out("ecx") ecx,
            out("edx") edx,
        );
    }
    (eax, ebx, ecx, edx)
}

/// Halt the CPU (wait for interrupt)
#[inline]
pub fn halt() {
    unsafe {
        asm!("hlt", options(nomem, nostack));
    }
}

/// Disable interrupts
#[inline]
pub fn disable_interrupts() {
    unsafe {
        asm!("cli", options(nomem, nostack));
    }
}

/// Enable interrupts
#[inline]
pub fn enable_interrupts() {
    unsafe {
        asm!("sti", options(nomem, nostack));
    }
}

/// Read timestamp counter
#[inline]
pub fn rdtsc() -> u64 {
    let (low, high): (u32, u32);
    unsafe {
        asm!(
            "rdtsc",
            out("eax") low,
            out("edx") high,
            options(nomem, nostack),
        );
    }
    ((high as u64) << 32) | (low as u64)
}
