//! Benchmarks for libnyx data structures
//!
//! These benchmarks measure operations that can be tested on the host,
//! such as message construction, tensor shape calculations, and rights operations.

#![feature(test)]
extern crate test;

use test::Bencher;
use std::hint::black_box;

// Simulate the structures from libnyx for host benchmarking
// (We can't use libnyx directly since it's no_std)

const MAX_MESSAGE_SIZE: usize = 4096;

#[repr(C)]
struct Message {
    tag: u32,
    length: u32,
    data: [u8; MAX_MESSAGE_SIZE],
}

impl Message {
    fn new() -> Self {
        Self {
            tag: 0,
            length: 0,
            data: [0; MAX_MESSAGE_SIZE],
        }
    }

    fn with_data(tag: u32, data: &[u8]) -> Self {
        let mut msg = Self::new();
        msg.tag = tag;
        msg.length = data.len().min(MAX_MESSAGE_SIZE) as u32;
        msg.data[..msg.length as usize].copy_from_slice(&data[..msg.length as usize]);
        msg
    }
}

#[derive(Clone, Debug, Default)]
struct TensorShape {
    dims: [u32; 8],
    ndims: u8,
}

impl TensorShape {
    fn new(dims: &[u32]) -> Self {
        let mut shape = Self::default();
        shape.ndims = dims.len().min(8) as u8;
        for (i, &d) in dims.iter().take(8).enumerate() {
            shape.dims[i] = d;
        }
        shape
    }

    fn numel(&self) -> usize {
        self.dims[..self.ndims as usize]
            .iter()
            .map(|&d| d as usize)
            .product()
    }
}

#[derive(Clone, Copy)]
struct Rights(u64);

impl Rights {
    const READ: Self = Self(1 << 0);
    const WRITE: Self = Self(1 << 1);
    const EXECUTE: Self = Self(1 << 2);
    const GRANT: Self = Self(1 << 3);

    fn contains(&self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    fn is_subset_of(&self, other: Self) -> bool {
        (self.0 & !other.0) == 0
    }
}

impl std::ops::BitOr for Rights {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

// ============================================================================
// Message Benchmarks
// ============================================================================

#[bench]
fn bench_message_new(b: &mut Bencher) {
    b.iter(|| {
        black_box(Message::new())
    });
}

#[bench]
fn bench_message_with_small_data(b: &mut Bencher) {
    let data = b"Hello, Nyx!";
    b.iter(|| {
        black_box(Message::with_data(1, data))
    });
}

#[bench]
fn bench_message_with_1kb_data(b: &mut Bencher) {
    let data = vec![0xABu8; 1024];
    b.iter(|| {
        black_box(Message::with_data(1, &data))
    });
}

#[bench]
fn bench_message_with_4kb_data(b: &mut Bencher) {
    let data = vec![0xABu8; 4096];
    b.iter(|| {
        black_box(Message::with_data(1, &data))
    });
}

// ============================================================================
// TensorShape Benchmarks
// ============================================================================

#[bench]
fn bench_tensor_shape_new_1d(b: &mut Bencher) {
    b.iter(|| {
        black_box(TensorShape::new(&[1024]))
    });
}

#[bench]
fn bench_tensor_shape_new_4d(b: &mut Bencher) {
    b.iter(|| {
        black_box(TensorShape::new(&[1, 3, 224, 224]))
    });
}

#[bench]
fn bench_tensor_shape_numel_4d(b: &mut Bencher) {
    let shape = TensorShape::new(&[1, 3, 224, 224]);
    b.iter(|| {
        black_box(shape.numel())
    });
}

#[bench]
fn bench_tensor_shape_numel_8d(b: &mut Bencher) {
    let shape = TensorShape::new(&[2, 4, 8, 16, 32, 64, 128, 256]);
    b.iter(|| {
        black_box(shape.numel())
    });
}

// ============================================================================
// Rights Benchmarks
// ============================================================================

#[bench]
fn bench_rights_or(b: &mut Bencher) {
    let r1 = Rights::READ;
    let r2 = Rights::WRITE;
    b.iter(|| {
        black_box(r1 | r2)
    });
}

#[bench]
fn bench_rights_contains(b: &mut Bencher) {
    let rights = Rights::READ | Rights::WRITE | Rights::EXECUTE;
    b.iter(|| {
        black_box(rights.contains(Rights::WRITE))
    });
}

#[bench]
fn bench_rights_is_subset(b: &mut Bencher) {
    let full = Rights::READ | Rights::WRITE | Rights::EXECUTE | Rights::GRANT;
    let partial = Rights::READ | Rights::WRITE;
    b.iter(|| {
        black_box(partial.is_subset_of(full))
    });
}

// ============================================================================
// Syscall Number Lookup Simulation
// ============================================================================

mod syscall_nr {
    pub const RING_SETUP: u64 = 0;
    pub const SEND: u64 = 2;
    pub const RECEIVE: u64 = 3;
    pub const CAP_DERIVE: u64 = 16;
    pub const MEM_MAP: u64 = 32;
    pub const THREAD_CREATE: u64 = 64;
    pub const PROCESS_SPAWN: u64 = 80;
    pub const TENSOR_ALLOC: u64 = 112;
    pub const CHECKPOINT: u64 = 144;
    pub const DEBUG: u64 = 240;
}

#[bench]
fn bench_syscall_number_lookup(b: &mut Bencher) {
    // Simulates looking up a syscall number (constant-time)
    b.iter(|| {
        black_box(syscall_nr::TENSOR_ALLOC)
    });
}

// ============================================================================
// Error Code Conversion
// ============================================================================

#[repr(i32)]
#[derive(Clone, Copy, Debug)]
enum Error {
    Success = 0,
    InvalidSyscall = -1,
    InvalidCapability = -2,
    PermissionDenied = -3,
    OutOfMemory = -4,
    InvalidArgument = -5,
}

impl Error {
    fn from_raw(value: i64) -> Result<u64, Self> {
        if value >= 0 {
            Ok(value as u64)
        } else {
            Err(match value as i32 {
                -1 => Self::InvalidSyscall,
                -2 => Self::InvalidCapability,
                -3 => Self::PermissionDenied,
                -4 => Self::OutOfMemory,
                -5 => Self::InvalidArgument,
                _ => Self::InvalidSyscall,
            })
        }
    }
}

#[bench]
fn bench_error_from_raw_success(b: &mut Bencher) {
    b.iter(|| {
        black_box(Error::from_raw(42))
    });
}

#[bench]
fn bench_error_from_raw_error(b: &mut Bencher) {
    b.iter(|| {
        black_box(Error::from_raw(-4))
    });
}
