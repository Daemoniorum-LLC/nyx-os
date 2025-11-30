//! Kernel heap allocator
//!
//! Implements a slab allocator with buddy allocator backing for the kernel heap.
//! The slab allocator provides O(1) allocation for common sizes, while the buddy
//! allocator handles larger allocations and provides memory to slabs.

use core::alloc::{GlobalAlloc, Layout};
use core::ptr::NonNull;
use spin::Mutex;

/// Kernel heap allocator
#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

/// Heap start address (in kernel space)
const HEAP_START: usize = 0xFFFF_8000_0000_0000;

/// Heap size (64 MB for kernel)
const HEAP_SIZE: usize = 64 * 1024 * 1024;

/// Minimum allocation size (8 bytes for 64-bit alignment)
const MIN_ALLOC_SIZE: usize = 8;

/// Maximum slab size (4 KB - above this use buddy directly)
const MAX_SLAB_SIZE: usize = 4096;

/// Number of slab size classes
const SLAB_CLASSES: usize = 10; // 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096

/// Initialize the kernel heap
pub fn init() {
    unsafe {
        ALLOCATOR.init(HEAP_START, HEAP_SIZE);
    }
    log::trace!(
        "Kernel heap initialized at {:#x}, size {} MB",
        HEAP_START,
        HEAP_SIZE / 1024 / 1024
    );
}

/// Locked heap wrapper
struct LockedHeap {
    inner: Mutex<Option<SlabAllocator>>,
}

impl LockedHeap {
    const fn empty() -> Self {
        Self {
            inner: Mutex::new(None),
        }
    }

    unsafe fn init(&self, start: usize, size: usize) {
        *self.inner.lock() = Some(SlabAllocator::new(start, size));
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

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if let Some(ref mut allocator) = *self.inner.lock() {
            allocator.realloc(ptr, layout, new_size)
        } else {
            core::ptr::null_mut()
        }
    }
}

/// Slab allocator with buddy backing
struct SlabAllocator {
    /// Slab caches for each size class
    slabs: [SlabCache; SLAB_CLASSES],
    /// Buddy allocator for large allocations and slab backing
    buddy: BuddyAllocator,
}

impl SlabAllocator {
    fn new(start: usize, size: usize) -> Self {
        Self {
            slabs: [
                SlabCache::new(8),
                SlabCache::new(16),
                SlabCache::new(32),
                SlabCache::new(64),
                SlabCache::new(128),
                SlabCache::new(256),
                SlabCache::new(512),
                SlabCache::new(1024),
                SlabCache::new(2048),
                SlabCache::new(4096),
            ],
            buddy: BuddyAllocator::new(start, size),
        }
    }

    fn alloc(&mut self, layout: Layout) -> *mut u8 {
        let size = layout.size().max(layout.align()).max(MIN_ALLOC_SIZE);

        if size <= MAX_SLAB_SIZE {
            // Use slab allocator
            let class = size_to_class(size);
            self.slabs[class].alloc(&mut self.buddy)
        } else {
            // Use buddy allocator directly
            self.buddy.alloc(size, layout.align())
        }
    }

    fn dealloc(&mut self, ptr: *mut u8, layout: Layout) {
        let size = layout.size().max(layout.align()).max(MIN_ALLOC_SIZE);

        if size <= MAX_SLAB_SIZE {
            // Return to slab
            let class = size_to_class(size);
            self.slabs[class].dealloc(ptr);
        } else {
            // Return to buddy
            self.buddy.dealloc(ptr, size);
        }
    }

    fn realloc(&mut self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let old_size = layout.size().max(layout.align()).max(MIN_ALLOC_SIZE);
        let new_size_adjusted = new_size.max(layout.align()).max(MIN_ALLOC_SIZE);

        // If same size class, no need to reallocate
        if old_size <= MAX_SLAB_SIZE && new_size_adjusted <= MAX_SLAB_SIZE {
            let old_class = size_to_class(old_size);
            let new_class = size_to_class(new_size_adjusted);
            if old_class == new_class {
                return ptr;
            }
        }

        // Allocate new, copy, free old
        let new_layout = Layout::from_size_align(new_size, layout.align()).unwrap();
        let new_ptr = self.alloc(new_layout);
        if !new_ptr.is_null() {
            unsafe {
                core::ptr::copy_nonoverlapping(ptr, new_ptr, old_size.min(new_size));
            }
            self.dealloc(ptr, layout);
        }
        new_ptr
    }
}

/// Convert size to slab class index
fn size_to_class(size: usize) -> usize {
    // Round up to next power of 2, then find index
    // Classes: 8(0), 16(1), 32(2), 64(3), 128(4), 256(5), 512(6), 1024(7), 2048(8), 4096(9)
    if size <= 8 {
        0
    } else {
        let rounded = size.next_power_of_two();
        (rounded.trailing_zeros() as usize).saturating_sub(3).min(SLAB_CLASSES - 1)
    }
}

