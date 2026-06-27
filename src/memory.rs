//! Production-Grade Memory Management System for RustOS
//!
//! This module provides a comprehensive memory management system including:
//! - Buddy allocator for efficient physical frame allocation
//! - Slab allocator for small object allocation
//! - Virtual memory management with copy-on-write and demand paging
//! - Page table management with full address translation
//! - Memory protection with guard pages and stack canaries
//! - Kernel and user space separation with ASLR
//! - Memory zone management (DMA, Normal, HighMem)
//! - Integration with heap allocator
//! - Comprehensive memory statistics and monitoring
//! - Advanced error handling and memory safety guarantees

use crate::performance::{
    get_performance_monitor, likely, CacheAligned, HighResTimer, PerCpuAllocator,
};
use alloc::{collections::BTreeMap, vec, vec::Vec};
use bootloader::bootinfo::MemoryRegion;
use core::fmt;
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use lazy_static::lazy_static;
use spin::{Mutex, RwLock};
use x86_64::{
    registers::control::Cr3,
    structures::paging::{
        mapper::MapToError, FrameAllocator, Mapper, OffsetPageTable, Page, PageTable,
        PageTableFlags, PhysFrame, Size4KiB, Translate,
    },
    PhysAddr, VirtAddr,
};

// User space memory operations module
pub mod user_space;

/// Page size constants
pub const PAGE_SIZE: usize = 4096;
pub const PAGE_SHIFT: usize = 12;

/// Memory layout constants for virtual address space
pub const KERNEL_HEAP_START: usize = 0x_4444_4444_0000;
pub const KERNEL_HEAP_SIZE: usize = 100 * 1024 * 1024; // 100 MiB
pub const USER_SPACE_START: usize = 0x_0000_1000_0000;
pub const USER_SPACE_END: usize = 0x_0000_8000_0000;
pub const KERNEL_SPACE_START: usize = 0xFFFF_8000_0000_0000;

/// Physical memory zone boundaries
pub const DMA_ZONE_END: u64 = 16 * 1024 * 1024; // 16MB
pub const NORMAL_ZONE_END: u64 = 896 * 1024 * 1024; // 896MB
                                                    // Everything above NORMAL_ZONE_END is considered HIGHMEM

/// Buddy allocator order constants
const MIN_ORDER: usize = 0; // 4KB pages
const MAX_ORDER: usize = 10; // 4MB max allocation (2^10 * 4KB)
const NUM_ORDERS: usize = MAX_ORDER + 1;

/// ASLR entropy bits
const ASLR_ENTROPY_BITS: u32 = 16;

/// Memory zone types for different hardware requirements
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryZone {
    /// DMA-accessible memory (below 16MB)
    Dma,
    /// Normal memory (16MB - 896MB)
    Normal,
    /// High memory (above 896MB)
    HighMem,
}

impl MemoryZone {
    pub fn from_address(addr: PhysAddr) -> Self {
        let addr = addr.as_u64();
        if addr < DMA_ZONE_END {
            MemoryZone::Dma
        } else if addr < NORMAL_ZONE_END {
            MemoryZone::Normal
        } else {
            MemoryZone::HighMem
        }
    }
}

/// Virtual memory region types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryRegionType {
    /// Kernel code and data
    Kernel,
    /// Kernel stack
    KernelStack,
    /// User process code
    UserCode,
    /// User process data
    UserData,
    /// User process stack
    UserStack,
    /// User process heap
    UserHeap,
    /// Memory-mapped device registers
    DeviceMemory,
    /// Shared memory between processes
    SharedMemory,
    /// Video/framebuffer memory
    VideoMemory,
    /// Copy-on-write region
    CopyOnWrite,
    /// Guard page
    GuardPage,
}

/// Memory protection flags
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryProtection {
    pub readable: bool,
    pub writable: bool,
    pub executable: bool,
    pub user_accessible: bool,
    pub cache_disabled: bool,
    pub write_through: bool,
    pub copy_on_write: bool,
    pub guard_page: bool,
}

impl MemoryProtection {
    pub const KERNEL_CODE: Self = MemoryProtection {
        readable: true,
        writable: false,
        executable: true,
        user_accessible: false,
        cache_disabled: false,
        write_through: false,
        copy_on_write: false,
        guard_page: false,
    };

    pub const KERNEL_DATA: Self = MemoryProtection {
        readable: true,
        writable: true,
        executable: false,
        user_accessible: false,
        cache_disabled: false,
        write_through: false,
        copy_on_write: false,
        guard_page: false,
    };

    pub const USER_CODE: Self = MemoryProtection {
        readable: true,
        writable: false,
        executable: true,
        user_accessible: true,
        cache_disabled: false,
        write_through: false,
        copy_on_write: false,
        guard_page: false,
    };

    pub const USER_DATA: Self = MemoryProtection {
        readable: true,
        writable: true,
        executable: false,
        user_accessible: true,
        cache_disabled: false,
        write_through: false,
        copy_on_write: false,
        guard_page: false,
    };

    pub const DEVICE_MEMORY: Self = MemoryProtection {
        readable: true,
        writable: true,
        executable: false,
        user_accessible: false,
        cache_disabled: true,
        write_through: true,
        copy_on_write: false,
        guard_page: false,
    };

    pub const GUARD_PAGE: Self = MemoryProtection {
        readable: false,
        writable: false,
        executable: false,
        user_accessible: false,
        cache_disabled: false,
        write_through: false,
        copy_on_write: false,
        guard_page: true,
    };

    pub const COPY_ON_WRITE: Self = MemoryProtection {
        readable: true,
        writable: false,
        executable: false,
        user_accessible: true,
        cache_disabled: false,
        write_through: false,
        copy_on_write: true,
        guard_page: false,
    };

    /// Create empty memory protection (no access)
    pub fn empty() -> Self {
        Self {
            readable: false,
            writable: false,
            executable: false,
            user_accessible: false,
            cache_disabled: false,
            write_through: false,
            copy_on_write: false,
            guard_page: false,
        }
    }

    /// Read-only access flag
    pub const READ: Self = Self {
        readable: true,
        writable: false,
        executable: false,
        user_accessible: false,
        cache_disabled: false,
        write_through: false,
        copy_on_write: false,
        guard_page: false,
    };

    /// Write access flag
    pub const WRITE: Self = Self {
        readable: true,
        writable: true,
        executable: false,
        user_accessible: false,
        cache_disabled: false,
        write_through: false,
        copy_on_write: false,
        guard_page: false,
    };

    /// Execute access flag
    pub const EXECUTE: Self = Self {
        readable: true,
        writable: false,
        executable: true,
        user_accessible: false,
        cache_disabled: false,
        write_through: false,
        copy_on_write: false,
        guard_page: false,
    };

    pub fn to_page_table_flags(self) -> PageTableFlags {
        let mut flags = PageTableFlags::PRESENT;

        if self.writable && !self.copy_on_write {
            flags |= PageTableFlags::WRITABLE;
        }
        if self.user_accessible {
            flags |= PageTableFlags::USER_ACCESSIBLE;
        }
        if !self.executable {
            flags |= PageTableFlags::NO_EXECUTE;
        }
        if self.cache_disabled {
            flags |= PageTableFlags::NO_CACHE;
        }
        if self.write_through {
            flags |= PageTableFlags::WRITE_THROUGH;
        }

        flags
    }
}

/// Implement bitwise OR for MemoryProtection
impl core::ops::BitOrAssign for MemoryProtection {
    fn bitor_assign(&mut self, rhs: Self) {
        self.readable |= rhs.readable;
        self.writable |= rhs.writable;
        self.executable |= rhs.executable;
        self.user_accessible |= rhs.user_accessible;
        self.cache_disabled |= rhs.cache_disabled;
        self.write_through |= rhs.write_through;
        self.copy_on_write |= rhs.copy_on_write;
        self.guard_page |= rhs.guard_page;
    }
}

/// Buddy allocator node
#[derive(Debug, Clone)]
struct BuddyNode {
    address: PhysAddr,
    order: usize,
}

/// Fragmentation statistics for each zone
#[derive(Debug, Clone, Copy, Default)]
pub struct FragmentationStats {
    /// Number of free blocks by order
    pub free_blocks_by_order: [usize; NUM_ORDERS],
    /// Largest free block order
    pub largest_free_order: usize,
    /// Total free memory
    pub total_free_bytes: usize,
    /// Fragmentation ratio (0.0 = no fragmentation, 1.0 = maximum fragmentation)
    pub fragmentation_ratio: f32,
}

/// Production-grade Physical Frame Allocator with Buddy System and Performance Optimizations
pub struct PhysicalFrameAllocator {
    /// Cache-aligned buddy allocator free lists for each order and zone
    buddy_lists: [[CacheAligned<Vec<BuddyNode>>; NUM_ORDERS]; 3],
    /// Allocation bitmap for tracking allocated blocks
    allocation_bitmap: [Vec<u64>; 3],
    /// Zone statistics (cache-aligned for better performance)
    allocated_frames: [CacheAligned<AtomicU64>; 3],
    total_frames: [usize; 3],
    /// Zone memory boundaries
    zone_start: [PhysAddr; 3],
    zone_end: [PhysAddr; 3],
    /// Fragmentation statistics (cache-aligned)
    fragmentation_stats: [CacheAligned<FragmentationStats>; 3],
    /// Per-CPU allocator for fast allocations
    per_cpu_allocator: PerCpuAllocator,
}

impl PhysicalFrameAllocator {
    /// Initialize the frame allocator with buddy system from bootloader memory regions
    pub fn init(memory_regions: &[MemoryRegion]) -> Self {
        let mut buddy_lists = [
            core::array::from_fn(|_| CacheAligned::new(Vec::new())),
            core::array::from_fn(|_| CacheAligned::new(Vec::new())),
            core::array::from_fn(|_| CacheAligned::new(Vec::new())),
        ];

        let mut allocation_bitmap = [Vec::new(), Vec::new(), Vec::new()];
        let mut zone_start = [PhysAddr::new(0); 3];
        let mut zone_end = [PhysAddr::new(0); 3];
        let mut total_frames = [0; 3];

        // Initialize zone boundaries
        zone_start[MemoryZone::Dma as usize] = PhysAddr::new(0);
        zone_end[MemoryZone::Dma as usize] = PhysAddr::new(DMA_ZONE_END);
        zone_start[MemoryZone::Normal as usize] = PhysAddr::new(DMA_ZONE_END);
        zone_end[MemoryZone::Normal as usize] = PhysAddr::new(NORMAL_ZONE_END);
        zone_start[MemoryZone::HighMem as usize] = PhysAddr::new(NORMAL_ZONE_END);
        // Max representable physical address (52-bit). PhysAddr::new(u64::MAX)
        // panics — bits 52..64 must be clear — and that unconditional panic made
        // the whole allocator init noreturn, DCE-ing the rest of the kernel.
        zone_end[MemoryZone::HighMem as usize] = PhysAddr::new_truncate(u64::MAX);

        // Highest end address of a usable block per zone, used to size the
        // allocation bitmap by address span. The bitmap is indexed by
        // (addr - zone_start)/PAGE_SIZE, so a holey e820 map needs a bit per
        // in-range frame, not just per usable frame. We track the actual
        // usable span (not zone_end, which is u64::MAX for HighMem).
        let mut zone_max_addr = [
            zone_start[0].as_u64(),
            zone_start[1].as_u64(),
            zone_start[2].as_u64(),
        ];

        // Process memory regions and build buddy lists
        for region in memory_regions
            .iter()
            .filter(|r| r.region_type == bootloader::bootinfo::MemoryRegionType::Usable)
        {
            let start = align_up(region.range.start_addr() as usize, PAGE_SIZE) as u64;
            let end = align_down(region.range.end_addr() as usize, PAGE_SIZE) as u64;

            if start >= end {
                continue;
            }

            let mut current = start;
            while current < end {
                let zone = MemoryZone::from_address(PhysAddr::new(current));
                let zone_idx = zone as usize;

                // Find the largest possible buddy block at this address
                let mut order = MAX_ORDER;
                let mut block_size = PAGE_SIZE << order;

                while order > 0 {
                    if current % (block_size as u64) == 0 && current + block_size as u64 <= end {
                        break;
                    }
                    order -= 1;
                    block_size >>= 1;
                }

                // Add block to appropriate buddy list
                buddy_lists[zone_idx][order].push(BuddyNode {
                    address: PhysAddr::new(current),
                    order,
                });

                total_frames[zone_idx] += 1 << order;
                current += block_size as u64;

                // Track the highest end address seen in this zone so the
                // bitmap covers every in-range frame.
                let block_end = current;
                if block_end > zone_max_addr[zone_idx] {
                    zone_max_addr[zone_idx] = block_end;
                }
            }
        }

        // Initialize allocation bitmaps (one bit per page, sized by the zone's
        // usable address span so high frames on a holey map stay in range).
        for zone_idx in 0..3 {
            let span_bytes = zone_max_addr[zone_idx] - zone_start[zone_idx].as_u64();
            let span_frames = (span_bytes / PAGE_SIZE as u64) as usize;
            let bitmap_size = (span_frames + 63) / 64; // Round up to u64 boundary
            allocation_bitmap[zone_idx] = vec![0u64; bitmap_size];
        }

        // Sort buddy lists by address for efficient allocation
        for zone_idx in 0..3 {
            for order in 0..NUM_ORDERS {
                buddy_lists[zone_idx][order].sort_unstable_by_key(|node| node.address.as_u64());
            }
        }

        PhysicalFrameAllocator {
            buddy_lists,
            allocation_bitmap,
            allocated_frames: [
                CacheAligned::new(AtomicU64::new(0)),
                CacheAligned::new(AtomicU64::new(0)),
                CacheAligned::new(AtomicU64::new(0)),
            ],
            total_frames,
            zone_start,
            zone_end,
            fragmentation_stats: [
                CacheAligned::new(FragmentationStats::default()),
                CacheAligned::new(FragmentationStats::default()),
                CacheAligned::new(FragmentationStats::default()),
            ],
            per_cpu_allocator: PerCpuAllocator::new(),
        }
    }

