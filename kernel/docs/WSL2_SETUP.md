# Running Nyx on WSL2

This guide covers building and running the Nyx kernel on Windows Subsystem for Linux 2.

## Prerequisites

### Windows Requirements
- Windows 11 (22H2 or later recommended for KVM support)
- WSL2 enabled with a Linux distribution (Ubuntu 22.04+ recommended)

### Enable Nested Virtualization (Optional but Recommended)

For hardware-accelerated virtualization (KVM), add to `%USERPROFILE%\.wslconfig`:

```ini
[wsl2]
nestedVirtualization=true
memory=8GB
processors=4
```

Then restart WSL:
```powershell
wsl --shutdown
```

## Installation

### 1. Install Build Dependencies

```bash
# Update system
sudo apt update && sudo apt upgrade -y

# Install build tools
sudo apt install -y \
    build-essential \
    qemu-system-x86 \
    grub-pc-bin \
    xorriso \
    mtools \
    llvm \
    lld \
    gdb

# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Add nightly toolchain and components
rustup toolchain install nightly
rustup component add rust-src llvm-tools-preview --toolchain nightly
```

### 2. Build the Kernel

```bash
cd nyx/kernel

# Build
cargo +nightly build

# Or use make
make build
```

### 3. Run with QEMU

```bash
# Using the run script (recommended)
./scripts/run.sh

# Serial only mode (no graphics required)
./scripts/run.sh --serial

# Or use make
make run-serial
```

## Display Options

### Option 1: WSLg (Windows 11)
WSLg provides native GUI support. QEMU windows appear automatically.

### Option 2: X11 Forwarding
Install an X server on Windows (VcXsrv, X410, etc.):

```bash
export DISPLAY=:0
make run
```

### Option 3: Serial Only (No Display)
Best for headless development:

```bash
make run-serial
# Or
./scripts/run.sh --serial
```

## Debugging

### GDB Debugging

```bash
# Terminal 1: Start QEMU with GDB server
./scripts/run.sh --debug

# Terminal 2: Connect GDB
gdb target/x86_64-nyx/debug/nyx-kernel -ex 'target remote :1234'
```

### Common GDB Commands
```gdb
(gdb) break kernel_main
(gdb) continue
(gdb) info registers
(gdb) x/10i $rip
```

## Troubleshooting

### "KVM not available"
1. Ensure Windows 11 22H2 or later
2. Enable nested virtualization in .wslconfig
3. Restart WSL: `wsl --shutdown`

### "qemu-system-x86_64: command not found"
```bash
sudo apt install qemu-system-x86
```

### Slow Emulation
Without KVM, QEMU uses software emulation which is ~10x slower. Enable nested virtualization for better performance.

### No Serial Output
Ensure the kernel's serial driver is initialized. Check `qemu.log` for errors.

## Performance Tips

1. **Enable KVM**: 10-100x faster than software emulation
2. **Use serial mode**: Avoids X11/display overhead
3. **Allocate more memory**: Update `.wslconfig` with more RAM
4. **Use WSL2 on SSD**: Faster disk I/O

## Quick Reference

```bash
# Build
make build

# Run (auto-detects best mode)
./scripts/run.sh

# Run serial only
make run-serial

# Run with debugger
./scripts/run.sh --debug

# Clean
make clean

# Check KVM status
make wsl-kvm
```
