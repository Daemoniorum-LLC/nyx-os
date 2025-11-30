//! Symmetric Multi-Processing (SMP) support
//!
//! Handles starting and managing Application Processors (APs) in a multi-core
//! system. Uses the INIT-SIPI-SIPI sequence as defined by Intel.

use core::arch::asm;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use spin::Mutex;

use crate::mem::PhysAddr;

/// Maximum number of CPUs supported
pub const MAX_CPUS: usize = 256;

/// Number of CPUs online
static CPU_COUNT: AtomicU32 = AtomicU32::new(1); // BSP is always online

/// Flag indicating AP startup is complete
static AP_STARTED: AtomicBool = AtomicBool::new(false);

/// Per-CPU data
static CPU_DATA: Mutex<[CpuData; MAX_CPUS]> = Mutex::new([CpuData::new(); MAX_CPUS]);

/// APIC base address (mapped)
static APIC_BASE: AtomicU32 = AtomicU32::new(0xFEE0_0000);

/// Per-CPU data structure
#[derive(Clone, Copy)]
pub struct CpuData {
    /// APIC ID
    pub apic_id: u32,
    /// Is this CPU online?
    pub online: bool,
    /// Kernel stack top
    pub kernel_stack: u64,
    /// Current thread ID
    pub current_thread: u64,
}

impl CpuData {
    const fn new() -> Self {
        Self {
            apic_id: 0,
            online: false,
            kernel_stack: 0,
            current_thread: 0,
        }
    }
}

/// Initialize SMP (called on BSP)
pub fn init() {
    // Detect APIC base address
    let apic_base = read_apic_base();
    APIC_BASE.store(apic_base as u32, Ordering::SeqCst);

    // Initialize BSP's CPU data
    let bsp_apic_id = read_apic_id();
    {
        let mut cpu_data = CPU_DATA.lock();
        cpu_data[0].apic_id = bsp_apic_id;
        cpu_data[0].online = true;
    }

    log::debug!(
        "SMP: BSP APIC ID = {}, APIC base = {:#x}",
        bsp_apic_id,
        apic_base
    );
}

/// Start Application Processors (secondary CPUs)
pub fn start_aps() {
    log::info!("SMP: Starting Application Processors");

    // Get list of APIC IDs from ACPI/MP tables
    // For now, we'll use a simple approach assuming sequential APIC IDs
    let bsp_apic_id = read_apic_id();

    // Copy AP trampoline code to low memory (below 1MB)
    // The trampoline must be at a 4KB-aligned address in the first 1MB
    let trampoline_addr = setup_trampoline();

    // Enumerate processors (typically from ACPI MADT)
    let processor_count = detect_processor_count();

    log::debug!("SMP: Detected {} processors", processor_count);

    // Start each AP
    for apic_id in 0..processor_count as u32 {
        if apic_id == bsp_apic_id {
            continue; // Skip BSP
        }

        start_ap(apic_id, trampoline_addr);
    }

    log::info!("SMP: {} CPUs online", CPU_COUNT.load(Ordering::SeqCst));
}

/// Start a single AP
fn start_ap(apic_id: u32, trampoline_addr: u64) {
    log::trace!("SMP: Starting AP {}", apic_id);

    // Reset the started flag
    AP_STARTED.store(false, Ordering::SeqCst);

    // Send INIT IPI
    send_ipi(apic_id, IpiType::Init);

    // Wait 10ms
    delay_us(10_000);

    // Send INIT de-assert (level-triggered)
    send_init_deassert(apic_id);

    // Wait 200us
    delay_us(200);

    // Send SIPI (Startup IPI) - twice as per Intel specification
    let vector = (trampoline_addr >> 12) as u8;
    send_ipi(apic_id, IpiType::Startup(vector));

    // Wait 200us
    delay_us(200);

    // Check if AP started
    if !AP_STARTED.load(Ordering::SeqCst) {
        // Send second SIPI
        send_ipi(apic_id, IpiType::Startup(vector));
        delay_us(200);
    }

    // Wait for AP to signal it's ready (with timeout)
    let mut timeout = 100_000; // 100ms timeout
    while !AP_STARTED.load(Ordering::SeqCst) && timeout > 0 {
        delay_us(100);
        timeout -= 100;
    }

    if AP_STARTED.load(Ordering::SeqCst) {
        let count = CPU_COUNT.fetch_add(1, Ordering::SeqCst) + 1;
        let mut cpu_data = CPU_DATA.lock();
        cpu_data[count as usize - 1].apic_id = apic_id;
        cpu_data[count as usize - 1].online = true;
        log::debug!("SMP: AP {} started successfully", apic_id);
    } else {
        log::warn!("SMP: AP {} failed to start", apic_id);
    }
}

/// IPI types
enum IpiType {
    Init,
    Startup(u8), // Vector (page number of trampoline code)
}