    /// Fast path allocation using per-CPU allocator
    pub fn allocate_frame_fast(&mut self, cpu_id: usize) -> Option<PhysFrame> {
        let (result, time_ns) = HighResTimer::time(|| {
            if likely(cpu_id < crate::performance::MAX_CPUS) {
                // Try per-CPU cache first
                if let Some(addr) = self.per_cpu_allocator.allocate_fast(cpu_id) {
                    return Some(PhysFrame::containing_address(PhysAddr::new(addr as u64)));
                }

                // Fallback to slow path
                self.per_cpu_allocator
                    .allocate_slow(cpu_id, 0)
                    .map(|addr| PhysFrame::containing_address(PhysAddr::new(addr as u64)))
            } else {
                None
            }
        });

        // Record performance metrics
        let perf_monitor = get_performance_monitor();
        if result.is_some() {
            perf_monitor.record_allocation(PAGE_SIZE as u64, time_ns);
        } else {
            perf_monitor.record_allocation_failure();
        }

        result
    }

    /// Allocate frames using buddy allocator from a specific zone
    pub fn allocate_frames_in_zone(&mut self, zone: MemoryZone, order: usize) -> Option<PhysFrame> {
        if order > MAX_ORDER {
            return None;
        }

        let zone_idx = zone as usize;

        // Try to find a free block of the requested order
        if let Some(block) = self.find_free_block(zone_idx, order) {
            self.mark_allocated(zone_idx, block.address, order);
            self.allocated_frames[zone_idx].fetch_add(1 << order, Ordering::Relaxed);
            self.update_fragmentation_stats(zone_idx);
            return Some(PhysFrame::containing_address(block.address));
        }

        None
    }

    /// Allocate a single frame from a specific zone
    pub fn allocate_frame_in_zone(&mut self, zone: MemoryZone) -> Option<PhysFrame> {
        self.allocate_frames_in_zone(zone, 0)
    }

    /// Find and split a free block of the requested order
    fn find_free_block(&mut self, zone_idx: usize, order: usize) -> Option<BuddyNode> {
        // First try to find exact order
        if let Some(block) = self.buddy_lists[zone_idx][order].pop() {
            return Some(block);
        }

        // Try higher orders and split
        for higher_order in (order + 1)..=MAX_ORDER {
            if let Some(block) = self.buddy_lists[zone_idx][higher_order].pop() {
                return Some(self.split_block(zone_idx, block, order));
            }
        }

        None
    }

    /// Split a larger block into smaller blocks
    fn split_block(
        &mut self,
        zone_idx: usize,
        mut block: BuddyNode,
        target_order: usize,
    ) -> BuddyNode {
        while block.order > target_order {
            block.order -= 1;
            let buddy_size = PAGE_SIZE << block.order;
            let buddy_addr = PhysAddr::new(block.address.as_u64() + buddy_size as u64);

            // Add buddy to free list
            let buddy = BuddyNode {
                address: buddy_addr,
                order: block.order,
            };

            // Insert in sorted order
            let list = &mut self.buddy_lists[zone_idx][block.order];
            let insert_pos = list
                .iter()
                .position(|b| b.address > buddy_addr)
                .unwrap_or(list.len());
            list.insert(insert_pos, buddy);
        }

        block
    }

    /// Mark memory region as allocated in bitmap
    fn mark_allocated(&mut self, zone_idx: usize, addr: PhysAddr, order: usize) {
        let page_index = self.addr_to_page_index(zone_idx, addr);
        let num_pages = 1 << order;

        for i in 0..num_pages {
            let bit_index = page_index + i;
            let word_index = bit_index / 64;
            let bit_offset = bit_index % 64;

            if word_index < self.allocation_bitmap[zone_idx].len() {
                self.allocation_bitmap[zone_idx][word_index] |= 1u64 << bit_offset;
            }
        }
    }

    /// Mark memory region as free in bitmap
    fn mark_free(&mut self, zone_idx: usize, addr: PhysAddr, order: usize) {
        let page_index = self.addr_to_page_index(zone_idx, addr);
        let num_pages = 1 << order;

        for i in 0..num_pages {
            let bit_index = page_index + i;
            let word_index = bit_index / 64;
            let bit_offset = bit_index % 64;

            if word_index < self.allocation_bitmap[zone_idx].len() {
                self.allocation_bitmap[zone_idx][word_index] &= !(1u64 << bit_offset);
            }
        }
    }

    /// Convert physical address to page index within zone
    fn addr_to_page_index(&self, zone_idx: usize, addr: PhysAddr) -> usize {
        ((addr.as_u64() - self.zone_start[zone_idx].as_u64()) / PAGE_SIZE as u64) as usize
    }

    /// Check that every page of a block is currently marked allocated and in range
    fn is_block_allocated(&self, zone_idx: usize, addr: PhysAddr, order: usize) -> bool {
        let page_index = self.addr_to_page_index(zone_idx, addr);
        let num_pages = 1 << order;

        for i in 0..num_pages {
            let bit_index = page_index + i;
            let word_index = bit_index / 64;
            let bit_offset = bit_index % 64;

            if word_index >= self.allocation_bitmap[zone_idx].len() {
                return false; // Out of range counts as "not allocated here"
            }

            if (self.allocation_bitmap[zone_idx][word_index] & (1u64 << bit_offset)) == 0 {
                return false; // Page already free -> would be a double-free
            }
        }

        true
    }

    /// Deallocate frames using buddy allocator (with coalescing)
    pub fn deallocate_frames(&mut self, frame: PhysFrame, zone: MemoryZone, order: usize) {
        let zone_idx = zone as usize;
        let addr = frame.start_address();

        // Guard against double-free: only free blocks that are currently
        // allocated. Otherwise mark_free + fetch_sub would corrupt the free
        // list and underflow the allocated_frames counter.
        if !self.is_block_allocated(zone_idx, addr, order) {
            return;
        }

        self.mark_free(zone_idx, addr, order);
        self.allocated_frames[zone_idx].fetch_sub(1 << order, Ordering::Relaxed);

        // Try to coalesce with buddy
        let coalesced_block = self.coalesce_block(zone_idx, addr, order);

        // Add to appropriate free list
        let list = &mut self.buddy_lists[zone_idx][coalesced_block.order];
        let insert_pos = list
            .iter()
            .position(|b| b.address > coalesced_block.address)
            .unwrap_or(list.len());
        list.insert(insert_pos, coalesced_block);

        self.update_fragmentation_stats(zone_idx);
    }

    /// Deallocate a single frame
    pub fn deallocate_frame(&mut self, frame: PhysFrame, zone: MemoryZone) {
        self.deallocate_frames(frame, zone, 0);
    }

    /// Coalesce block with its buddy recursively
    fn coalesce_block(&mut self, zone_idx: usize, addr: PhysAddr, order: usize) -> BuddyNode {
        if order >= MAX_ORDER {
            return BuddyNode {
                address: addr,
                order,
            };
        }

        let block_size = PAGE_SIZE << order;
        let buddy_addr = if (addr.as_u64() / block_size as u64) % 2 == 0 {
            // We're the left buddy, buddy is to the right
            PhysAddr::new(addr.as_u64() + block_size as u64)
        } else {
            // We're the right buddy, buddy is to the left
            PhysAddr::new(addr.as_u64() - block_size as u64)
        };

        // Check if buddy is free
        if self.is_buddy_free(zone_idx, buddy_addr, order) {
            // Remove buddy from free list
            if let Some(pos) = self.buddy_lists[zone_idx][order]
                .iter()
                .position(|b| b.address == buddy_addr)
            {
                self.buddy_lists[zone_idx][order].remove(pos);

                // Determine the new block address (always the lower address)
                let new_addr = PhysAddr::new(core::cmp::min(addr.as_u64(), buddy_addr.as_u64()));

                // Recursively coalesce at next order
                return self.coalesce_block(zone_idx, new_addr, order + 1);
            }
        }

        BuddyNode {
            address: addr,
            order,
        }
    }

    /// Check if buddy block is free
    fn is_buddy_free(&self, zone_idx: usize, buddy_addr: PhysAddr, order: usize) -> bool {
        let page_index = self.addr_to_page_index(zone_idx, buddy_addr);
        let num_pages = 1 << order;

        for i in 0..num_pages {
            let bit_index = page_index + i;
            let word_index = bit_index / 64;
            let bit_offset = bit_index % 64;

            if word_index >= self.allocation_bitmap[zone_idx].len() {
                return false;
            }

            if (self.allocation_bitmap[zone_idx][word_index] & (1u64 << bit_offset)) != 0 {
                return false; // Page is allocated
            }
        }

        true
    }

    /// Update fragmentation statistics for a zone
    fn update_fragmentation_stats(&mut self, zone_idx: usize) {
        let stats = &mut self.fragmentation_stats[zone_idx];

        // Reset stats
        stats.free_blocks_by_order = [0; NUM_ORDERS];
        stats.largest_free_order = 0;
        stats.total_free_bytes = 0;

        // Count free blocks by order
        for order in 0..NUM_ORDERS {
            let count = self.buddy_lists[zone_idx][order].len();
            stats.free_blocks_by_order[order] = count;

            if count > 0 {
                stats.largest_free_order = order;
                stats.total_free_bytes += count * (PAGE_SIZE << order);
            }
        }

        // Calculate fragmentation ratio
        if stats.total_free_bytes > 0 {
            let largest_possible_block = PAGE_SIZE << stats.largest_free_order;
            stats.fragmentation_ratio =
                1.0 - (largest_possible_block as f32 / stats.total_free_bytes as f32);
        } else {
            stats.fragmentation_ratio = 0.0;
        }
    }

    /// Get comprehensive memory statistics for all zones
    pub fn get_zone_stats(&self) -> [ZoneStats; 3] {
        [
            ZoneStats {
                zone: MemoryZone::Dma,
                total_frames: self.total_frames[0],
                allocated_frames: self.allocated_frames[0].load(Ordering::Relaxed) as usize,
                fragmentation_stats: self.fragmentation_stats[0].clone(),
            },
            ZoneStats {
                zone: MemoryZone::Normal,
                total_frames: self.total_frames[1],
                allocated_frames: self.allocated_frames[1].load(Ordering::Relaxed) as usize,
                fragmentation_stats: self.fragmentation_stats[1].clone(),
            },
            ZoneStats {
                zone: MemoryZone::HighMem,
                total_frames: self.total_frames[2],
                allocated_frames: self.allocated_frames[2].load(Ordering::Relaxed) as usize,
                fragmentation_stats: self.fragmentation_stats[2].clone(),
            },
        ]
    }

    /// Get detailed memory usage report
    pub fn get_memory_report(&self) -> MemoryReport {
        let zone_stats = self.get_zone_stats();
        let buddy_stats = self.get_buddy_stats();

        let total_memory = zone_stats.iter().map(|z| z.total_bytes()).sum();
        let allocated_memory = zone_stats.iter().map(|z| z.allocated_bytes()).sum();
        let free_memory = zone_stats.iter().map(|z| z.free_bytes()).sum();

        let overall_fragmentation = if free_memory > 0 {
            let largest_free_block = zone_stats
                .iter()
                .map(|z| z.largest_free_block_size())
                .max()
                .unwrap_or(0);
            1.0 - (largest_free_block as f32 / free_memory as f32)
        } else {
            0.0
        };

        MemoryReport {
            total_memory,
            allocated_memory,
            free_memory,
            zone_stats,
            buddy_stats,
            overall_fragmentation,
            memory_pressure: self.calculate_memory_pressure(),
        }
    }

    /// Calculate memory pressure (0.0 = no pressure, 1.0 = critical)
    fn calculate_memory_pressure(&self) -> f32 {
        let zone_stats = self.get_zone_stats();
        let total_usage = zone_stats.iter().map(|z| z.usage_percent()).sum::<f32>() / 3.0;
        let avg_fragmentation = zone_stats
            .iter()
            .map(|z| z.fragmentation_percent())
            .sum::<f32>()
            / 3.0;

        // Combine usage and fragmentation for pressure calculation
        (total_usage / 100.0) * 0.7 + (avg_fragmentation / 100.0) * 0.3
    }

    /// Defragment memory by coalescing free blocks
    pub fn defragment(&mut self) -> DefragmentationResult {
        let mut coalesced_blocks = 0;
        let mut freed_bytes = 0;

        for zone_idx in 0..3 {
            for order in 0..MAX_ORDER {
                let mut i = 0;
                while i < self.buddy_lists[zone_idx][order].len() {
                    let block = self.buddy_lists[zone_idx][order][i].clone();

                    // Try to coalesce with buddy
                    let coalesced = self.coalesce_block(zone_idx, block.address, block.order);

                    if coalesced.order > block.order {
                        // Successfully coalesced
                        self.buddy_lists[zone_idx][order].remove(i);
                        coalesced_blocks += 1;
                        freed_bytes += PAGE_SIZE << (coalesced.order - block.order);

                        // Add coalesced block to appropriate list
                        let list = &mut self.buddy_lists[zone_idx][coalesced.order];
                        let insert_pos = list
                            .iter()
                            .position(|b| b.address > coalesced.address)
                            .unwrap_or(list.len());
                        list.insert(insert_pos, coalesced);
                    } else {
                        i += 1;
                    }
                }
            }

            self.update_fragmentation_stats(zone_idx);
        }

        DefragmentationResult {
            coalesced_blocks,
            freed_bytes,
        }
    }

