//! # libnyx - Nyx Kernel Userspace Library
//!
//! This crate provides the userspace interface to the Nyx kernel.
//!
//! ## Features
//!
//! - Capability management
//! - IPC ring operations
//! - Tensor buffer allocation
//! - Inference submission

#![no_std]

pub mod cap;
pub mod ipc;
pub mod tensor;
pub mod syscall;

/// Re-export common types
pub use cap::{Capability, Rights};
pub use ipc::{IpcRing, Message};
