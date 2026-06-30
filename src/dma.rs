// SPDX-License-Identifier: MIT
// RustOS DMA coherent/streaming abstractions (ported from Linux rust/kernel/dma.rs)
// All bindings:: calls replaced with native Rust implementations.

#![allow(dead_code, unused_variables)]

use core::ptr;

pub use crate::scatterlist::DmaDirection;
use crate::io::{mb, rmb, wmb};

// ---------------------------------------------------------------------------
// DMA constants
// ---------------------------------------------------------------------------

/// 32-bit DMA address mask (device can only address the low 4 GiB).
pub const DMA_BIT_MASK_32: u64 = 0xFFFF_FFFF;

/// 64-bit DMA address mask (device can address the full 64-bit bus).
pub const DMA_BIT_MASK_64: u64 = 0xFFFF_FFFF_FFFF_FFFF;

// ---------------------------------------------------------------------------
// Error constants
// ---------------------------------------------------------------------------

const ENOMEM: i32 = 12;
const EINVAL: i32 = 22;

// ---------------------------------------------------------------------------
// DmaCoherent — coherent (uncached) DMA allocation
// ---------------------------------------------------------------------------

/// A DMA-coherent memory allocation.
///
/// Memory allocated here is visible to both the CPU and the device without
/// explicit cache-flushing operations.  On x86 (all memory is cache-coherent)
/// this simply allocates kernel memory and records the physical address.
pub struct DmaCoherent {
    phys: u64,
    virt: *mut u8,
    size: usize,
}

// SAFETY: DmaCoherent wraps a pointer to kernel memory that is not aliased
// by any other Rust reference outside of explicit unsafe code.
unsafe impl Send for DmaCoherent {}

impl DmaCoherent {
    /// Allocate `size` bytes of DMA-coherent memory.
    ///
    /// Returns `Err(ENOMEM)` if the allocation fails.
    pub fn alloc(size: usize) -> Result<Self, i32> {
        if size == 0 {
            return Err(EINVAL);
        }

        // Align to page size for DMA safety.
        let aligned = (size + 0xFFF) & !0xFFF;

        // Use the global kernel allocator (which must be initialised by the
        // time DMA allocations happen).
        let layout = core::alloc::Layout::from_size_align(aligned, 4096)
            .map_err(|_| EINVAL)?;

        // SAFETY: layout is non-zero and properly aligned.
        let virt = unsafe { alloc::alloc::alloc_zeroed(layout) };
        if virt.is_null() {
            return Err(ENOMEM);
        }

        // On identity-mapped RustOS, physical address == virtual address.
        let phys = virt as u64;

        Ok(DmaCoherent { phys, virt, size: aligned })
    }

    /// Physical (bus) address of the allocation.
    #[inline]
    pub fn phys_addr(&self) -> u64 {
        self.phys
    }

    /// Kernel virtual pointer to the allocation.
    #[inline]
    pub fn as_ptr(&self) -> *mut u8 {
        self.virt
    }

    /// Shared byte slice view.
    #[inline]
    pub fn as_slice(&self) -> &[u8] {
        unsafe { core::slice::from_raw_parts(self.virt, self.size) }
    }

    /// Mutable byte slice view.
    #[inline]
    pub fn as_slice_mut(&mut self) -> &mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self.virt, self.size) }
    }

    /// Size of the allocation in bytes.
    #[inline]
    pub fn size(&self) -> usize {
        self.size
    }

    /// Zero the entire allocation.
    pub fn zero(&mut self) {
        unsafe { ptr::write_bytes(self.virt, 0u8, self.size) }
    }
}

impl Drop for DmaCoherent {
    fn drop(&mut self) {
        // SAFETY: `virt` was allocated with the same layout.
        if !self.virt.is_null() {
            let layout = core::alloc::Layout::from_size_align(self.size, 4096)
                .expect("DmaCoherent drop: layout error");
            unsafe { alloc::alloc::dealloc(self.virt, layout) };
        }
    }
}

// ---------------------------------------------------------------------------
// Streaming DMA sync operations
// ---------------------------------------------------------------------------

/// Synchronise a streaming DMA buffer for CPU access after a device write.
///
/// On x86 (coherent architecture) this is a read fence; on non-coherent
/// architectures (ARM, RISC-V without extensions) this would invalidate cache.
#[inline]
pub fn dma_sync_for_cpu(phys: u64, size: usize, dir: DmaDirection) {
    let _ = (phys, size);
    match dir {
        DmaDirection::FromDevice | DmaDirection::Bidirectional => rmb(),
        _ => {}
    }
}

/// Synchronise a streaming DMA buffer for device access after a CPU write.
///
/// On x86 this is a write fence; on non-coherent architectures this would
/// flush and/or clean cache lines.
#[inline]
pub fn dma_sync_for_device(phys: u64, size: usize, dir: DmaDirection) {
    let _ = (phys, size);
    match dir {
        DmaDirection::ToDevice | DmaDirection::Bidirectional => wmb(),
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// IoMem — MMIO region mapping
// ---------------------------------------------------------------------------
// Re-export from io.rs for convenience (Linux's dma.rs also provides ioremap).
pub use crate::io::IoMem;

/// Map a physical MMIO region into the kernel address space.
///
/// # Safety
/// `phys` must be a valid MMIO physical address backed by a device BAR.
pub unsafe fn ioremap(phys: u64, size: usize) -> Result<IoMem, i32> {
    // SAFETY: caller guarantees validity.
    unsafe { IoMem::new(phys, size) }
}

/// Unmap an MMIO region previously mapped with `ioremap`.
///
/// On early-boot RustOS with identity mapping this is a no-op.
/// A real implementation would call `vunmap`.
pub fn iounmap(_iomem: IoMem) {
    // Drop of IoMem is a no-op for identity-mapped regions.
}

// ---------------------------------------------------------------------------
// DMA pool — simple bump allocator within a DmaCoherent region
// ---------------------------------------------------------------------------

/// A simple pool allocator backed by a DMA-coherent region.
///
/// Useful for allocating many small DMA descriptors without the overhead of
/// individual allocations.
pub struct DmaPool {
    backing: DmaCoherent,
    offset: usize,
    chunk_size: usize,
}

impl DmaPool {
    /// Create a pool of `count` slots each of `chunk_size` bytes.
    pub fn new(count: usize, chunk_size: usize) -> Result<Self, i32> {
        let total = count.checked_mul(chunk_size).ok_or(EINVAL)?;
        let backing = DmaCoherent::alloc(total)?;
        Ok(DmaPool { backing, offset: 0, chunk_size })
    }

    /// Allocate one chunk, returning `(virt_ptr, phys_addr)`.
    pub fn alloc_chunk(&mut self) -> Option<(*mut u8, u64)> {
        if self.offset + self.chunk_size > self.backing.size() {
            return None;
        }
        let virt = unsafe { self.backing.as_ptr().add(self.offset) };
        let phys = self.backing.phys_addr() + self.offset as u64;
        self.offset += self.chunk_size;
        Some((virt, phys))
    }

    /// Reset the pool (all previously-allocated chunks become invalid).
    pub fn reset(&mut self) {
        self.offset = 0;
        self.backing.zero();
    }
}

extern crate alloc;