/// Send Inter-Processor Interrupt
fn send_ipi(apic_id: u32, ipi_type: IpiType) {
    let apic_base = APIC_BASE.load(Ordering::SeqCst) as u64;

    // ICR (Interrupt Command Register) is at offset 0x300 (low) and 0x310 (high)
    let icr_low_addr = apic_base + 0x300;
    let icr_high_addr = apic_base + 0x310;

    // Write destination APIC ID to ICR high
    unsafe {
        let icr_high = (apic_id as u64) << 24;
        core::ptr::write_volatile(icr_high_addr as *mut u32, icr_high as u32);
    }

    // Build ICR low value based on IPI type
    let icr_low = match ipi_type {
        IpiType::Init => {
            0x0000_4500 // INIT, edge-triggered, assert, physical destination
        }
        IpiType::Startup(vector) => {
            0x0000_4600 | (vector as u32) // SIPI, edge-triggered, assert, physical destination
        }
    };

    // Write ICR low to send IPI
    unsafe {
        core::ptr::write_volatile(icr_low_addr as *mut u32, icr_low);
    }

    // Wait for delivery
    wait_ipi_delivery();
}

/// Send INIT de-assert
fn send_init_deassert(apic_id: u32) {
    let apic_base = APIC_BASE.load(Ordering::SeqCst) as u64;
    let icr_low_addr = apic_base + 0x300;
    let icr_high_addr = apic_base + 0x310;

    unsafe {
        let icr_high = (apic_id as u64) << 24;
        core::ptr::write_volatile(icr_high_addr as *mut u32, icr_high as u32);

        // INIT de-assert: level-triggered, de-assert
        let icr_low = 0x0000_8500u32;
        core::ptr::write_volatile(icr_low_addr as *mut u32, icr_low);
    }

    wait_ipi_delivery();
}

/// Wait for IPI delivery to complete
fn wait_ipi_delivery() {
    let apic_base = APIC_BASE.load(Ordering::SeqCst) as u64;
    let icr_low_addr = apic_base + 0x300;

    // Wait until delivery status bit clears
    loop {
        let icr_low = unsafe { core::ptr::read_volatile(icr_low_addr as *const u32) };
        if (icr_low & (1 << 12)) == 0 {
            break;
        }
        core::hint::spin_loop();
    }
}

/// Setup AP trampoline code in low memory
fn setup_trampoline() -> u64 {
    // Trampoline code needs to be in low memory (< 1MB) at 4KB boundary
    // We'll use address 0x8000 (32KB)
    const TRAMPOLINE_ADDR: u64 = 0x8000;

    // AP trampoline code (16-bit real mode -> 32-bit protected -> 64-bit long mode)
    // This is simplified - real implementation would have actual assembly code
    static TRAMPOLINE_CODE: &[u8] = &[
        // Real mode entry point
        0xFA,                         // cli
        0x31, 0xC0,                   // xor ax, ax
        0x8E, 0xD8,                   // mov ds, ax
        0x8E, 0xC0,                   // mov es, ax
        0x8E, 0xD0,                   // mov ss, ax
        // Load GDT pointer
        0x0F, 0x01, 0x16, 0x50, 0x80, // lgdt [0x8050]
        // Enable protected mode
        0x0F, 0x20, 0xC0,             // mov eax, cr0
        0x0C, 0x01,                   // or al, 1
        0x0F, 0x22, 0xC0,             // mov cr0, eax
        // Far jump to 32-bit code
        0xEA, 0x20, 0x80, 0x00, 0x00, 0x08, 0x00, // jmp 0x08:0x8020
        // ... (32-bit and 64-bit transition code would follow)
        // Padding
        0x90, 0x90, 0x90, 0x90, 0x90, 0x90, 0x90, 0x90,
    ];

    // Copy trampoline code to low memory
    unsafe {
        let dest = TRAMPOLINE_ADDR as *mut u8;
        for (i, byte) in TRAMPOLINE_CODE.iter().enumerate() {
            core::ptr::write_volatile(dest.add(i), *byte);
        }
    }

    TRAMPOLINE_ADDR
}

/// Detect number of processors (from ACPI or MP tables)
fn detect_processor_count() -> usize {
    // In a real implementation, this would parse ACPI MADT or MP tables
    // For now, return a reasonable default or use CPUID
    let (_, ebx, _, _) = cpuid(1);
    let logical_cpus = ((ebx >> 16) & 0xFF) as usize;

    if logical_cpus > 0 {
        logical_cpus
    } else {
        1 // At least BSP
    }
}

/// Read APIC base address from MSR
fn read_apic_base() -> u64 {
    const IA32_APIC_BASE_MSR: u32 = 0x1B;

    let (low, high): (u32, u32);
    unsafe {
        asm!(
            "rdmsr",
            in("ecx") IA32_APIC_BASE_MSR,
            out("eax") low,
            out("edx") high,
            options(nostack, preserves_flags)
        );
    }

    let msr = ((high as u64) << 32) | (low as u64);
    msr & 0xFFFF_F000 // Mask to get base address
}