/// Class index to size
fn class_to_size(class: usize) -> usize {
    8 << class
}

/// Slab cache for a specific size class
struct SlabCache {
    /// Object size for this cache
    object_size: usize,
    /// Free list head
    free_list: Option<NonNull<FreeObject>>,
    /// Number of free objects
    free_count: usize,
    /// Number of allocated objects
    alloc_count: usize,
}

/// Free object in slab (embedded free list)
struct FreeObject {
    next: Option<NonNull<FreeObject>>,
}

impl SlabCache {
    const fn new(object_size: usize) -> Self {
        Self {
            object_size,
            free_list: None,
            free_count: 0,
            alloc_count: 0,
        }
    }

    fn alloc(&mut self, buddy: &mut BuddyAllocator) -> *mut u8 {
        // Try free list first
        if let Some(obj) = self.free_list {
            unsafe {
                self.free_list = obj.as_ref().next;
                self.free_count -= 1;
                self.alloc_count += 1;
                return obj.as_ptr() as *mut u8;
            }
        }

        // Need to grow - allocate a page from buddy and partition
        let page = buddy.alloc(4096, 4096);
        if page.is_null() {
            return core::ptr::null_mut();
        }

        // Partition page into objects and add to free list
        let objects_per_page = 4096 / self.object_size;
        for i in 1..objects_per_page {
            // Skip first object (we'll return it)
            let obj_ptr = unsafe { page.add(i * self.object_size) } as *mut FreeObject;
            unsafe {
                (*obj_ptr).next = self.free_list;
                self.free_list = Some(NonNull::new_unchecked(obj_ptr));
            }
            self.free_count += 1;
        }

        self.alloc_count += 1;
        page
    }

    fn dealloc(&mut self, ptr: *mut u8) {
        let obj = ptr as *mut FreeObject;
        unsafe {
            (*obj).next = self.free_list;
            self.free_list = Some(NonNull::new_unchecked(obj));
        }
        self.free_count += 1;
        self.alloc_count -= 1;
    }
}

/// Buddy allocator for larger allocations
///
/// Implements a classic buddy allocation scheme with coalescing.
/// Order 0 = 4KB, Order N = 4KB * 2^N
struct BuddyAllocator {
    /// Base address
    base: usize,
    /// Total size
    size: usize,
    /// Free lists by order (order 0 = 4KB)
    free_lists: [Option<NonNull<BuddyBlock>>; MAX_BUDDY_ORDER],
    /// Bitmap tracking allocated blocks for coalescing
    /// Each bit represents whether a block at that position is split
    split_bitmap: [u64; BITMAP_SIZE],
}

/// Maximum buddy order (order 14 = 64MB max block)
const MAX_BUDDY_ORDER: usize = 15;

/// Bitmap size (enough for 64MB with 4KB granularity = 16K blocks = 256 u64s)
const BITMAP_SIZE: usize = 256;

/// Buddy block header (stored in free blocks)
struct BuddyBlock {
    next: Option<NonNull<BuddyBlock>>,
    order: usize,
}

impl BuddyAllocator {
    fn new(base: usize, size: usize) -> Self {
        let mut allocator = Self {
            base,
            size,
            free_lists: [None; MAX_BUDDY_ORDER],
            split_bitmap: [0; BITMAP_SIZE],
        };

        // Add initial region as large blocks
        allocator.add_region(base, size);
        allocator
    }

    fn add_region(&mut self, start: usize, size: usize) {
        // Align to page boundaries
        let start_aligned = (start + 4095) & !4095;
        let end_aligned = (start + size) & !4095;

        if end_aligned <= start_aligned {
            return;
        }

        let mut addr = start_aligned;
        while addr < end_aligned {
            // Find largest order that fits and is aligned
            let mut order = 0;
            while order < MAX_BUDDY_ORDER - 1 {
                let block_size = 4096 << (order + 1);
                if addr + block_size > end_aligned || (addr & (block_size - 1)) != 0 {
                    break;
                }
                order += 1;
            }

            // Add block to free list
            self.add_to_free_list(addr, order);
            addr += 4096 << order;
        }
    }

    fn alloc(&mut self, size: usize, align: usize) -> *mut u8 {
        // Calculate required order
        let size_with_align = size.max(align);
        let pages_needed = (size_with_align + 4095) / 4096;
        let order = pages_needed.next_power_of_two().trailing_zeros() as usize;

        if order >= MAX_BUDDY_ORDER {
            return core::ptr::null_mut();
        }

        // Find a block
        self.alloc_order(order)
    }

