//! Interrupt Descriptor Table (IDT)
//!
//! The IDT maps interrupt vectors to handler functions. We handle:
//! - CPU exceptions (0-31): Division by zero, page fault, etc.
//! - Hardware interrupts (32-47): Timer, keyboard, etc.
//! - Software interrupts (48-255): Syscalls, etc.

use core::arch::asm;
use spin::Lazy;

use super::gdt;

/// Number of IDT entries
const IDT_ENTRIES: usize = 256;

/// IDT entries
static mut IDT: [IdtEntry; IDT_ENTRIES] = [IdtEntry::missing(); IDT_ENTRIES];

/// IDT pointer for LIDT instruction
static mut IDT_PTR: IdtPointer = IdtPointer { limit: 0, base: 0 };

/// Initialize IDT
pub fn init() {
    unsafe {
        // CPU Exceptions (0-31)
        set_handler(0, divide_error as usize, 0, 0);           // #DE
        set_handler(1, debug as usize, 0, 0);                   // #DB
        set_handler(2, nmi as usize, 0, 2);                     // NMI (IST 2)
        set_handler(3, breakpoint as usize, 3, 0);              // #BP (ring 3 accessible)
        set_handler(4, overflow as usize, 0, 0);                // #OF
        set_handler(5, bound_range as usize, 0, 0);             // #BR
        set_handler(6, invalid_opcode as usize, 0, 0);          // #UD
        set_handler(7, device_not_available as usize, 0, 0);    // #NM
        set_handler_with_error(8, double_fault as usize, 0, 1); // #DF (IST 1)
        // 9: Coprocessor segment overrun (reserved)
        set_handler_with_error(10, invalid_tss as usize, 0, 0);      // #TS
        set_handler_with_error(11, segment_not_present as usize, 0, 0); // #NP
        set_handler_with_error(12, stack_segment as usize, 0, 0);    // #SS
        set_handler_with_error(13, general_protection as usize, 0, 0); // #GP
        set_handler_with_error(14, page_fault as usize, 0, 0);       // #PF
        // 15: Reserved
        set_handler(16, x87_floating_point as usize, 0, 0);          // #MF
        set_handler_with_error(17, alignment_check as usize, 0, 0);  // #AC
        set_handler(18, machine_check as usize, 0, 3);               // #MC (IST 3)
        set_handler(19, simd_floating_point as usize, 0, 0);         // #XM
        set_handler(20, virtualization as usize, 0, 0);              // #VE
        set_handler_with_error(21, control_protection as usize, 0, 0); // #CP
        // 22-27: Reserved
        set_handler(28, hypervisor_injection as usize, 0, 0);        // #HV
        set_handler_with_error(29, vmm_communication as usize, 0, 0); // #VC
        set_handler_with_error(30, security_exception as usize, 0, 0); // #SX
        // 31: Reserved

        // Hardware interrupts (IRQs 0-15 mapped to 32-47)
        set_handler(32, irq0_timer as usize, 0, 0);
        set_handler(33, irq1_keyboard as usize, 0, 0);
        set_handler(34, irq2_cascade as usize, 0, 0);
        set_handler(35, irq3_com2 as usize, 0, 0);
        set_handler(36, irq4_com1 as usize, 0, 0);
        set_handler(37, irq5_lpt2 as usize, 0, 0);
        set_handler(38, irq6_floppy as usize, 0, 0);
        set_handler(39, irq7_lpt1 as usize, 0, 0);
        set_handler(40, irq8_rtc as usize, 0, 0);
        set_handler(41, irq9_acpi as usize, 0, 0);
        set_handler(42, irq10 as usize, 0, 0);
        set_handler(43, irq11 as usize, 0, 0);
        set_handler(44, irq12_mouse as usize, 0, 0);
        set_handler(45, irq13_fpu as usize, 0, 0);
        set_handler(46, irq14_ata1 as usize, 0, 0);
        set_handler(47, irq15_ata2 as usize, 0, 0);

        // Syscall interrupt (0x80 for compatibility, but we prefer syscall instruction)
        set_handler(0x80, syscall_interrupt as usize, 3, 0);

        // APIC spurious interrupt
        set_handler(0xFF, spurious as usize, 0, 0);

        // Set up IDT pointer
        IDT_PTR = IdtPointer {
            limit: (core::mem::size_of::<[IdtEntry; IDT_ENTRIES]>() - 1) as u16,
            base: IDT.as_ptr() as u64,
        };

        // Load IDT
        asm!(
            "lidt [{}]",
            in(reg) &IDT_PTR,
            options(nostack, preserves_flags)
        );
    }

    log::trace!("IDT initialized with {} entries", IDT_ENTRIES);
}

