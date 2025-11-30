//! Global Descriptor Table (GDT)
//!
//! The GDT defines memory segments for the CPU. In 64-bit mode, segmentation
//! is largely disabled, but we still need:
//! - A null descriptor (required)
//! - Kernel code segment (ring 0)
//! - Kernel data segment (ring 0)
//! - User code segment (ring 3)
//! - User data segment (ring 3)
//! - TSS descriptor (for interrupt stack switching)

use core::arch::asm;
use core::mem::size_of;
use spin::Lazy;

/// GDT with all required segments
static GDT: Lazy<Gdt> = Lazy::new(|| {
    let mut gdt = Gdt::new();
    gdt.add_entry(Descriptor::null());                    // 0x00: Null
    gdt.add_entry(Descriptor::kernel_code());             // 0x08: Kernel code
    gdt.add_entry(Descriptor::kernel_data());             // 0x10: Kernel data
    gdt.add_entry(Descriptor::user_data());               // 0x18: User data (before code for syscall)
    gdt.add_entry(Descriptor::user_code());               // 0x20: User code
    gdt
});

/// TSS (Task State Segment) for interrupt stack switching
static mut TSS: TaskStateSegment = TaskStateSegment::new();

/// Pointer to GDT for LGDT instruction
static mut GDT_PTR: DescriptorTablePointer = DescriptorTablePointer { limit: 0, base: 0 };

/// Segment selectors
pub mod selectors {
    use super::SegmentSelector;

    pub const KERNEL_CODE: SegmentSelector = SegmentSelector::new(1, 0);
    pub const KERNEL_DATA: SegmentSelector = SegmentSelector::new(2, 0);
    pub const USER_DATA: SegmentSelector = SegmentSelector::new(3, 3);
    pub const USER_CODE: SegmentSelector = SegmentSelector::new(4, 3);
    pub const TSS: SegmentSelector = SegmentSelector::new(5, 0);
}

/// Initialize GDT
pub fn init() {
    unsafe {
        // Set up TSS with interrupt stacks
        TSS.interrupt_stack_table[0] = allocate_stack(); // Double fault stack
        TSS.interrupt_stack_table[1] = allocate_stack(); // NMI stack
        TSS.interrupt_stack_table[2] = allocate_stack(); // Machine check stack

        // Get TSS address and add to GDT
        let tss_addr = &TSS as *const _ as u64;
        let tss_len = size_of::<TaskStateSegment>() as u64;

        // Add TSS descriptor (16 bytes in 64-bit mode)
        let tss_low = Descriptor::tss_low(tss_addr, tss_len);
        let tss_high = Descriptor::tss_high(tss_addr);

        // We need to manually add TSS after GDT is initialized
        // For now, create a mutable copy
        let gdt = &*GDT;

        // Set up GDT pointer
        GDT_PTR = DescriptorTablePointer {
            limit: (gdt.len() * 8 - 1) as u16,
            base: gdt.entries.as_ptr() as u64,
        };

        // Load GDT
        asm!(
            "lgdt [{}]",
            in(reg) &GDT_PTR,
            options(nostack, preserves_flags)
        );

        // Reload code segment (far jump)
        asm!(
            "push {sel}",
            "lea {tmp}, [rip + 2f]",
            "push {tmp}",
            "retfq",
            "2:",
            sel = in(reg) selectors::KERNEL_CODE.0 as u64,
            tmp = lateout(reg) _,
            options(preserves_flags)
        );

        // Reload data segments
        asm!(
            "mov ds, {0:x}",
            "mov es, {0:x}",
            "mov fs, {0:x}",
            "mov gs, {0:x}",
            "mov ss, {0:x}",
            in(reg) selectors::KERNEL_DATA.0,
            options(nostack, preserves_flags)
        );
    }

    log::trace!("GDT initialized with {} entries", GDT.len());
}

/// Load TSS after GDT is set up
pub fn load_tss() {
    unsafe {
        asm!(
            "ltr {0:x}",
            in(reg) selectors::TSS.0,
            options(nostack, preserves_flags)
        );
    }
    log::trace!("TSS loaded");
}

/// Set interrupt stack for a given IST index (1-7)
pub fn set_interrupt_stack(ist_index: usize, stack_top: u64) {
    if ist_index > 0 && ist_index <= 7 {
        unsafe {
            TSS.interrupt_stack_table[ist_index - 1] = stack_top;
        }
    }
}

