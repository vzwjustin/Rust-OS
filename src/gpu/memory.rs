//! Advanced GPU Memory Management for RustOS
//!
//! This module provides comprehensive GPU memory management including:
//! - VRAM allocation and management
//! - GPU page table management
//! - DMA buffer handling
//! - Memory bandwidth optimization
//! - Cross-GPU memory sharing
//! - Memory-mapped GPU access
//! - GPU memory defragmentation

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::ptr::NonNull;
use core::sync::atomic::AtomicU64;
use lazy_static::lazy_static;
use spin::Mutex;
use x86_64::structures::paging::PhysFrame;
use x86_64::{PhysAddr, VirtAddr};

use super::{GPUCapabilities, GPUTier, GPUVendor};

/// GPU memory statistics structure
#[derive(Debug)]
pub struct GPUMemoryStats {
    pub total_transfers: AtomicU64,
    pub total_allocations: AtomicU64,
    pub total_deallocations: AtomicU64,
    pub peak_memory_usage: AtomicU64,
}

impl GPUMemoryStats {
    pub const fn new() -> Self {
        Self {
            total_transfers: AtomicU64::new(0),
            total_allocations: AtomicU64::new(0),
            total_deallocations: AtomicU64::new(0),
            peak_memory_usage: AtomicU64::new(0),
        }
    }
}

/// GPU memory types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GPUMemoryType {
    VRAM,          // Dedicated GPU memory
    SharedSystem,  // Shared system memory (Intel iGPU)
    UnifiedMemory, // Unified memory architecture
    HostVisible,   // Host-visible GPU memory
    DeviceLocal,   // Device-local GPU memory
}

/// Memory allocation flags
#[derive(Debug, Clone, Copy)]
pub struct MemoryFlags {
    pub readable: bool,
    pub writable: bool,
    pub executable: bool,
    pub cached: bool,
    pub coherent: bool,
    pub persistent: bool,
}

impl MemoryFlags {
    pub const DEFAULT: Self = Self {
        readable: true,
        writable: true,
        executable: false,
        cached: true,
        coherent: false,
        persistent: false,
    };

    pub const VERTEX_BUFFER: Self = Self {
        readable: false,
        writable: true,
        executable: false,
        cached: false,
        coherent: true,
        persistent: false,
    };

    pub const TEXTURE: Self = Self {
        readable: true,
        writable: false,
        executable: false,
        cached: true,
        coherent: false,
        persistent: true,
    };

    pub const COMPUTE_BUFFER: Self = Self {
        readable: true,
        writable: true,
        executable: false,
        cached: false,
        coherent: true,
        persistent: false,
    };
}

/// GPU memory allocation descriptor
#[derive(Debug, Clone)]
pub struct MemoryAllocation {
    pub id: u32,
    pub gpu_address: u64,
    pub host_address: Option<NonNull<u8>>,
    pub size: usize,
    pub alignment: usize,
    pub memory_type: GPUMemoryType,
    pub flags: MemoryFlags,
    pub owner_process: Option<u32>,
    pub reference_count: u32,
}

// SAFETY: MemoryAllocation is safe to send between threads as the NonNull<u8> points to
// properly allocated GPU memory that is managed by the kernel
unsafe impl Send for MemoryAllocation {}
unsafe impl Sync for MemoryAllocation {}

/// GPU host memory allocation tracking
#[derive(Debug)]
struct GPUHostAllocation {
    virt_addr: u64,
    size: usize,
    pages: Vec<x86_64::structures::paging::PhysFrame>,
}

/// GPU page table entry
#[derive(Debug, Clone, Copy)]
pub struct GPUPageTableEntry {
    pub physical_address: u64,
    pub flags: u32,
    pub valid: bool,
    pub readable: bool,
    pub writable: bool,
    pub cached: bool,
}

/// DMA buffer for GPU-CPU data transfers
#[derive(Debug)]
pub struct DMABuffer {
    pub id: u32,
    pub cpu_address: NonNull<u8>,
    pub gpu_address: u64,
    pub size: usize,
    pub direction: DMADirection,
    pub coherent: bool,
    pub in_use: bool,
}

// SAFETY: DMABuffer is safe to send between threads as the NonNull<u8> points to
// properly allocated GPU/DMA memory that is managed by the kernel
unsafe impl Send for DMABuffer {}
unsafe impl Sync for DMABuffer {}

/// DMA transfer direction
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DMADirection {
    CPUToGPU,
    GPUToCPU,
    Bidirectional,
}

/// Memory bandwidth optimization settings
#[derive(Debug, Clone)]
pub struct BandwidthOptimization {
    pub compression_enabled: bool,
    pub prefetch_enabled: bool,
    pub cache_policy: CachePolicy,
    pub memory_clock_boost: bool,
    pub interleaving_enabled: bool,
}

/// Memory cache policy
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CachePolicy {
    WriteBack,
    WriteThrough,
    WriteAround,
    NoCache,
}

/// Cross-GPU memory sharing configuration
#[derive(Debug)]
pub struct CrossGPUSharing {
    pub enabled: bool,
    pub peer_gpus: Vec<u32>, // GPU IDs that can share memory
    pub shared_pools: Vec<SharedMemoryPool>,
    pub bandwidth_priority: BandwidthPriority,
}

/// Shared memory pool between GPUs
#[derive(Debug)]
pub struct SharedMemoryPool {
    pub pool_id: u32,
    pub size: usize,
    pub participating_gpus: Vec<u32>,
    pub base_address: u64,
    pub allocation_bitmap: Vec<u64>, // Bitfield for free/used blocks
}

/// Memory bandwidth priority for multi-GPU systems
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BandwidthPriority {
    Balanced,
    LatencyOptimized,
    ThroughputOptimized,
    PowerEfficient,
}

/// GPU memory manager for a single GPU
pub struct GPUMemoryManager {
    pub gpu_id: u32,
    pub total_memory: usize,
    pub available_memory: usize,
    pub memory_type: GPUMemoryType,
    pub allocations: BTreeMap<u32, MemoryAllocation>,
    pub page_table: Vec<GPUPageTableEntry>,
    pub dma_buffers: Vec<DMABuffer>,
    pub free_blocks: BTreeMap<usize, Vec<u64>>, // Size -> list of addresses
    pub bandwidth_optimization: BandwidthOptimization,
    pub stats: GPUMemoryStats,
    pub next_allocation_id: u32,
    pub next_dma_id: u32,
    pub fragmentation_threshold: f32,
    pub compaction_enabled: bool,
    /// Base virtual address of the GPU-coherent memory window assigned to this
    /// GPU during init. 0 means no real GPU is present and coherent-memory
    /// operations should no-op/return an error rather than use fake addresses.
    pub gpu_memory_base: u64,
    /// Size in bytes of the GPU-coherent memory window starting at
    /// `gpu_memory_base`. 0 means the window is unconfigured.
    pub gpu_memory_size: u64,
}

impl GPUMemoryManager {
    pub fn new(gpu_id: u32, capabilities: &GPUCapabilities) -> Self {
        let memory_type = match capabilities.vendor {
            GPUVendor::Intel => GPUMemoryType::SharedSystem,
            GPUVendor::Nvidia | GPUVendor::AMD => GPUMemoryType::VRAM,
            GPUVendor::Unknown => GPUMemoryType::SharedSystem,
        };

        let total_memory = capabilities.memory_size as usize;

        // Initialize free blocks with the entire memory range
        let mut free_blocks = BTreeMap::new();
        free_blocks.insert(total_memory, vec![0]);

        // Configure bandwidth optimization based on GPU tier
        let bandwidth_optimization = BandwidthOptimization {
            compression_enabled: matches!(
                capabilities.tier,
                GPUTier::Performance | GPUTier::HighEnd | GPUTier::Enthusiast
            ),
            prefetch_enabled: true,
            cache_policy: match capabilities.tier {
                GPUTier::Entry | GPUTier::Budget => CachePolicy::WriteBack,
                GPUTier::Mainstream => CachePolicy::WriteThrough,
                _ => CachePolicy::WriteBack,
            },
            memory_clock_boost: matches!(capabilities.tier, GPUTier::HighEnd | GPUTier::Enthusiast),
            interleaving_enabled: total_memory > 2 * 1024 * 1024 * 1024, // Enable for >2GB
        };

        Self {
            gpu_id,
            total_memory,
            available_memory: total_memory,
            memory_type,
            allocations: BTreeMap::new(),
            page_table: Vec::new(),
            dma_buffers: Vec::new(),
            free_blocks,
            bandwidth_optimization,
            stats: GPUMemoryStats::new(),
            next_allocation_id: 1,
            next_dma_id: 1,
            fragmentation_threshold: 0.3, // 30% fragmentation threshold
            compaction_enabled: true,
            gpu_memory_base: 0,
            gpu_memory_size: 0,
        }
    }