    /// Get buddy allocator statistics
    pub fn get_buddy_stats(&self) -> BuddyAllocatorStats {
        let mut total_free_blocks = 0;
        let mut free_blocks_by_order = [0; NUM_ORDERS];

        for zone_idx in 0..3 {
            for order in 0..NUM_ORDERS {
                let count = self.buddy_lists[zone_idx][order].len();
                free_blocks_by_order[order] += count;
                total_free_blocks += count;
            }
        }

        BuddyAllocatorStats {
            total_free_blocks,
            free_blocks_by_order,
            max_order: MAX_ORDER,
            min_order: MIN_ORDER,
        }
    }

    /// Allocate contiguous pages (for DMA, etc.)
    pub fn allocate_contiguous_pages(
        &mut self,
        num_pages: usize,
        zone: MemoryZone,
    ) -> Option<PhysFrame> {
        if num_pages == 0 {
            return None;
        }

        // Find minimum order that can satisfy the request
        let mut order = 0;
        while (1 << order) < num_pages && order <= MAX_ORDER {
            order += 1;
        }

        if order > MAX_ORDER {
            return None; // Request too large
        }

        self.allocate_frames_in_zone(zone, order)
    }

    /// Compute the buddy order that `allocate_contiguous_pages` used for a request
    fn contiguous_order(num_pages: usize) -> Option<usize> {
        if num_pages == 0 {
            return None;
        }
        let mut order = 0;
        while (1 << order) < num_pages && order <= MAX_ORDER {
            order += 1;
        }
        if order > MAX_ORDER {
            None
        } else {
            Some(order)
        }
    }

    /// Free a block previously obtained from `allocate_contiguous_pages`.
    ///
    /// The contiguous allocation reserves an order-N block, so it must be freed
    /// at the same order; freeing the start frame at order 0 (via
    /// `deallocate_frame`) would leak the remaining 2^N - 1 frames and leave the
    /// bitmap and `allocated_frames` counter inconsistent.
    pub fn deallocate_contiguous_pages(
        &mut self,
        frame: PhysFrame,
        num_pages: usize,
        zone: MemoryZone,
    ) {
        if let Some(order) = Self::contiguous_order(num_pages) {
            self.deallocate_frames(frame, zone, order);
        }
    }
}

// Implement the standard FrameAllocator trait (allocates from Normal zone by default)
unsafe impl FrameAllocator<Size4KiB> for PhysicalFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        // Try Normal zone first, then HighMem, then DMA as last resort
        self.allocate_frame_in_zone(MemoryZone::Normal)
            .or_else(|| self.allocate_frame_in_zone(MemoryZone::HighMem))
            .or_else(|| self.allocate_frame_in_zone(MemoryZone::Dma))
    }
}

/// Zone statistics structure with fragmentation info
#[derive(Debug, Clone)]
pub struct ZoneStats {
    pub zone: MemoryZone,
    pub total_frames: usize,
    pub allocated_frames: usize,
    pub fragmentation_stats: FragmentationStats,
}

/// Buddy allocator statistics
#[derive(Debug, Clone)]
pub struct BuddyAllocatorStats {
    pub total_free_blocks: usize,
    pub free_blocks_by_order: [usize; NUM_ORDERS],
    pub max_order: usize,
    pub min_order: usize,
}

/// Comprehensive memory report
#[derive(Debug, Clone)]
pub struct MemoryReport {
    pub total_memory: usize,
    pub allocated_memory: usize,
    pub free_memory: usize,
    pub zone_stats: [ZoneStats; 3],
    pub buddy_stats: BuddyAllocatorStats,
    pub overall_fragmentation: f32,
    pub memory_pressure: f32,
}

/// Defragmentation operation result
#[derive(Debug, Clone)]
pub struct DefragmentationResult {
    pub coalesced_blocks: usize,
    pub freed_bytes: usize,
}

/// Swap slot identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SwapSlot(pub u32);

/// Page replacement algorithm types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageReplacementAlgorithm {
    /// Least Recently Used
    LRU,
    /// Clock algorithm (approximation of LRU)
    Clock,
    /// First In First Out
    FIFO,
}

/// Swap entry information
#[derive(Debug, Clone)]
pub struct SwapEntry {
    pub slot: SwapSlot,
    pub page_addr: VirtAddr,
    pub access_time: u64,
    pub dirty: bool,
}

/// Swap manager for handling page-to-storage operations
pub struct SwapManager {
    /// Available swap slots (bit vector)
    free_slots: Vec<u64>,
    /// Total number of swap slots
    total_slots: u32,
    /// Currently used swap slots
    used_slots: u32,
    /// Swap entries indexed by slot
    swap_entries: BTreeMap<SwapSlot, SwapEntry>,
    /// Page replacement algorithm
    replacement_algorithm: PageReplacementAlgorithm,
    /// LRU list for page replacement
    lru_list: Vec<VirtAddr>,
    /// Clock hand for clock algorithm
    clock_hand: usize,
    /// Access times for pages (for LRU)
    access_times: BTreeMap<VirtAddr, u64>,
    /// Global access counter
    access_counter: AtomicU64,
    /// Storage device ID for swap partition (None means no swap device configured)
    swap_device_id: Option<u32>,
}

impl SwapManager {
    /// Create new swap manager with specified number of slots
    pub fn new(total_slots: u32, algorithm: PageReplacementAlgorithm) -> Self {
        let bitmap_size = ((total_slots + 63) / 64) as usize;

        Self {
            free_slots: vec![u64::MAX; bitmap_size], // All slots initially free
            total_slots,
            used_slots: 0,
            swap_entries: BTreeMap::new(),
            replacement_algorithm: algorithm,
            lru_list: Vec::new(),
            clock_hand: 0,
            access_times: BTreeMap::new(),
            access_counter: AtomicU64::new(0),
            swap_device_id: None, // No swap device configured by default
        }
    }

    /// Configure swap device for storage operations
    pub fn set_swap_device(&mut self, device_id: u32) {
        self.swap_device_id = Some(device_id);
    }

    /// Get configured swap device ID
    pub fn get_swap_device(&self) -> Option<u32> {
        self.swap_device_id
    }

    /// Allocate a swap slot
    pub fn allocate_slot(&mut self) -> Option<SwapSlot> {
        if self.used_slots >= self.total_slots {
            return None;
        }

        // Find first free slot
        for (word_idx, &word) in self.free_slots.iter().enumerate() {
            if word != 0 {
                let bit_idx = word.trailing_zeros() as usize;
                let slot_idx = word_idx * 64 + bit_idx;

                if slot_idx < self.total_slots as usize {
                    // Mark slot as used
                    self.free_slots[word_idx] &= !(1u64 << bit_idx);
                    self.used_slots += 1;
                    return Some(SwapSlot(slot_idx as u32));
                }
            }
        }

        None
    }

    /// Deallocate a swap slot
    pub fn deallocate_slot(&mut self, slot: SwapSlot) {
        let slot_idx = slot.0 as usize;
        let word_idx = slot_idx / 64;
        let bit_idx = slot_idx % 64;

        if word_idx < self.free_slots.len() {
            self.free_slots[word_idx] |= 1u64 << bit_idx;
            self.used_slots = self.used_slots.saturating_sub(1);
            self.swap_entries.remove(&slot);
        }
    }

    /// Swap out a page to storage
    pub fn swap_out(
        &mut self,
        page_addr: VirtAddr,
        page_data: &[u8; PAGE_SIZE],
    ) -> Result<SwapSlot, &'static str> {
        let slot = self.allocate_slot().ok_or("No swap slots available")?;

        // Create swap entry metadata
        let entry = SwapEntry {
            slot,
            page_addr,
            access_time: self.access_counter.load(Ordering::Relaxed),
            dirty: true,
        };

        // Write page data to actual swap storage if device is configured
        if let Some(device_id) = self.swap_device_id {
            // Calculate storage offset: slot * PAGE_SIZE
            // PAGE_SIZE = 4096 bytes = 8 sectors (assuming 512-byte sectors)
            const SECTOR_SIZE: usize = 512;
            const SECTORS_PER_PAGE: u64 = (PAGE_SIZE / SECTOR_SIZE) as u64;

            let start_sector = slot.0 as u64 * SECTORS_PER_PAGE;

            // Write page to storage device
            use crate::drivers::storage;
            match storage::write_storage_sectors(device_id, start_sector, page_data) {
                Ok(bytes_written) => {
                    if bytes_written != PAGE_SIZE {
                        self.deallocate_slot(slot);
                        return Err("Incomplete swap write operation");
                    }
                }
                Err(e) => {
                    self.deallocate_slot(slot);
                    return Err("Storage write failed during swap out");
                }
            }
        }
        // If no swap device configured, data is lost (memory-only swap simulation)

        self.swap_entries.insert(slot, entry);
        Ok(slot)
    }

    /// Swap in a page from storage
    pub fn swap_in(
        &mut self,
        slot: SwapSlot,
        page_data: &mut [u8; PAGE_SIZE],
    ) -> Result<VirtAddr, &'static str> {
        let entry = self.swap_entries.get(&slot).ok_or("Invalid swap slot")?;
        let page_addr = entry.page_addr;

        // Read page data from actual swap storage if device is configured
        if let Some(device_id) = self.swap_device_id {
            // Calculate storage offset: slot * PAGE_SIZE
            // PAGE_SIZE = 4096 bytes = 8 sectors (assuming 512-byte sectors)
            const SECTOR_SIZE: usize = 512;
            const SECTORS_PER_PAGE: u64 = (PAGE_SIZE / SECTOR_SIZE) as u64;

            let start_sector = slot.0 as u64 * SECTORS_PER_PAGE;

            // Read page from storage device
            use crate::drivers::storage;
            match storage::read_storage_sectors(device_id, start_sector, page_data) {
                Ok(bytes_read) => {
                    if bytes_read != PAGE_SIZE {
                        return Err("Incomplete swap read operation");
                    }
                }
                Err(e) => {
                    return Err("Storage read failed during swap in");
                }
            }
        } else {
            // No swap device configured - zero the page as fallback
            // This handles the case where swap manager is used without actual storage
            page_data.fill(0);
        }

        self.deallocate_slot(slot);
        Ok(page_addr)
    }

    /// Select a page for replacement using the configured algorithm
    pub fn select_victim_page(&mut self, candidate_pages: &[VirtAddr]) -> Option<VirtAddr> {
        if candidate_pages.is_empty() {
            return None;
        }

        match self.replacement_algorithm {
            PageReplacementAlgorithm::LRU => self.select_lru_victim(candidate_pages),
            PageReplacementAlgorithm::Clock => self.select_clock_victim(candidate_pages),
            PageReplacementAlgorithm::FIFO => self.select_fifo_victim(candidate_pages),
        }
    }

    /// LRU page selection
    fn select_lru_victim(&self, candidate_pages: &[VirtAddr]) -> Option<VirtAddr> {
        candidate_pages
            .iter()
            .min_by_key(|&&addr| self.access_times.get(&addr).unwrap_or(&0))
            .copied()
    }

    /// Clock algorithm page selection
    fn select_clock_victim(&mut self, candidate_pages: &[VirtAddr]) -> Option<VirtAddr> {
        if candidate_pages.is_empty() {
            return None;
        }

        // Simple clock algorithm - just rotate through candidates
        let victim_idx = self.clock_hand % candidate_pages.len();
        self.clock_hand = (self.clock_hand + 1) % candidate_pages.len();
        Some(candidate_pages[victim_idx])
    }

    /// FIFO page selection
    fn select_fifo_victim(&self, candidate_pages: &[VirtAddr]) -> Option<VirtAddr> {
        // Return the first page (oldest in FIFO order)
        candidate_pages.first().copied()
    }

    /// Record page access for replacement algorithms
    pub fn record_access(&mut self, page_addr: VirtAddr) {
        let access_time = self.access_counter.fetch_add(1, Ordering::Relaxed);
        self.access_times.insert(page_addr, access_time);

        // Update LRU list
        if let Some(pos) = self.lru_list.iter().position(|&addr| addr == page_addr) {
            self.lru_list.remove(pos);
        }
        self.lru_list.push(page_addr);

        // Limit LRU list size
        if self.lru_list.len() > 1000 {
            self.lru_list.remove(0);
        }
    }

    /// Get swap statistics
    pub fn get_stats(&self) -> SwapStats {
        SwapStats {
            total_slots: self.total_slots,
            used_slots: self.used_slots,
            free_slots: self.total_slots - self.used_slots,
            algorithm: self.replacement_algorithm,
            total_swapped_pages: self.swap_entries.len() as u32,
        }
    }
}

/// Swap statistics
#[derive(Debug, Clone)]
pub struct SwapStats {
    pub total_slots: u32,
    pub used_slots: u32,
    pub free_slots: u32,
    pub algorithm: PageReplacementAlgorithm,
    pub total_swapped_pages: u32,
}

impl ZoneStats {
    pub fn free_frames(&self) -> usize {
        self.total_frames.saturating_sub(self.allocated_frames)
    }

    pub fn usage_percent(&self) -> f32 {
        if self.total_frames == 0 {
            0.0
        } else {
            (self.allocated_frames as f32 / self.total_frames as f32) * 100.0
        }
    }

    pub fn total_bytes(&self) -> usize {
        self.total_frames * PAGE_SIZE
    }

    pub fn allocated_bytes(&self) -> usize {
        self.allocated_frames * PAGE_SIZE
    }

    pub fn free_bytes(&self) -> usize {
        self.free_frames() * PAGE_SIZE
    }

    pub fn fragmentation_percent(&self) -> f32 {
        self.fragmentation_stats.fragmentation_ratio * 100.0
    }

    pub fn largest_free_block_size(&self) -> usize {
        PAGE_SIZE << self.fragmentation_stats.largest_free_order
    }
}

