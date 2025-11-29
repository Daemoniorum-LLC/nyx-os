//! Physical frame allocator

use super::{PhysAddr, PAGE_SIZE};
use alloc::vec::Vec;

/// Buddy allocator for physical frames
pub struct FrameAllocator {
    /// Free lists by order (order 0 = 4KB, order 9 = 2MB, etc.)
    free_lists: [Vec<PhysAddr>; MAX_ORDER],
    /// Total frames managed
    total_frames: usize,
    /// Free frames available
    free_frames: usize,
}

const MAX_ORDER: usize = 10; // Up to 4MB blocks

impl FrameAllocator {
    /// Create a new frame allocator
    pub fn new() -> Self {
        Self {
            free_lists: Default::default(),
            total_frames: 0,
            free_frames: 0,
        }
    }

    /// Add a memory region to the allocator
    pub fn add_region(&mut self, start: u64, size: u64) {
        let start_aligned = (start + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
        let end_aligned = (start + size) & !(PAGE_SIZE - 1);

        if end_aligned <= start_aligned {
            return;
        }

        let num_frames = ((end_aligned - start_aligned) / PAGE_SIZE) as usize;
        self.total_frames += num_frames;

        // Add frames to appropriate free lists
        let mut addr = start_aligned;
        while addr < end_aligned {
            // Find largest block that fits and is aligned
            let mut order = 0;
            while order < MAX_ORDER - 1 {
                let block_size = PAGE_SIZE << (order + 1);
                if addr + block_size > end_aligned || (addr & (block_size - 1)) != 0 {
                    break;
                }
                order += 1;
            }

            self.free_lists[order].push(PhysAddr::new(addr));
            self.free_frames += 1 << order;
            addr += PAGE_SIZE << order;
        }
    }

    /// Allocate a single frame
    pub fn alloc_frame(&mut self) -> Option<PhysAddr> {
        self.alloc_order(0)
    }

    /// Allocate contiguous frames
    pub fn alloc_frames(&mut self, count: usize) -> Option<PhysAddr> {
        // Find minimum order that fits
        let order = (count.next_power_of_two().trailing_zeros()) as usize;
        self.alloc_order(order.min(MAX_ORDER - 1))
    }

    /// Allocate block of given order
    fn alloc_order(&mut self, order: usize) -> Option<PhysAddr> {
        // Try to get from exact order
        if let Some(addr) = self.free_lists[order].pop() {
            self.free_frames -= 1 << order;
            return Some(addr);
        }

        // Try to split from higher order
        for higher_order in (order + 1)..MAX_ORDER {
            if let Some(addr) = self.free_lists[higher_order].pop() {
                // Split and return lower half, put upper half back
                let mut current_order = higher_order;
                let mut current_addr = addr;

                while current_order > order {
                    current_order -= 1;
                    let buddy_addr = PhysAddr::new(
                        current_addr.as_u64() + (PAGE_SIZE << current_order)
                    );
                    self.free_lists[current_order].push(buddy_addr);
                }

                self.free_frames -= 1 << order;
                return Some(current_addr);
            }
        }

        None
    }

    /// Free a frame
    pub fn free_frame(&mut self, addr: PhysAddr) {
        self.free_order(addr, 0);
    }

    /// Free block of given order
    fn free_order(&mut self, addr: PhysAddr, order: usize) {
        self.free_lists[order].push(addr);
        self.free_frames += 1 << order;

        // TODO: Coalesce with buddy if both free
    }

    /// Get free frame count
    pub fn free_count(&self) -> usize {
        self.free_frames
    }

    /// Get total frame count
    pub fn total_count(&self) -> usize {
        self.total_frames
    }
}

impl Default for FrameAllocator {
    fn default() -> Self {
        Self::new()
    }
}
