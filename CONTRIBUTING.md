# Contributing to Nyx OS

Welcome! This guide will help you get up and running with Nyx development.

## Development Setup

### Prerequisites

```bash
# Install Rust (stable + nightly)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup toolchain install nightly
rustup component add rust-src llvm-tools-preview --toolchain nightly

# Install system dependencies (Ubuntu/Debian/WSL)
sudo apt install qemu-system-x86 grub-pc-bin xorriso mtools llvm lld
```

### Repository Layout

Nyx is split into two build targets:

| Target | Toolchain | Build Command |
|--------|-----------|---------------|
| Userspace (daemons, libs) | Stable | `cargo build` from repo root |
| Kernel | Nightly | `make build` from `kernel/` |

This split exists because the kernel requires unstable Rust features (`#![feature(...)]`) that aren't available on stable.

### Building Everything

```bash
# Clone the repo
git clone https://github.com/Daemoniorum-LLC/nyx-os.git
cd nyx-os

# Build userspace
cargo build

# Build kernel
cd kernel && make build
```

### Running the Kernel

```bash
cd kernel

# With graphics (Linux/macOS)
make run

# Serial only (WSL2 or headless)
make run-serial

# With GDB server on :1234
make run-debug
```

## Code Organization

### Naming Convention

All daemons follow an occult/mythological naming theme:

- **Guardian** — Security (guards the system)
- **Arachne** — Network (weaves connections like a spider)
- **Archon** — Process management (Greek: ruler)
- **Grimoire** — AI personas (book of spells)
- **Vesper** — Audio (evening star, associated with music)
- **Phantom** — Devices (appears/disappears like devices)
- **Chronos** — Time (Greek god of time)
- **Umbra** — Shell (Latin: shadow)

When adding new components, follow the theme or ask in your PR.

### Directory Structure

```
component/
├── src/
│   ├── main.rs      # Entry point with clap Args struct
│   ├── config.rs    # Configuration loading
│   ├── ipc.rs       # IPC message handlers
│   └── [feature].rs # Feature modules
├── Cargo.toml
└── README.md        # (optional but encouraged)
```

### Standard Patterns

**CLI Arguments** — Use `clap` derive macros:

```rust
#[derive(Parser, Debug)]
#[command(name = "daemon-name", version, about)]
struct Args {
    #[arg(short, long, default_value = "/grimoire/system/daemon.yaml")]
    config: PathBuf,

    #[arg(short, long)]
    debug: bool,
}
```

**Logging** — Use `tracing`:

```rust
use tracing::{info, warn, error, debug};

info!("Daemon ready");
error!("Failed to connect: {}", err);
```

**Error Handling** — Prefer typed errors over `anyhow!()`:

```rust
// Good
#[derive(thiserror::Error, Debug)]
pub enum DaemonError {
    #[error("Connection failed to {host}: {source}")]
    ConnectionFailed { host: String, source: io::Error },
}

// Avoid in new code
Err(anyhow!("Connection failed"))
```

**Async** — Use `tokio`:

```rust
#[tokio::main]
async fn main() -> Result<()> {
    // ...
}
```

## Making Changes

### Workflow

1. Create a feature branch: `git checkout -b feature/your-feature`
2. Make your changes
3. Run checks: `cargo fmt && cargo clippy && cargo test`
4. For kernel changes: `cd kernel && make check`
5. Commit with conventional commits: `feat:`, `fix:`, `docs:`, etc.
6. Push and open a PR

### Commit Messages

Use [Conventional Commits](https://www.conventionalcommits.org/):

```
feat: add WireGuard support to Arachne
fix: correct capability derivation for child processes
docs: add architecture diagram to README
refactor: split process.rs into smaller modules
```

### Code Style

- Run `cargo fmt` before committing
- Run `cargo clippy` and address warnings
- Keep functions under ~100 lines when reasonable
- Add doc comments for public APIs
- Use `// TODO:` sparingly — prefer GitHub issues for tracking work

## Testing

### Running Tests

```bash
# All userspace tests
cargo test

# Specific crate
cargo test -p arachne

# Kernel tests (limited — runs test stubs)
cd kernel && cargo test
```

### Writing Tests

Place unit tests in the same file:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capability_derivation() {
        // ...
    }
}
```

## Excluded Crates

Some crates are excluded from the main workspace due to special requirements:

| Crate | Reason | How to Build |
|-------|--------|--------------|
| `kernel` | Requires nightly Rust, `no_std` | `cd kernel && make build` |
| `spectre` | Requires libclang for PAM bindings | Install `libclang-dev`, then `cargo build -p spectre` |
| `aether` | Dependency resolution in progress | Not currently buildable |

## Getting Help

- Check existing issues before filing new ones
- For architecture questions, read the module-level docs (`//!` comments)
- See `kernel/docs/` for kernel-specific documentation

## License

By contributing, you agree that your contributions will be licensed under the project's proprietary license.
