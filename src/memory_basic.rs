//! Basic Memory Management for RustOS
//!
//! Simple memory management without heap allocation (for incremental development)

use bootloader::bootinfo::MemoryRegion;

/// Page size constants
pub const PAGE_SIZE: usize = 4096;

/// Memory layout constants for virtual address space
/// Note: Using address well past kernel load area, identity-mapped by bootloader
pub const KERNEL_HEAP_START: usize = 0x_0080_0000; // 8MB - past kernel, safe region
pub const KERNEL_HEAP_SIZE: usize = 16 * 1024 * 1024; // 16 MiB heap

/// Simple memory statistics
#[derive(Debug, Clone)]
pub struct MemoryStats {
    pub total_memory: usize,
    pub usable_memory: usize,
    pub memory_regions: usize,
}

/// Analyze memory map from bootloader
pub fn analyze_memory_map(memory_regions: &[MemoryRegion]) -> MemoryStats {
    let mut total_memory = 0;
    let mut usable_memory = 0;
    let memory_regions_count = memory_regions.len();

    for region in memory_regions {
        let size = region.range.end_addr() - region.range.start_addr();
        total_memory += size as usize;

        if region.region_type == bootloader::bootinfo::MemoryRegionType::Usable {
            usable_memory += size as usize;
        }
    }

    MemoryStats {
        total_memory,
        usable_memory,
        memory_regions: memory_regions_count,
    }
}

/// Initialize memory management system (simplified)
pub fn init_memory(
    memory_regions: &[MemoryRegion],
    _physical_memory_offset: x86_64::VirtAddr,
) -> Result<MemoryStats, &'static str> {
    // For now, just analyze the memory map without setting up heap
    let stats = analyze_memory_map(memory_regions);

    // Store stats for health monitoring
    store_memory_stats(stats.clone());

    Ok(stats)
}

/// Utility function to align up to nearest boundary
pub fn align_up(addr: usize, align: usize) -> usize {
    (addr + align - 1) & !(align - 1)
}

/// Utility function to align down to nearest boundary
pub fn align_down(addr: usize, align: usize) -> usize {
    addr & !(align - 1)
}

/// Global memory statistics for health monitoring
static mut GLOBAL_MEMORY_STATS: Option<MemoryStats> = None;

/// Store memory statistics for later retrieval
pub fn store_memory_stats(stats: MemoryStats) {
    unsafe {
        GLOBAL_MEMORY_STATS = Some(stats);
    }
}

/// Get current memory statistics for health monitoring
pub fn get_memory_stats() -> Result<MemoryStats, &'static str> {
    unsafe {
        GLOBAL_MEMORY_STATS
            .clone()
            .ok_or("Memory statistics not available")
    }
}

pub static HEAP_PHYS_START: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
pub static HEAP_PHYS_SIZE: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// Initialize the kernel heap (simplified)
pub fn init_heap(allocator: &linked_list_allocator::LockedHeap) -> Result<(), &'static str> {
    // Use the static heap region defined in memory constants
    // For early boot, we use a simple approach
    unsafe {
        allocator.lock().init(KERNEL_HEAP_START, KERNEL_HEAP_SIZE);
    }
    HEAP_PHYS_START.store(
        KERNEL_HEAP_START as u64,
        core::sync::atomic::Ordering::SeqCst,
    );
    HEAP_PHYS_SIZE.store(
        KERNEL_HEAP_SIZE as u64,
        core::sync::atomic::Ordering::SeqCst,
    );
    Ok(())
}

/// Initialize the kernel heap from bootloader memory map
/// This finds usable memory and sets up the heap there
///
/// # Arguments
/// * `allocator` - The heap allocator to initialize
/// * `memory_regions` - Memory map from bootloader
/// * `physical_memory_offset` - Offset where physical memory is mapped in virtual space
pub fn init_heap_from_memory_map(
    allocator: &linked_list_allocator::LockedHeap,
    memory_regions: &[MemoryRegion],
    physical_memory_offset: u64,
) -> Result<(), &'static str> {
    // Find a usable memory region that's large enough for the heap
    // Skip the first 1MB to avoid conflicts with low memory
    const MIN_HEAP_ADDR: u64 = 0x10_0000; // 1MB
    const DESIRED_HEAP_SIZE: usize = 16 * 1024 * 1024; // 16MB

    // Find the highest usable memory region that's large enough for the heap.
    // Using the last region avoids overlap with the kernel image which is
    // typically loaded at low physical addresses.
    let mut best_region: Option<(u64, usize)> = None;
    for region in memory_regions {
        if region.region_type != bootloader::bootinfo::MemoryRegionType::Usable {
            continue;
        }

        let phys_start = region.range.start_addr();
        let phys_end = region.range.end_addr();
        let size = (phys_end - phys_start) as usize;

        // Skip regions that start too low
        if phys_start < MIN_HEAP_ADDR {
            continue;
        }

        // Check if region is large enough
        if size >= DESIRED_HEAP_SIZE {
            best_region = Some((phys_start, size));
        }
    }

    if let Some((phys_start, size)) = best_region {
        // Convert physical address to virtual address using the offset
        let virt_start = phys_start + physical_memory_offset;
        let heap_size = DESIRED_HEAP_SIZE.min(size);

        unsafe {
            allocator.lock().init(virt_start as usize, heap_size);
        }
        HEAP_PHYS_START.store(phys_start, core::sync::atomic::Ordering::SeqCst);
        HEAP_PHYS_SIZE.store(heap_size as u64, core::sync::atomic::Ordering::SeqCst);
        return Ok(());
    }

    Err("No suitable memory region found for heap")
}
