//! Kernel heap allocator

use core::alloc::{GlobalAlloc, Layout};
use spin::Mutex;

/// Kernel heap allocator
#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

/// Heap start address
const HEAP_START: usize = 0xFFFF_8000_0000_0000;

/// Heap size (16 MB)
const HEAP_SIZE: usize = 16 * 1024 * 1024;

/// Initialize the kernel heap
pub fn init() {
    unsafe {
        ALLOCATOR.init(HEAP_START, HEAP_SIZE);
    }
    log::trace!("Kernel heap initialized at {:#x}, size {} MB",
        HEAP_START, HEAP_SIZE / 1024 / 1024);
}

/// Locked heap wrapper
struct LockedHeap {
    inner: Mutex<Option<BumpAllocator>>,
}

impl LockedHeap {
    const fn empty() -> Self {
        Self {
            inner: Mutex::new(None),
        }
    }

    unsafe fn init(&self, start: usize, size: usize) {
        *self.inner.lock() = Some(BumpAllocator::new(start, size));
    }
}

unsafe impl GlobalAlloc for LockedHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if let Some(ref mut allocator) = *self.inner.lock() {
            allocator.alloc(layout)
        } else {
            core::ptr::null_mut()
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if let Some(ref mut allocator) = *self.inner.lock() {
            allocator.dealloc(ptr, layout);
        }
    }
}

/// Simple bump allocator (for initial bootstrap)
/// TODO: Replace with proper slab/buddy allocator
struct BumpAllocator {
    start: usize,
    end: usize,
    next: usize,
}

impl BumpAllocator {
    fn new(start: usize, size: usize) -> Self {
        Self {
            start,
            end: start + size,
            next: start,
        }
    }

    fn alloc(&mut self, layout: Layout) -> *mut u8 {
        let aligned = (self.next + layout.align() - 1) & !(layout.align() - 1);
        let end = aligned + layout.size();

        if end > self.end {
            return core::ptr::null_mut();
        }

        self.next = end;
        aligned as *mut u8
    }

    fn dealloc(&mut self, _ptr: *mut u8, _layout: Layout) {
        // Bump allocator doesn't support deallocation
        // TODO: Implement proper allocator
    }
}