    /// Configure the GPU-coherent memory window for this GPU.
    ///
    /// Called during GPU init once the real aperture base/size are known
    /// (e.g. from a PCI BAR or firmware table). Until this is called, the
    /// coherent-memory mapping helpers refuse to operate so that no fake
    /// addresses are ever used.
    pub fn set_gpu_memory_window(&mut self, base: u64, size: u64) {
        self.gpu_memory_base = base;
        self.gpu_memory_size = size;
    }

    /// Allocate GPU memory with specified size and alignment
    pub fn allocate(
        &mut self,
        size: usize,
        alignment: usize,
        flags: MemoryFlags,
    ) -> Result<u32, &'static str> {
        if size == 0 {
            return Err("Cannot allocate zero-sized memory");
        }

        if size > self.available_memory {
            // Try defragmentation first
            if self.compaction_enabled {
                self.defragment_memory()?;
                if size > self.available_memory {
                    return Err("Insufficient GPU memory available");
                }
            } else {
                return Err("Insufficient GPU memory available");
            }
        }

        // Find suitable free block
        let aligned_size = self.align_size(size, alignment);
        let gpu_address = self.find_free_block(aligned_size, alignment)?;

        // Create allocation
        let allocation_id = self.next_allocation_id;
        self.next_allocation_id += 1;

        let allocation = MemoryAllocation {
            id: allocation_id,
            gpu_address,
            host_address: None, // Will be mapped if needed
            size: aligned_size,
            alignment,
            memory_type: self.memory_type,
            flags,
            owner_process: None, // Could be set by process manager
            reference_count: 1,
        };

        // Update available memory
        self.available_memory -= aligned_size;

        // Remove from free blocks and update
        self.remove_from_free_blocks(gpu_address, aligned_size);

        // Insert allocation
        self.allocations.insert(allocation_id, allocation);

        // Update page table if needed
        self.update_page_table(gpu_address, aligned_size, &flags)?;

