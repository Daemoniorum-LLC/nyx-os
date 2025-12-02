# Nyx OS

> A capability-based microkernel with AI-native syscalls

Part of the [Persona Framework](https://github.com/Daemoniorum-LLC/persona-framework) ecosystem.

## What is Nyx?

Nyx is an experimental operating system designed from the ground up for AI workloads. Unlike traditional kernels that bolt on ML support, Nyx treats tensor operations and inference as first-class citizens alongside files, processes, and network sockets.

**Core Design Principles:**

- **Zero Ambient Authority** — Pure capability-based security. No process has implicit permissions.
- **Memory Safety** — Rust everywhere except hardware interfaces.
- **Async-First** — io_uring-style completion queues for all IPC.
- **AI-Native** — First-class tensor operations and inference syscalls.
- **Time-Travel Debugging** — Checkpoint and restore process state for deterministic debugging.

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              Userspace Agents                                │
├──────────┬──────────┬──────────┬──────────┬──────────┬──────────┬──────────┤
│ Guardian │ Arachne  │  Archon  │ Grimoire │  Vesper  │ Phantom  │  Umbra   │
│ Security │ Network  │ Process  │ Personas │  Audio   │ Devices  │  Shell   │
├──────────┴──────────┴──────────┴──────────┴──────────┴──────────┴──────────┤
│                           nyx-init / nyx-serviced                           │
│                         Service supervision & IPC                            │
├─────────────────────────────────────────────────────────────────────────────┤
│                              libnyx / libnyx-ipc                             │
│                           Userspace system interface                         │
├─────────────────────────────────────────────────────────────────────────────┤
│                              Nyx Microkernel                                 │
│  ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐   │
│  │   Cap   │ │   IPC   │ │   Mem   │ │  Sched  │ │ Tensor  │ │  Time   │   │
│  │ System  │ │  Rings  │ │  Mgmt   │ │         │ │ Runtime │ │ Travel  │   │
│  └─────────┘ └─────────┘ └─────────┘ └─────────┘ └─────────┘ └─────────┘   │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Component Glossary

Nyx uses a daemon/occult naming theme. Here's what each component does:

| Component | Role | Description |
|-----------|------|-------------|
| **Kernel** | Microkernel | Capability-based core with AI-native syscalls |
| **nyx-init** | Init system | Service supervision and dependency management |
| **Guardian** | Security agent | Policy enforcement, sandboxing, MAC |
| **Arachne** | Network agent | Firewall, DNS, VPN, connection monitoring |
| **Archon** | Process agent | Orchestration, resource allocation |
| **Grimoire** | Persona manager | AI persona loading and configuration |
| **Vesper** | Audio daemon | PipeWire-compatible audio server |
| **Phantom** | Device manager | udev-like hotplug and device nodes |
| **Chronos** | Time daemon | NTP sync, timers, scheduling |
| **Scribe** | Logging daemon | Structured system logging |
| **Sentinel** | Monitor daemon | System metrics and health checks |
| **Umbra** | Shell | Conversational shell with AI integration |
| **Cipher** | Secrets daemon | Credential storage and encryption |
| **Vault** | Key storage | Secure key management |
| **Wraith** | Network config | DHCP, interface management |
| **Nexus** | Package manager | Dependency resolution, installation |
| **Spectre** | Auth daemon | PAM-compatible authentication |
| **Slumber** | Power daemon | Suspend, hibernate, power management |
| **Herald** | Notification | Desktop notifications |
| **Iris** | Display daemon | Display and graphics management |
| **Summoner** | Session manager | Login sessions and seat management |

## Project Structure

```
nyx-os/
├── kernel/              # Bare-metal microkernel (nightly Rust)
│   ├── src/
│   │   ├── arch/        # x86_64 architecture support
│   │   ├── cap/         # Capability system
│   │   ├── ipc/         # Inter-process communication
│   │   ├── mem/         # Memory management
│   │   ├── sched/       # Scheduler
│   │   ├── tensor/      # AI/tensor runtime
│   │   └── timetravel/  # Checkpoint/restore
│   ├── Makefile
│   └── scripts/run.sh   # QEMU runner
├── libs/                # Shared libraries
│   ├── libnyx/          # Core userspace interface
│   ├── libnyx-ipc/      # IPC client library
│   ├── libnyx-platform/ # Platform abstractions
│   ├── grimoire-core/   # Persona system core
│   └── grimoire-client/ # Grimoire daemon client
├── agents/              # AI-powered system agents
│   ├── guardian/        # Security
│   ├── arachne/         # Network
│   ├── archon/          # Process orchestration
│   └── grimoire/        # Persona management
├── init/                # Init system
├── nyx-serviced/        # Service manager
└── [other daemons]/     # vesper, phantom, chronos, etc.
```

## Getting Started

### Prerequisites

- **Rust 1.85+** (install via [rustup](https://rustup.rs/))
- **Rust nightly** (for kernel only): `rustup toolchain install nightly`
- **QEMU** (for running the kernel): `apt install qemu-system-x86`

### Building Userspace Components

The userspace daemons and libraries build with stable Rust:

```bash
# Build all userspace components
cargo build --release

# Run tests
cargo test
```

### Building the Kernel

The kernel requires nightly Rust and is built separately:

```bash
cd kernel

# Install dependencies (Ubuntu/Debian/WSL)
make deps

# Build the kernel
make build

# Run in QEMU
make run

# Run with serial output only (best for WSL2)
make run-serial

# Run with GDB debugging
make run-debug
```

### WSL2 Users

See [kernel/docs/WSL2_SETUP.md](kernel/docs/WSL2_SETUP.md) for detailed WSL2 setup instructions, including KVM acceleration.

Quick check for KVM support:
```bash
cd kernel && make wsl-kvm
```

### Example Boot Output

```
  ███╗   ██╗██╗   ██╗██╗  ██╗
  ████╗  ██║╚██╗ ██╔╝╚██╗██╔╝
  ██╔██╗ ██║ ╚████╔╝  ╚███╔╝
  ██║╚██╗██║  ╚██╔╝   ██╔██╗
  ██║ ╚████║   ██║   ██╔╝ ██╗
  ╚═╝  ╚═══╝   ╚═╝   ╚═╝  ╚═╝

  DaemonOS Microkernel

[*] Detected: Linux
[*] KVM: Available and accessible
[*] Building kernel...
[*] Kernel built: 1.2M
[*] Starting QEMU...

=== Nyx Kernel Output ===

[INFO ] Nyx Kernel v0.1.0 starting...
[DEBUG] Initializing memory subsystem
[DEBUG] Initializing capability system
[DEBUG] Initializing IPC subsystem
[DEBUG] Initializing filesystem subsystem
[DEBUG] Initializing process subsystem
[DEBUG] Initializing scheduler
[DEBUG] Initializing time-travel subsystem
[DEBUG] Initializing device driver framework
[DEBUG] Initializing network stack
[DEBUG] Initializing signal subsystem
[INFO ] Loading init process
[INFO ] Found init at /init
[INFO ] Init process spawned with PID 1
[INFO ] Starting scheduler
```

## Configuration

Services load configuration from `/grimoire/system/` by default. Override with environment variables or CLI flags:

```bash
# Example: run arachne with custom config
arachne --config ./my-config.yaml --socket /tmp/arachne.sock
```

Configuration files use YAML format. See `examples/services/` for examples.

## Development Status

Nyx is under active development. Current state:

- [x] Kernel boots and initializes subsystems
- [x] Capability system implemented
- [x] IPC ring buffers working
- [x] Basic process/thread management
- [ ] Tensor runtime (in progress)
- [ ] Time-travel debugging (in progress)
- [ ] Full userspace daemon suite (in progress)

See individual component READMEs for detailed status.

## License

Proprietary - Daemoniorum LLC

## Links

- [Persona Framework](https://github.com/Daemoniorum-LLC/persona-framework) — Parent project
- [WSL2 Setup Guide](kernel/docs/WSL2_SETUP.md) — Running Nyx on Windows
