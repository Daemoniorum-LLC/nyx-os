# Nyx Kernel Makefile
# Supports building and running on Linux, macOS, and WSL2

KERNEL_NAME := nyx
TARGET := x86_64-nyx
KERNEL_ELF := target/$(TARGET)/debug/$(KERNEL_NAME)
KERNEL_BIN := $(KERNEL_NAME).bin
ISO_IMAGE := $(KERNEL_NAME).iso

# Tools
CARGO := cargo +nightly
OBJCOPY := llvm-objcopy
QEMU := qemu-system-x86_64

# Detect WSL
IS_WSL := $(shell grep -qi microsoft /proc/version 2>/dev/null && echo 1 || echo 0)

# QEMU acceleration
ifeq ($(IS_WSL),1)
    # WSL2 with KVM support (Windows 11 22H2+)
    QEMU_ACCEL := $(shell [ -e /dev/kvm ] && echo "-enable-kvm -cpu host" || echo "-cpu qemu64")
    $(info Detected WSL2 environment)
else ifeq ($(shell uname),Linux)
    QEMU_ACCEL := $(shell [ -e /dev/kvm ] && echo "-enable-kvm -cpu host" || echo "-cpu qemu64")
else ifeq ($(shell uname),Darwin)
    # macOS with HVF
    QEMU_ACCEL := -accel hvf -cpu host
else
    QEMU_ACCEL := -cpu qemu64
endif

# QEMU common options
QEMU_COMMON := -m 512M \
    -serial stdio \
    -no-reboot \
    -no-shutdown \
    -d int,cpu_reset \
    -D qemu.log

# QEMU display options for WSL2
ifeq ($(IS_WSL),1)
    # Use SDL if available, otherwise no display
    QEMU_DISPLAY := -display sdl 2>/dev/null || -nographic
else
    QEMU_DISPLAY := -display sdl
endif

.PHONY: all build clean run run-debug iso check fmt clippy

all: build

# Build the kernel
build:
	@echo "Building Nyx kernel..."
	$(CARGO) build
	@echo "Creating flat binary..."
	$(OBJCOPY) -O binary $(KERNEL_ELF) $(KERNEL_BIN) 2>/dev/null || \
		rust-objcopy -O binary $(KERNEL_ELF) $(KERNEL_BIN) 2>/dev/null || \
		echo "Warning: Could not create binary (install llvm-tools-preview)"
	@echo "Build complete!"
	@ls -lh $(KERNEL_BIN) 2>/dev/null || true

# Create bootable ISO with GRUB
iso: build
	@echo "Creating bootable ISO..."
	@mkdir -p iso/boot/grub
	@cp $(KERNEL_ELF) iso/boot/nyx.elf
	@echo 'set timeout=0' > iso/boot/grub/grub.cfg
	@echo 'set default=0' >> iso/boot/grub/grub.cfg
	@echo '' >> iso/boot/grub/grub.cfg
	@echo 'menuentry "Nyx Kernel" {' >> iso/boot/grub/grub.cfg
	@echo '    multiboot2 /boot/nyx.elf' >> iso/boot/grub/grub.cfg
	@echo '    boot' >> iso/boot/grub/grub.cfg
	@echo '}' >> iso/boot/grub/grub.cfg
	grub-mkrescue -o $(ISO_IMAGE) iso 2>/dev/null || \
		echo "Install grub-pc-bin and xorriso for ISO creation"
	@rm -rf iso
	@echo "ISO created: $(ISO_IMAGE)"

# Run with QEMU (multiboot direct boot)
run: build
	@echo "Starting QEMU..."
	@echo "Acceleration: $(QEMU_ACCEL)"
	$(QEMU) $(QEMU_ACCEL) $(QEMU_COMMON) \
		-kernel $(KERNEL_ELF)

# Run with serial output only (best for WSL2 without display)
run-serial: build
	@echo "Starting QEMU (serial only)..."
	$(QEMU) $(QEMU_ACCEL) $(QEMU_COMMON) \
		-nographic \
		-kernel $(KERNEL_ELF)

# Run with debug options
run-debug: build
	@echo "Starting QEMU with GDB server..."
	$(QEMU) $(QEMU_ACCEL) $(QEMU_COMMON) \
		-s -S \
		-kernel $(KERNEL_ELF)

# Run ISO image
run-iso: iso
	$(QEMU) $(QEMU_ACCEL) $(QEMU_COMMON) \
		-cdrom $(ISO_IMAGE)

# Clean build artifacts
clean:
	$(CARGO) clean
	rm -f $(KERNEL_BIN) $(ISO_IMAGE) qemu.log
	rm -rf iso

# Check code
check:
	$(CARGO) check

# Format code
fmt:
	$(CARGO) fmt

# Run clippy
clippy:
	$(CARGO) clippy -- -D warnings

# Show build info
info:
	@echo "Kernel:      $(KERNEL_NAME)"
	@echo "Target:      $(TARGET)"
	@echo "WSL:         $(IS_WSL)"
	@echo "QEMU Accel:  $(QEMU_ACCEL)"
	@echo "QEMU:        $(QEMU)"

# Install dependencies (Ubuntu/Debian/WSL)
deps:
	@echo "Installing build dependencies..."
	sudo apt-get update
	sudo apt-get install -y \
		qemu-system-x86 \
		grub-pc-bin \
		xorriso \
		mtools \
		llvm \
		lld
	@echo "Installing Rust components..."
	rustup component add rust-src llvm-tools-preview
	@echo "Done!"

# WSL2 specific: Enable KVM (requires Windows 11 22H2+)
wsl-kvm:
	@echo "Checking WSL2 KVM support..."
	@if [ -e /dev/kvm ]; then \
		echo "KVM is available!"; \
		ls -la /dev/kvm; \
	else \
		echo "KVM not available. To enable:"; \
		echo "1. Ensure Windows 11 22H2 or later"; \
		echo "2. Enable 'Virtual Machine Platform' in Windows Features"; \
		echo "3. Add to .wslconfig:"; \
		echo "   [wsl2]"; \
		echo "   nestedVirtualization=true"; \
		echo "4. Run: wsl --shutdown && wsl"; \
	fi
