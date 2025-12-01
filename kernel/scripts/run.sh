#!/bin/bash
# Nyx Kernel Runner - WSL2/Linux/macOS compatible
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
KERNEL_DIR="$(dirname "$SCRIPT_DIR")"
KERNEL_ELF="$KERNEL_DIR/target/x86_64-nyx/debug/nyx-kernel"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

print_banner() {
    echo -e "${BLUE}"
    echo "  ███╗   ██╗██╗   ██╗██╗  ██╗"
    echo "  ████╗  ██║╚██╗ ██╔╝╚██╗██╔╝"
    echo "  ██╔██╗ ██║ ╚████╔╝  ╚███╔╝ "
    echo "  ██║╚██╗██║  ╚██╔╝   ██╔██╗ "
    echo "  ██║ ╚████║   ██║   ██╔╝ ██╗"
    echo "  ╚═╝  ╚═══╝   ╚═╝   ╚═╝  ╚═╝"
    echo -e "${NC}"
    echo "  DaemonOS Microkernel"
    echo ""
}

detect_environment() {
    if grep -qi microsoft /proc/version 2>/dev/null; then
        ENV_TYPE="wsl2"
        echo -e "${GREEN}[*]${NC} Detected: WSL2"
    elif [[ "$(uname)" == "Darwin" ]]; then
        ENV_TYPE="macos"
        echo -e "${GREEN}[*]${NC} Detected: macOS"
    else
        ENV_TYPE="linux"
        echo -e "${GREEN}[*]${NC} Detected: Linux"
    fi
}

check_kvm() {
    if [[ -e /dev/kvm ]]; then
        if [[ -r /dev/kvm ]] && [[ -w /dev/kvm ]]; then
            KVM_AVAILABLE=1
            echo -e "${GREEN}[*]${NC} KVM: Available and accessible"
        else
            KVM_AVAILABLE=0
            echo -e "${YELLOW}[!]${NC} KVM: Available but not accessible (try: sudo chmod 666 /dev/kvm)"
        fi
    else
        KVM_AVAILABLE=0
        if [[ "$ENV_TYPE" == "wsl2" ]]; then
            echo -e "${YELLOW}[!]${NC} KVM: Not available"
            echo -e "    To enable on WSL2 (Windows 11 22H2+):"
            echo -e "    1. Edit %USERPROFILE%\\.wslconfig"
            echo -e "    2. Add: [wsl2]"
            echo -e "           nestedVirtualization=true"
            echo -e "    3. Run: wsl --shutdown"
        else
            echo -e "${YELLOW}[!]${NC} KVM: Not available (software emulation will be used)"
        fi
    fi
}

setup_qemu_args() {
    QEMU_ARGS=()

    # Memory
    QEMU_ARGS+=("-m" "512M")

    # CPU/Acceleration
    if [[ $KVM_AVAILABLE -eq 1 ]]; then
        QEMU_ARGS+=("-enable-kvm" "-cpu" "host")
    elif [[ "$ENV_TYPE" == "macos" ]]; then
        QEMU_ARGS+=("-accel" "hvf" "-cpu" "host")
    else
        QEMU_ARGS+=("-cpu" "qemu64,+sse,+sse2,+sse3,+ssse3")
    fi

    # Serial output
    QEMU_ARGS+=("-serial" "stdio")

    # Debug options
    QEMU_ARGS+=("-no-reboot" "-no-shutdown")

    # Display handling for WSL2
    if [[ "$ENV_TYPE" == "wsl2" ]]; then
        if [[ -n "$DISPLAY" ]] || command -v wslview &>/dev/null; then
            # WSLg or X11 available
            QEMU_ARGS+=("-display" "gtk")
        else
            # Fallback to no graphics
            QEMU_ARGS+=("-nographic")
        fi
    elif [[ "$MODE" == "serial" ]]; then
        QEMU_ARGS+=("-nographic")
    fi

    # Kernel
    QEMU_ARGS+=("-kernel" "$KERNEL_ELF")
}

build_kernel() {
    echo -e "${BLUE}[*]${NC} Building kernel..."
    cd "$KERNEL_DIR"
    cargo +nightly build 2>&1 | tail -5

    if [[ ! -f "$KERNEL_ELF" ]]; then
        echo -e "${RED}[!]${NC} Build failed: kernel not found"
        exit 1
    fi

    echo -e "${GREEN}[*]${NC} Kernel built: $(ls -lh "$KERNEL_ELF" | awk '{print $5}')"
}

run_qemu() {
    echo -e "${BLUE}[*]${NC} Starting QEMU..."
    echo -e "${BLUE}[*]${NC} Args: ${QEMU_ARGS[*]}"
    echo ""
    echo -e "${GREEN}=== Nyx Kernel Output ===${NC}"
    echo ""

    qemu-system-x86_64 "${QEMU_ARGS[@]}"
}

# Parse arguments
MODE="normal"
SKIP_BUILD=0

while [[ $# -gt 0 ]]; do
    case $1 in
        --serial|-s)
            MODE="serial"
            shift
            ;;
        --debug|-d)
            MODE="debug"
            shift
            ;;
        --no-build|-n)
            SKIP_BUILD=1
            shift
            ;;
        --help|-h)
            echo "Usage: $0 [options]"
            echo ""
            echo "Options:"
            echo "  --serial, -s    Run with serial output only (no graphics)"
            echo "  --debug, -d     Run with GDB server (port 1234)"
            echo "  --no-build, -n  Skip building, use existing kernel"
            echo "  --help, -h      Show this help"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

# Main
print_banner
detect_environment
check_kvm
echo ""

if [[ $SKIP_BUILD -eq 0 ]]; then
    build_kernel
fi

setup_qemu_args

if [[ "$MODE" == "debug" ]]; then
    QEMU_ARGS+=("-s" "-S")
    echo -e "${YELLOW}[*]${NC} GDB server waiting on port 1234"
    echo -e "${YELLOW}[*]${NC} Connect with: gdb -ex 'target remote :1234'"
fi

run_qemu