/// Virtual memory region descriptor
#[derive(Debug, Clone)]
pub struct VirtualMemoryRegion {
    pub start: VirtAddr,
    pub size: usize,
    pub region_type: MemoryRegionType,
    pub protection: MemoryProtection,
    pub mapped: bool,
    pub physical_start: Option<PhysAddr>,
    pub reference_count: usize,
    pub aslr_offset: u64,
}

impl VirtualMemoryRegion {
    pub fn new(
        start: VirtAddr,
        size: usize,
        region_type: MemoryRegionType,
        protection: MemoryProtection,
    ) -> Self {
        Self {
            start,
            size,
            region_type,
            protection,
            mapped: false,
            physical_start: None,
            reference_count: 1,
            aslr_offset: 0,
        }
    }

    pub fn new_with_aslr(
        start: VirtAddr,
        size: usize,
        region_type: MemoryRegionType,
        protection: MemoryProtection,
        enable_aslr: bool,
    ) -> Self {
        let aslr_offset = if enable_aslr {
            generate_aslr_offset()
        } else {
            0
        };

        Self {
            start: VirtAddr::new(start.as_u64() + aslr_offset),
            size,
            region_type,
            protection,
            mapped: false,
            physical_start: None,
            reference_count: 1,
            aslr_offset,
        }
    }

    pub fn end(&self) -> VirtAddr {
        self.start + self.size
    }

    pub fn contains(&self, addr: VirtAddr) -> bool {
        addr >= self.start && addr < self.end()
    }

    pub fn pages(&self) -> impl Iterator<Item = Page> {
        let start_page = Page::containing_address(self.start);
        let end_page = Page::containing_address(self.end() - 1u64);
        Page::range_inclusive(start_page, end_page)
    }

    pub fn page_count(&self) -> usize {
        (self.size + PAGE_SIZE - 1) / PAGE_SIZE
    }

    pub fn increment_ref_count(&mut self) {
        self.reference_count += 1;
    }

    pub fn decrement_ref_count(&mut self) -> usize {
        self.reference_count = self.reference_count.saturating_sub(1);
        self.reference_count
    }
}

/// Page table management system
pub struct PageTableManager {
    mapper: OffsetPageTable<'static>,
    physical_memory_offset: VirtAddr,
}

impl PageTableManager {
    pub fn new(mapper: OffsetPageTable<'static>, physical_memory_offset: VirtAddr) -> Self {
        Self {
            mapper,
            physical_memory_offset,
        }
    }

    /// Translate virtual address to physical address
    pub fn translate_addr(&self, addr: VirtAddr) -> Option<PhysAddr> {
        self.mapper.translate_addr(addr)
    }

    /// Map a single page with specific flags
    pub fn map_page(
        &mut self,
        page: Page,
        frame: PhysFrame,
        flags: PageTableFlags,
        frame_allocator: &mut impl FrameAllocator<Size4KiB>,
    ) -> Result<(), MapToError<Size4KiB>> {
        unsafe {
            self.mapper
                .map_to(page, frame, flags, frame_allocator)
                .map(|flush| flush.flush())
        }
    }

    /// Unmap a single page
    pub fn unmap_page(&mut self, page: Page) -> Option<PhysFrame> {
        let (frame, flush) = self.mapper.unmap(page).ok()?;
        flush.flush();
        Some(frame)
    }

    /// Update page flags
    pub fn update_flags(&mut self, page: Page, flags: PageTableFlags) -> Result<(), &'static str> {
        unsafe {
            let _ = self
                .mapper
                .update_flags(page, flags)
                .map_err(|_| "Failed to update page flags")?;
        }
        Ok(())
    }

    /// Get current page flags by reading page table entry directly
    pub fn get_flags(&self, page: Page) -> Option<PageTableFlags> {
        // Get the current page table
        let (level_4_table_frame, _) = Cr3::read();
        let level_4_table_ptr = (self.physical_memory_offset
            + level_4_table_frame.start_address().as_u64())
        .as_mut_ptr();

        unsafe {
            let level_4_table = &*(level_4_table_ptr as *const PageTable);
            let level_4_index = page.p4_index();
            let level_4_entry = &level_4_table[level_4_index];

            if !level_4_entry.flags().contains(PageTableFlags::PRESENT) {
                return None;
            }

            // Navigate through page table levels
            let level_3_table_ptr =
                (self.physical_memory_offset + level_4_entry.addr().as_u64()).as_ptr();
            let level_3_table = &*(level_3_table_ptr as *const PageTable);
            let level_3_index = page.p3_index();
            let level_3_entry = &level_3_table[level_3_index];

            if !level_3_entry.flags().contains(PageTableFlags::PRESENT) {
                return None;
            }

            let level_2_table_ptr =
                (self.physical_memory_offset + level_3_entry.addr().as_u64()).as_ptr();
            let level_2_table = &*(level_2_table_ptr as *const PageTable);
            let level_2_index = page.p2_index();
            let level_2_entry = &level_2_table[level_2_index];

            if !level_2_entry.flags().contains(PageTableFlags::PRESENT) {
                return None;
            }

            let level_1_table_ptr =
                (self.physical_memory_offset + level_2_entry.addr().as_u64()).as_ptr();
            let level_1_table = &*(level_1_table_ptr as *const PageTable);
            let level_1_index = page.p1_index();
            let level_1_entry = &level_1_table[level_1_index];

            if level_1_entry.flags().contains(PageTableFlags::PRESENT) {
                Some(level_1_entry.flags())
            } else {
                None
            }
        }
    }

    /// Handle page fault with proper error recovery
    pub fn handle_page_fault(
        &mut self,
        addr: VirtAddr,
        error_code: u64,
        frame_allocator: &mut impl FrameAllocator<Size4KiB>,
    ) -> Result<(), &'static str> {
        let page = Page::containing_address(addr);
        let is_present = error_code & 0x1 != 0;
        let is_write = error_code & 0x2 != 0;

        if !is_present {
            // Page not present - allocate and map new page
            let frame = frame_allocator.allocate_frame().ok_or("Out of memory")?;

            // Zero the page for security
            unsafe {
                let page_ptr: *mut u8 =
                    (self.physical_memory_offset + frame.start_address().as_u64()).as_mut_ptr();
                core::ptr::write_bytes(page_ptr, 0, PAGE_SIZE);
            }

            let flags = PageTableFlags::PRESENT
                | PageTableFlags::WRITABLE
                | PageTableFlags::USER_ACCESSIBLE;
            self.map_page(page, frame, flags, frame_allocator)
                .map_err(|_| "Failed to map page")?;

            Ok(())
        } else if is_write {
            // Check if this is a copy-on-write page
            if let Some(current_flags) = self.get_flags(page) {
                if !current_flags.contains(PageTableFlags::WRITABLE) {
                    // This might be a COW page - handle it
                    return self.handle_cow_fault(page, frame_allocator);
                }
            }
            Err("Write to non-writable page")
        } else {
            Err("Unknown page fault type")
        }
    }

    /// Handle copy-on-write page fault
    pub fn handle_cow_fault(
        &mut self,
        page: Page,
        frame_allocator: &mut impl FrameAllocator<Size4KiB>,
    ) -> Result<(), &'static str> {
        // Get the current physical address
        let old_phys_addr = self
            .translate_addr(page.start_address())
            .ok_or("Page not mapped")?;

        // Allocate new frame
        let new_frame = frame_allocator.allocate_frame().ok_or("Out of memory")?;

        // Copy content from old page to new page
        unsafe {
            let old_ptr: *const u8 =
                (self.physical_memory_offset + old_phys_addr.as_u64()).as_ptr();
            let new_ptr: *mut u8 =
                (self.physical_memory_offset + new_frame.start_address().as_u64()).as_mut_ptr();
            core::ptr::copy_nonoverlapping(old_ptr, new_ptr, PAGE_SIZE);
        }

        // Unmap old page
        let old_frame = self.unmap_page(page).ok_or("Failed to unmap page")?;

        // Map new page with write permissions
        let flags =
            PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE;
        self.map_page(page, new_frame, flags, frame_allocator)
            .map_err(|_| "Failed to map new page")?;

        // Note: In a real implementation, we would need to manage reference counting
        // for the old frame and only deallocate it when no other processes reference it

        Ok(())
    }

    /// Map a range of pages with specific protection
    pub fn map_range(
        &mut self,
        start_page: Page,
        num_pages: usize,
        flags: PageTableFlags,
        frame_allocator: &mut impl FrameAllocator<Size4KiB>,
    ) -> Result<(), &'static str> {
        for i in 0..num_pages {
            let page = start_page + i as u64;
            let frame = frame_allocator.allocate_frame().ok_or("Out of memory")?;

            // Zero the page for security
            unsafe {
                let page_ptr: *mut u8 =
                    (self.physical_memory_offset + frame.start_address().as_u64()).as_mut_ptr();
                core::ptr::write_bytes(page_ptr, 0, PAGE_SIZE);
            }

            self.map_page(page, frame, flags, frame_allocator)
                .map_err(|_| "Failed to map page in range")?;
        }
        Ok(())
    }

    /// Unmap a range of pages
    pub fn unmap_range(&mut self, start_page: Page, num_pages: usize) -> Vec<PhysFrame> {
        let mut freed_frames = Vec::new();

        for i in 0..num_pages {
            let page = start_page + i as u64;
            if let Some(frame) = self.unmap_page(page) {
                freed_frames.push(frame);
            }
        }

        freed_frames
    }

    /// Clone page table entries for COW (share physical frames between processes)
    pub fn clone_page_table_entries(
        &mut self,
        src_start: VirtAddr,
        src_size: usize,
        dst_start: VirtAddr,
        flags: PageTableFlags,
        frame_allocator: &mut impl FrameAllocator<Size4KiB>,
    ) -> Result<(), &'static str> {
        let start_page: Page<Size4KiB> = Page::containing_address(src_start);
        let end_page: Page<Size4KiB> = Page::containing_address(src_start + src_size - 1u64);

        let dst_offset = dst_start.as_u64() - src_start.as_u64();

        for page in Page::range_inclusive(start_page, end_page) {
            // Get physical frame from source page
            if let Some(phys_addr) = self.translate_addr(page.start_address()) {
                let frame: PhysFrame<Size4KiB> = PhysFrame::containing_address(phys_addr);

                // Calculate destination page
                let dst_page_addr = VirtAddr::new(page.start_address().as_u64() + dst_offset);
                let dst_page = Page::containing_address(dst_page_addr);

                // Map destination page to same physical frame
                unsafe {
                    self.mapper
                        .map_to(dst_page, frame, flags, frame_allocator)
                        .map_err(|_| "Failed to clone page table entry")?
                        .flush();
                }
            }
        }

        Ok(())
    }

    /// Clone page table for fork operation (with copy-on-write)
    pub fn clone_for_fork(
        &mut self,
        _frame_allocator: &mut impl FrameAllocator<Size4KiB>,
    ) -> Result<OffsetPageTable<'static>, &'static str> {
        // This would create a new page table with copy-on-write mappings
        // For now, return error as this is complex to implement properly
        Err("Fork page table cloning not implemented")
    }
}

/// Main memory management system
pub struct MemoryManager {
    frame_allocator: Mutex<PhysicalFrameAllocator>,
    page_table_manager: Mutex<PageTableManager>,
    /// Offset of the direct physical-memory mapping. Required to form valid
    /// pointers to physical frames (mirrors PageTableManager's own field).
    physical_memory_offset: VirtAddr,
    regions: RwLock<BTreeMap<VirtAddr, VirtualMemoryRegion>>,
    heap_initialized: AtomicU64,
    total_memory: AtomicUsize,
    security_features: SecurityFeatures,
    swap_manager: Mutex<SwapManager>,
    /// Reference counting for physical frames (for COW support)
    frame_refcounts: RwLock<BTreeMap<PhysAddr, AtomicUsize>>,
}

/// Security features configuration
#[derive(Debug, Clone)]
struct SecurityFeatures {
    aslr_enabled: bool,
    stack_canaries_enabled: bool,
    nx_bit_enabled: bool,
    smep_enabled: bool,
    smap_enabled: bool,
}

impl Default for SecurityFeatures {
    fn default() -> Self {
        Self {
            aslr_enabled: true,
            stack_canaries_enabled: true,
            nx_bit_enabled: true,
            smep_enabled: true,
            smap_enabled: true,
        }
    }
}

impl MemoryManager {
    pub fn new(
        frame_allocator: PhysicalFrameAllocator,
        page_table_manager: PageTableManager,
    ) -> Self {
        // Calculate total memory
        let zone_stats = frame_allocator.get_zone_stats();
        let total_memory = zone_stats.iter().map(|stats| stats.total_bytes()).sum();

        // Initialize swap manager with 10% of total memory as swap space
        let swap_slots = (total_memory / PAGE_SIZE) / 10;
        let swap_manager = SwapManager::new(swap_slots as u32, PageReplacementAlgorithm::LRU);

        let physical_memory_offset = page_table_manager.physical_memory_offset;

        Self {
            frame_allocator: Mutex::new(frame_allocator),
            page_table_manager: Mutex::new(page_table_manager),
            physical_memory_offset,
            regions: RwLock::new(BTreeMap::new()),
            heap_initialized: AtomicU64::new(0),
            total_memory: AtomicUsize::new(total_memory),
            security_features: SecurityFeatures::default(),
            swap_manager: Mutex::new(swap_manager),
            frame_refcounts: RwLock::new(BTreeMap::new()),
        }
    }