/// Allocate a kernel stack (16KB)
fn allocate_stack() -> u64 {
    // In a real implementation, this would allocate from the frame allocator
    // For now, use static stacks
    static mut STACK_POOL: [[u8; 16384]; 8] = [[0; 16384]; 8];
    static mut STACK_INDEX: usize = 0;

    unsafe {
        let index = STACK_INDEX;
        STACK_INDEX += 1;
        if index >= 8 {
            panic!("Out of interrupt stacks");
        }
        // Return top of stack (stacks grow down)
        STACK_POOL[index].as_ptr().add(16384) as u64
    }
}

/// GDT structure
struct Gdt {
    entries: [u64; 8],
    len: usize,
}

impl Gdt {
    const fn new() -> Self {
        Self {
            entries: [0; 8],
            len: 0,
        }
    }

    fn add_entry(&mut self, entry: Descriptor) {
        self.entries[self.len] = entry.0;
        self.len += 1;
    }

    fn len(&self) -> usize {
        self.len
    }
}

/// Segment descriptor (64-bit)
#[derive(Clone, Copy)]
struct Descriptor(u64);

impl Descriptor {
    /// Null descriptor
    const fn null() -> Self {
        Self(0)
    }

    /// Kernel code segment (64-bit, ring 0)
    const fn kernel_code() -> Self {
        Self(
            (1 << 43)       // Executable
            | (1 << 44)     // Code/data segment
            | (1 << 47)     // Present
            | (1 << 53)     // 64-bit mode
        )
    }

    /// Kernel data segment (ring 0)
    const fn kernel_data() -> Self {
        Self(
            (1 << 41)       // Writable
            | (1 << 44)     // Code/data segment
            | (1 << 47)     // Present
        )
    }

    /// User code segment (64-bit, ring 3)
    const fn user_code() -> Self {
        Self(
            (1 << 43)       // Executable
            | (1 << 44)     // Code/data segment
            | (1 << 47)     // Present
            | (1 << 53)     // 64-bit mode
            | (3 << 45)     // DPL = 3 (ring 3)
        )
    }

    /// User data segment (ring 3)
    const fn user_data() -> Self {
        Self(
            (1 << 41)       // Writable
            | (1 << 44)     // Code/data segment
            | (1 << 47)     // Present
            | (3 << 45)     // DPL = 3 (ring 3)
        )
    }

    /// TSS descriptor (low 64 bits)
    fn tss_low(base: u64, limit: u64) -> Self {
        let mut desc = 0u64;
        // Limit (bits 0-15)
        desc |= limit & 0xFFFF;
        // Base (bits 16-39)
        desc |= (base & 0xFFFFFF) << 16;
        // Type: 64-bit TSS (available) = 0x9
        desc |= 0x9 << 40;
        // Present
        desc |= 1 << 47;
        // Limit (bits 16-19)
        desc |= ((limit >> 16) & 0xF) << 48;
        // Base (bits 24-31)
        desc |= ((base >> 24) & 0xFF) << 56;

        Self(desc)
    }

    /// TSS descriptor (high 64 bits - base bits 32-63)
    fn tss_high(base: u64) -> Self {
        Self(base >> 32)
    }
}

/// Segment selector
#[derive(Clone, Copy)]
pub struct SegmentSelector(pub u16);

impl SegmentSelector {
    /// Create a new segment selector
    pub const fn new(index: u16, rpl: u16) -> Self {
        Self((index << 3) | (rpl & 3))
    }
}

/// Descriptor table pointer for LGDT/LIDT
#[repr(C, packed)]
struct DescriptorTablePointer {
    limit: u16,
    base: u64,
}

/// Task State Segment (64-bit)
#[repr(C, packed)]
pub struct TaskStateSegment {
    reserved_1: u32,
    /// Privilege stack table (RSP for ring 0-2)
    pub privilege_stack_table: [u64; 3],
    reserved_2: u64,
    /// Interrupt stack table (IST1-IST7)
    pub interrupt_stack_table: [u64; 7],
    reserved_3: u64,
    reserved_4: u16,
    /// I/O map base address
    pub iomap_base: u16,
}

impl TaskStateSegment {
    const fn new() -> Self {
        Self {
            reserved_1: 0,
            privilege_stack_table: [0; 3],
            reserved_2: 0,
            interrupt_stack_table: [0; 7],
            reserved_3: 0,
            reserved_4: 0,
            iomap_base: size_of::<TaskStateSegment>() as u16,
        }
    }
}

/// Get kernel code selector
pub fn kernel_code_selector() -> u16 {
    selectors::KERNEL_CODE.0
}

/// Get kernel data selector
pub fn kernel_data_selector() -> u16 {
    selectors::KERNEL_DATA.0
}

/// Get user code selector
pub fn user_code_selector() -> u16 {
    selectors::USER_CODE.0
}

/// Get user data selector
pub fn user_data_selector() -> u16 {
    selectors::USER_DATA.0
}
