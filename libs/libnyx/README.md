# libnyx - Nyx Kernel Userspace Library

The official userspace library for Nyx OS, providing safe Rust wrappers around kernel syscalls.

## Features

- **Capability-based security** - Unforgeable tokens for fine-grained access control
- **io_uring-style IPC** - High-performance async inter-process communication
- **First-class AI/ML support** - Tensor buffers, device migration, inference submission
- **Process/Thread management** - Spawn processes, create threads, synchronization
- **Memory management** - Virtual memory mapping, protection, allocation

## Quick Start

```rust
use libnyx::prelude::*;

fn main() -> Result<(), Error> {
    // Get current process ID
    let pid = getpid()?;

    // Spawn a child process
    let child = spawn("/bin/hello")?;

    // Wait for it to exit
    let result = wait(Some(child))?;
    println!("Child exited with code {}", result.exit_code);

    Ok(())
}
```

## Modules

### `cap` - Capability Management

```rust
use libnyx::cap::{Capability, Rights, ObjectType};

// Derive a read-only capability
let readonly = cap.derive(Rights::READ)?;

// Grant to another process
let granted = cap.grant(target_pid, Rights::READ | Rights::WRITE)?;

// Revoke all derived capabilities
cap.revoke()?;
```

### `ipc` - Inter-Process Communication

```rust
use libnyx::ipc;

// Simple send/receive
ipc::send(endpoint, b"Hello!", None)?;
let len = ipc::receive(endpoint, &mut buffer, None)?;

// RPC-style call
let response_len = ipc::call(service, &request, &mut response)?;

// Notifications
ipc::signal(notif, 0x01)?;
let bits = ipc::wait(notif, 0xFF, None)?;
```

### `process` - Process Management

```rust
use libnyx::process;

let pid = process::getpid()?;
let ppid = process::getppid()?;

let child = process::spawn("/bin/app")?;
let result = process::wait(Some(child))?;

process::exit(0);
```

### `thread` - Thread Management

```rust
use libnyx::thread;

// Create a thread
let tid = unsafe {
    thread::thread_create(entry_fn, stack_ptr, arg)?
};

// Thread control
thread::thread_yield();
thread::sleep_ms(100)?;
let exit_code = thread::thread_join(tid)?;
```

### `memory` - Memory Management

```rust
use libnyx::memory::{self, prot, flags};

// Map anonymous memory
let addr = memory::mmap(0, 4096, prot::RW, flags::ANONYMOUS)?;

// Change protection
memory::mprotect(addr, 4096, prot::READ)?;

// Unmap
memory::munmap(addr, 4096)?;

// Convenience functions
let page = memory::alloc_page()?;
memory::free(page, memory::PAGE_SIZE)?;
```

### `tensor` - AI/ML Support

```rust
use libnyx::tensor::{TensorBuffer, TensorShape, DType, Device};

// Allocate tensor buffer
let shape = TensorShape::tensor4d(1, 3, 224, 224);
let input = TensorBuffer::alloc_for(&shape, DType::F32, Device::Gpu)?;

// Migrate between devices
let cpu_tensor = input.migrate(Device::Cpu)?;

// Submit inference
let request_id = tensor::inference_submit(
    model_id,
    input.id(),
    output.id(),
    tensor::flags::HIGH_PRIORITY
)?;
```

### `time` - Time Functions

```rust
use libnyx::time::{self, Instant};

// Get current time
let now = time::now_ns()?;
let now_ms = time::now_ms()?;

// Measure duration
let start = Instant::now()?;
// ... do work ...
let elapsed = start.elapsed_ms()?;
```

## Error Handling

All functions return `Result<T, Error>` with typed error variants:

```rust
use libnyx::syscall::Error;

match process::spawn("/nonexistent") {
    Ok(pid) => println!("Spawned {}", pid.as_raw()),
    Err(Error::NotFound) => println!("Executable not found"),
    Err(Error::PermissionDenied) => println!("Permission denied"),
    Err(Error::OutOfMemory) => println!("Out of memory"),
    Err(e) => println!("Error: {}", e),
}
```

## Syscall Numbers

All syscall numbers are defined in `libnyx::syscall::nr` and match the kernel exactly:

| Category | Range | Examples |
|----------|-------|----------|
| IPC | 0-15 | RING_SETUP, SEND, RECEIVE, CALL |
| Capabilities | 16-31 | CAP_DERIVE, CAP_REVOKE, CAP_GRANT |
| Memory | 32-63 | MEM_MAP, MEM_UNMAP, MEM_PROTECT |
| Threads | 64-79 | THREAD_CREATE, THREAD_EXIT, THREAD_YIELD |
| Process | 80-95 | PROCESS_SPAWN, PROCESS_EXIT, PROCESS_WAIT |
| Tensor/AI | 112-143 | TENSOR_ALLOC, INFERENCE_SUBMIT |
| Time-Travel | 144-159 | CHECKPOINT, RESTORE |
| System | 240-255 | DEBUG, GET_TIME, SHUTDOWN |

## Examples

See the `examples/` directory:

- `hello.rs` - Minimal program structure
- `ipc_echo.rs` - IPC messaging between processes
- `tensor_inference.rs` - Tensor allocation and inference

## Building

```bash
cargo build --release
```

## License

See the root LICENSE file.
