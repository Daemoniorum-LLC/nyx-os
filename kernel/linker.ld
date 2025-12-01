/* Nyx Kernel Linker Script */
/* Target: x86_64 */

OUTPUT_FORMAT(elf64-x86-64)
OUTPUT_ARCH(i386:x86-64)
ENTRY(_start)

KERNEL_VMA = 0x100000;   /* 1MB virtual address */
KERNEL_LMA = 0x100000;   /* 1MB physical/load address */

SECTIONS
{
    . = KERNEL_VMA;

    _kernel_start = .;

    .multiboot ALIGN(8) : AT(ADDR(.multiboot) - KERNEL_VMA + KERNEL_LMA)
    {
        KEEP(*(.multiboot.header))
        KEEP(*(.multiboot))
    }

    .text ALIGN(4K) : AT(ADDR(.text) - KERNEL_VMA + KERNEL_LMA)
    {
        _text_start = .;
        *(.text.boot)
        *(.text .text.*)
        _text_end = .;
    }

    .rodata ALIGN(4K) : AT(ADDR(.rodata) - KERNEL_VMA + KERNEL_LMA)
    {
        _rodata_start = .;
        *(.rodata .rodata.*)
        _rodata_end = .;
    }

    .data ALIGN(4K) : AT(ADDR(.data) - KERNEL_VMA + KERNEL_LMA)
    {
        _data_start = .;
        *(.data .data.*)
        _data_end = .;
    }

    .bss ALIGN(4K) : AT(ADDR(.bss) - KERNEL_VMA + KERNEL_LMA)
    {
        _bss_start = .;
        *(COMMON)
        *(.bss .bss.*)
        _bss_end = .;
    }

    . = ALIGN(16);
    _boot_stack_bottom = .;
    . += 64K;
    _boot_stack_top = .;

    _kernel_end = .;

    /DISCARD/ :
    {
        *(.comment*)
        *(.note*)
        *(.eh_frame*)
        *(.debug*)
    }
}
