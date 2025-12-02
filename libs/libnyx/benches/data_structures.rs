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

// ============================================================================
// Optimized Message Benchmarks (MaybeUninit)
// ============================================================================

impl Message {
    /// Fast path constructor using MaybeUninit
    #[inline]
    unsafe fn new_uninit() -> Self {
        use std::mem::MaybeUninit;
        let mut msg = MaybeUninit::<Self>::uninit();
        let ptr = msg.as_mut_ptr();
        (*ptr).tag = 0;
        (*ptr).length = 0;
        msg.assume_init()
    }

    /// Fast path with data
    #[inline]
    fn with_data_fast(tag: u32, data: &[u8]) -> Self {
        let len = data.len().min(MAX_MESSAGE_SIZE);
        // SAFETY: We immediately initialize all used fields
        let mut msg = unsafe { Self::new_uninit() };
        msg.tag = tag;
        msg.length = len as u32;
        msg.data[..len].copy_from_slice(&data[..len]);
        msg
    }
}

#[bench]
fn bench_message_new_uninit(b: &mut Bencher) {
    b.iter(|| {
        // SAFETY: We don't read uninit data
        unsafe { black_box(Message::new_uninit()) }
    });
}

#[bench]
fn bench_message_with_small_data_fast(b: &mut Bencher) {
    let data = b"Hello, Nyx!";
    b.iter(|| {
        black_box(Message::with_data_fast(1, data))
    });
}

#[bench]
fn bench_message_with_1kb_data_fast(b: &mut Bencher) {
    let data = vec![0xABu8; 1024];
    b.iter(|| {
        black_box(Message::with_data_fast(1, &data))
    });
}

#[bench]
fn bench_message_with_4kb_data_fast(b: &mut Bencher) {
    let data = vec![0xABu8; 4096];
    b.iter(|| {
        black_box(Message::with_data_fast(1, &data))
    });
}

// ============================================================================
// MessagePool Benchmarks
// ============================================================================

struct MessagePool<const N: usize> {
    messages: [Message; N],
    in_use: u64,
    used_count: usize,
}

impl<const N: usize> MessagePool<N> {
    fn new() -> Self {
        Self {
            messages: std::array::from_fn(|_| Message::new()),
            in_use: 0,
            used_count: 0,
        }
    }

    #[inline]
    fn acquire(&mut self) -> Option<usize> {
        if self.used_count >= N {
            return None;
        }
        let free_mask = !self.in_use;
        let idx = free_mask.trailing_zeros() as usize;
        if idx >= N {
            return None;
        }
        self.in_use |= 1 << idx;
        self.used_count += 1;
        // Clear header only (fast)
        self.messages[idx].tag = 0;
        self.messages[idx].length = 0;
        Some(idx)
    }

    #[inline]
    fn release(&mut self, idx: usize) {
        self.in_use &= !(1 << idx);
        self.used_count -= 1;
    }

    #[inline]
    fn get_mut(&mut self, idx: usize) -> &mut Message {
        &mut self.messages[idx]
    }
}

#[bench]
fn bench_pool_acquire_release(b: &mut Bencher) {
    let mut pool = MessagePool::<16>::new();
    b.iter(|| {
        let idx = pool.acquire().unwrap();
        pool.release(idx);
        black_box(idx)
    });
}

#[bench]
fn bench_pool_acquire_write_release(b: &mut Bencher) {
    let mut pool = MessagePool::<16>::new();
    let data = b"Hello, pool!";
    b.iter(|| {
        let idx = pool.acquire().unwrap();
        let msg = pool.get_mut(idx);
        msg.tag = 1;
        msg.length = data.len() as u32;
        msg.data[..data.len()].copy_from_slice(data);
        pool.release(idx);
        black_box(idx)
    });
}

// ============================================================================
// Batch Submission Benchmarks
// ============================================================================

#[repr(u8)]
#[derive(Clone, Copy)]
#[allow(dead_code)]
enum OpType {
    Send = 0,
    Receive = 1,
    Call = 2,
    Reply = 3,
    Signal = 4,
    Wait = 5,
    Poll = 6,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct SubmissionEntry {
    op: OpType,
    flags: u8,
    _reserved: u16,
    user_data: u32,
    cap: u64,
    addr: u64,
    len: u32,
    param: u32,
}

struct SubmissionBatch<const N: usize> {
    entries: [std::mem::MaybeUninit<SubmissionEntry>; N],
    count: usize,
}

impl<const N: usize> SubmissionBatch<N> {
    fn new() -> Self {
        Self {
            // SAFETY: MaybeUninit array doesn't need initialization
            entries: unsafe { std::mem::MaybeUninit::uninit().assume_init() },
            count: 0,
        }
    }

    #[inline]
    fn push(&mut self, entry: SubmissionEntry) -> bool {
        if self.count >= N {
            return false;
        }
        self.entries[self.count].write(entry);
        self.count += 1;
        true
    }

    #[inline]
    fn clear(&mut self) {
        self.count = 0;
    }

    #[inline]
    fn len(&self) -> usize {
        self.count
    }
}

#[bench]
fn bench_batch_build_8_entries(b: &mut Bencher) {
    b.iter(|| {
        let mut batch = SubmissionBatch::<16>::new();
        for i in 0..8u32 {
            batch.push(SubmissionEntry {
                op: OpType::Signal,
                flags: 0,
                _reserved: 0,
                user_data: i,
                cap: 1,
                addr: 0xFF,
                len: 0,
                param: 0,
            });
        }
        black_box(batch.len())
    });
}

#[bench]
fn bench_batch_reuse_8_entries(b: &mut Bencher) {
    let mut batch = SubmissionBatch::<16>::new();
    b.iter(|| {
        batch.clear();
        for i in 0..8u32 {
            batch.push(SubmissionEntry {
                op: OpType::Signal,
                flags: 0,
                _reserved: 0,
                user_data: i,
                cap: 1,
                addr: 0xFF,
                len: 0,
                param: 0,
            });
        }
        black_box(batch.len())
    });
}