/// Set an interrupt handler (no error code)
unsafe fn set_handler(vector: usize, handler: usize, dpl: u8, ist: u8) {
    IDT[vector] = IdtEntry::new(handler as u64, gdt::selectors::KERNEL_CODE.0, ist, dpl, false);
}

/// Set an interrupt handler with error code
unsafe fn set_handler_with_error(vector: usize, handler: usize, dpl: u8, ist: u8) {
    IDT[vector] = IdtEntry::new(handler as u64, gdt::selectors::KERNEL_CODE.0, ist, dpl, true);
}

/// IDT entry (16 bytes in 64-bit mode)
#[derive(Clone, Copy)]
#[repr(C)]
struct IdtEntry {
    offset_low: u16,
    selector: u16,
    ist: u8,
    type_attr: u8,
    offset_mid: u16,
    offset_high: u32,
    reserved: u32,
}

impl IdtEntry {
    /// Create a missing/null entry
    const fn missing() -> Self {
        Self {
            offset_low: 0,
            selector: 0,
            ist: 0,
            type_attr: 0,
            offset_mid: 0,
            offset_high: 0,
            reserved: 0,
        }
    }

    /// Create a new IDT entry
    fn new(offset: u64, selector: u16, ist: u8, dpl: u8, _has_error_code: bool) -> Self {
        Self {
            offset_low: offset as u16,
            selector,
            ist: ist & 0x7,
            type_attr: 0x8E | ((dpl & 3) << 5), // Present, 64-bit interrupt gate
            offset_mid: (offset >> 16) as u16,
            offset_high: (offset >> 32) as u32,
            reserved: 0,
        }
    }
}

/// IDT pointer for LIDT instruction
#[repr(C, packed)]
struct IdtPointer {
    limit: u16,
    base: u64,
}

/// Interrupt stack frame pushed by CPU
#[repr(C)]
#[derive(Debug)]
pub struct InterruptStackFrame {
    pub instruction_pointer: u64,
    pub code_segment: u64,
    pub cpu_flags: u64,
    pub stack_pointer: u64,
    pub stack_segment: u64,
}

/// Extended interrupt frame with error code
#[repr(C)]
#[derive(Debug)]
pub struct InterruptStackFrameWithError {
    pub error_code: u64,
    pub instruction_pointer: u64,
    pub code_segment: u64,
    pub cpu_flags: u64,
    pub stack_pointer: u64,
    pub stack_segment: u64,
}

// =============================================================================
// Exception Handlers
// =============================================================================

macro_rules! interrupt_handler {
    ($name:ident) => {
        #[naked]
        extern "C" fn $name() {
            unsafe {
                asm!(
                    // Save all registers
                    "push rax",
                    "push rbx",
                    "push rcx",
                    "push rdx",
                    "push rsi",
                    "push rdi",
                    "push rbp",
                    "push r8",
                    "push r9",
                    "push r10",
                    "push r11",
                    "push r12",
                    "push r13",
                    "push r14",
                    "push r15",
                    // Call Rust handler
                    "mov rdi, rsp",      // Pass stack frame pointer
                    "call {handler}",
                    // Restore registers
                    "pop r15",
                    "pop r14",
                    "pop r13",
                    "pop r12",
                    "pop r11",
                    "pop r10",
                    "pop r9",
                    "pop r8",
                    "pop rbp",
                    "pop rdi",
                    "pop rsi",
                    "pop rdx",
                    "pop rcx",
                    "pop rbx",
                    "pop rax",
                    "iretq",
                    handler = sym concat_idents!($name, _inner),
                    options(noreturn)
                )
            }
        }
    };
}

