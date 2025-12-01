//! Nyx Kernel Entry Point
//!
//! This is the binary entry point that links against the kernel library
//! and provides the actual kernel executable.

#![no_std]
#![no_main]

// Link against the kernel library
// The entry point (_start) and panic handler are defined there
extern crate nyx_kernel;
