//! Physical frame allocator
//!
//! Implements a buddy allocator for physical memory frames with full coalescing
//! support. This is used for allocating physical pages for virtual memory,
//! DMA buffers, and other low-level memory needs.

use super::{PhysAddr, PAGE_SIZE};
use alloc::vec::Vec;
use core::cmp::Ordering;

/// Maximum buddy order (order 0 = 4KB, order 9 = 2MB, order 10 = 4MB)
const MAX_ORDER: usize = 11;

/// Buddy allocator for physical frames
pub struct FrameAllocator {
    /// Free lists by order (order 0 = 4KB, order 9 = 2MB, etc.)
    free_lists: [Vec<PhysAddr>; MAX_ORDER],
    /// Total frames managed
    total_frames: usize,
    /// Free frames available
    free_frames: usize,
    /// Base address of managed region (for buddy calculations)
    base_addr: u64,
    /// Bitmap for tracking split status (for coalescing)
    /// One bit per page at each order level
    split_bitmap: Vec<u64>,
}

impl FrameAllocator {
    /// Create a new frame allocator
    pub fn new() -> Self {
        Self {
            free_lists: Default::default(),
            total_frames: 0,
            free_frames: 0,
            base_addr: 0,
            split_bitmap: Vec::new(),
        }
    }