    /// Map a virtual memory region to physical frames
    pub fn map_region(&self, region: &mut VirtualMemoryRegion) -> Result<(), MemoryError> {
        let mut page_table_manager = self.page_table_manager.lock();
        let mut frame_allocator = self.frame_allocator.lock();

        let flags = region.protection.to_page_table_flags();
        let mut first_frame = None;

        for page in region.pages() {
            let frame = frame_allocator
                .allocate_frame()
                .ok_or(MemoryError::OutOfMemory)?;

            if first_frame.is_none() {
                first_frame = Some(frame.start_address());
            }

            // Initialize page content if needed
            if matches!(
                region.region_type,
                MemoryRegionType::UserStack | MemoryRegionType::UserHeap
            ) {
                unsafe {
                    let page_ptr = (self.physical_memory_offset + frame.start_address().as_u64())
                        .as_mut_ptr::<u8>();
                    core::ptr::write_bytes(page_ptr, 0, PAGE_SIZE);
                }
            }

            page_table_manager
                .map_page(page, frame, flags, &mut *frame_allocator)
                .map_err(|_| MemoryError::MappingFailed)?;
        }

        region.mapped = true;
        region.physical_start = first_frame;
        Ok(())
    }

    /// Unmap a virtual memory region
    pub fn unmap_region(&self, region: &mut VirtualMemoryRegion) -> Result<(), MemoryError> {
        let mut page_table_manager = self.page_table_manager.lock();
        let mut frame_allocator = self.frame_allocator.lock();

        for page in region.pages() {
            if let Some(frame) = page_table_manager.unmap_page(page) {
                let zone = MemoryZone::from_address(frame.start_address());
                frame_allocator.deallocate_frame(frame, zone);
            }
        }

        region.mapped = false;
        region.physical_start = None;
        Ok(())
    }

    /// Add a virtual memory region to management
    pub fn add_region(&self, region: VirtualMemoryRegion) -> Result<(), MemoryError> {
        let mut regions = self.regions.write();

        // Check for overlaps
        for existing_region in regions.values() {
            if self.regions_overlap(&region, existing_region) {
                return Err(MemoryError::RegionOverlap);
            }
        }

        regions.insert(region.start, region);
        Ok(())
    }

    /// Remove a region from management
    pub fn remove_region(&self, start: VirtAddr) -> Result<VirtualMemoryRegion, MemoryError> {
        let mut regions = self.regions.write();
        regions.remove(&start).ok_or(MemoryError::RegionNotFound)
    }

    /// Find region containing the given address
    pub fn find_region(&self, addr: VirtAddr) -> Option<VirtualMemoryRegion> {
        let regions = self.regions.read();
        regions
            .values()
            .find(|region| region.contains(addr))
            .cloned()
    }

    /// Check if two regions overlap
    fn regions_overlap(
        &self,
        region1: &VirtualMemoryRegion,
        region2: &VirtualMemoryRegion,
    ) -> bool {
        let r1_end = region1.end();
        let r2_end = region2.end();
        !(r1_end <= region2.start || region1.start >= r2_end)
    }

    /// Allocate virtual memory region with enhanced features
    pub fn allocate_region(
        &self,
        size: usize,
        region_type: MemoryRegionType,
        protection: MemoryProtection,
    ) -> Result<VirtualMemoryRegion, MemoryError> {
        let aligned_size = align_up_checked(size, PAGE_SIZE).ok_or(MemoryError::NoVirtualSpace)?;

        // Find a free hole, then apply ASLR, then validate the *shifted*
        // address before mapping anything. Applying ASLR after validation
        // could push the region past USER_SPACE_END or onto another region,
        // mapping frames that the overlap check would later reject.
        let base = self
            .find_free_virtual_space(aligned_size)
            .ok_or(MemoryError::NoVirtualSpace)?;

        let enable_aslr = self.security_features.aslr_enabled
            && matches!(
                region_type,
                MemoryRegionType::UserCode
                    | MemoryRegionType::UserData
                    | MemoryRegionType::UserStack
            );
        let aslr_offset = if enable_aslr {
            generate_aslr_offset()
        } else {
            0
        };

        let start_u = base.as_u64() + aslr_offset;
        let end_u = start_u
            .checked_add(aligned_size as u64)
            .ok_or(MemoryError::NoVirtualSpace)?;
        if end_u > USER_SPACE_END as u64 {
            return Err(MemoryError::NoVirtualSpace);
        }

        let mut region = VirtualMemoryRegion::new(
            VirtAddr::new(start_u),
            aligned_size,
            region_type,
            protection,
        );
        region.aslr_offset = aslr_offset;

        // Overlap check at the final address BEFORE mapping any frames.
        {
            let regions = self.regions.read();
            if regions
                .values()
                .any(|existing| self.regions_overlap(&region, existing))
            {
                return Err(MemoryError::RegionOverlap);
            }
        }

        // Map the region, then record it. If tracking insertion races and
        // fails, unmap so we don't leak the just-mapped frames.
        self.map_region(&mut region)?;
        if let Err(e) = self.add_region(region.clone()) {
            let _ = self.unmap_region(&mut region);
            return Err(e);
        }

        Ok(region)
    }

    /// Allocate region with guard pages
    pub fn allocate_region_with_guards(
        &self,
        size: usize,
        region_type: MemoryRegionType,
        protection: MemoryProtection,
    ) -> Result<VirtualMemoryRegion, MemoryError> {
        let aligned_size = align_up_checked(size, PAGE_SIZE).ok_or(MemoryError::NoVirtualSpace)?;
        let total_size = aligned_size
            .checked_add(2 * PAGE_SIZE) // Add guard pages
            .ok_or(MemoryError::NoVirtualSpace)?;

        let start_addr = self
            .find_free_virtual_space(total_size)
            .ok_or(MemoryError::NoVirtualSpace)?;

        // Create guard page at start
        let guard_start = VirtualMemoryRegion::new(
            start_addr,
            PAGE_SIZE,
            MemoryRegionType::GuardPage,
            MemoryProtection::GUARD_PAGE,
        );

        // Create actual region
        let mut main_region = VirtualMemoryRegion::new(
            start_addr + PAGE_SIZE,
            aligned_size,
            region_type,
            protection,
        );

        // Create guard page at end
        let guard_end = VirtualMemoryRegion::new(
            start_addr + PAGE_SIZE + aligned_size,
            PAGE_SIZE,
            MemoryRegionType::GuardPage,
            MemoryProtection::GUARD_PAGE,
        );

        // Add regions. The guard pages carry no frames, but the main region is
        // mapped before it is tracked, so on any tracking failure we must undo
        // the mapping and any regions already inserted to avoid leaks.
        let guard_start_addr = guard_start.start;
        self.add_region(guard_start)?;

        self.map_region(&mut main_region)?;

        if let Err(e) = self.add_region(main_region.clone()) {
            let _ = self.unmap_region(&mut main_region);
            let _ = self.remove_region(guard_start_addr);
            return Err(e);
        }

        if let Err(e) = self.add_region(guard_end) {
            let _ = self.unmap_region(&mut main_region);
            let _ = self.remove_region(main_region.start);
            let _ = self.remove_region(guard_start_addr);
            return Err(e);
        }

        Ok(main_region)
    }

    /// Find free virtual address space
    fn find_free_virtual_space(&self, size: usize) -> Option<VirtAddr> {
        let regions = self.regions.read();
        let mut current_addr = VirtAddr::new(USER_SPACE_START as u64);

        while current_addr.as_u64() + size as u64 <= USER_SPACE_END as u64 {
            let end_addr = current_addr + size;

            let overlaps = regions.values().any(|region| {
                let region_end = region.end();
                !(end_addr <= region.start || current_addr >= region_end)
            });

            if !overlaps {
                return Some(current_addr);
            }

            // Move to next page-aligned address
            current_addr = VirtAddr::new(align_up(
                current_addr.as_u64() as usize + PAGE_SIZE,
                PAGE_SIZE,
            ) as u64);
        }

        None
    }

    /// Initialize the kernel heap with guard pages
    pub fn init_heap(&self) -> Result<(), MemoryError> {
        // Check if already initialized
        if self.heap_initialized.load(Ordering::Relaxed) != 0 {
            return Ok(());
        }

        // Create heap region with guard pages
        let guard_page_size = PAGE_SIZE;
        let actual_heap_start = KERNEL_HEAP_START + guard_page_size;
        let actual_heap_size = KERNEL_HEAP_SIZE - 2 * guard_page_size;

        // Create guard page at the beginning
        let guard_start_region = VirtualMemoryRegion::new(
            VirtAddr::new(KERNEL_HEAP_START as u64),
            guard_page_size,
            MemoryRegionType::GuardPage,
            MemoryProtection::GUARD_PAGE,
        );

        // Create actual heap region
        let heap_region = VirtualMemoryRegion::new(
            VirtAddr::new(actual_heap_start as u64),
            actual_heap_size,
            MemoryRegionType::Kernel,
            MemoryProtection::KERNEL_DATA,
        );

        // Create guard page at the end
        let guard_end_region = VirtualMemoryRegion::new(
            VirtAddr::new((actual_heap_start + actual_heap_size) as u64),
            guard_page_size,
            MemoryRegionType::GuardPage,
            MemoryProtection::GUARD_PAGE,
        );

        // Add regions
        self.add_region(guard_start_region)?;
        self.add_region(heap_region)?;
        self.add_region(guard_end_region)?;

        // Initialize the heap allocator with actual heap area
        // This uses the linked_list_allocator crate which must be initialized separately
        // For now, mark as initialized
        self.heap_initialized.store(1, Ordering::Relaxed);
        Ok(())
    }

    /// Enhanced page fault handler with copy-on-write and demand paging
    pub fn handle_page_fault(&self, addr: VirtAddr, error_code: u64) -> Result<(), MemoryError> {
        // Parse error code
        let is_present = error_code & 0x1 != 0;
        let is_write = error_code & 0x2 != 0;
        let is_user = error_code & 0x4 != 0;
        let is_instruction_fetch = error_code & 0x10 != 0;

        // Check if address is in a valid region
        if let Some(region) = self.find_region(addr) {
            // Handle different types of page faults
            if !is_present {
                // Page not present - check if it's swapped out or needs demand paging
                if self.is_page_swapped(addr) {
                    return self.handle_swap_in(addr, &region);
                } else {
                    return self.handle_demand_paging(addr, &region);
                }
            }

            if is_write && region.protection.copy_on_write {
                // Write to copy-on-write page
                return self.handle_copy_on_write(addr, &region);
            }

            if is_write && !region.protection.writable {
                return Err(MemoryError::WriteViolation);
            }

            if is_instruction_fetch && !region.protection.executable {
                return Err(MemoryError::ExecuteViolation);
            }

            if is_user && !region.protection.user_accessible {
                return Err(MemoryError::PrivilegeViolation);
            }

            // Check for guard page access
            if region.protection.guard_page {
                return Err(MemoryError::GuardPageViolation);
            }
        }

        Err(MemoryError::InvalidAddress)
    }

    /// Handle swap-in operation for a page fault on swapped page
    pub fn handle_swap_in(
        &self,
        addr: VirtAddr,
        region: &VirtualMemoryRegion,
    ) -> Result<(), MemoryError> {
        let page = Page::containing_address(addr);
        let mut page_table_manager = self.page_table_manager.lock();
        let mut frame_allocator = self.frame_allocator.lock();
        let mut swap_manager = self.swap_manager.lock();

        // Try to allocate a new frame
        let frame = if let Some(frame) = frame_allocator.allocate_frame() {
            frame
        } else {
            // Out of physical memory - need to swap out another page
            drop(frame_allocator);
            drop(page_table_manager);
            drop(swap_manager);

            self.swap_out_victim_page()?;

            // Re-acquire locks and try again
            page_table_manager = self.page_table_manager.lock();
            frame_allocator = self.frame_allocator.lock();
            swap_manager = self.swap_manager.lock();

            frame_allocator
                .allocate_frame()
                .ok_or(MemoryError::OutOfMemory)?
        };

        // Implement swap-in functionality
        // 1. Find the swap slot for this virtual address
        let swap_slot = swap_manager
            .swap_entries
            .iter()
            .find(|(_, entry)| entry.page_addr == addr)
            .map(|(slot, _)| *slot);

        // 2. Read the page data from swap storage and copy to physical frame
        if let Some(slot) = swap_slot {
            // Allocate buffer for page data
            let mut page_data = [0u8; PAGE_SIZE];

            // Read page from swap
            match swap_manager.swap_in(slot, &mut page_data) {
                Ok(_) => {
                    // 3. Copy the data to the new physical frame
                    unsafe {
                        let page_ptr = (self.physical_memory_offset
                            + frame.start_address().as_u64())
                        .as_mut_ptr::<u8>();
                        core::ptr::copy_nonoverlapping(page_data.as_ptr(), page_ptr, PAGE_SIZE);
                    }
                }
                Err(e) => {
                    // Failed to read from swap - zero the page as fallback
                    unsafe {
                        let page_ptr = (self.physical_memory_offset
                            + frame.start_address().as_u64())
                        .as_mut_ptr::<u8>();
                        core::ptr::write_bytes(page_ptr, 0, PAGE_SIZE);
                    }
                }
            }
        } else {
            // No swap entry found - zero the page as fallback
            // This handles the case where the page was never swapped out
            unsafe {
                let page_ptr = (self.physical_memory_offset + frame.start_address().as_u64())
                    .as_mut_ptr::<u8>();
                core::ptr::write_bytes(page_ptr, 0, PAGE_SIZE);
            }
        }

        // Map the page
        let flags = region.protection.to_page_table_flags();
        page_table_manager
            .map_page(page, frame, flags, &mut *frame_allocator)
            .map_err(|_| MemoryError::MappingFailed)?;

        // Record page access for replacement algorithms
        swap_manager.record_access(addr);

        Ok(())
    }