        Ok(allocation_id)
    }

    /// Free GPU memory allocation
    pub fn free(&mut self, allocation_id: u32) -> Result<(), &'static str> {
        let allocation = self
            .allocations
            .remove(&allocation_id)
            .ok_or("Invalid allocation ID")?;

        // Update available memory
        self.available_memory += allocation.size;

        // Add back to free blocks
        self.add_to_free_blocks(allocation.gpu_address, allocation.size);

        // Clear page table entries
        self.clear_page_table(allocation.gpu_address, allocation.size);

        // Unmap host memory if mapped
        if allocation.host_address.is_some() {
            self.unmap_memory(allocation.gpu_address)?;
        }

        // Merge adjacent free blocks for defragmentation
        self.merge_free_blocks(allocation.gpu_address, allocation.size);

        Ok(())
    }

    /// Map GPU memory to host address space
    pub fn map_memory(&mut self, allocation_id: u32) -> Result<NonNull<u8>, &'static str> {
        // Check if already mapped first
        if let Some(allocation) = self.allocations.get(&allocation_id) {
            if let Some(host_ptr) = allocation.host_address {
                return Ok(host_ptr);
            }
        } else {
            return Err("Invalid allocation ID");
        }

        // Get allocation size without holding a mutable reference
        let allocation_size = self
            .allocations
            .get(&allocation_id)
            .ok_or("Invalid allocation ID")?
            .size;

        // Production memory mapping - allocate host memory
        let host_ptr = self.allocate_host_memory(allocation_size)?;

        // Now we can safely get the mutable reference
        let allocation = self
            .allocations
            .get_mut(&allocation_id)
            .ok_or("Invalid allocation ID")?;
        allocation.host_address = Some(host_ptr);

        Ok(host_ptr)
    }

    /// Unmap GPU memory from host address space
    pub fn unmap_memory(&mut self, gpu_address: u64) -> Result<(), &'static str> {
        // Find allocation by GPU address
        let mut allocation_id = None;
        {
            // Separate scope for immutable borrow
            for (&id, allocation) in &self.allocations {
                if allocation.gpu_address == gpu_address {
                    allocation_id = Some(id);
                    break;
                }
            }
        }

        if let Some(id) = allocation_id {
            // Get the host address and size first
            let (host_ptr, size) = {
                let allocation = self.allocations.get(&id).unwrap();
                (allocation.host_address, allocation.size)
            };

            if let Some(host_ptr) = host_ptr {
                self.free_host_memory(host_ptr, size)?;
                let allocation = self.allocations.get_mut(&id).unwrap();
                allocation.host_address = None;
                return Ok(());
            }
        }

        Err("Memory not mapped or invalid address")
    }

    /// Create DMA buffer for efficient GPU-CPU transfers
    pub fn create_dma_buffer(
        &mut self,
        size: usize,
        direction: DMADirection,
    ) -> Result<u32, &'static str> {
        if size == 0 {
            return Err("Cannot create zero-sized DMA buffer");
        }

        // Allocate GPU memory for DMA buffer
        let flags = MemoryFlags {
            coherent: true,
            cached: false,
            ..MemoryFlags::DEFAULT
        };

        let allocation_id = self.allocate(size, 4096, flags)?; // 4KB alignment for DMA
        let allocation = self.allocations.get(&allocation_id).unwrap();

        // Allocate host memory
        let cpu_address = self.allocate_host_memory(size)?;

        let dma_id = self.next_dma_id;
        self.next_dma_id += 1;

        let dma_buffer = DMABuffer {
            id: dma_id,
            cpu_address,
            gpu_address: allocation.gpu_address,
            size,
            direction,
            coherent: true,
            in_use: false,
        };

        self.dma_buffers.push(dma_buffer);

        Ok(dma_id)
    }

    /// Destroy DMA buffer
    pub fn destroy_dma_buffer(&mut self, dma_id: u32) -> Result<(), &'static str> {
        let buffer_index = self
            .dma_buffers
            .iter()
            .position(|b| b.id == dma_id)
            .ok_or("Invalid DMA buffer ID")?;

        let buffer = self.dma_buffers.remove(buffer_index);

        // Free host memory
        self.free_host_memory(buffer.cpu_address, buffer.size)?;

        // Free GPU memory (find allocation by address)
        let mut allocation_to_free = None;
        for (&id, allocation) in &self.allocations {
            if allocation.gpu_address == buffer.gpu_address {
                allocation_to_free = Some(id);
                break;
            }
        }

        if let Some(allocation_id) = allocation_to_free {
            self.free(allocation_id)?;
        }

        Ok(())
    }

    /// Perform DMA transfer
    pub fn dma_transfer(
        &mut self,
        dma_id: u32,
        offset: usize,
        size: usize,
    ) -> Result<(), &'static str> {
        // First, gather the information we need and validate
        let (cpu_address, gpu_address, direction, _buffer_size) = {
            let buffer = self
                .dma_buffers
                .iter()
                .find(|b| b.id == dma_id)
                .ok_or("Invalid DMA buffer ID")?;

            if buffer.in_use {
                return Err("DMA buffer is currently in use");
            }

            if offset + size > buffer.size {
                return Err("Transfer size exceeds buffer size");
            }

            (
                buffer.cpu_address.as_ptr() as u64,
                buffer.gpu_address,
                buffer.direction,
                buffer.size,
            )
        };

        // Mark buffer as in use
        let buffer = self
            .dma_buffers
            .iter_mut()
            .find(|b| b.id == dma_id)
            .ok_or("Invalid DMA buffer ID")?;
        buffer.in_use = true;

        // Perform the transfer
        let result = match direction {
            DMADirection::CPUToGPU => self.perform_memory_transfer(
                cpu_address + offset as u64,
                gpu_address + offset as u64,
                size,
            ),
            DMADirection::GPUToCPU => self.perform_memory_transfer(
                gpu_address + offset as u64,
                cpu_address + offset as u64,
                size,
            ),
            DMADirection::Bidirectional => self.perform_memory_transfer(
                cpu_address + offset as u64,
                gpu_address + offset as u64,
                size,
            ),
        };

        // Mark buffer as not in use
        let buffer = self
            .dma_buffers
            .iter_mut()
            .find(|b| b.id == dma_id)
            .ok_or("Invalid DMA buffer ID")?;
        buffer.in_use = false;

        result
    }

    /// Optimize memory bandwidth
    pub fn optimize_bandwidth(&mut self) -> Result<(), &'static str> {
        // Enable compression for large allocations
        if self.bandwidth_optimization.compression_enabled {
            self.enable_memory_compression()?;
        }

        // Adjust cache policies based on usage patterns
        self.optimize_cache_policy()?;

        // Enable memory clock boost if available
        if self.bandwidth_optimization.memory_clock_boost {
            self.boost_memory_clock()?;
        }

        // Configure memory interleaving for multi-channel memory
        if self.bandwidth_optimization.interleaving_enabled {
            self.configure_memory_interleaving()?;
        }

        Ok(())
    }

    /// Defragment GPU memory
    pub fn defragment_memory(&mut self) -> Result<(), &'static str> {
        let fragmentation_ratio = self.calculate_fragmentation_ratio();

        if fragmentation_ratio < self.fragmentation_threshold {
            return Ok(()); // No defragmentation needed
        }

        // Collect all allocations that can be moved
        let mut movable_allocations: Vec<u32> = self
            .allocations
            .keys()
            .filter(|&&id| self.can_move_allocation(id))
            .cloned()
            .collect();

        // Sort by size (smallest first for better packing)
        movable_allocations.sort_by(|&a, &b| {
            let size_a = self.allocations[&a].size;
            let size_b = self.allocations[&b].size;
            size_a.cmp(&size_b)
        });

        // Compact memory by moving allocations
        let mut current_address = 0u64;
        for allocation_id in movable_allocations {
            let old_address = {
                let allocation = self.allocations.get(&allocation_id).unwrap();
                allocation.gpu_address
            };

            // Find new location at current_address
            if old_address != current_address {
                // Move allocation
                self.move_allocation(allocation_id, current_address)?;
            }

            let allocation = self.allocations.get(&allocation_id).unwrap();
            current_address += allocation.size as u64;
        }

        // Rebuild free block list
        self.rebuild_free_blocks();

        Ok(())
    }

    /// Get memory statistics
    pub fn get_statistics(&self) -> GPUMemoryStatistics {
        let allocated_memory = self.total_memory - self.available_memory;
        let largest_free_block = self.find_largest_free_block();
        let fragmentation_ratio = self.calculate_fragmentation_ratio();

        GPUMemoryStatistics {
            total_memory: self.total_memory,
            available_memory: self.available_memory,
            allocated_memory,
            allocation_count: self.allocations.len(),
            dma_buffer_count: self.dma_buffers.len(),
            largest_free_block,
            fragmentation_ratio,
            memory_utilization: (allocated_memory as f32 / self.total_memory as f32) * 100.0,
        }
    }

    // Private helper methods

    fn align_size(&self, size: usize, alignment: usize) -> usize {
        (size + alignment - 1) & !(alignment - 1)
    }

    fn find_free_block(&mut self, size: usize, alignment: usize) -> Result<u64, &'static str> {
        // Find the smallest free block that can fit the allocation
        for (&block_size, addresses) in &mut self.free_blocks {
            if block_size >= size {
                if let Some(address) = addresses.pop() {
                    // Check alignment
                    let aligned_address =
                        (address + alignment as u64 - 1) & !(alignment as u64 - 1);
                    let alignment_padding = aligned_address - address;

                    if block_size >= size + alignment_padding as usize {
                        // Remove this block if it was the last one
                        if addresses.is_empty() {
                            self.free_blocks.remove(&block_size);
                        }

                        // Add back remainder if any
                        let remainder_size = block_size - size - alignment_padding as usize;
                        if remainder_size > 0 {
                            self.add_to_free_blocks(aligned_address + size as u64, remainder_size);
                        }

                        // Add padding back to free blocks if any
                        if alignment_padding > 0 {
                            self.add_to_free_blocks(address, alignment_padding as usize);
                        }

                        return Ok(aligned_address);
                    } else {
                        // Put the address back
                        addresses.push(address);
                    }
                }
            }
        }

        Err("No suitable free block found")
    }

    fn remove_from_free_blocks(&mut self, address: u64, size: usize) {
        if let Some(addresses) = self.free_blocks.get_mut(&size) {
            addresses.retain(|&addr| addr != address);
            if addresses.is_empty() {
                self.free_blocks.remove(&size);
            }
        }
    }

    fn add_to_free_blocks(&mut self, address: u64, size: usize) {
        self.free_blocks
            .entry(size)
            .or_insert_with(Vec::new)
            .push(address);
    }

    fn merge_free_blocks(&mut self, address: u64, size: usize) {
        // Try to merge with the block immediately before (address == prev_end)
        // and the block immediately after (address + size == next_start).
        let mut merged_address = address;
        let mut merged_size = size;

        // Look for a preceding block whose end == our start
        // We need to scan all size groups since the preceding block
        // could be any size.
        let mut found_prev: Option<(usize, u64)> = None;
        for (&block_size, addresses) in self.free_blocks.iter_mut() {
            if let Some(idx) = addresses
                .iter()
                .position(|&addr| addr + block_size as u64 == address)
            {
                found_prev = Some((block_size, addresses.remove(idx)));
                if addresses.is_empty() {
                    // Will be cleaned up below
                }
                break;
            }
        }

        if let Some((prev_size, prev_addr)) = found_prev {
            // Remove the empty entry if the vec is now empty
            self.free_blocks.retain(|_, v| !v.is_empty());
            merged_address = prev_addr;
            merged_size += prev_size;
        }

        // Look for a following block whose start == our end
        let end = merged_address + merged_size as u64;
        let mut found_next: Option<(usize, u64)> = None;
        for (&block_size, addresses) in self.free_blocks.iter_mut() {
            if let Some(idx) = addresses.iter().position(|&addr| addr == end) {
                found_next = Some((block_size, addresses.remove(idx)));
                break;
            }
        }

        if let Some((next_size, _next_addr)) = found_next {
            self.free_blocks.retain(|_, v| !v.is_empty());
            merged_size += next_size;
        }

        self.add_to_free_blocks(merged_address, merged_size);
    }

    fn update_page_table(
        &mut self,
        address: u64,
        size: usize,
        flags: &MemoryFlags,
    ) -> Result<(), &'static str> {
        let page_size = 4096; // 4KB pages
        let start_page = (address / page_size) as usize;
        let page_count = (size + page_size as usize - 1) / page_size as usize;

        // Ensure page table is large enough
        while self.page_table.len() < start_page + page_count {
            self.page_table.push(GPUPageTableEntry {
                physical_address: 0,
                flags: 0,
                valid: false,
                readable: false,
                writable: false,
                cached: false,
            });
        }

        // Update page table entries
        for i in 0..page_count {
            let page_index = start_page + i;
            self.page_table[page_index] = GPUPageTableEntry {
                physical_address: address + (i * page_size as usize) as u64,
                flags: self.create_page_flags(flags),
                valid: true,
                readable: flags.readable,
                writable: flags.writable,
                cached: flags.cached,
            };
        }

        Ok(())
    }

    fn clear_page_table(&mut self, address: u64, size: usize) {
        let page_size = 4096;
        let start_page = (address / page_size) as usize;
        let page_count = (size + page_size as usize - 1) / page_size as usize;

        for i in 0..page_count {
            let page_index = start_page + i;
            if page_index < self.page_table.len() {
                self.page_table[page_index].valid = false;
                self.page_table[page_index].physical_address = 0;
            }
        }
    }

    fn create_page_flags(&self, flags: &MemoryFlags) -> u32 {
        let mut page_flags = 0u32;
        if flags.readable {
            page_flags |= 0x1;
        }
        if flags.writable {
            page_flags |= 0x2;
        }
        if flags.executable {
            page_flags |= 0x4;
        }
        if flags.cached {
            page_flags |= 0x8;
        }
        if flags.coherent {
            page_flags |= 0x10;
        }
        page_flags
    }

    fn allocate_host_memory(&self, size: usize) -> Result<NonNull<u8>, &'static str> {
        // Production implementation using actual memory allocation with GPU coherency

        // Calculate number of pages needed (align to page boundary)
        let page_size = 4096;
        let pages_needed = (size + page_size - 1) / page_size;

        // Allocate contiguous physical pages for GPU DMA coherency
        let physical_pages = self.allocate_contiguous_pages(pages_needed)?;

        // Map pages to virtual address space with GPU-appropriate flags
        let virt_addr = self.map_gpu_coherent_memory(&physical_pages)?;

        // Configure GPU memory controller for this allocation
        self.configure_gpu_memory_access(virt_addr, size)?;

        // Store allocation info for cleanup
        let alloc_info = GPUHostAllocation {
            virt_addr,
            size,
            pages: physical_pages,
        };

        // Track allocation in GPU memory manager
        self.track_allocation(alloc_info)?;

        self.stats
            .total_allocations
            .fetch_add(1, core::sync::atomic::Ordering::Relaxed);

        NonNull::new(virt_addr as *mut u8).ok_or("Invalid virtual address")
    }

    /// Allocate contiguous physical pages for GPU DMA
    fn allocate_contiguous_pages(&self, page_count: usize) -> Result<Vec<PhysFrame>, &'static str> {
        let mut allocated_pages = Vec::new();

        // Try to allocate contiguous pages for better GPU performance
        let start_frame = self.find_contiguous_frames(page_count)?;

        for i in 0..page_count {
            let frame_addr = start_frame + i * 4096;
            let frame = PhysFrame::containing_address(PhysAddr::new(frame_addr as u64));

            if !self.allocate_physical_frame(frame) {
                // Clean up on failure
                for allocated_frame in allocated_pages {
                    self.deallocate_physical_frame(allocated_frame);
                }
                return Err("Failed to allocate contiguous physical frames");
            }

            allocated_pages.push(frame);
        }

        Ok(allocated_pages)
    }

    /// Find contiguous physical memory frames
    fn find_contiguous_frames(&self, page_count: usize) -> Result<usize, &'static str> {
        // Scan physical memory for contiguous free frames
        // This is a simplified implementation - production would use a buddy allocator

        let start_addr = 0x100000; // Start after 1MB
        let end_addr = 0x40000000; // Search up to 1GB
        let page_size = 4096;

        for addr in (start_addr..end_addr).step_by(page_size) {
            if self.check_contiguous_free(addr, page_count * page_size) {
                return Ok(addr);
            }
        }

        Err("No contiguous physical memory available")
    }

    /// Check if a range of physical memory is free
    fn check_contiguous_free(&self, start_addr: usize, size: usize) -> bool {
        // Without a per-frame "is allocated" query on the buddy allocator,
        // we verify that the memory manager has enough free frames in the
        // relevant zone and that the range is within valid physical memory.
        if let Some(mm) = crate::memory::get_memory_manager() {
            let page_size = 4096;
            let pages = (size + page_size - 1) / page_size;
            let zone = if start_addr < 0x100_0000 {
                crate::memory::MemoryZone::Dma
            } else {
                crate::memory::MemoryZone::Normal
            };
            let stats = mm.get_zone_stats();
            let zone_stats = &stats[zone as usize];
            let free_frames = zone_stats
                .total_frames
                .saturating_sub(zone_stats.allocated_frames);
            // Check that the zone has enough free frames
            if free_frames < pages {
                return false;
            }
            // Range must be within valid physical memory bounds
            start_addr >= 0x1000000 && (start_addr + size) < 0x4000_0000
        } else {
            start_addr >= 0x1000000 && start_addr + size < 0x40000000
        }
    }

    /// Allocate a specific physical frame from the buddy allocator
    fn allocate_physical_frame(&self, frame: PhysFrame) -> bool {
        if let Some(mm) = crate::memory::get_memory_manager() {
            // Try to allocate from the DMA zone (below 16 MB) or normal zone
            let zone = if frame.start_address().as_u64() < 0x100_0000 {
                crate::memory::MemoryZone::Dma
            } else {
                crate::memory::MemoryZone::Normal
            };
            // The buddy allocator doesn't support allocating a specific
            // frame, so we allocate any frame and check if it matches.
            // If it doesn't match, we free it and return false.
            if let Some(allocated) = mm.allocate_frame_in_zone(zone) {
                if allocated.start_address() == frame.start_address() {
                    return true;
                }
                // Wrong frame — free it and report failure
                mm.deallocate_frame(allocated, zone);
                return false;
            }
            false
        } else {
            false
        }
    }

    /// Deallocate a physical frame back to the buddy allocator
    fn deallocate_physical_frame(&self, frame: PhysFrame) {
        if let Some(mm) = crate::memory::get_memory_manager() {
            let zone = if frame.start_address().as_u64() < 0x100_0000 {
                crate::memory::MemoryZone::Dma
            } else {
                crate::memory::MemoryZone::Normal
            };
            mm.deallocate_frame(frame, zone);
        }
    }

    /// Map GPU coherent memory with appropriate caching flags
    fn map_gpu_coherent_memory(&self, pages: &[PhysFrame]) -> Result<u64, &'static str> {
        // Use the GPU-coherent memory window configured during GPU init.
        // If no real GPU is present (base == 0), refuse rather than fall back
        // to a hardcoded fake address range.
        if self.gpu_memory_base == 0 || self.gpu_memory_size == 0 {
            return Err("GPU coherent memory window not configured");
        }

        let needed = (pages.len() * 4096) as u64;
        if needed > self.gpu_memory_size {
            return Err("Requested mapping exceeds GPU coherent memory window");
        }

        let virt_base = self.gpu_memory_base;

        for (i, frame) in pages.iter().enumerate() {
            let virt_addr = virt_base + (i * 4096) as u64;
            let phys_addr = frame.start_address();

            // Map with GPU-coherent flags
            if let Err(_) = self.map_page_gpu_coherent(virt_addr, phys_addr.as_u64()) {
                // Clean up on failure
                for j in 0..i {
                    let cleanup_addr = virt_base + (j * 4096) as u64;
                    let _ = self.unmap_page(cleanup_addr);
                }
                return Err("Failed to map GPU coherent memory");
            }
        }

        Ok(virt_base)
    }

    /// Map a page with GPU-coherent caching attributes
    fn map_page_gpu_coherent(&self, virt_addr: u64, phys_addr: u64) -> Result<(), &'static str> {
        // GPU-coherent memory uses write-combining (write-through) caching so
        // streaming writes are coalesced without polluting the CPU cache.
        // Map through the real kernel page-table manager rather than writing
        // page-table entries to a hardcoded address.

        if self.gpu_memory_base == 0 || self.gpu_memory_size == 0 {
            return Err("GPU coherent memory window not configured");
        }

        let window_end = self.gpu_memory_base.saturating_add(self.gpu_memory_size);
        if virt_addr < self.gpu_memory_base || virt_addr >= window_end {
            return Err("Virtual address outside GPU memory window");
        }

        if phys_addr >= 0x1_0000_0000 {
            return Err("Physical address above 4GB not supported");
        }

        // PRESENT | WRITABLE | WRITE_COMBINING (write-through) for GPU-coherent
        // memory. This goes through the active OffsetPageTable via the global
        // memory manager, allocating any intermediate page tables as needed.
        let flags = crate::memory::MemoryFlags::PRESENT
            | crate::memory::MemoryFlags::WRITABLE
            | crate::memory::MemoryFlags::WRITE_COMBINING;

        crate::memory::map_physical_memory(virt_addr as usize, phys_addr as usize, flags)
    }

    /// Configure GPU memory controller for new allocation
    fn configure_gpu_memory_access(&self, virt_addr: u64, size: usize) -> Result<(), &'static str> {
        // Configure GPU memory management unit (MMU) if present
        match self.get_gpu_vendor() {
            GPUVendor::Intel => self.configure_intel_gpu_mmu(virt_addr, size),
            GPUVendor::AMD => self.configure_amd_gpu_mmu(virt_addr, size),
            GPUVendor::Nvidia => self.configure_nvidia_gpu_mmu(virt_addr, size),
            GPUVendor::Unknown => Ok(()), // Skip configuration for unknown GPUs
        }
    }

    /// Configure Intel GPU memory management (GGTT).
    ///
    /// Writes Global GTT (GGTT) PTEs via the GTTMMADR BAR (PCI BAR 0).
    /// On Gen6 the GTTADR starts at 2 MB offset within the 4 MB GTTMMADR;
    /// on Gen8+ it starts at 8 MB within the 16 MB GTTMMADR.
    ///
    /// Each GGTT PTE is 8 bytes (gen8_pte_t = u64):
    ///   bit 0     = present (GEN8_PAGE_PRESENT)
    ///   bit 1     = read/write (GEN8_PAGE_RW)
    ///   bits 45:12 = physical address (GEN12_GGTT_PTE_ADDR_MASK)
    ///
    /// After writing PTEs we flush via GFX_FLSH_CNTL_GEN6 (offset 0x101008).
    ///
    /// Reference: drivers/gpu/drm/i915/gt/intel_ggtt.c
    ///           gen8_ggtt_insert_page(), gen8_ggtt_pte_encode()
    fn configure_intel_gpu_mmu(&self, virt_addr: u64, size: usize) -> Result<(), &'static str> {
        // Find the Intel GPU PCI device for this gpu_id.
        let display_devices = crate::pci::get_devices_by_class(crate::pci::PciClass::Display);
        let intel_dev = display_devices
            .iter()
            .filter(|d| d.vendor_id == 0x8086)
            .nth(self.gpu_id as usize)
            .ok_or("No Intel GPU PCI device found")?;

        // Read BAR 0 (GTTMMADR) from PCI config space at offset 0x10.
        let bar0_raw =
            read_pci_config_dword(intel_dev.bus, intel_dev.device, intel_dev.function, 0x10);
        let bar1_raw =
            read_pci_config_dword(intel_dev.bus, intel_dev.device, intel_dev.function, 0x14);
        if (bar0_raw & 0x1) != 0 {
            return Err("Intel GPU BAR0 is I/O space, not memory-mapped");
        }
        let bar0_base = ((bar0_raw & 0xFFFFFFF0) as u64) | (((bar1_raw as u64) & 0xFFFFFFFF) << 32);
        if bar0_base == 0 {
            return Err("Intel GPU BAR0 is not configured");
        }

        // Determine GTTADR offset within GTTMMADR.
        // Gen6 (Sandy Bridge through Haswell): 4 MB GTTMMADR, GTT at 2 MB.
        // Gen8+ (Broadwell+): 16 MB GTTMMADR, GTT at 8 MB.
        // We detect generation via device ID ranges.
        let gttmmadr_size = if intel_dev.device_id >= 0x1600 {
            16 * 1024 * 1024
        } else {
            4 * 1024 * 1024
        };
        let gttadr_offset = gttmmadr_size / 2;

        // Map the GTTMMADR region via the kernel direct physical mapping.
        let phys_offset = crate::memory::get_physical_memory_offset();
        if phys_offset == 0 {
            return Err("Kernel physical memory offset not initialized");
        }
        let gttmmadr_virt = phys_offset + bar0_base;
        let gtt_base = gttmmadr_virt + gttadr_offset as u64;

        // GGTT PTE encoding (gen8 style, 64-bit entries).
        const GEN8_PAGE_PRESENT: u64 = 1 << 0;
        const GEN8_PAGE_RW: u64 = 1 << 1;
        const I915_GTT_PAGE_SIZE: u64 = 4096;
        const GFX_FLSH_CNTL_GEN6_OFFSET: u64 = 0x101008;
        const GFX_FLSH_CNTL_EN: u32 = 1 << 0;

        let num_pages = (size + 4095) / 4096;
        let first_entry = virt_addr / I915_GTT_PAGE_SIZE;

        // SAFETY: We are writing to the GGTT PTE region of the Intel GPU's
        // MMIO BAR, which was validated above. The physical address from
        // virt_to_phys is the system RAM page that the GPU should access.
        unsafe {
            for i in 0..num_pages {
                let pte_offset = (first_entry + i as u64) * 8;
                let gtt_addr = gtt_base + pte_offset;

                // Translate the virtual address to physical for the PTE.
                let page_virt = virt_addr + (i as u64) * I915_GTT_PAGE_SIZE;
                let phys = match self.virt_to_phys(page_virt) {
                    Ok(p) => p,
                    Err(_) => return Err("Failed to translate virtual address for GGTT PTE"),
                };

                // Encode the PTE: present | rw | physical address (bits 45:12).
                let pte = GEN8_PAGE_PRESENT | GEN8_PAGE_RW | (phys & 0x0000_FFFF_FFFF_F000);

                // Write the 8-byte PTE to the GGTT region.
                core::ptr::write_volatile(gtt_addr as *mut u64, pte);
            }

            // Flush the GGTT writes via GFX_FLSH_CNTL_GEN6.
            let flush_reg = gttmmadr_virt + GFX_FLSH_CNTL_GEN6_OFFSET;
            core::ptr::write_volatile(flush_reg as *mut u32, GFX_FLSH_CNTL_EN);
            // Posting read to ensure the flush completes.
            let _ = core::ptr::read_volatile(flush_reg as *const u32);
        }

        Ok(())
    }

    /// Configure AMD GPU memory management (GMMU).
    ///
    /// Programs the AMD GPU's VM (GMC) registers via MMIO BAR 0 to set up
    /// the GART (Graphics Address Remapping Table) page table for the
    /// allocation at `virt_addr`.
    ///
    /// The AMD VM uses a flat page table (depth=0 for GART) where each PTE
    /// maps a 4 KB page. The page table base address, context range, and
    /// L2 cache settings are programmed via VM registers.
    ///
    /// Register offsets (from gmc_7_0_d.h):
    ///   VM_L2_CNTL          = 0x500
    ///   VM_L2_CNTL2         = 0x501
    ///   VM_L2_CNTL3         = 0x502
    ///   VM_CONTEXT0_CNTL     = 0x504
    ///   VM_CONTEXT0_CNTL2    = 0x50c
    ///   VM_CONTEXT0_PAGE_TABLE_BASE_ADDR   = 0x54f
    ///   VM_CONTEXT0_PAGE_TABLE_START_ADDR  = 0x557
    ///   VM_CONTEXT0_PAGE_TABLE_END_ADDR    = 0x55f
    ///   VM_CONTEXT0_PROTECTION_FAULT_DEFAULT_ADDR = 0x546
    ///   MC_VM_MX_L1_TLB_CNTL = 0x819
    ///
    /// Reference: drivers/gpu/drm/amd/amdgpu/gmc_v7_0.c gmc_v7_0_gart_enable()
    fn configure_amd_gpu_mmu(&self, virt_addr: u64, size: usize) -> Result<(), &'static str> {
        // Find the AMD GPU PCI device for this gpu_id.
        let display_devices = crate::pci::get_devices_by_class(crate::pci::PciClass::Display);
        let amd_dev = display_devices
            .iter()
            .filter(|d| d.vendor_id == 0x1002)
            .nth(self.gpu_id as usize)
            .ok_or("No AMD GPU PCI device found")?;

        // Read BAR 0 from PCI config space.
        let bar0_raw = read_pci_config_dword(amd_dev.bus, amd_dev.device, amd_dev.function, 0x10);
        let bar1_raw = read_pci_config_dword(amd_dev.bus, amd_dev.device, amd_dev.function, 0x14);
        if (bar0_raw & 0x1) != 0 {
            return Err("AMD GPU BAR0 is I/O space, not memory-mapped");
        }
        let bar0_base = ((bar0_raw & 0xFFFFFFF0) as u64) | (((bar1_raw as u64) & 0xFFFFFFFF) << 32);
        if bar0_base == 0 {
            return Err("AMD GPU BAR0 is not configured");
        }

        // Map via the kernel direct physical mapping.
        let phys_offset = crate::memory::get_physical_memory_offset();
        if phys_offset == 0 {
            return Err("Kernel physical memory offset not initialized");
        }
        let mmio_base = phys_offset + bar0_base;

        // AMD VM register offsets (as 32-bit register indices, each 4 bytes).
        // These are dword offsets from mmio_base.
        const VM_L2_CNTL: u64 = 0x500 * 4;
        const VM_L2_CNTL2: u64 = 0x501 * 4;
        const VM_L2_CNTL3: u64 = 0x502 * 4;
        const VM_CONTEXT0_CNTL: u64 = 0x504 * 4;
        const VM_CONTEXT0_CNTL2: u64 = 0x50c * 4;
        const VM_CONTEXT0_PAGE_TABLE_BASE_ADDR: u64 = 0x54f * 4;
        const VM_CONTEXT0_PAGE_TABLE_START_ADDR: u64 = 0x557 * 4;
        const VM_CONTEXT0_PAGE_TABLE_END_ADDR: u64 = 0x55f * 4;
        const VM_CONTEXT0_PROTECTION_FAULT_DEFAULT_ADDR: u64 = 0x546 * 4;
        const MC_VM_MX_L1_TLB_CNTL: u64 = 0x819 * 4;

        // Translate the virtual address to physical for the page table base.
        let phys = self.virt_to_phys(virt_addr)?;
        let num_pages = ((size + 4095) / 4096) as u64;

        // SAFETY: We are programming AMD GPU VM registers via MMIO. The BAR
        // was validated above. Register writes follow the Linux amdgpu
        // gmc_v7_0_gart_enable() sequence.
        unsafe {
            let reg = |offset: u64| -> *mut u32 { (mmio_base + offset) as *mut u32 };

            // 1. Enable L1 TLB.
            let mut tmp = core::ptr::read_volatile(reg(MC_VM_MX_L1_TLB_CNTL));
            tmp |= 1; // ENABLE_L1_TLB
            tmp |= 1 << 1; // ENABLE_L1_FRAGMENT_PROCESSING
            tmp |= 3 << 4; // SYSTEM_ACCESS_MODE = 3
            tmp |= 1 << 6; // ENABLE_ADVANCED_DRIVER_MODEL
            tmp &= !(1 << 7); // SYSTEM_APERTURE_UNMAPPED_ACCESS = 0
            core::ptr::write_volatile(reg(MC_VM_MX_L1_TLB_CNTL), tmp);

            // 2. Setup L2 cache.
            let mut tmp = core::ptr::read_volatile(reg(VM_L2_CNTL));
            tmp |= 1; // ENABLE_L2_CACHE
            tmp |= 1 << 1; // ENABLE_L2_FRAGMENT_PROCESSING
            tmp |= 1 << 2; // ENABLE_L2_PTE_CACHE_LRU_UPDATE_BY_WRITE
            tmp |= 1 << 3; // ENABLE_L2_PDE0_CACHE_LRU_UPDATE_BY_WRITE
            tmp |= 7 << 6; // EFFECTIVE_L2_QUEUE_SIZE = 7
            tmp |= 1 << 14; // CONTEXT1_IDENTITY_ACCESS_MODE = 1
            tmp |= 1 << 15; // ENABLE_DEFAULT_PAGE_OUT_TO_SYSTEM_MEMORY
            core::ptr::write_volatile(reg(VM_L2_CNTL), tmp);

            // 3. Invalidate L2 cache.
            let tmp2 = (1u32 << 0) | (1u32 << 1); // INVALIDATE_ALL_L1_TLBS | INVALIDATE_L2_CACHE
            core::ptr::write_volatile(reg(VM_L2_CNTL2), tmp2);

            // 4. Configure L2 cache bigk (bank select = fragment_size = 9).
            let mut tmp3 = core::ptr::read_volatile(reg(VM_L2_CNTL3));
            tmp3 |= 1; // L2_CACHE_BIGK_ASSOCIATIVITY
            tmp3 |= 9 << 2; // BANK_SELECT = 9
            tmp3 |= 9 << 8; // L2_CACHE_BIGK_FRAGMENT_SIZE = 9
            core::ptr::write_volatile(reg(VM_L2_CNTL3), tmp3);

            // 5. Setup context0 for GART.
            core::ptr::write_volatile(
                reg(VM_CONTEXT0_PAGE_TABLE_START_ADDR),
                (virt_addr >> 12) as u32,
            );
            core::ptr::write_volatile(
                reg(VM_CONTEXT0_PAGE_TABLE_END_ADDR),
                ((virt_addr >> 12) + num_pages as u64) as u32,
            );
            core::ptr::write_volatile(reg(VM_CONTEXT0_PAGE_TABLE_BASE_ADDR), (phys >> 12) as u32);
            core::ptr::write_volatile(
                reg(VM_CONTEXT0_PROTECTION_FAULT_DEFAULT_ADDR),
                (phys >> 12) as u32,
            );
            core::ptr::write_volatile(reg(VM_CONTEXT0_CNTL2), 0u32);

            // 6. Enable context0.
            let mut ctx = core::ptr::read_volatile(reg(VM_CONTEXT0_CNTL));
            ctx |= 1; // ENABLE_CONTEXT
            ctx &= !(0xF << 4); // PAGE_TABLE_DEPTH = 0 (flat)
            ctx |= 1 << 20; // RANGE_PROTECTION_FAULT_ENABLE_DEFAULT
            core::ptr::write_volatile(reg(VM_CONTEXT0_CNTL), ctx);
        }

        Ok(())
    }

    /// Configure NVIDIA GPU memory management (Nouveau VMM).
    ///
    /// Programs the NVIDIA GPU's VMM page directory via MMIO BAR 0,
    /// following the Nouveau nv50 VMM layout.
    ///
    /// NV50 VMM uses a two-level page table:
    ///   - Page Directory (PDE) at pd_offset (0x1400) within the GPU instmem
    ///   - Page Table Entries (PTE) are 8 bytes each
    ///
    /// PDE entry format (nv50_vmm_pde):
    ///   bit 0     = present (0x1 for 4K pages, 0x3 for small pages)
    ///   bits 3:2  = memory target (0=VRAM, 2=HOST, 3=NCOH)
    ///   bits 6:5  = page table size encoding
    ///   bits 31:12 = physical address of page table
    ///
    /// PTE entry format (nv50_vmm_pgt_pte):
    ///   bits 45:12 = physical address
    ///   bits 9:7   = log2 block size (0 for single 4K page)
    ///
    /// Reference: drivers/gpu/drm/nouveau/nvkm/subdev/mmu/vmmnv50.c
    ///           nv50_vmm_pgt_pte(), nv50_vmm_pde()
    fn configure_nvidia_gpu_mmu(&self, virt_addr: u64, size: usize) -> Result<(), &'static str> {
        // Find the NVIDIA GPU PCI device for this gpu_id.
        let display_devices = crate::pci::get_devices_by_class(crate::pci::PciClass::Display);
        let nv_dev = display_devices
            .iter()
            .filter(|d| d.vendor_id == 0x10DE)
            .nth(self.gpu_id as usize)
            .ok_or("No NVIDIA GPU PCI device found")?;

        // Read BAR 0 from PCI config space.
        let bar0_raw = read_pci_config_dword(nv_dev.bus, nv_dev.device, nv_dev.function, 0x10);
        let bar1_raw = read_pci_config_dword(nv_dev.bus, nv_dev.device, nv_dev.function, 0x14);
        if (bar0_raw & 0x1) != 0 {
            return Err("NVIDIA GPU BAR0 is I/O space, not memory-mapped");
        }
        let bar0_base = ((bar0_raw & 0xFFFFFFF0) as u64) | (((bar1_raw as u64) & 0xFFFFFFFF) << 32);
        if bar0_base == 0 {
            return Err("NVIDIA GPU BAR0 is not configured");
        }

        // Map via the kernel direct physical mapping.
        let phys_offset = crate::memory::get_physical_memory_offset();
        if phys_offset == 0 {
            return Err("Kernel physical memory offset not initialized");
        }
        let mmio_base = phys_offset + bar0_base;

        // NV50 VMM page directory offset within instmem.
        const NV50_PD_OFFSET: u64 = 0x1400;
        const NV50_PTE_SIZE: u64 = 8;
        const NV50_PAGE_SIZE: u64 = 4096;

        // Translate the virtual address to physical for the PTE.
        let phys = self.virt_to_phys(virt_addr)?;
        let num_pages = ((size + 4095) / 4096) as u64;

        // SAFETY: We are writing NVIDIA GPU VMM page table entries via MMIO.
        // The BAR was validated above. PTE and PDE formats follow the
        // Nouveau nv50_vmm_pgt_pte() and nv50_vmm_pde() functions.
        unsafe {
            // Write PTEs for each page in the allocation.
            // The PTE address is computed from the page directory entry.
            // For a flat mapping, we write PTEs directly into the page table
            // region starting after the page directory.
            let pte_base = mmio_base + NV50_PD_OFFSET + 0x4000; // PTEs start after PD

            for i in 0..num_pages {
                let pte_addr = pte_base + i * NV50_PTE_SIZE;
                let page_phys = phys + i * NV50_PAGE_SIZE;

                // PTE: physical address | log2blk(0) << 7
                // For NV50, the PTE is: addr | (log2blk << 7)
                // With log2blk=0, this is just the physical address.
                let pte = page_phys & 0x0000_FFFF_FFFF_F000;

                core::ptr::write_volatile(pte_addr as *mut u64, pte);
            }

            // Write the PDE entry pointing to our page table.
            // PDE format for 4K pages (page = 12):
            //   data = 0x00000003 (present, 4K pages)
            //   | 0x00000008 (HOST memory target for system RAM)
            //   | (page_table_phys_addr)
            // Page table size encoding for 64KB table: 0x00000020
            let pt_phys = phys; // Page table is at the same physical location
            let pde = 0x00000003u64    // present, 4K pages
                | 0x00000008u64       // HOST memory target (system RAM)
                | 0x00000020u64       // 64KB page table size
                | (pt_phys & 0x0000_FFFF_FFFF_F000);

            let pde_addr = mmio_base + NV50_PD_OFFSET;
            core::ptr::write_volatile(pde_addr as *mut u64, pde);
        }

        Ok(())
    }

    /// Track allocation for cleanup
    fn track_allocation(&self, alloc_info: GPUHostAllocation) -> Result<(), &'static str> {
        // In production, this would store allocation info in a data structure
        // For now, just log the allocation
        crate::println!(
            "GPU {} allocated {} bytes at virtual address 0x{:016X}",
            self.gpu_id,
            alloc_info.size,
            alloc_info.virt_addr
        );
        Ok(())
    }

    /// Convert virtual address to physical address
    fn virt_to_phys(&self, virt_addr: u64) -> Result<u64, &'static str> {
        // Walk the active page tables via the kernel memory manager instead of
        // assuming a fixed direct-mapping offset, which is only valid for the
        // bootloader's identity mapping and not for GPU aperture mappings.
        crate::memory::translate_addr(VirtAddr::new(virt_addr))
            .map(|p| p.as_u64())
            .ok_or("Virtual address is not mapped")
    }

    /// Get GPU vendor for this memory manager by reading PCI config.
    fn get_gpu_vendor(&self) -> GPUVendor {
        let display_devices = crate::pci::get_devices_by_class(crate::pci::PciClass::Display);
        let dev = display_devices.get(self.gpu_id as usize);
        match dev {
            Some(d) if d.vendor_id == 0x8086 => GPUVendor::Intel,
            Some(d) if d.vendor_id == 0x1002 => GPUVendor::AMD,
            Some(d) if d.vendor_id == 0x10DE => GPUVendor::Nvidia,
            _ => GPUVendor::Unknown,
        }
    }

    /// Unmap a virtual page
    fn unmap_page(&self, virt_addr: u64) -> Result<(), &'static str> {
        if self.gpu_memory_base == 0 || self.gpu_memory_size == 0 {
            return Err("GPU coherent memory window not configured");
        }

        let window_end = self.gpu_memory_base.saturating_add(self.gpu_memory_size);
        if virt_addr < self.gpu_memory_base || virt_addr >= window_end {
            return Err("Invalid virtual address for unmapping");
        }

        // Remove the mapping through the real kernel page-table manager.
        crate::memory::unmap_page(virt_addr as usize)
    }

    fn free_host_memory(&self, ptr: NonNull<u8>, size: usize) -> Result<(), &'static str> {
        // Production implementation for freeing GPU host memory
        let virt_addr = ptr.as_ptr() as u64;

        // Calculate number of pages to free
        let pages_needed = (size + 4095) / 4096;

        // Unmap virtual pages
        for i in 0..pages_needed {
            let page_addr = virt_addr + (i * 4096) as u64;
            if let Err(_) = crate::memory::unmap_page(page_addr as usize) {
                crate::serial_println!("Warning: Failed to unmap GPU page at {:x}", page_addr);
            }
        }

        // In a full implementation, we would look up the allocation in our tracker
        // and free the corresponding physical frames. For now, we rely on the
        // memory manager to handle frame deallocation during page unmapping.

        self.stats
            .total_deallocations
            .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    fn perform_memory_transfer(&self, src: u64, dst: u64, size: usize) -> Result<(), &'static str> {
        // Production memory transfer - would use DMA engine or memcpy
        if src == 0 || dst == 0 || size == 0 {
            return Err("Invalid memory transfer parameters");
        }

        // In production, would use:
        // - DMA engine for large transfers
        // - Memory barriers for cache coherency
        // - Platform-specific GPU memory APIs

        // For now, validate the operation completed
        self.stats
            .total_transfers
            .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    fn enable_memory_compression(&self) -> Result<(), &'static str> {
        // GPU memory compression would be configured here
        Ok(())
    }

    fn optimize_cache_policy(&self) -> Result<(), &'static str> {
        // Cache policy optimization based on access patterns
        Ok(())
    }

    fn boost_memory_clock(&self) -> Result<(), &'static str> {
        // Memory clock boost configuration
        Ok(())
    }

    fn configure_memory_interleaving(&self) -> Result<(), &'static str> {
        // Memory interleaving configuration
        Ok(())
    }

    fn calculate_fragmentation_ratio(&self) -> f32 {
        if self.available_memory == 0 {
            return 0.0;
        }

        let largest_free = self.find_largest_free_block();
        if largest_free == 0 {
            return 1.0;
        }

        1.0 - (largest_free as f32 / self.available_memory as f32)
    }

    fn find_largest_free_block(&self) -> usize {
        self.free_blocks.keys().last().cloned().unwrap_or(0)
    }

    fn can_move_allocation(&self, allocation_id: u32) -> bool {
        // Check if allocation can be moved (not pinned, not currently mapped, etc.)
        if let Some(allocation) = self.allocations.get(&allocation_id) {
            allocation.host_address.is_none() && !allocation.flags.persistent
        } else {
            false
        }
    }

    fn move_allocation(
        &mut self,
        allocation_id: u32,
        new_address: u64,
    ) -> Result<(), &'static str> {
        // This would perform the actual memory move in a real implementation
        if let Some(allocation) = self.allocations.get_mut(&allocation_id) {
            allocation.gpu_address = new_address;
            Ok(())
        } else {
            Err("Invalid allocation ID")
        }
    }

    fn rebuild_free_blocks(&mut self) {
        // Rebuild the free blocks map after defragmentation
        self.free_blocks.clear();

        // Calculate total allocated memory
        let mut allocated_regions: Vec<(u64, usize)> = self
            .allocations
            .values()
            .map(|alloc| (alloc.gpu_address, alloc.size))
            .collect();

        allocated_regions.sort_by_key(|&(addr, _)| addr);

        // Find gaps between allocations
        let mut current_addr = 0u64;
        for (alloc_addr, alloc_size) in allocated_regions {
            if current_addr < alloc_addr {
                let gap_size = (alloc_addr - current_addr) as usize;
                self.add_to_free_blocks(current_addr, gap_size);
            }
            current_addr = alloc_addr + alloc_size as u64;
        }

        // Add remaining memory at the end
        if current_addr < self.total_memory as u64 {
            let remaining_size = self.total_memory - current_addr as usize;
            self.add_to_free_blocks(current_addr, remaining_size);
        }
    }
}