/// Read local APIC ID
fn read_apic_id() -> u32 {
    let apic_base = APIC_BASE.load(Ordering::SeqCst) as u64;
    let apic_id_addr = apic_base + 0x20; // APIC ID register

    let value = unsafe { core::ptr::read_volatile(apic_id_addr as *const u32) };
    (value >> 24) & 0xFF
}

/// CPUID instruction
fn cpuid(leaf: u32) -> (u32, u32, u32, u32) {
    let (eax, ebx, ecx, edx): (u32, u32, u32, u32);
    unsafe {
        asm!(
            "cpuid",
            inout("eax") leaf => eax,
            out("ebx") ebx,
            out("ecx") ecx,
            out("edx") edx,
            options(nostack, preserves_flags)
        );
    }
    (eax, ebx, ecx, edx)
}

/// Microsecond delay using TSC
fn delay_us(us: u64) {
    // Simple busy-wait delay
    // In real implementation, calibrate TSC frequency
    let cycles = us * 1000; // Approximate: 1GHz = 1000 cycles/us
    let start = rdtsc();
    while rdtsc() - start < cycles {
        core::hint::spin_loop();
    }
}

/// Read timestamp counter
fn rdtsc() -> u64 {
    let (low, high): (u32, u32);
    unsafe {
        asm!(
            "rdtsc",
            out("eax") low,
            out("edx") high,
            options(nostack, preserves_flags)
        );
    }
    ((high as u64) << 32) | (low as u64)
}

/// AP entry point (called by trampoline code after transitioning to 64-bit mode)
#[no_mangle]
pub extern "C" fn ap_entry() {
    // Initialize this AP
    let apic_id = read_apic_id();

    log::trace!("AP {} entered 64-bit mode", apic_id);

    // Set up GDT for this AP
    super::gdt::init();

    // Set up IDT (shared with BSP)
    super::idt::init();

    // Signal that we're ready
    AP_STARTED.store(true, Ordering::SeqCst);

    // Enable interrupts and enter scheduler
    unsafe {
        asm!("sti", options(nostack, preserves_flags));
    }

    // Enter idle loop (scheduler will pick up threads)
    loop {
        unsafe {
            asm!("hlt", options(nomem, nostack));
        }
    }
}

/// Get number of online CPUs
pub fn cpu_count() -> u32 {
    CPU_COUNT.load(Ordering::SeqCst)
}

/// Get current CPU's APIC ID
pub fn current_cpu_id() -> u32 {
    read_apic_id()
}

/// Send IPI to all CPUs (except self)
pub fn send_ipi_all_excluding_self(vector: u8) {
    let apic_base = APIC_BASE.load(Ordering::SeqCst) as u64;
    let icr_low_addr = apic_base + 0x300;

    // All excluding self, fixed delivery mode
    let icr_low = 0x000C_4000 | (vector as u32);

    unsafe {
        core::ptr::write_volatile(icr_low_addr as *mut u32, icr_low);
    }

    wait_ipi_delivery();
}

/// Send IPI to specific CPU
pub fn send_ipi_to(apic_id: u32, vector: u8) {
    let apic_base = APIC_BASE.load(Ordering::SeqCst) as u64;
    let icr_low_addr = apic_base + 0x300;
    let icr_high_addr = apic_base + 0x310;

    unsafe {
        // Set destination
        let icr_high = (apic_id as u32) << 24;
        core::ptr::write_volatile(icr_high_addr as *mut u32, icr_high);

        // Fixed delivery mode
        let icr_low = 0x0000_4000 | (vector as u32);
        core::ptr::write_volatile(icr_low_addr as *mut u32, icr_low);
    }

    wait_ipi_delivery();
}

/// Initialize APIC timer for this CPU
pub fn init_apic_timer(frequency_hz: u32) {
    let apic_base = APIC_BASE.load(Ordering::SeqCst) as u64;

    // Timer configuration registers
    let timer_lvt = apic_base + 0x320;    // LVT Timer
    let timer_initial = apic_base + 0x380; // Initial Count
    let timer_divide = apic_base + 0x3E0;  // Divide Configuration

    unsafe {
        // Set divider to 16
        core::ptr::write_volatile(timer_divide as *mut u32, 0x3);

        // Set timer vector and mode (periodic, vector 32)
        core::ptr::write_volatile(timer_lvt as *mut u32, 0x20020); // Periodic, vector 32

        // Set initial count (calibrated value would be computed from actual frequency)
        let count = 1_000_000 / frequency_hz; // Approximate
        core::ptr::write_volatile(timer_initial as *mut u32, count);
    }
}

/// Send EOI (End of Interrupt) to local APIC
pub fn send_eoi() {
    let apic_base = APIC_BASE.load(Ordering::SeqCst) as u64;
    let eoi_addr = apic_base + 0xB0;

    unsafe {
        core::ptr::write_volatile(eoi_addr as *mut u32, 0);
    }
}