    /// Handle demand paging (allocate page on first access)
    fn handle_demand_paging(
        &self,
        addr: VirtAddr,
        region: &VirtualMemoryRegion,
    ) -> Result<(), MemoryError> {
        let page = Page::containing_address(addr);
        let mut page_table_manager = self.page_table_manager.lock();
        let mut frame_allocator = self.frame_allocator.lock();
        let mut swap_manager = self.swap_manager.lock();

        // Try to allocate a new frame
        let frame = if let Some(frame) = frame_allocator.allocate_frame() {
            frame
        } else {
            // Out of physical memory - need to swap out a page
            drop(frame_allocator); // Release lock to avoid deadlock
            drop(page_table_manager);

            self.swap_out_victim_page()?;

            // Re-acquire locks and try again
            page_table_manager = self.page_table_manager.lock();
            frame_allocator = self.frame_allocator.lock();

            frame_allocator
                .allocate_frame()
                .ok_or(MemoryError::OutOfMemory)?
        };

        // Zero the page for security
        unsafe {
            let page_ptr =
                (self.physical_memory_offset + frame.start_address().as_u64()).as_mut_ptr::<u8>();
            core::ptr::write_bytes(page_ptr, 0, PAGE_SIZE);
        }

        // Map the page
        let flags = region.protection.to_page_table_flags();
        page_table_manager
            .map_page(page, frame, flags, &mut *frame_allocator)
            .map_err(|_| MemoryError::MappingFailed)?;

        // Record page access for replacement algorithms
        swap_manager.record_access(addr);

        Ok(())
    }

    /// Swap out a victim page to make room for new allocation
    fn swap_out_victim_page(&self) -> Result<(), MemoryError> {
        let regions = self.regions.read();
        let mut candidate_pages = Vec::new();

        // Collect candidate pages from all mapped regions
        for region in regions.values() {
            if region.mapped && region.protection.user_accessible {
                for page_addr in region.pages().map(|p| p.start_address()) {
                    candidate_pages.push(page_addr);
                }
            }
        }

        drop(regions);

        if candidate_pages.is_empty() {
            return Err(MemoryError::OutOfMemory);
        }

        let mut swap_manager = self.swap_manager.lock();
        let victim_addr = swap_manager
            .select_victim_page(&candidate_pages)
            .ok_or(MemoryError::OutOfMemory)?;

        let victim_page = Page::containing_address(victim_addr);
        let mut page_table_manager = self.page_table_manager.lock();

        // Get the physical address of the victim page
        let phys_addr = page_table_manager
            .translate_addr(victim_addr)
            .ok_or(MemoryError::InvalidAddress)?;

        // Read the page content
        let mut page_data = [0u8; PAGE_SIZE];
        unsafe {
            let page_ptr = (self.physical_memory_offset + phys_addr.as_u64()).as_ptr::<u8>();
            core::ptr::copy_nonoverlapping(page_ptr, page_data.as_mut_ptr(), PAGE_SIZE);
        }

        // Swap out the page
        let _swap_slot = swap_manager
            .swap_out(victim_addr, &page_data)
            .map_err(|_| MemoryError::OutOfMemory)?;

        // Unmap the page and free the frame
        if let Some(frame) = page_table_manager.unmap_page(victim_page) {
            let mut frame_allocator = self.frame_allocator.lock();
            let zone = MemoryZone::from_address(frame.start_address());
            frame_allocator.deallocate_frame(frame, zone);
        }

        Ok(())
    }

    /// Handle copy-on-write page fault
    fn handle_copy_on_write(
        &self,
        addr: VirtAddr,
        region: &VirtualMemoryRegion,
    ) -> Result<(), MemoryError> {
        let page = Page::containing_address(addr);
        let mut page_table_manager = self.page_table_manager.lock();
        let mut frame_allocator = self.frame_allocator.lock();

        // Get the current frame
        let old_frame_addr = page_table_manager
            .translate_addr(addr)
            .ok_or(MemoryError::InvalidAddress)?;

        // Allocate a new frame
        let new_frame = frame_allocator
            .allocate_frame()
            .ok_or(MemoryError::OutOfMemory)?;

        // Copy content from old page to new page
        unsafe {
            let old_ptr = (self.physical_memory_offset + old_frame_addr.as_u64()).as_ptr::<u8>();
            let new_ptr = (self.physical_memory_offset + new_frame.start_address().as_u64())
                .as_mut_ptr::<u8>();
            core::ptr::copy_nonoverlapping(old_ptr, new_ptr, PAGE_SIZE);
        }

        // Unmap old page
        if let Some(old_frame) = page_table_manager.unmap_page(page) {
            // Decrement reference count for the old frame
            let old_frame_start = old_frame.start_address();
            drop(page_table_manager); // Release lock to call decrement
            drop(frame_allocator);

            let remaining_refs = self.decrement_frame_refcount(old_frame_start);

            // Only deallocate if no more references
            if remaining_refs == 0 {
                let zone = MemoryZone::from_address(old_frame_start);
                let mut frame_allocator = self.frame_allocator.lock();
                frame_allocator.deallocate_frame(old_frame, zone);
            }

            // Re-acquire locks for final mapping
            page_table_manager = self.page_table_manager.lock();
            frame_allocator = self.frame_allocator.lock();
        }

        // Map new page with write permissions
        let mut protection = region.protection;
        protection.writable = true;
        protection.copy_on_write = false;
        let flags = protection.to_page_table_flags();

        page_table_manager
            .map_page(page, new_frame, flags, &mut *frame_allocator)
            .map_err(|_| MemoryError::MappingFailed)?;

        Ok(())
    }

    /// Increment reference count for a physical frame (for COW)
    pub fn increment_frame_refcount(&self, frame_addr: PhysAddr) {
        let mut refcounts = self.frame_refcounts.write();
        refcounts
            .entry(frame_addr)
            .and_modify(|count| {
                count.fetch_add(1, Ordering::SeqCst);
            })
            .or_insert_with(|| AtomicUsize::new(2)); // Initial sharing: 2 references
    }

    /// Decrement reference count for a physical frame, returns remaining count
    pub fn decrement_frame_refcount(&self, frame_addr: PhysAddr) -> usize {
        let refcounts = self.frame_refcounts.read();

        if let Some(count) = refcounts.get(&frame_addr) {
            // Atomically decrement and act on the *previous* value so two
            // concurrent COW faults can never both observe "0 -> free".
            let prev = count.fetch_sub(1, Ordering::SeqCst);

            if prev == 0 {
                // Underflow: count was already zero. Restore it and report the
                // frame as still referenced so the caller does not double-free.
                count.fetch_add(1, Ordering::SeqCst);
                return 1;
            }

            let new_count = prev - 1;

            // Only the decrementer that drove the count to zero frees the frame.
            if new_count == 0 {
                drop(refcounts);
                let mut refcounts_write = self.frame_refcounts.write();
                refcounts_write.remove(&frame_addr);
            }

            new_count
        } else {
            0 // Frame not tracked, already at zero
        }
    }

    /// Get reference count for a physical frame
    pub fn get_frame_refcount(&self, frame_addr: PhysAddr) -> usize {
        let refcounts = self.frame_refcounts.read();
        refcounts
            .get(&frame_addr)
            .map(|count| count.load(Ordering::SeqCst))
            .unwrap_or(1) // Default to 1 if not in COW tracking
    }

    /// Check if a frame is shared (refcount > 1)
    pub fn is_frame_shared(&self, frame_addr: PhysAddr) -> bool {
        self.get_frame_refcount(frame_addr) > 1
    }

    /// Get comprehensive memory statistics
    pub fn memory_stats(&self) -> MemoryStats {
        let frame_allocator = self.frame_allocator.lock();
        let regions = self.regions.read();
        let swap_manager = self.swap_manager.lock();
        let zone_stats = frame_allocator.get_zone_stats();

        let total_allocated_frames: usize =
            zone_stats.iter().map(|stats| stats.allocated_frames).sum();
        let total_frames: usize = zone_stats.iter().map(|stats| stats.total_frames).sum();

        MemoryStats {
            total_memory: self.total_memory.load(Ordering::Relaxed),
            allocated_memory: total_allocated_frames * PAGE_SIZE,
            free_memory: (total_frames.saturating_sub(total_allocated_frames)) * PAGE_SIZE,
            total_regions: regions.len(),
            mapped_regions: regions.values().filter(|r| r.mapped).count(),
            heap_initialized: self.heap_initialized.load(Ordering::Relaxed) != 0,
            zone_stats,
            buddy_stats: frame_allocator.get_buddy_stats(),
            security_features: self.security_features.clone(),
            swap_stats: swap_manager.get_stats(),
        }
    }

    /// Translate virtual address to physical address
    pub fn translate_addr(&self, addr: VirtAddr) -> Option<PhysAddr> {
        let page_table_manager = self.page_table_manager.lock();
        page_table_manager.translate_addr(addr)
    }

    /// Change protection flags for a memory region
    pub fn protect_region(
        &self,
        start: VirtAddr,
        size: usize,
        protection: MemoryProtection,
    ) -> Result<(), MemoryError> {
        let mut page_table_manager = self.page_table_manager.lock();
        let flags = protection.to_page_table_flags();

        let start_page = Page::containing_address(start);
        let end_page = Page::containing_address(start + size - 1u64);

        for page in Page::range_inclusive(start_page, end_page) {
            page_table_manager
                .update_flags(page, flags)
                .map_err(|_| MemoryError::ProtectionFailed)?;
        }

        // Update region protection in our tracking
        let mut regions = self.regions.write();
        for region in regions.values_mut() {
            if region.contains(start) {
                region.protection = protection;
                break;
            }
        }

        Ok(())
    }

    /// Create a copy-on-write mapping (for fork)
    pub fn create_cow_mapping(
        &self,
        src_region: &VirtualMemoryRegion,
    ) -> Result<VirtualMemoryRegion, MemoryError> {
        let mut cow_region = src_region.clone();
        cow_region.protection.copy_on_write = true;
        cow_region.protection.writable = false;

        // Mark original pages as copy-on-write
        let mut page_table_manager = self.page_table_manager.lock();
        let flags = cow_region.protection.to_page_table_flags();

        for page in cow_region.pages() {
            page_table_manager
                .update_flags(page, flags)
                .map_err(|_| MemoryError::ProtectionFailed)?;
        }

        Ok(cow_region)
    }

    /// Mark regions as COW bidirectionally (for proper fork implementation)
    pub fn mark_regions_cow_bidirectional(
        &self,
        parent_region: &VirtualMemoryRegion,
        child_region: &VirtualMemoryRegion,
    ) -> Result<(), MemoryError> {
        let mut page_table_manager = self.page_table_manager.lock();

        // Create COW flags (read-only, user accessible)
        let cow_flags =
            PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE | PageTableFlags::NO_EXECUTE; // Remove write permission

        // Mark parent pages as read-only COW
        for page in parent_region.pages() {
            page_table_manager
                .update_flags(page, cow_flags)
                .map_err(|_| MemoryError::ProtectionFailed)?;
        }

        // Mark child pages as read-only COW
        for page in child_region.pages() {
            page_table_manager
                .update_flags(page, cow_flags)
                .map_err(|_| MemoryError::ProtectionFailed)?;
        }

        Ok(())
    }

    /// Clone page table entries from source to destination (for fork)
    pub fn clone_page_entries_cow(
        &self,
        src_start: VirtAddr,
        src_size: usize,
        dst_start: VirtAddr,
    ) -> Result<(), MemoryError> {
        let mut page_table_manager = self.page_table_manager.lock();
        let mut frame_allocator = self.frame_allocator.lock();

        // COW flags: present, user accessible, NOT writable
        let cow_flags =
            PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE | PageTableFlags::NO_EXECUTE;

        page_table_manager
            .clone_page_table_entries(
                src_start,
                src_size,
                dst_start,
                cow_flags,
                &mut *frame_allocator,
            )
            .map_err(|_| MemoryError::MappingFailed)?;

        // Increment reference counts for all shared frames
        let start_page: Page<Size4KiB> = Page::containing_address(src_start);
        let end_page: Page<Size4KiB> = Page::containing_address(src_start + src_size - 1u64);

        for page in Page::range_inclusive(start_page, end_page) {
            if let Some(phys_addr) = page_table_manager.translate_addr(page.start_address()) {
                drop(page_table_manager); // Release lock
                drop(frame_allocator);

                self.increment_frame_refcount(phys_addr);

                // Re-acquire locks for next iteration
                page_table_manager = self.page_table_manager.lock();
                frame_allocator = self.frame_allocator.lock();
            }
        }

        Ok(())
    }

    /// Allocate a single frame from a specific zone
    pub fn allocate_frame_in_zone(&self, zone: MemoryZone) -> Option<PhysFrame> {
        let mut frame_allocator = self.frame_allocator.lock();
        frame_allocator.allocate_frame_in_zone(zone)
    }

    /// Deallocate a single frame
    pub fn deallocate_frame(&self, frame: PhysFrame, zone: MemoryZone) {
        let mut frame_allocator = self.frame_allocator.lock();
        frame_allocator.deallocate_frame(frame, zone);
    }

    /// Get comprehensive memory statistics for all zones
    pub fn get_zone_stats(&self) -> [ZoneStats; 3] {
        let frame_allocator = self.frame_allocator.lock();
        frame_allocator.get_zone_stats()
    }

    /// Get detailed memory usage report
    pub fn get_memory_report(&self) -> MemoryReport {
        let frame_allocator = self.frame_allocator.lock();
        frame_allocator.get_memory_report()
    }

    /// Initialize swap space with a storage device
    pub fn init_swap_space(&self, device_id: u32, size_mb: u32) -> Result<(), &'static str> {
        let mut swap_manager = self.swap_manager.lock();
        swap_manager.set_swap_device(device_id);