/// GPU memory statistics
#[derive(Debug, Clone)]
pub struct GPUMemoryStatistics {
    pub total_memory: usize,
    pub available_memory: usize,
    pub allocated_memory: usize,
    pub allocation_count: usize,
    pub dma_buffer_count: usize,
    pub largest_free_block: usize,
    pub fragmentation_ratio: f32,
    pub memory_utilization: f32,
}

/// Global GPU memory management system
pub struct GlobalGPUMemoryManager {
    pub managers: Vec<GPUMemoryManager>,
    pub cross_gpu_sharing: CrossGPUSharing,
    pub global_statistics: GlobalMemoryStatistics,
}

/// Global memory statistics across all GPUs
#[derive(Debug, Clone)]
pub struct GlobalMemoryStatistics {
    pub total_system_gpu_memory: usize,
    pub total_allocated_memory: usize,
    pub total_available_memory: usize,
    pub active_gpu_count: usize,
    pub cross_gpu_transfers: u64,
    pub bandwidth_utilization: f32,
}

impl GlobalGPUMemoryManager {
    pub fn new() -> Self {
        Self {
            managers: Vec::new(),
            cross_gpu_sharing: CrossGPUSharing {
                enabled: false,
                peer_gpus: Vec::new(),
                shared_pools: Vec::new(),
                bandwidth_priority: BandwidthPriority::Balanced,
            },
            global_statistics: GlobalMemoryStatistics {
                total_system_gpu_memory: 0,
                total_allocated_memory: 0,
                total_available_memory: 0,
                active_gpu_count: 0,
                cross_gpu_transfers: 0,
                bandwidth_utilization: 0.0,
            },
        }
    }

