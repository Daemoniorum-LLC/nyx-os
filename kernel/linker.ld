/* Nyx Kernel Linker Script */
/* Target: x86_64, Higher-half kernel at -2GB */

OUTPUT_FORMAT(elf64-x86-64)
OUTPUT_ARCH(i386:x86-64)
ENTRY(_start)

/* Kernel virtual base address (higher-half) */
KERNEL_VIRT_BASE = 0xFFFFFFFF80000000;

/* Physical load address */
KERNEL_PHYS_BASE = 0x100000;  /* 1MB - above legacy BIOS area */

PHDRS
{
    text   PT_LOAD FLAGS(5);   /* r-x */
    rodata PT_LOAD FLAGS(4);   /* r-- */
    data   PT_LOAD FLAGS(6);   /* rw- */
    bss    PT_LOAD FLAGS(6);   /* rw- */
}

SECTIONS
{
    /* Start at physical 1MB, mapped to higher-half */
    . = KERNEL_VIRT_BASE + KERNEL_PHYS_BASE;

    _kernel_start = .;

    /* Multiboot header must be in first 8KB */
    .multiboot : AT(ADDR(.multiboot) - KERNEL_VIRT_BASE)
    {
        KEEP(*(.multiboot.header))
    } :text

    /* Executable code */
    .text ALIGN(4K) : AT(ADDR(.text) - KERNEL_VIRT_BASE)
    {
        _text_start = .;
        *(.text.boot)         /* Boot code first */
        *(.text .text.*)
        _text_end = .;
    } :text

    /* Read-only data */
    .rodata ALIGN(4K) : AT(ADDR(.rodata) - KERNEL_VIRT_BASE)
    {
        _rodata_start = .;
        *(.rodata .rodata.*)
        _rodata_end = .;
    } :rodata

    /* Exception handling tables */
    .eh_frame ALIGN(4K) : AT(ADDR(.eh_frame) - KERNEL_VIRT_BASE)
    {
        *(.eh_frame .eh_frame.*)
    } :rodata

    /* Initialized data */
    .data ALIGN(4K) : AT(ADDR(.data) - KERNEL_VIRT_BASE)
    {
        _data_start = .;
        *(.data .data.*)
        _data_end = .;
    } :data

    /* Uninitialized data (zeroed at runtime) */
    .bss ALIGN(4K) : AT(ADDR(.bss) - KERNEL_VIRT_BASE)
    {
        _bss_start = .;
        *(.bss .bss.*)
        *(COMMON)
        _bss_end = .;
    } :bss

    /* Boot stack (64KB) */
    . = ALIGN(16);
    _boot_stack_bottom = .;
    . += 0x10000;
    _boot_stack_top = .;

    _kernel_end = .;

    /* Calculate physical addresses */
    _kernel_phys_start = _kernel_start - KERNEL_VIRT_BASE;
    _kernel_phys_end = _kernel_end - KERNEL_VIRT_BASE;
    _kernel_size = _kernel_end - _kernel_start;

    /* Discard unnecessary sections */
    /DISCARD/ :
    {
        *(.comment)
        *(.note.*)
        *(.debug_*)
    }
}