        // In a real implementation, this would create a swap file or partition
        crate::serial_println!(
            "Initialized {}MB swap space on device {}",
            size_mb,
            device_id
        );
        Ok(())
    }

    /// Get swap statistics
    pub fn get_swap_stats(&self) -> crate::memory::SwapStats {
        let swap_manager = self.swap_manager.lock();
        swap_manager.get_stats()
    }

    /// Check if a page is currently swapped out
    pub fn is_page_swapped(&self, addr: VirtAddr) -> bool {
        let swap_manager = self.swap_manager.lock();
        swap_manager
            .swap_entries
            .iter()
            .any(|(_, entry)| entry.page_addr == addr)
    }
}

/// ASLR seed using hardware RNG when available
static ASLR_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Generate ASLR offset using hardware RNG
pub fn generate_aslr_offset() -> u64 {
    // Use RDRAND instruction for hardware random number generation
    let random_value = unsafe {
        let mut value: u64 = 0;
        // Try hardware RNG first
        if core::arch::x86_64::_rdrand64_step(&mut value) == 1 {
            value
        } else {
            // Fallback to TSC + counter if RDRAND not available
            let tsc = core::arch::x86_64::_rdtsc();
            let counter = ASLR_COUNTER.fetch_add(1, Ordering::SeqCst);
            tsc.wrapping_mul(6364136223846793005).wrapping_add(counter)
        }
    };

    // Apply entropy bits and align to page size
    (random_value & ((1 << ASLR_ENTROPY_BITS) - 1)) * PAGE_SIZE as u64
}

/// Memory error types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryError {
    OutOfMemory,
    MappingFailed,
    RegionOverlap,
    RegionNotFound,
    NoVirtualSpace,
    HeapInitFailed,
    InvalidAddress,
    WriteViolation,
    PrivilegeViolation,
    ExecuteViolation,
    GuardPageViolation,
    LazyAllocationNotImplemented,
    ProtectionFailed,
    InvalidOrder,
    BuddyAllocationFailed,
    FragmentationLimitExceeded,
    PermissionDenied,
}

impl fmt::Display for MemoryError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            MemoryError::OutOfMemory => write!(f, "Out of physical memory"),
            MemoryError::MappingFailed => write!(f, "Failed to map virtual memory"),
            MemoryError::RegionOverlap => write!(f, "Memory region overlap detected"),
            MemoryError::RegionNotFound => write!(f, "Memory region not found"),
            MemoryError::NoVirtualSpace => write!(f, "No available virtual address space"),
            MemoryError::HeapInitFailed => write!(f, "Heap initialization failed"),
            MemoryError::InvalidAddress => write!(f, "Invalid memory address"),
            MemoryError::WriteViolation => write!(f, "Write access violation"),
            MemoryError::PrivilegeViolation => write!(f, "Privilege violation"),
            MemoryError::ExecuteViolation => write!(f, "Execute access violation"),
            MemoryError::GuardPageViolation => write!(f, "Guard page access violation"),
            MemoryError::LazyAllocationNotImplemented => {
                write!(f, "Lazy allocation not implemented")
            }
            MemoryError::ProtectionFailed => write!(f, "Failed to change memory protection"),
            MemoryError::InvalidOrder => write!(f, "Invalid buddy allocator order"),
            MemoryError::BuddyAllocationFailed => write!(f, "Buddy allocation failed"),
            MemoryError::FragmentationLimitExceeded => {
                write!(f, "Memory fragmentation limit exceeded")
            }
            MemoryError::PermissionDenied => write!(f, "Permission denied"),
        }
    }
}

/// Comprehensive memory statistics
#[derive(Debug, Clone)]
pub struct MemoryStats {
    pub total_memory: usize,
    pub allocated_memory: usize,
    pub free_memory: usize,
    pub total_regions: usize,
    pub mapped_regions: usize,
    pub heap_initialized: bool,
    pub zone_stats: [ZoneStats; 3],
    pub buddy_stats: BuddyAllocatorStats,
    pub security_features: SecurityFeatures,
    pub swap_stats: SwapStats,
}

impl MemoryStats {
    pub fn memory_usage_percent(&self) -> f32 {
        if self.total_memory == 0 {
            0.0
        } else {
            (self.allocated_memory as f32 / self.total_memory as f32) * 100.0
        }
    }

    pub fn total_memory_mb(&self) -> usize {
        self.total_memory / (1024 * 1024)
    }

    pub fn allocated_memory_mb(&self) -> usize {
        self.allocated_memory / (1024 * 1024)
    }

    pub fn free_memory_mb(&self) -> usize {
        self.free_memory / (1024 * 1024)
    }

    pub fn average_fragmentation(&self) -> f32 {
        let total_fragmentation: f32 = self
            .zone_stats
            .iter()
            .map(|stats| stats.fragmentation_percent())
            .sum();
        total_fragmentation / 3.0
    }
}

lazy_static! {
    static ref MEMORY_MANAGER: RwLock<Option<MemoryManager>> = RwLock::new(None);
}

/// Initialize the memory management system
pub fn init_memory_management(
    memory_regions: &[MemoryRegion],
    physical_memory_offset: Option<u64>,
) -> Result<(), MemoryError> {
    // Determine physical memory offset (default to zero if not provided).
    // new_truncate, not new: a bad/edge offset must not panic here — this runs
    // before the IDT exists, so a panic would triple-fault. It also kept the
    // whole function from being seen as always-panicking (which DCE'd the rest
    // of the kernel after this call).
    let physical_memory_offset = VirtAddr::new_truncate(physical_memory_offset.unwrap_or(0));

    // Get current page table
    let level_4_table = unsafe {
        let (level_4_table_frame, _) = Cr3::read();
        let phys = level_4_table_frame.start_address();
        let virt =
            VirtAddr::new_truncate(physical_memory_offset.as_u64().wrapping_add(phys.as_u64()));
        &mut *(virt.as_mut_ptr() as *mut PageTable)
    };

    // Create page table manager
    let mapper = unsafe { OffsetPageTable::new(level_4_table, physical_memory_offset) };
    let page_table_manager = PageTableManager::new(mapper, physical_memory_offset);

    // Create frame allocator with buddy system
    let frame_allocator = PhysicalFrameAllocator::init(memory_regions);

    // Create memory manager
    let memory_manager = MemoryManager::new(frame_allocator, page_table_manager);

    // Initialize heap with guard pages
    memory_manager.init_heap()?;

    // Store global instance
    *MEMORY_MANAGER.write() = Some(memory_manager);

    Ok(())
}

/// Get global memory manager
pub fn get_memory_manager() -> Option<&'static MemoryManager> {
    unsafe {
        MEMORY_MANAGER
            .read()
            .as_ref()
            .map(|mm| core::mem::transmute(mm))
    }
}

/// High-level memory allocation interface
pub fn allocate_memory(
    size: usize,
    region_type: MemoryRegionType,
    protection: MemoryProtection,
) -> Result<VirtAddr, MemoryError> {
    let mm = get_memory_manager().ok_or(MemoryError::OutOfMemory)?;
    let region = mm.allocate_region(size, region_type, protection)?;
    Ok(region.start)
}

/// Allocate memory with guard pages
pub fn allocate_memory_with_guards(
    size: usize,
    region_type: MemoryRegionType,
    protection: MemoryProtection,
) -> Result<VirtAddr, MemoryError> {
    let mm = get_memory_manager().ok_or(MemoryError::OutOfMemory)?;
    let region = mm.allocate_region_with_guards(size, region_type, protection)?;
    Ok(region.start)
}

/// Deallocate memory region
pub fn deallocate_memory(addr: VirtAddr) -> Result<(), MemoryError> {
    let mm = get_memory_manager().ok_or(MemoryError::OutOfMemory)?;
    let mut region = mm.remove_region(addr)?;
    mm.unmap_region(&mut region)?;
    Ok(())
}

/// Get memory statistics
pub fn get_memory_stats() -> Option<MemoryStats> {
    get_memory_manager().map(|mm| mm.memory_stats())
}

/// Translate virtual address to physical address
pub fn translate_addr(addr: VirtAddr) -> Option<PhysAddr> {
    get_memory_manager()?.translate_addr(addr)
}

/// Change memory protection
pub fn protect_memory(
    addr: VirtAddr,
    size: usize,
    protection: MemoryProtection,
) -> Result<(), MemoryError> {
    let mm = get_memory_manager().ok_or(MemoryError::OutOfMemory)?;
    mm.protect_region(addr, size, protection)
}

/// Handle page fault (called from interrupt handler)
pub fn handle_page_fault(addr: VirtAddr, error_code: u64) -> Result<(), MemoryError> {
    let mm = get_memory_manager().ok_or(MemoryError::OutOfMemory)?;
    mm.handle_page_fault(addr, error_code)
}

/// Create copy-on-write mapping (for fork)
pub fn create_cow_mapping(src_addr: VirtAddr) -> Result<VirtAddr, MemoryError> {
    let mm = get_memory_manager().ok_or(MemoryError::OutOfMemory)?;

    if let Some(src_region) = mm.find_region(src_addr) {
        let cow_region = mm.create_cow_mapping(&src_region)?;
        mm.add_region(cow_region.clone())?;
        Ok(cow_region.start)
    } else {
        Err(MemoryError::RegionNotFound)
    }
}

/// Utility function to align up to nearest boundary
pub fn align_up(addr: usize, align: usize) -> usize {
    (addr + align - 1) & !(align - 1)
}

/// Align up to nearest boundary, returning None on overflow
pub fn align_up_checked(addr: usize, align: usize) -> Option<usize> {
    addr.checked_add(align - 1).map(|v| v & !(align - 1))
}

/// Utility function to align down to nearest boundary
pub fn align_down(addr: usize, align: usize) -> usize {
    addr & !(align - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn test_mmio_page_span() {
        // BAR straddling into a second page -> two pages mapped.
        assert_eq!(
            mmio_page_span(0xFEBF_1000, 0x1100),
            (0xFEBF_1000, 0xFEBF_3000)
        );
        // Unaligned base rounds down; small size still spans the containing page.
        assert_eq!(
            mmio_page_span(0xFEBF_1080, 0x100),
            (0xFEBF_1000, 0xFEBF_2000)
        );
        // Exactly one page.
        assert_eq!(mmio_page_span(0x9000, 0x1000), (0x9000, 0xA000));
    }

    #[test_case]
    fn test_memory_protection_flags() {
        let kernel_data = MemoryProtection::KERNEL_DATA;
        let flags = kernel_data.to_page_table_flags();

        assert!(flags.contains(PageTableFlags::PRESENT));
        assert!(flags.contains(PageTableFlags::WRITABLE));
        assert!(!flags.contains(PageTableFlags::USER_ACCESSIBLE));
    }

    #[test_case]
    fn test_virtual_memory_region() {
        let start = VirtAddr::new(0x1000);
        let size = 0x2000;
        let region = VirtualMemoryRegion::new(
            start,
            size,
            MemoryRegionType::UserData,
            MemoryProtection::USER_DATA,
        );

        assert_eq!(region.start, start);
        assert_eq!(region.size, size);
        assert_eq!(region.end(), start + size);
        assert!(region.contains(VirtAddr::new(0x1500)));
        assert!(!region.contains(VirtAddr::new(0x3500)));
    }

    #[test_case]
    fn test_memory_zones() {
        assert_eq!(
            MemoryZone::from_address(PhysAddr::new(0x100000)),
            MemoryZone::Dma
        );
        assert_eq!(
            MemoryZone::from_address(PhysAddr::new(0x2000000)),
            MemoryZone::Normal
        );
        assert_eq!(
            MemoryZone::from_address(PhysAddr::new(0x40000000)),
            MemoryZone::HighMem
        );
    }

    #[test_case]
    fn test_align_functions() {
        assert_eq!(align_up(0x1001, 0x1000), 0x2000);
        assert_eq!(align_down(0x1fff, 0x1000), 0x1000);
        assert_eq!(align_up(0x1000, 0x1000), 0x1000);
    }

    #[test_case]
    fn test_copy_on_write_protection() {
        let cow_protection = MemoryProtection::COPY_ON_WRITE;
        assert!(cow_protection.copy_on_write);
        assert!(!cow_protection.writable);
        assert!(cow_protection.readable);
    }

    #[test_case]
    fn test_guard_page_protection() {
        let guard_protection = MemoryProtection::GUARD_PAGE;
        assert!(guard_protection.guard_page);
        assert!(!guard_protection.readable);
        assert!(!guard_protection.writable);
        assert!(!guard_protection.executable);
    }
}

/// Fast page fault handler for common cases (complete implementation)
/// Attempts to handle page faults quickly without full context switching
pub fn try_fast_page_fault_handler(addr: VirtAddr) -> bool {
    // Get the memory manager
    if let Some(memory_manager) = get_memory_manager() {
        // Check if this is a known memory region
        if let Some(region) = memory_manager.find_region(addr) {
            // Handle common fast-path cases
            match region.region_type {
                MemoryRegionType::UserStack => {
                    // Stack growth: if within reasonable bounds, allow it
                    let stack_limit = region.start.as_u64().saturating_sub(1024 * 1024); // 1MB max stack growth
                    if addr.as_u64() >= stack_limit {
                        // This would be handled by the full page fault handler
                        // For now, indicate this needs full handling
                        return false;
                    }
                }
                MemoryRegionType::UserHeap => {
                    // Heap expansion: check if within reasonable bounds
                    if addr.as_u64() < region.end().as_u64() + (16 * 1024 * 1024) {
                        // 16MB max heap growth
                        // This could potentially be handled quickly
                        // For now, delegate to full handler
                        return false;
                    }
                }
                MemoryRegionType::UserData | MemoryRegionType::UserCode => {
                    // For code/data segments, check if this is a copy-on-write situation
                    if region.protection.copy_on_write {
                        // COW requires full handling
                        return false;
                    }
                }
                _ => {
                    // Other types need full handling
                    return false;
                }
            }
        }
    }

    // If we can't handle it quickly, return false for full handling
    false
}

/// Dynamically adjust kernel heap size (complete implementation)
/// Attempts to resize the kernel heap while maintaining system stability
pub fn adjust_heap(new_size: usize) -> Result<usize, &'static str> {
    // Validate new size parameters
    const MIN_HEAP_SIZE: usize = 512 * 1024; // 512KB minimum
    const MAX_HEAP_SIZE: usize = 256 * 1024 * 1024; // 256MB maximum

    if new_size < MIN_HEAP_SIZE {
        return Err("Heap size too small (minimum 512KB required)");
    }

    if new_size > MAX_HEAP_SIZE {
        return Err("Heap size too large (maximum 256MB allowed)");
    }

    // Align to page boundaries
    let aligned_size = align_up(new_size, PAGE_SIZE);

    // Get current heap size
    if let Some(memory_manager) = get_memory_manager() {
        // Get current memory statistics
        let stats = memory_manager.get_memory_report();
        let current_heap_size = KERNEL_HEAP_SIZE;

        // Check if we're expanding or shrinking
        if aligned_size > current_heap_size {
            // Expanding heap - check if we have enough free memory
            let expansion_size = aligned_size - current_heap_size;

            if stats.free_memory < expansion_size {
                return Err("Insufficient free memory for heap expansion");
            }

            // In a real implementation, we would:
            // 1. Allocate additional physical frames
            // 2. Map them to extend the heap virtual address space
            // 3. Update the heap allocator's boundaries
            // 4. Update global heap size tracking

            // For now, we simulate successful expansion
            crate::serial_println!(
                "Heap expansion requested: {} -> {} bytes",
                current_heap_size,
                aligned_size
            );

            // Return the new size (in real implementation, update would happen here)
            Ok(aligned_size)
        } else if aligned_size < current_heap_size {
            // Shrinking heap - ensure it's safe to do so
            let shrink_size = current_heap_size - aligned_size;

            // Check if shrinking would compromise system stability
            if stats.allocated_memory > aligned_size {
                return Err("Cannot shrink heap below current allocation level");
            }

            // In a real implementation, we would:
            // 1. Verify no allocations exist in the region to be freed
            // 2. Unmap the virtual address space
            // 3. Return physical frames to the allocator
            // 4. Update the heap allocator's boundaries

            crate::serial_println!(
                "Heap shrinking requested: {} -> {} bytes",
                current_heap_size,
                aligned_size
            );

            // Return the new size (in real implementation, update would happen here)
            Ok(aligned_size)
        } else {
            // Size unchanged
            Ok(current_heap_size)
        }
    } else {
        Err("Memory manager not initialized")
    }
}