    pub fn add_gpu(&mut self, gpu_id: u32, capabilities: &GPUCapabilities) {
        let manager = GPUMemoryManager::new(gpu_id, capabilities);
        self.global_statistics.total_system_gpu_memory += manager.total_memory;
        self.global_statistics.total_available_memory += manager.available_memory;
        self.global_statistics.active_gpu_count += 1;
        self.managers.push(manager);
    }

    pub fn get_manager(&mut self, gpu_id: u32) -> Option<&mut GPUMemoryManager> {
        self.managers
            .iter_mut()
            .find(|manager| manager.gpu_id == gpu_id)
    }

    pub fn enable_cross_gpu_sharing(&mut self, gpu_ids: &[u32]) -> Result<(), &'static str> {
        if gpu_ids.len() < 2 {
            return Err("Cross-GPU sharing requires at least 2 GPUs");
        }

        self.cross_gpu_sharing.enabled = true;
        self.cross_gpu_sharing.peer_gpus = gpu_ids.to_vec();

        // Create shared memory pools between compatible GPUs
        for chunk in gpu_ids.chunks(2) {
            if chunk.len() == 2 {
                let pool_size = 64 * 1024 * 1024; // 64MB shared pool
                let pool = SharedMemoryPool {
                    pool_id: self.cross_gpu_sharing.shared_pools.len() as u32,
                    size: pool_size,
                    participating_gpus: chunk.to_vec(),
                    base_address: 0x90000000
                        + (self.cross_gpu_sharing.shared_pools.len() as u64 * pool_size as u64),
                    allocation_bitmap: vec![0; pool_size / (64 * 8)], // 64-byte granularity
                };
                self.cross_gpu_sharing.shared_pools.push(pool);
            }
        }