    fn alloc_order(&mut self, order: usize) -> *mut u8 {
        // Try to get from exact order
        if let Some(block) = self.remove_from_free_list(order) {
            self.mark_allocated(block, order);
            return block as *mut u8;
        }

        // Try to split from higher order
        for higher_order in (order + 1)..MAX_BUDDY_ORDER {
            if let Some(block) = self.remove_from_free_list(higher_order) {
                // Split and add buddy to free list
                let mut current_order = higher_order;
                let mut current_addr = block;

                while current_order > order {
                    current_order -= 1;
                    let buddy_addr = current_addr + (4096 << current_order);
                    self.add_to_free_list(buddy_addr, current_order);
                    self.mark_split(current_addr, current_order);
                }

                self.mark_allocated(current_addr, order);
                return current_addr as *mut u8;
            }
        }

        core::ptr::null_mut()
    }

    fn dealloc(&mut self, ptr: *mut u8, size: usize) {
        let addr = ptr as usize;
        let pages = (size + 4095) / 4096;
        let order = pages.next_power_of_two().trailing_zeros() as usize;

        self.dealloc_order(addr, order);
    }

    fn dealloc_order(&mut self, mut addr: usize, mut order: usize) {
        self.mark_free(addr, order);

        // Try to coalesce with buddy
        while order < MAX_BUDDY_ORDER - 1 {
            let buddy_addr = self.buddy_of(addr, order);

            // Check if buddy is free (not split and in free list)
            if self.is_split(buddy_addr.min(addr), order) {
                break;
            }

            // Try to remove buddy from free list
            if !self.remove_specific_from_free_list(buddy_addr, order) {
                break;
            }

            // Coalesce: use lower address as new block
            addr = addr.min(buddy_addr);
            self.clear_split(addr, order);
            order += 1;
        }

        // Add coalesced block to free list
        self.add_to_free_list(addr, order);
    }

    /// Calculate buddy address
    fn buddy_of(&self, addr: usize, order: usize) -> usize {
        let block_size = 4096 << order;
        addr ^ block_size
    }

    /// Add block to free list
    fn add_to_free_list(&mut self, addr: usize, order: usize) {
        let block = addr as *mut BuddyBlock;
        unsafe {
            (*block).next = self.free_lists[order];
            (*block).order = order;
            self.free_lists[order] = Some(NonNull::new_unchecked(block));
        }
    }

    /// Remove any block from free list
    fn remove_from_free_list(&mut self, order: usize) -> Option<usize> {
        if let Some(block) = self.free_lists[order] {
            unsafe {
                self.free_lists[order] = block.as_ref().next;
                Some(block.as_ptr() as usize)
            }
        } else {
            None
        }
    }

    /// Remove specific block from free list
    fn remove_specific_from_free_list(&mut self, addr: usize, order: usize) -> bool {
        let target = addr as *mut BuddyBlock;

        // Check head
        if let Some(head) = self.free_lists[order] {
            if head.as_ptr() == target {
                unsafe {
                    self.free_lists[order] = head.as_ref().next;
                }
                return true;
            }

            // Walk list
            let mut prev = head;
            unsafe {
                while let Some(next) = prev.as_ref().next {
                    if next.as_ptr() == target {
                        prev.as_mut().next = next.as_ref().next;
                        return true;
                    }
                    prev = next;
                }
            }
        }
        false
    }

    /// Bitmap index for an address and order
    fn bitmap_index(&self, addr: usize, order: usize) -> (usize, usize) {
        let relative = (addr - self.base) >> 12; // Page index
        let block_index = relative >> order;
        (block_index / 64, block_index % 64)
    }

    fn mark_split(&mut self, addr: usize, order: usize) {
        let (word, bit) = self.bitmap_index(addr, order);
        if word < BITMAP_SIZE {
            self.split_bitmap[word] |= 1 << bit;
        }
    }

    fn clear_split(&mut self, addr: usize, order: usize) {
        let (word, bit) = self.bitmap_index(addr, order);
        if word < BITMAP_SIZE {
            self.split_bitmap[word] &= !(1 << bit);
        }
    }

    fn is_split(&self, addr: usize, order: usize) -> bool {
        let (word, bit) = self.bitmap_index(addr, order);
        if word < BITMAP_SIZE {
            (self.split_bitmap[word] & (1 << bit)) != 0
        } else {
            false
        }
    }

    fn mark_allocated(&mut self, addr: usize, order: usize) {
        self.mark_split(addr, order);
    }

    fn mark_free(&mut self, addr: usize, order: usize) {
        self.clear_split(addr, order);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_size_to_class() {
        assert_eq!(size_to_class(1), 0);
        assert_eq!(size_to_class(8), 0);
        assert_eq!(size_to_class(9), 1);
        assert_eq!(size_to_class(16), 1);
        assert_eq!(size_to_class(17), 2);
        assert_eq!(size_to_class(32), 2);
        assert_eq!(size_to_class(4096), 9);
    }

    #[test]
    fn test_class_to_size() {
        assert_eq!(class_to_size(0), 8);
        assert_eq!(class_to_size(1), 16);
        assert_eq!(class_to_size(9), 4096);
    }
}