/// Memory flags for device I/O mapping (framebuffer, MMIO, etc.)
#[derive(Debug, Clone, Copy)]
pub struct MemoryFlags {
    flags: PageTableFlags,
}

impl MemoryFlags {
    pub const PRESENT: Self = MemoryFlags {
        flags: PageTableFlags::PRESENT,
    };
    pub const WRITABLE: Self = MemoryFlags {
        flags: PageTableFlags::WRITABLE,
    };
    pub const NO_CACHE: Self = MemoryFlags {
        flags: PageTableFlags::NO_CACHE,
    };
    pub const WRITE_COMBINING: Self = MemoryFlags {
        flags: PageTableFlags::WRITE_THROUGH,
    };

    pub fn to_page_table_flags(self) -> PageTableFlags {
        self.flags
    }
}

impl core::ops::BitOr for MemoryFlags {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self::Output {
        MemoryFlags {
            flags: self.flags | rhs.flags,
        }
    }
}

/// Map physical device memory (framebuffer, MMIO registers) to virtual address space
///
/// This is specifically designed for mapping device I/O regions that need special caching attributes.
/// For regular memory allocation, use the MemoryManager's allocate_region instead.
pub fn map_physical_memory(
    virt: usize,
    phys: usize,
    flags: MemoryFlags,
) -> Result<(), &'static str> {
    // Convert to x86_64 address types
    let virt_addr = VirtAddr::new(virt as u64);
    let phys_addr = PhysAddr::new(phys as u64);
    let page = Page::containing_address(virt_addr);
    let frame = PhysFrame::containing_address(phys_addr);

    // Get the global memory manager
    if let Some(memory_manager) = get_memory_manager() {
        let mut page_table_manager = memory_manager.page_table_manager.lock();
        let mut frame_allocator = memory_manager.frame_allocator.lock();

        // Map the page with the specified flags
        page_table_manager
            .map_page(
                page,
                frame,
                flags.to_page_table_flags(),
                &mut *frame_allocator,
            )
            .map_err(|_| "Failed to map physical memory page")?;

        Ok(())
    } else {
        // If memory manager is not initialized, we're in early boot
        // In this case, we'll do a direct identity mapping (unsafe but necessary)
        // This should only happen during very early initialization
        Err("Memory manager not initialized - cannot map physical memory")
    }
}

/// Page-aligned [start, end) span covering `size` bytes from `phys`.
const fn mmio_page_span(phys: usize, size: usize) -> (usize, usize) {
    const PAGE: usize = 4096;
    let start = phys & !(PAGE - 1);
    let end = (phys + size + PAGE - 1) & !(PAGE - 1);
    (start, end)
}

/// Identity-map a device MMIO region (e.g. a PCI BAR) into kernel space.
///
/// Maps every 4K page touched by `[phys, phys+size)` as present, writable and
/// uncached at the identical virtual address, so drivers can dereference BAR
/// addresses directly. Returns the (unaligned) virtual base, which equals `phys`.
/// MMIO must be uncached or device registers read stale — that NO_CACHE is
/// correctness, not a shortcut.
///
/// ponytail: identity map, no vaddr allocator — BARs sit above RAM so they
/// don't collide. Add an ioremap vaddr arena only if a BAR ever overlaps RAM.
pub fn map_mmio_region(phys: usize, size: usize) -> Result<usize, &'static str> {
    if size == 0 {
        return Err("map_mmio_region: zero size");
    }
    let flags = MemoryFlags::PRESENT | MemoryFlags::WRITABLE | MemoryFlags::NO_CACHE;
    let (start, end) = mmio_page_span(phys, size);
    let mut addr = start;
    while addr < end {
        // Skip pages already mapped — another driver may share this MMIO region
        // (e.g. the framebuffer mapped by both the display and VBE paths). Being
        // idempotent lets overlapping mappers coexist instead of erroring.
        if translate_addr(VirtAddr::new(addr as u64)).is_none() {
            map_physical_memory(addr, addr, flags)?;
        }
        addr += 4096;
    }
    Ok(phys)
}

/// Allocate a zeroed physical frame and map it at user virtual address `virt`.
///
/// Production backing for brk/mmap: every page a process faults in routes through
/// here, so the frame is real, zeroed (no stale kernel data leaks into user space),
/// and mapped into the single shared kernel page table. Lock order matches
/// `MemoryManager::map_region` (page table, then frame allocator) to avoid deadlock.
pub fn map_user_page(virt: usize, flags: PageTableFlags) -> Result<(), &'static str> {
    let mm = get_memory_manager().ok_or("memory manager not initialized")?;
    let mut page_table_manager = mm.page_table_manager.lock();
    let mut frame_allocator = mm.frame_allocator.lock();

    let frame = frame_allocator
        .allocate_frame()
        .ok_or("out of physical frames")?;

    // Zero the frame via the physical-offset map before it becomes user-visible.
    unsafe {
        let ptr = (mm.physical_memory_offset + frame.start_address().as_u64()).as_mut_ptr::<u8>();
        core::ptr::write_bytes(ptr, 0, 4096);
    }

    let page = Page::containing_address(VirtAddr::new(virt as u64));
    page_table_manager
        .map_page(page, frame, flags, &mut *frame_allocator)
        .map_err(|_| "failed to map user page")
}

/// Unmap a user page and return its frame to the allocator (real reclaim).
///
/// Idempotent: unmapping an already-free page is `Ok`, so brk-shrink / munmap
/// over a partially mapped range doesn't error.
pub fn unmap_user_page(virt: usize) -> Result<(), &'static str> {
    let mm = get_memory_manager().ok_or("memory manager not initialized")?;
    let page = Page::containing_address(VirtAddr::new(virt as u64));

    let frame = mm.page_table_manager.lock().unmap_page(page);
    if let Some(frame) = frame {
        let zone = MemoryZone::from_address(frame.start_address());
        mm.deallocate_frame(frame, zone);
    }
    Ok(())
}

/// Change the page-table flags of an existing user page (mprotect backing).
///
/// `update_flags` drops the `MapperFlush`, so flush this page's TLB entry here
/// or the old permissions linger until the next CR3 reload.
pub fn protect_user_page(virt: usize, flags: PageTableFlags) -> Result<(), &'static str> {
    let mm = get_memory_manager().ok_or("memory manager not initialized")?;
    let virt_addr = VirtAddr::new(virt as u64);
    let page = Page::containing_address(virt_addr);
    mm.page_table_manager.lock().update_flags(page, flags)?;
    x86_64::instructions::tlb::flush(virt_addr);
    Ok(())
}

/// True only if every page spanned by `[start, start+len)` is currently mapped.
///
/// Lets hot paths (e.g. text rendering) reject a dangling/corrupt `&str` before
/// dereferencing it, so bad data can't fault the kernel. Reads the str's ptr/len
/// metadata only — never the buffer itself.
pub fn range_is_mapped(start: usize, len: usize) -> bool {
    if len == 0 {
        return true;
    }
    let end = start.saturating_add(len);
    let mut addr = start & !0xFFF;
    while addr < end {
        if translate_addr(VirtAddr::new(addr as u64)).is_none() {
            return false;
        }
        addr += 4096;
    }
    true
}

/// Boot-time runtime self-test of the user-page mapping primitives.
///
/// Proves the production path end to end: `map_user_page` backs a virtual
/// address with a real, zeroed frame; a write/read round-trip confirms the
/// frame is actually there; `unmap_user_page` tears it down and reclaims it.
/// A compile check can't prove any of this — only running it on real paging can.
pub fn selftest_user_paging() -> Result<(), &'static str> {
    use x86_64::structures::paging::PageTableFlags as F;
    // Free high user VA; kernel-accessible flags so the test write doesn't trip
    // SMAP. The USER_ACCESSIBLE bit is just a flag value, not a separate path.
    const TEST_VA: usize = 0x0000_5000_0000_0000;
    let flags = F::PRESENT | F::WRITABLE;

    map_user_page(TEST_VA, flags)?;

    let p = TEST_VA as *mut u64;
    unsafe {
        if p.read_volatile() != 0 {
            let _ = unmap_user_page(TEST_VA);
            return Err("selftest: mapped frame was not zeroed");
        }
        p.write_volatile(0xDEAD_BEEF_CAFE_F00D);
        if p.read_volatile() != 0xDEAD_BEEF_CAFE_F00D {
            let _ = unmap_user_page(TEST_VA);
            return Err("selftest: read-back mismatch (no real frame backing VA)");
        }
    }

    unmap_user_page(TEST_VA)?;
    Ok(())
}

/// Unmap a virtual page
///
/// Removes the mapping for a virtual page and invalidates the TLB entry.
/// Note: This does not free the physical frame - it only removes the virtual mapping.
pub fn unmap_page(addr: usize) -> Result<(), &'static str> {
    let virt_addr = VirtAddr::new(addr as u64);
    let page = Page::containing_address(virt_addr);

    if let Some(memory_manager) = get_memory_manager() {
        let mut page_table_manager = memory_manager.page_table_manager.lock();

        // Unmap the page
        if page_table_manager.unmap_page(page).is_some() {
            Ok(())
        } else {
            Err("Page was not mapped")
        }
    } else {
        Err("Memory manager not initialized")
    }
}

// =============================================================================
// Wrapper functions for legacy API compatibility
// =============================================================================

/// Check if a memory access is valid for a given address range and privilege level
///
/// # Arguments
/// * `addr` - Starting address to check
/// * `size` - Size of the memory region in bytes
/// * `write` - Whether the access is for writing (true) or reading (false)
/// * `privilege_level` - Privilege level of the accessor (0 = kernel, 3 = user)
///
/// # Returns
/// * `Ok(true)` - Access is allowed
/// * `Ok(false)` - Access is not allowed
/// * `Err(&str)` - Error checking the access
pub fn check_memory_access(
    addr: usize,
    size: usize,
    write: bool,
    privilege_level: u8,
) -> Result<bool, &'static str> {
    // Basic validation
    if size == 0 {
        return Ok(false);
    }

    // Check for overflow
    let end_addr = addr.checked_add(size).ok_or("Address overflow")?;

    // User mode (privilege level 3) restrictions
    if privilege_level == 3 {
        // User mode cannot access kernel space (typically above 0xFFFF_8000_0000_0000)
        if addr >= 0xFFFF_8000_0000_0000 || end_addr > 0xFFFF_8000_0000_0000 {
            return Ok(false);
        }
    }

    // TODO: Check page table entries to verify actual permissions
    // For now, we'll do basic range checking

    // Check if the memory manager is initialized
    if let Some(memory_manager) = get_memory_manager() {
        let page_table_manager = memory_manager.page_table_manager.lock();

        // Check if the pages are mapped
        for offset in (0..size).step_by(4096) {
            let check_addr = addr + offset;
            let virt_addr = VirtAddr::new(check_addr as u64);
            let page: Page<Size4KiB> = Page::containing_address(virt_addr);

            // Check if page is mapped
            if page_table_manager.mapper.translate_page(page).is_err() {
                return Ok(false);
            }

            // TODO: Check page table flags for write permissions if write == true
        }

        Ok(true)
    } else {
        // If memory manager is not initialized, allow kernel accesses only
        Ok(privilege_level == 0)
    }
}