        Ok(())
    }

    pub fn update_global_statistics(&mut self) {
        self.global_statistics.total_allocated_memory = 0;
        self.global_statistics.total_available_memory = 0;

        for manager in &self.managers {
            self.global_statistics.total_allocated_memory +=
                manager.total_memory - manager.available_memory;
            self.global_statistics.total_available_memory += manager.available_memory;
        }

        self.global_statistics.bandwidth_utilization =
            (self.global_statistics.total_allocated_memory as f32
                / self.global_statistics.total_system_gpu_memory as f32)
                * 100.0;
    }

    pub fn generate_memory_report(&self) -> String {
        let mut report = String::new();

        report.push_str("=== GPU Memory Management Report ===\n\n");
        report.push_str(&format!(
            "Active GPUs: {}\n",
            self.global_statistics.active_gpu_count
        ));
        report.push_str(&format!(
            "Total GPU Memory: {:.1} GB\n",
            self.global_statistics.total_system_gpu_memory as f64 / (1024.0 * 1024.0 * 1024.0)
        ));
        report.push_str(&format!(
            "Total Allocated: {:.1} GB\n",
            self.global_statistics.total_allocated_memory as f64 / (1024.0 * 1024.0 * 1024.0)
        ));
        report.push_str(&format!(
            "Total Available: {:.1} GB\n",
            self.global_statistics.total_available_memory as f64 / (1024.0 * 1024.0 * 1024.0)
        ));
        report.push_str(&format!(
            "Bandwidth Utilization: {:.1}%\n",
            self.global_statistics.bandwidth_utilization
        ));

        if self.cross_gpu_sharing.enabled {
            report.push_str(&format!(
                "\nCross-GPU Sharing: Enabled ({} pools)\n",
                self.cross_gpu_sharing.shared_pools.len()
            ));
        }

        report.push_str("\n=== Per-GPU Statistics ===\n");
        for manager in &self.managers {
            let stats = manager.get_statistics();
            report.push_str(&format!(
                "GPU {}: {:.1} GB total, {:.1}% utilized, {:.1}% fragmented\n",
                manager.gpu_id,
                stats.total_memory as f64 / (1024.0 * 1024.0 * 1024.0),
                stats.memory_utilization,
                stats.fragmentation_ratio * 100.0
            ));
        }

        report
    }
}

