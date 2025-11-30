# DaemonOS (Nyx)

A modern, Rust-based operating system built on the Nyx microkernel architecture. Part of the Persona Framework.

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                        User Applications                         │
├─────────────────────────────────────────────────────────────────┤
│  Summoner   │   Herald    │    Umbra    │      Aether           │
│  (launcher) │ (notifier)  │ (compositor)│  (display server)     │
├─────────────────────────────────────────────────────────────────┤
│  Nexus   │ Wraith  │ Cipher │ Scribe │ Vesper │ Phantom │Spectre│
│  (pkg)   │ (net)   │ (keys) │ (logs) │ (audio)│ (udev)  │(login)│
├─────────────────────────────────────────────────────────────────┤
│  Guardian  │   Archon    │   Arachne   │      Grimoire          │
│ (security) │ (processes) │ (networking)│   (config mgmt)        │
├─────────────────────────────────────────────────────────────────┤
│              nyx-serviced (service manager)                      │
├─────────────────────────────────────────────────────────────────┤
│                    nyx-init (PID 1)                              │
├─────────────────────────────────────────────────────────────────┤
│   libnyx   │   libnyx-ipc   │   libnyx-platform (WSL compat)    │
├─────────────────────────────────────────────────────────────────┤
│                      Nyx Microkernel                             │
└─────────────────────────────────────────────────────────────────┘
```

## Components

### Core System

| Component | Binary | Description |
|-----------|--------|-------------|
| **kernel** | - | Nyx microkernel with capability-based security |
| **init** | `nyx-init` | PID 1, system bootstrap, service supervision |
| **nyx-serviced** | `nyx-serviced` | Service manager with dependency resolution, socket activation |
| **spectre** | `spectred` | Session/login manager with PAM, multi-seat support |
| **phantom** | `phantomd` | Device manager (udev replacement), netlink monitoring |
| **nexus** | `nexus`, `nexusd` | Package manager with content-addressable store |
| **wraith** | `wraithd`, `wraithctl` | Network manager with DHCP, WiFi, profiles |
| **cipher** | `cipherd`, `cipher` | Secrets daemon with ChaCha20-Poly1305 encryption |
| **scribe** | `scribed`, `scribectl` | Logging daemon with structured JSON journal |
| **vesper** | `vesperd` | Audio daemon with ALSA, per-app mixing, Bluetooth |

### Agents

| Agent | Description |
|-------|-------------|
| **guardian** | Security policy enforcement, capability management |
| **archon** | Process orchestration, resource allocation |
| **arachne** | Network services, connection management |
| **grimoire** | Configuration management, system state |

### Desktop

| Component | Binary | Description |
|-----------|--------|-------------|
| **aether** | `aether` | Wayland display server |
| **umbra** | `umbra` | Compositor with animations, effects |
| **summoner** | `summoner` | Application launcher, desktop integration |
| **herald** | `herald` | Notification daemon |

### Libraries

| Library | Description |
|---------|-------------|
| **libnyx** | Core system library, syscall wrappers |
| **libnyx-ipc** | IPC primitives, message passing |
| **libnyx-platform** | Platform abstraction, WSL compatibility layer |

## Building

```bash
cd nyx
cargo build --release
```

### Build Individual Components

```bash
cargo build -p nexus --release
cargo build -p wraith --release
cargo build -p scribe --release
```

## Configuration

### Service Units

Service units are defined in YAML format in `/etc/nyx/services/`:

```yaml
name: example
description: Example service
type: simple

exec:
  start: /usr/bin/example-daemon

dependencies:
  requires:
    - network.target
  after:
    - network.target

resources:
  memory_max: 512M
  cpu_weight: 100
```

### Network Profiles

Network profiles in `/etc/wraith/profiles/`:

```toml
name = "Home WiFi"
interface_match = "wl*"

[config]
type = "Dhcp"

[options]
metered = false
```

### Package Repositories

Repository configuration in `/etc/nexus/repos.d/`:

```toml
name = "nyx-core"
url = "https://packages.daemoniorum.com/nyx/core"
enabled = true
priority = 100
```

## Key Features

### Content-Addressable Package Store

Nexus uses a Nix-inspired content-addressable store:
- Packages stored by hash: `/nyx/store/{hash}-{name}-{version}`
- Atomic upgrades via generations
- Rollback support
- Reproducible builds

### Structured Logging

Scribe provides structured JSON logging:
- Binary journal format for efficiency
- Log rotation with compression
- Query by time, priority, identifier
- Kernel message collection

### Secure Secrets Storage

Cipher provides encrypted secrets:
- ChaCha20-Poly1305 encryption
- Argon2id key derivation
- Session-based access control
- Memory-safe handling (zeroize)

### WSL Compatibility

libnyx-platform provides transparent WSL support:
- Automatic environment detection
- Graceful degradation for unavailable features
- Native performance where possible

## Directory Structure

```
nyx/
├── kernel/           # Microkernel
├── init/             # PID 1
├── nyx-serviced/     # Service manager
├── spectre/          # Login manager
├── phantom/          # Device manager
├── nexus/            # Package manager
├── wraith/           # Network manager
├── cipher/           # Secrets daemon
├── scribe/           # Logging daemon
├── vesper/           # Audio daemon
├── aether/           # Display server
├── umbra/            # Compositor
├── summoner/         # App launcher
├── herald/           # Notifications
├── agents/
│   ├── guardian/     # Security
│   ├── archon/       # Process orchestration
│   ├── arachne/      # Networking
│   └── grimoire/     # Configuration
└── libs/
    ├── libnyx/       # Core library
    ├── libnyx-ipc/   # IPC library
    └── libnyx-platform/  # Platform abstraction
```

## Statistics

- **186 Rust source files**
- **~48,000 lines of code**
- **21 workspace members**

## Roadmap

### Planned Components

- **Chronos** - Time/NTP daemon
- **Slumber** - Power management
- **Sentinel** - Polkit alternative
- **Iris** - Bluetooth daemon
- **Vault** - Disk encryption

### Future Goals

- Bootable ISO generation
- Graphical installer
- Container runtime (OCI compatible)

## License

MIT OR Apache-2.0

## Authors

Daemoniorum Engineering <engineering@daemoniorum.com>