macro_rules! interrupt_handler_with_error {
    ($name:ident) => {
        #[naked]
        extern "C" fn $name() {
            unsafe {
                asm!(
                    // Error code is already on stack
                    // Save all registers
                    "push rax",
                    "push rbx",
                    "push rcx",
                    "push rdx",
                    "push rsi",
                    "push rdi",
                    "push rbp",
                    "push r8",
                    "push r9",
                    "push r10",
                    "push r11",
                    "push r12",
                    "push r13",
                    "push r14",
                    "push r15",
                    // Call Rust handler
                    "mov rdi, rsp",
                    "call {handler}",
                    // Restore registers
                    "pop r15",
                    "pop r14",
                    "pop r13",
                    "pop r12",
                    "pop r11",
                    "pop r10",
                    "pop r9",
                    "pop r8",
                    "pop rbp",
                    "pop rdi",
                    "pop rsi",
                    "pop rdx",
                    "pop rcx",
                    "pop rbx",
                    "pop rax",
                    // Pop error code
                    "add rsp, 8",
                    "iretq",
                    handler = sym concat_idents!($name, _inner),
                    options(noreturn)
                )
            }
        }
    };
}

// Use simpler approach without macros for now
#[naked]
extern "C" fn divide_error() {
    unsafe {
        asm!(
            "push 0",  // Fake error code for uniform handling
            "push 0",  // Exception number
            "jmp {common}",
            common = sym exception_common,
            options(noreturn)
        )
    }
}

#[naked]
extern "C" fn debug() {
    unsafe {
        asm!(
            "push 0",
            "push 1",
            "jmp {common}",
            common = sym exception_common,
            options(noreturn)
        )
    }
}

#[naked]
extern "C" fn nmi() {
    unsafe {
        asm!(
            "push 0",
            "push 2",
            "jmp {common}",
            common = sym exception_common,
            options(noreturn)
        )
    }
}

#[naked]
extern "C" fn breakpoint() {
    unsafe {
        asm!(
            "push 0",
            "push 3",
            "jmp {common}",
            common = sym exception_common,
            options(noreturn)
        )
    }
}

#[naked]
extern "C" fn overflow() {
    unsafe {
        asm!(
            "push 0",
            "push 4",
            "jmp {common}",
            common = sym exception_common,
            options(noreturn)
        )
    }
}

#[naked]
extern "C" fn bound_range() {
    unsafe {
        asm!(
            "push 0",
            "push 5",
            "jmp {common}",
            common = sym exception_common,
            options(noreturn)
        )
    }
}

#[naked]
extern "C" fn invalid_opcode() {
    unsafe {
        asm!(
            "push 0",
            "push 6",
            "jmp {common}",
            common = sym exception_common,
            options(noreturn)
        )
    }
}

#[naked]
extern "C" fn device_not_available() {
    unsafe {
        asm!(
            "push 0",
            "push 7",
            "jmp {common}",
            common = sym exception_common,
            options(noreturn)
        )
    }
}

#[naked]
extern "C" fn double_fault() {
    unsafe {
        asm!(
            // Error code already pushed by CPU
            "push 8",
            "jmp {common}",
            common = sym exception_common_with_error,
            options(noreturn)
        )
    }
}

#[naked]
extern "C" fn invalid_tss() {
    unsafe {
        asm!(
            "push 10",
            "jmp {common}",
            common = sym exception_common_with_error,
            options(noreturn)
        )
    }
}

#[naked]
extern "C" fn segment_not_present() {
    unsafe {
        asm!(
            "push 11",
            "jmp {common}",
            common = sym exception_common_with_error,
            options(noreturn)
        )
    }
}

#[naked]
extern "C" fn stack_segment() {
    unsafe {
        asm!(
            "push 12",
            "jmp {common}",
            common = sym exception_common_with_error,
            options(noreturn)
        )
    }
}