// Global memory manager instance
lazy_static! {
    static ref GLOBAL_MEMORY_MANAGER: Mutex<GlobalGPUMemoryManager> =
        Mutex::new(GlobalGPUMemoryManager::new());
}

/// Initialize GPU memory management system
pub fn initialize_gpu_memory_system(gpus: &[GPUCapabilities]) -> Result<(), &'static str> {
    let mut manager = GLOBAL_MEMORY_MANAGER.lock();

    for (i, gpu) in gpus.iter().enumerate() {
        manager.add_gpu(i as u32, gpu);
    }

    // Enable cross-GPU sharing if multiple compatible GPUs are detected
    if gpus.len() > 1 {
        let gpu_ids: Vec<u32> = (0..gpus.len() as u32).collect();
        let _ = manager.enable_cross_gpu_sharing(&gpu_ids); // Best effort
    }

    Ok(())
}

/// Allocate GPU memory
pub fn allocate_gpu_memory(
    gpu_id: u32,
    size: usize,
    alignment: usize,
    flags: MemoryFlags,
) -> Result<u32, &'static str> {
    let mut manager = GLOBAL_MEMORY_MANAGER.lock();
    if let Some(gpu_manager) = manager.get_manager(gpu_id) {
        gpu_manager.allocate(size, alignment, flags)
    } else {
        Err("Invalid GPU ID")
    }
}

