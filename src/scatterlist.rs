// SPDX-License-Identifier: MIT
// RustOS scatter-gather list for DMA (ported from Linux rust/kernel/scatterlist.rs)
// All bindings:: calls replaced with native Rust implementations.

#![allow(dead_code, unused_variables)]

extern crate alloc;
use alloc::vec::Vec;

use crate::page::{PAGE_SIZE, phys_to_virt};

// ---------------------------------------------------------------------------
// DMA direction
// ---------------------------------------------------------------------------

/// Direction of a DMA transfer (mirrors `enum dma_data_direction`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DmaDirection {
    /// Data flows from memory to device.
    ToDevice,
    /// Data flows from device to memory.
    FromDevice,
    /// Data flows in both directions.
    Bidirectional,
    /// No data transfer (used for no-op mappings).
    None,
}

// ---------------------------------------------------------------------------
// ScatterList entry flags
// ---------------------------------------------------------------------------

/// Marks the last entry in a scatter-gather chain.
pub const SG_END: u32 = 1 << 0;
/// Entry is a chain link pointing to another table.
pub const SG_CHAIN: u32 = 1 << 1;

// ---------------------------------------------------------------------------
// ScatterList — a single sg entry
// ---------------------------------------------------------------------------

/// A single scatter-gather entry.
///
/// Matches the layout of Linux `struct scatterlist` (simplified).
#[repr(C)]
pub struct ScatterList {
    /// Physical address of the page backing this entry.
    pub page_phys: u64,
    /// Byte offset within the page.
    pub offset: u32,
    /// Length of the data region in bytes.
    pub length: u32,
    /// Bus address assigned after DMA mapping.
    pub dma_addr: u64,
    /// DMA-mapped length (may differ from `length` on some architectures).
    pub dma_len: u32,
    /// Entry flags (`SG_END`, `SG_CHAIN`).
    pub flags: u32,
}

impl ScatterList {
    /// Construct a new scatter-gather entry for `page_phys` + `offset` of `length` bytes.
    pub fn new(page_phys: u64, offset: u32, length: u32) -> Self {
        ScatterList {
            page_phys,
            offset,
            length,
            dma_addr: 0,
            dma_len: 0,
            flags: 0,
        }
    }

    /// Mark this entry as the last in the chain.
    #[inline]
    pub fn mark_end(&mut self) {
        self.flags |= SG_END;
    }

    /// Return `true` if this is the last entry.
    #[inline]
    pub fn is_last(&self) -> bool {
        self.flags & SG_END != 0
    }

    /// Return a pointer to the kernel virtual address of this entry's data.
    #[inline]
    pub fn kernel_addr(&self) -> *mut u8 {
        let page_va = phys_to_virt(self.page_phys);
        // SAFETY: offset is within page bounds (caller's responsibility).
        unsafe { page_va.add(self.offset as usize) }
    }

    /// Return `true` if this entry has been DMA-mapped.
    #[inline]
    pub fn is_dma_mapped(&self) -> bool {
        self.dma_addr != 0
    }
}

// ---------------------------------------------------------------------------
// SgTable — a scatter-gather table
// ---------------------------------------------------------------------------

/// Error constant.
const ENOMEM: i32 = 12;
const EINVAL: i32 = 22;

/// A scatter-gather table (a `Vec` of `ScatterList` entries).
pub struct SgTable {
    /// All scatter-list entries.
    pub sgl: Vec<ScatterList>,
    /// Number of valid entries (may be less than `sgl.len()`).
    pub nents: usize,
    /// Original number of entries requested.
    pub orig_nents: usize,
}

impl SgTable {
    /// Allocate a scatter-gather table with `nents` pre-allocated entries.
    pub fn alloc(nents: usize) -> Result<Self, i32> {
        if nents == 0 {
            return Err(EINVAL);
        }
        let mut sgl = Vec::with_capacity(nents);
        for _ in 0..nents {
            sgl.push(ScatterList::new(0, 0, 0));
        }
        // Mark the last entry as the end of the chain.
        if let Some(last) = sgl.last_mut() {
            last.mark_end();
        }
        Ok(SgTable { sgl, nents: 0, orig_nents: nents })
    }

    /// Free the table (drops the inner Vec).
    pub fn free(self) {
        // Drop is implicit.
    }

    /// Set entry `index` to point to `page_phys + offset` for `length` bytes.
    pub fn set_page(&mut self, index: usize, page_phys: u64, offset: u32, length: u32) {
        if index >= self.sgl.len() {
            return;
        }
        let is_last = index == self.orig_nents.saturating_sub(1);
        let entry = &mut self.sgl[index];
        entry.page_phys = page_phys;
        entry.offset = offset;
        entry.length = length;
        if is_last {
            entry.flags |= SG_END;
        } else {
            entry.flags &= !SG_END;
        }
        if index >= self.nents {
            self.nents = index + 1;
        }
    }

    /// Number of valid entries.
    #[inline]
    pub fn nents(&self) -> usize {
        self.nents
    }

    /// Iterate over valid entries.
    pub fn iter(&self) -> impl Iterator<Item = &ScatterList> {
        self.sgl[..self.nents].iter()
    }

    /// Iterate over mutable valid entries.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut ScatterList> {
        let nents = self.nents;
        self.sgl[..nents].iter_mut()
    }

    /// Perform a software DMA mapping.
    ///
    /// On x86_64 with direct-mapped physical memory and no IOMMU, the DMA
    /// address equals the physical address.  A real driver would call the
    /// platform DMA API here.
    pub fn dma_map(&mut self, direction: DmaDirection) -> Result<(), i32> {
        for entry in self.iter_mut() {
            // On x86_64 without an IOMMU: dma_addr == phys_addr + offset.
            entry.dma_addr = entry.page_phys + entry.offset as u64;
            entry.dma_len = entry.length;
        }
        // Issue a full memory barrier so the CPU's write buffer is flushed
        // before the device is told to begin the DMA.
        crate::io::wmb();
        Ok(())
    }

    /// Undo the DMA mapping.
    pub fn dma_unmap(&mut self, direction: DmaDirection) {
        crate::io::rmb();
        for entry in self.iter_mut() {
            entry.dma_addr = 0;
            entry.dma_len = 0;
        }
    }

    /// Total byte count across all valid entries.
    pub fn total_len(&self) -> usize {
        self.iter().map(|e| e.length as usize).sum()
    }
}
