# Nyx Kernel Boot Assembly
# x86_64 multiboot2 entry point

.section .multiboot2, "a"
.align 8

# Multiboot2 header
multiboot2_header:
    .long 0xe85250d6                    # Magic
    .long 0                             # Architecture (i386/x86)
    .long multiboot2_header_end - multiboot2_header  # Header length
    .long -(0xe85250d6 + 0 + (multiboot2_header_end - multiboot2_header))  # Checksum

    # Framebuffer tag (optional)
    .align 8
    .short 5                            # Type: framebuffer
    .short 0                            # Flags
    .long 20                            # Size
    .long 1024                          # Width
    .long 768                           # Height
    .long 32                            # Depth

    # End tag
    .align 8
    .short 0                            # Type: end
    .short 0                            # Flags
    .long 8                             # Size

multiboot2_header_end:

# Boot stack
.section .bss
.align 4096
boot_stack_bottom:
    .space 16384                        # 16 KB stack
boot_stack_top:

# Entry point
.section .text
.global _start
.type _start, @function

_start:
    # Disable interrupts
    cli

    # Set up stack
    lea rsp, [rip + boot_stack_top]

    # Clear direction flag
    cld

    # Zero BSS section
    lea rdi, [rip + __bss_start]
    lea rcx, [rip + __bss_end]
    sub rcx, rdi
    shr rcx, 3                          # Divide by 8 (qwords)
    xor eax, eax
    rep stosq

    # Save multiboot info pointer (passed in ebx by bootloader)
    # We'll pass it to the Rust entry point
    mov edi, ebx                        # Multiboot info pointer

    # Jump to Rust entry point
    call kernel_main

    # Should never return, but if it does, halt
.halt:
    hlt
    jmp .halt

.size _start, . - _start

# Symbols defined by linker script
.extern __bss_start
.extern __bss_end
.extern kernel_main