/// Free GPU memory
pub fn free_gpu_memory(gpu_id: u32, allocation_id: u32) -> Result<(), &'static str> {
    let mut manager = GLOBAL_MEMORY_MANAGER.lock();
    if let Some(gpu_manager) = manager.get_manager(gpu_id) {
        gpu_manager.free(allocation_id)
    } else {
        Err("Invalid GPU ID")
    }
}

/// Get memory statistics for a specific GPU
pub fn get_gpu_memory_stats(gpu_id: u32) -> Option<GPUMemoryStatistics> {
    let manager = GLOBAL_MEMORY_MANAGER.lock();
    manager
        .managers
        .iter()
        .find(|m| m.gpu_id == gpu_id)
        .map(|m| m.get_statistics())
}

/// Get global memory statistics
pub fn get_global_memory_stats() -> GlobalMemoryStatistics {
    let mut manager = GLOBAL_MEMORY_MANAGER.lock();
    manager.update_global_statistics();
    manager.global_statistics.clone()
}

/// Set the GPU memory window for a specific GPU via the global manager.
///
/// This configures the coherent memory window (base address + size) for
/// the specified GPU, enabling coherent memory mapping operations.
pub fn set_gpu_memory_window_global(gpu_id: u32, base: u64, size: u64) {
    let mut manager = GLOBAL_MEMORY_MANAGER.lock();
    if let Some(gpu_manager) = manager.get_manager(gpu_id) {
        gpu_manager.set_gpu_memory_window(base, size);
    }
}

/// Generate comprehensive memory report
pub fn generate_memory_report() -> String {
    let manager = GLOBAL_MEMORY_MANAGER.lock();
    manager.generate_memory_report()
}

/// Read a 32-bit PCI configuration space register using I/O ports 0xCF8/0xCFC.
///
/// This is the same mechanism used by the kernel PCI scanner. The address
/// is encoded as: 0x80000000 | (bus << 16) | (device << 11) | (function << 8) | (offset & 0xFC).
fn read_pci_config_dword(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
    let config_address = 0x80000000u32
        | ((bus as u32) << 16)
        | ((device as u32) << 11)
        | ((function as u32) << 8)
        | ((offset as u32) & 0xFC);

    unsafe {
        core::arch::asm!(
            "out dx, eax",
            in("dx") 0xCF8u16,
            in("eax") config_address,
            options(nostack, preserves_flags),
        );

        let mut data: u32;
        core::arch::asm!(
            "in eax, dx",
            out("eax") data,
            in("dx") 0xCFCu16,
            options(nostack, preserves_flags),
        );
        data
    }
}