#[naked]
extern "C" fn general_protection() {
    unsafe {
        asm!(
            "push 13",
            "jmp {common}",
            common = sym exception_common_with_error,
            options(noreturn)
        )
    }
}

#[naked]
extern "C" fn page_fault() {
    unsafe {
        asm!(
            "push 14",
            "jmp {common}",
            common = sym exception_common_with_error,
            options(noreturn)
        )
    }
}

#[naked]
extern "C" fn x87_floating_point() {
    unsafe {
        asm!(
            "push 0",
            "push 16",
            "jmp {common}",
            common = sym exception_common,
            options(noreturn)
        )
    }
}

#[naked]
extern "C" fn alignment_check() {
    unsafe {
        asm!(
            "push 17",
            "jmp {common}",
            common = sym exception_common_with_error,
            options(noreturn)
        )
    }
}

#[naked]
extern "C" fn machine_check() {
    unsafe {
        asm!(
            "push 0",
            "push 18",
            "jmp {common}",
            common = sym exception_common,
            options(noreturn)
        )
    }
}

#[naked]
extern "C" fn simd_floating_point() {
    unsafe {
        asm!(
            "push 0",
            "push 19",
            "jmp {common}",
            common = sym exception_common,
            options(noreturn)
        )
    }
}

#[naked]
extern "C" fn virtualization() {
    unsafe {
        asm!(
            "push 0",
            "push 20",
            "jmp {common}",
            common = sym exception_common,
            options(noreturn)
        )
    }
}

#[naked]
extern "C" fn control_protection() {
    unsafe {
        asm!(
            "push 21",
            "jmp {common}",
            common = sym exception_common_with_error,
            options(noreturn)
        )
    }
}

#[naked]
extern "C" fn hypervisor_injection() {
    unsafe {
        asm!(
            "push 0",
            "push 28",
            "jmp {common}",
            common = sym exception_common,
            options(noreturn)
        )
    }
}

#[naked]
extern "C" fn vmm_communication() {
    unsafe {
        asm!(
            "push 29",
            "jmp {common}",
            common = sym exception_common_with_error,
            options(noreturn)
        )
    }
}

#[naked]
extern "C" fn security_exception() {
    unsafe {
        asm!(
            "push 30",
            "jmp {common}",
            common = sym exception_common_with_error,
            options(noreturn)
        )
    }
}

/// Common exception handler (no error code)
#[naked]
extern "C" fn exception_common() {
    unsafe {
        asm!(
            // Save all general-purpose registers
            "push rax",
            "push rbx",
            "push rcx",
            "push rdx",
            "push rsi",
            "push rdi",
            "push rbp",
            "push r8",
            "push r9",
            "push r10",
            "push r11",
            "push r12",
            "push r13",
            "push r14",
            "push r15",
            // First argument: pointer to saved state
            "mov rdi, rsp",
            // Call Rust handler
            "call {handler}",
            // Restore registers
            "pop r15",
            "pop r14",
            "pop r13",
            "pop r12",
            "pop r11",
            "pop r10",
            "pop r9",
            "pop r8",
            "pop rbp",
            "pop rdi",
            "pop rsi",
            "pop rdx",
            "pop rcx",
            "pop rbx",
            "pop rax",
            // Pop exception number and fake error code
            "add rsp, 16",
            "iretq",
            handler = sym exception_handler_rust,
            options(noreturn)
        )
    }
}

/// Common exception handler (with error code)
#[naked]
extern "C" fn exception_common_with_error() {
    unsafe {
        asm!(
            // Save all general-purpose registers
            "push rax",
            "push rbx",
            "push rcx",
            "push rdx",
            "push rsi",
            "push rdi",
            "push rbp",
            "push r8",
            "push r9",
            "push r10",
            "push r11",
            "push r12",
            "push r13",
            "push r14",
            "push r15",
            // First argument: pointer to saved state
            "mov rdi, rsp",
            // Call Rust handler
            "call {handler}",
            // Restore registers
            "pop r15",
            "pop r14",
            "pop r13",
            "pop r12",
            "pop r11",
            "pop r10",
            "pop r9",
            "pop r8",
            "pop rbp",
            "pop rdi",
            "pop rsi",
            "pop rdx",
            "pop rcx",
            "pop rbx",
            "pop rax",
            // Pop exception number and error code
            "add rsp, 16",
            "iretq",
            handler = sym exception_handler_rust,
            options(noreturn)
        )
    }
}