    /// Add a memory region to the allocator
    pub fn add_region(&mut self, start: u64, size: u64) {
        let start_aligned = (start + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
        let end_aligned = (start + size) & !(PAGE_SIZE - 1);

        if end_aligned <= start_aligned {
            return;
        }

        // Set base address if this is the first region
        if self.base_addr == 0 || start_aligned < self.base_addr {
            self.base_addr = start_aligned;
        }

        let num_frames = ((end_aligned - start_aligned) / PAGE_SIZE) as usize;
        self.total_frames += num_frames;

        // Ensure bitmap is large enough
        let bitmap_size = (num_frames + 63) / 64;
        if self.split_bitmap.len() < bitmap_size * MAX_ORDER {
            self.split_bitmap.resize(bitmap_size * MAX_ORDER, 0);
        }

        // Add frames to appropriate free lists using buddy algorithm
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

        log::trace!(
            "Added memory region: {:#x}-{:#x} ({} frames)",
            start_aligned,
            end_aligned,
            num_frames
        );
    }

    /// Allocate a single frame
    pub fn alloc_frame(&mut self) -> Option<PhysAddr> {
        self.alloc_order(0)
    }

    /// Allocate contiguous frames
    pub fn alloc_frames(&mut self, count: usize) -> Option<PhysAddr> {
        if count == 0 {
            return None;
        }
        // Find minimum order that fits
        let order = (count.next_power_of_two().trailing_zeros()) as usize;
        self.alloc_order(order.min(MAX_ORDER - 1))
    }

    /// Allocate a 2MB huge page (order 9)
    pub fn alloc_huge_page(&mut self) -> Option<PhysAddr> {
        self.alloc_order(9) // 512 pages = 2MB
    }

    /// Allocate block of given order
    fn alloc_order(&mut self, order: usize) -> Option<PhysAddr> {
        if order >= MAX_ORDER {
            return None;
        }

        // Try to get from exact order
        if let Some(addr) = self.free_lists[order].pop() {
            self.free_frames -= 1 << order;
            self.mark_allocated(addr, order);
            return Some(addr);
        }

        // Try to split from higher order
        for higher_order in (order + 1)..MAX_ORDER {
            if let Some(addr) = self.free_lists[higher_order].pop() {
                // Split block down to required order
                let mut current_order = higher_order;
                let mut current_addr = addr;

                while current_order > order {
                    current_order -= 1;
                    // Add buddy (upper half) to free list
                    let buddy_addr =
                        PhysAddr::new(current_addr.as_u64() + (PAGE_SIZE << current_order));
                    self.free_lists[current_order].push(buddy_addr);
                    // Mark this level as split
                    self.mark_split(current_addr, current_order);
                }

                self.free_frames -= 1 << order;
                self.mark_allocated(current_addr, order);
                return Some(current_addr);
            }
        }

        None
    }

    /// Free a frame
    pub fn free_frame(&mut self, addr: PhysAddr) {
        self.free_order(addr, 0);
    }

    /// Free contiguous frames
    pub fn free_frames(&mut self, addr: PhysAddr, count: usize) {
        if count == 0 {
            return;
        }
        let order = (count.next_power_of_two().trailing_zeros()) as usize;
        self.free_order(addr, order.min(MAX_ORDER - 1));
    }

    /// Free a 2MB huge page
    pub fn free_huge_page(&mut self, addr: PhysAddr) {
        self.free_order(addr, 9);
    }

    /// Free block of given order with coalescing
    fn free_order(&mut self, addr: PhysAddr, order: usize) {
        let mut current_addr = addr;
        let mut current_order = order;

        self.mark_free(current_addr, current_order);

        // Try to coalesce with buddy
        while current_order < MAX_ORDER - 1 {
            let buddy_addr = self.buddy_of(current_addr, current_order);

            // Check if buddy is free and at same order
            if !self.is_buddy_free(buddy_addr, current_order) {
                break;
            }

            // Remove buddy from free list
            if !self.remove_from_free_list(buddy_addr, current_order) {
                break;
            }

            // Clear split marker for parent
            let parent_addr = if current_addr.as_u64() < buddy_addr.as_u64() {
                current_addr
            } else {
                buddy_addr
            };
            self.clear_split(parent_addr, current_order);

            // Move up to parent
            current_addr = parent_addr;
            current_order += 1;
        }

        // Add coalesced block to free list
        self.free_lists[current_order].push(current_addr);
        self.free_frames += 1 << order;
    }

    /// Calculate buddy address for a block
    fn buddy_of(&self, addr: PhysAddr, order: usize) -> PhysAddr {
        let block_size = PAGE_SIZE << order;
        PhysAddr::new(addr.as_u64() ^ block_size)
    }

    /// Check if buddy is free at the given order
    fn is_buddy_free(&self, buddy_addr: PhysAddr, order: usize) -> bool {
        // Buddy is free if it's in the free list at this order
        // and not marked as split (meaning it hasn't been subdivided)
        if self.is_split(buddy_addr, order) {
            return false;
        }

        // Check if buddy is in free list
        self.free_lists[order]
            .iter()
            .any(|&addr| addr == buddy_addr)
    }

    /// Remove specific address from free list
    fn remove_from_free_list(&mut self, addr: PhysAddr, order: usize) -> bool {
        if let Some(pos) = self.free_lists[order].iter().position(|&a| a == addr) {
            self.free_lists[order].swap_remove(pos);
            true
        } else {
            false
        }
    }

    /// Get bitmap index for address and order
    fn bitmap_index(&self, addr: PhysAddr, order: usize) -> Option<(usize, usize)> {
        if addr.as_u64() < self.base_addr {
            return None;
        }
        let page_index = ((addr.as_u64() - self.base_addr) / PAGE_SIZE) as usize;
        let block_index = page_index >> order;
        let word = (order * (self.total_frames / 64 + 1)) + block_index / 64;
        let bit = block_index % 64;

        if word < self.split_bitmap.len() {
            Some((word, bit))
        } else {
            None
        }
    }

    /// Mark block as split (subdivided)
    fn mark_split(&mut self, addr: PhysAddr, order: usize) {
        if let Some((word, bit)) = self.bitmap_index(addr, order) {
            self.split_bitmap[word] |= 1 << bit;
        }
    }

    /// Clear split marker
    fn clear_split(&mut self, addr: PhysAddr, order: usize) {
        if let Some((word, bit)) = self.bitmap_index(addr, order) {
            self.split_bitmap[word] &= !(1 << bit);
        }
    }

    /// Check if block is split
    fn is_split(&self, addr: PhysAddr, order: usize) -> bool {
        if let Some((word, bit)) = self.bitmap_index(addr, order) {
            (self.split_bitmap[word] & (1 << bit)) != 0
        } else {
            false
        }
    }

    /// Mark block as allocated
    fn mark_allocated(&mut self, addr: PhysAddr, order: usize) {
        self.mark_split(addr, order);
    }

    /// Mark block as free
    fn mark_free(&mut self, addr: PhysAddr, order: usize) {
        self.clear_split(addr, order);
    }

    /// Get free frame count
    pub fn free_count(&self) -> usize {
        self.free_frames
    }

    /// Get total frame count
    pub fn total_count(&self) -> usize {
        self.total_frames
    }

    /// Get free memory in bytes
    pub fn free_memory(&self) -> u64 {
        (self.free_frames as u64) * PAGE_SIZE
    }

    /// Get total memory in bytes
    pub fn total_memory(&self) -> u64 {
        (self.total_frames as u64) * PAGE_SIZE
    }

    /// Get fragmentation info
    pub fn fragmentation_stats(&self) -> FragmentationStats {
        let mut stats = FragmentationStats::default();

        for (order, list) in self.free_lists.iter().enumerate() {
            let block_size = PAGE_SIZE << order;
            stats.blocks_per_order[order] = list.len();
            stats.bytes_per_order[order] = (list.len() as u64) * block_size;
        }

        stats.total_free_bytes = self.free_memory();
        stats.largest_free_order = self
            .free_lists
            .iter()
            .enumerate()
            .rev()
            .find(|(_, list)| !list.is_empty())
            .map(|(order, _)| order)
            .unwrap_or(0);

        stats
    }
}

impl Default for FrameAllocator {
    fn default() -> Self {
        Self::new()
    }
}

/// Fragmentation statistics
#[derive(Debug, Default)]
pub struct FragmentationStats {
    /// Number of free blocks at each order
    pub blocks_per_order: [usize; MAX_ORDER],
    /// Free bytes at each order
    pub bytes_per_order: [u64; MAX_ORDER],
    /// Total free bytes
    pub total_free_bytes: u64,
    /// Largest available order
    pub largest_free_order: usize,
}

impl FragmentationStats {
    /// Calculate fragmentation percentage (0 = no fragmentation, 100 = fully fragmented)
    pub fn fragmentation_percent(&self) -> u8 {
        if self.total_free_bytes == 0 {
            return 0;
        }

        // Ideal: all memory in largest possible blocks
        // Fragmented: memory scattered in small blocks
        let max_block_size = PAGE_SIZE << (MAX_ORDER - 1);
        let ideal_blocks = self.total_free_bytes / max_block_size;

        if ideal_blocks == 0 {
            // Less than one max block, check distribution
            let small_block_ratio =
                self.bytes_per_order[0..3].iter().sum::<u64>() as f64 / self.total_free_bytes as f64;
            return (small_block_ratio * 100.0) as u8;
        }

        let actual_large_blocks = self.blocks_per_order[MAX_ORDER - 1];
        let ratio = actual_large_blocks as f64 / ideal_blocks as f64;
        ((1.0 - ratio) * 100.0) as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buddy_calculation() {
        let mut allocator = FrameAllocator::new();
        allocator.base_addr = 0x1000_0000;

        // Order 0 (4KB) buddies
        let addr = PhysAddr::new(0x1000_0000);
        let buddy = allocator.buddy_of(addr, 0);
        assert_eq!(buddy.as_u64(), 0x1000_1000);

        // Order 1 (8KB) buddies
        let addr = PhysAddr::new(0x1000_0000);
        let buddy = allocator.buddy_of(addr, 1);
        assert_eq!(buddy.as_u64(), 0x1000_2000);
    }
}
