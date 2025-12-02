//! # libnyx - Nyx Kernel Userspace Library
//!
//! This crate provides the userspace interface to the Nyx kernel.
//!
//! ## Features
//!
//! - **Capability management** - Unforgeable tokens for access control
//! - **IPC** - io_uring-style async inter-process communication
//! - **Process/Thread** - Process spawning and thread management
//! - **Memory** - Virtual memory mapping and protection
//! - **Tensor/AI** - First-class AI/ML acceleration support
//! - **Time** - Monotonic time and duration measurement
//!
//! ## Quick Start
//!
//! ```no_run
//! use libnyx::{process, ipc, cap::Capability};
//!
//! // Get current PID
//! let pid = process::getpid()?;
//!
//! // Spawn a child process
//! let child = process::spawn("/bin/hello")?;
//!
//! // Wait for child to exit
//! let result = process::wait(Some(child))?;
//! println!("Child exited with code {}", result.exit_code);
//! ```
//!
//! ## IPC Example
//!
//! ```no_run
//! use libnyx::ipc;
//! use libnyx::cap::Capability;
//!
//! // Send a message
//! ipc::send(endpoint, b"Hello!", None)?;
//!
//! // Receive a message
//! let mut buf = [0u8; 4096];
//! let len = ipc::receive(endpoint, &mut buf, None)?;
//! ```
//!
//! ## Error Handling
//!
//! All functions return `Result<T, syscall::Error>` with typed error variants:
//!
//! ```no_run
//! use libnyx::syscall::Error;
//!
//! match process::spawn("/nonexistent") {
//!     Ok(pid) => println!("Spawned {}", pid.as_raw()),
//!     Err(Error::NotFound) => println!("Executable not found"),
//!     Err(Error::PermissionDenied) => println!("Permission denied"),
//!     Err(e) => println!("Error: {}", e),
//! }
//! ```

#![no_std]

// Core modules
pub mod cap;
pub mod ipc;
pub mod memory;
pub mod process;
pub mod syscall;
pub mod tensor;
pub mod thread;
pub mod time;

// Re-export commonly used types at the crate root
pub use cap::{Capability, ObjectType, Rights};
pub use ipc::{IpcRing, Message, MAX_MESSAGE_SIZE};
pub use memory::{flags as mmap_flags, prot, PAGE_SIZE};
pub use process::{ProcessId, WaitResult};
pub use syscall::Error;
pub use tensor::{DType, Device, InferenceConfig, TensorBuffer, TensorShape};
pub use thread::ThreadId;
pub use time::Instant;

/// Prelude module for convenient imports
pub mod prelude {
    pub use crate::cap::{Capability, Rights};
    pub use crate::ipc::{self, IpcRing, Message};
    pub use crate::memory::{self, alloc, free, mmap, munmap};
    pub use crate::process::{self, exit, getpid, spawn, wait, ProcessId};
    pub use crate::syscall::Error;
    pub use crate::tensor::{self, Device, DType, TensorBuffer, TensorShape};
    pub use crate::thread::{self, sleep_ms, sleep_secs, thread_yield, ThreadId};
    pub use crate::time::{self, now_ms, now_ns, Instant};
}