/// Saved CPU state during exception
#[repr(C)]
#[derive(Debug)]
pub struct ExceptionFrame {
    // Pushed by common handler
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rbp: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub rbx: u64,
    pub rax: u64,
    // Pushed by handler stub
    pub exception_number: u64,
    pub error_code: u64,
    // Pushed by CPU
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

/// Rust exception handler
extern "C" fn exception_handler_rust(frame: &ExceptionFrame) {
    let exception_name = match frame.exception_number {
        0 => "Divide Error (#DE)",
        1 => "Debug (#DB)",
        2 => "Non-Maskable Interrupt",
        3 => "Breakpoint (#BP)",
        4 => "Overflow (#OF)",
        5 => "Bound Range Exceeded (#BR)",
        6 => "Invalid Opcode (#UD)",
        7 => "Device Not Available (#NM)",
        8 => "Double Fault (#DF)",
        10 => "Invalid TSS (#TS)",
        11 => "Segment Not Present (#NP)",
        12 => "Stack-Segment Fault (#SS)",
        13 => "General Protection (#GP)",
        14 => "Page Fault (#PF)",
        16 => "x87 FPU Error (#MF)",
        17 => "Alignment Check (#AC)",
        18 => "Machine Check (#MC)",
        19 => "SIMD Floating-Point (#XM)",
        20 => "Virtualization (#VE)",
        21 => "Control Protection (#CP)",
        28 => "Hypervisor Injection (#HV)",
        29 => "VMM Communication (#VC)",
        30 => "Security Exception (#SX)",
        _ => "Unknown Exception",
    };

    log::error!(
        "EXCEPTION: {} at {:#x}\n\
         Error code: {:#x}\n\
         RAX={:#018x} RBX={:#018x} RCX={:#018x} RDX={:#018x}\n\
         RSI={:#018x} RDI={:#018x} RBP={:#018x} RSP={:#018x}\n\
         R8 ={:#018x} R9 ={:#018x} R10={:#018x} R11={:#018x}\n\
         R12={:#018x} R13={:#018x} R14={:#018x} R15={:#018x}\n\
         RIP={:#018x} RFLAGS={:#018x} CS={:#04x} SS={:#04x}",
        exception_name,
        frame.rip,
        frame.error_code,
        frame.rax, frame.rbx, frame.rcx, frame.rdx,
        frame.rsi, frame.rdi, frame.rbp, frame.rsp,
        frame.r8, frame.r9, frame.r10, frame.r11,
        frame.r12, frame.r13, frame.r14, frame.r15,
        frame.rip, frame.rflags, frame.cs, frame.ss
    );

    // Special handling for page faults
    if frame.exception_number == 14 {
        let cr2: u64;
        unsafe {
            asm!("mov {}, cr2", out(reg) cr2, options(nostack, preserves_flags));
        }
        log::error!(
            "Page fault address: {:#x}\n\
             Flags: {}{}{}{}{}",
            cr2,
            if frame.error_code & 1 != 0 { "P " } else { "" },
            if frame.error_code & 2 != 0 { "W " } else { "R " },
            if frame.error_code & 4 != 0 { "U " } else { "S " },
            if frame.error_code & 8 != 0 { "RSVD " } else { "" },
            if frame.error_code & 16 != 0 { "I " } else { "" },
        );
    }

    // For now, halt on exceptions (except breakpoint and debug)
    if frame.exception_number != 1 && frame.exception_number != 3 {
        loop {
            unsafe { asm!("hlt", options(nomem, nostack)); }
        }
    }
}

// =============================================================================
// IRQ Handlers
// =============================================================================

macro_rules! irq_handler {
    ($name:ident, $irq:expr) => {
        #[naked]
        extern "C" fn $name() {
            unsafe {
                asm!(
                    "push rax",
                    "push rbx",
                    "push rcx",
                    "push rdx",
                    "push rsi",
                    "push rdi",
                    "push rbp",
                    "push r8",
                    "push r9",
                    "push r10",
                    "push r11",
                    "push r12",
                    "push r13",
                    "push r14",
                    "push r15",
                    "mov rdi, {irq}",
                    "call {handler}",
                    "pop r15",
                    "pop r14",
                    "pop r13",
                    "pop r12",
                    "pop r11",
                    "pop r10",
                    "pop r9",
                    "pop r8",
                    "pop rbp",
                    "pop rdi",
                    "pop rsi",
                    "pop rdx",
                    "pop rcx",
                    "pop rbx",
                    "pop rax",
                    "iretq",
                    irq = const $irq,
                    handler = sym irq_handler_rust,
                    options(noreturn)
                )
            }
        }
    };
}

irq_handler!(irq0_timer, 0);
irq_handler!(irq1_keyboard, 1);
irq_handler!(irq2_cascade, 2);
irq_handler!(irq3_com2, 3);
irq_handler!(irq4_com1, 4);
irq_handler!(irq5_lpt2, 5);
irq_handler!(irq6_floppy, 6);
irq_handler!(irq7_lpt1, 7);
irq_handler!(irq8_rtc, 8);
irq_handler!(irq9_acpi, 9);
irq_handler!(irq10, 10);
irq_handler!(irq11, 11);
irq_handler!(irq12_mouse, 12);
irq_handler!(irq13_fpu, 13);
irq_handler!(irq14_ata1, 14);
irq_handler!(irq15_ata2, 15);

/// Rust IRQ handler
extern "C" fn irq_handler_rust(irq: u64) {
    match irq {
        0 => {
            // Timer tick - trigger scheduler
            crate::sched::timer_tick();
        }
        1 => {
            // Keyboard - read scancode
            log::trace!("Keyboard IRQ");
        }
        _ => {
            log::trace!("IRQ {}", irq);
        }
    }

    // Send EOI to PIC/APIC
    send_eoi(irq as u8);
}

/// Send End-Of-Interrupt
fn send_eoi(irq: u8) {
    unsafe {
        if irq >= 8 {
            // Send EOI to slave PIC
            asm!("out 0xA0, al", in("al") 0x20u8, options(nostack, preserves_flags));
        }
        // Send EOI to master PIC
        asm!("out 0x20, al", in("al") 0x20u8, options(nostack, preserves_flags));
    }
}

// =============================================================================
// Syscall and Spurious Handlers
// =============================================================================

#[naked]
extern "C" fn syscall_interrupt() {
    unsafe {
        asm!(
            "push rax",
            "push rbx",
            "push rcx",
            "push rdx",
            "push rsi",
            "push rdi",
            "push rbp",
            "push r8",
            "push r9",
            "push r10",
            "push r11",
            "push r12",
            "push r13",
            "push r14",
            "push r15",
            // rax = syscall number, rdi/rsi/rdx/r10/r8/r9 = args
            "mov rdi, rsp",
            "call {handler}",
            "pop r15",
            "pop r14",
            "pop r13",
            "pop r12",
            "pop r11",
            "pop r10",
            "pop r9",
            "pop r8",
            "pop rbp",
            "pop rdi",
            "pop rsi",
            "pop rdx",
            "pop rcx",
            "pop rbx",
            "pop rax",
            "iretq",
            handler = sym syscall_handler_rust,
            options(noreturn)
        )
    }
}

extern "C" fn syscall_handler_rust(_frame: &ExceptionFrame) {
    // Will be implemented with proper syscall dispatch
    log::trace!("Syscall via INT 0x80");
}

#[naked]
extern "C" fn spurious() {
    unsafe {
        asm!(
            "iretq",
            options(noreturn)
        )
    }
}
