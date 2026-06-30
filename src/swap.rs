//! Swap subsystem — paging to backing store
//!
//! Ported from Linux mm/swapfile.c, mm/swap_state.c, mm/vmscan.c.
//! Provides:
//! - Swap slot management (bitmap-based slot allocator)
//! - Swap cache (page → swap entry mapping)
//! - Page replacement (simplified LRU + second-chance)
//! - Swap-in / swap-out operations
//! - Multiple swap areas (like Linux swap files/partitions)
//!
//! ## Swap entry format
//! [ swap_area_index (8 bits) | slot_offset (56 bits) ]
//! A swap entry of 0 means "not swapped".

use alloc::collections::BTreeMap;
use alloc::vec; // for vec! macro
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::{Mutex, RwLock};

// ── Constants ───────────────────────────────────────────────────────────

pub const PAGE_SIZE: usize = 4096;
pub const SWAP_ENTRY_INVALID: u64 = 0;

// ── Swap area ───────────────────────────────────────────────────────────

/// A swap area is a backing store region (disk partition or file).
pub struct SwapArea {
    pub index: u8,
    pub priority: i32,
    pub nr_pages: u64,
    pub nr_used: AtomicU64,
    /// Bitmap: bit set = slot in use
    bitmap: Vec<u64>,
    /// Write callback — writes a page to slot offset
    write_page: fn(slot: u64, data: &[u8; PAGE_SIZE]) -> Result<(), &'static str>,
    /// Read callback — reads a page from slot offset
    read_page: fn(slot: u64, data: &mut [u8; PAGE_SIZE]) -> Result<(), &'static str>,
}

impl SwapArea {
    pub fn new(
        index: u8,
        priority: i32,
        nr_pages: u64,
        write_page: fn(u64, &[u8; PAGE_SIZE]) -> Result<(), &'static str>,
        read_page: fn(u64, &mut [u8; PAGE_SIZE]) -> Result<(), &'static str>,
    ) -> Self {
        let bitmap_words = ((nr_pages as usize) + 63) / 64;
        Self {
            index,
            priority,
            nr_pages,
            nr_used: AtomicU64::new(0),
            bitmap: vec![0u64; bitmap_words],
            write_page,
            read_page,
        }
    }

    /// Allocate a free swap slot. Returns the slot index.
    fn alloc_slot(&mut self) -> Option<u64> {
        for word_idx in 0..self.bitmap.len() {
            if self.bitmap[word_idx] != u64::MAX {
                // Find first zero bit
                let bit = (!self.bitmap[word_idx]).trailing_zeros() as usize;
                let slot = (word_idx * 64 + bit) as u64;
                if slot >= self.nr_pages {
                    return None;
                }
                self.bitmap[word_idx] |= 1u64 << bit;
                self.nr_used.fetch_add(1, Ordering::Relaxed);
                return Some(slot);
            }
        }
        None
    }

    /// Free a swap slot.
    fn free_slot(&mut self, slot: u64) {
        let word_idx = (slot / 64) as usize;
        let bit = (slot % 64) as u64;
        if word_idx < self.bitmap.len() {
            self.bitmap[word_idx] &= !(1u64 << bit);
            self.nr_used.fetch_sub(1, Ordering::Relaxed);
        }
    }

    /// Check if a slot is in use.
    fn slot_in_use(&self, slot: u64) -> bool {
        let word_idx = (slot / 64) as usize;
        let bit = (slot % 64) as u64;
        if word_idx < self.bitmap.len() {
            self.bitmap[word_idx] & (1u64 << bit) != 0
        } else {
            false
        }
    }

    /// Write a page to a swap slot.
    fn write(&self, slot: u64, data: &[u8; PAGE_SIZE]) -> Result<(), &'static str> {
        (self.write_page)(slot, data)
    }

    /// Read a page from a swap slot.
    fn read(&self, slot: u64, data: &mut [u8; PAGE_SIZE]) -> Result<(), &'static str> {
        (self.read_page)(slot, data)
    }

    fn free_slots(&self) -> u64 {
        self.nr_pages - self.nr_used.load(Ordering::Relaxed)
    }
}

// ── Swap entry encoding ─────────────────────────────────────────────────

/// Encode a swap area index and slot into a swap entry.
pub fn encode_swap_entry(area_index: u8, slot: u64) -> u64 {
    ((area_index as u64) << 56) | (slot & 0x00FF_FFFF_FFFF_FFFF)
}

/// Decode a swap entry into (area_index, slot).
pub fn decode_swap_entry(entry: u64) -> (u8, u64) {
    let area_index = (entry >> 56) as u8;
    let slot = entry & 0x00FF_FFFF_FFFF_FFFF;
    (area_index, slot)
}

/// Check if a swap entry is valid (non-zero).
pub fn swap_entry_valid(entry: u64) -> bool {
    entry != SWAP_ENTRY_INVALID
}

// ── Swap cache ──────────────────────────────────────────────────────────

/// The swap cache maps virtual page addresses to swap entries.
/// When a page is being swapped out, it's first added to the swap cache
/// so that concurrent accesses can find it.
struct SwapCache {
    /// Virtual address → swap entry
    entries: BTreeMap<u64, u64>,
}

impl SwapCache {
    const fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }
}

static SWAP_CACHE: Mutex<SwapCache> = Mutex::new(SwapCache::new());

// ── Global swap state ───────────────────────────────────────────────────

static SWAP_AREAS: RwLock<Vec<SwapArea>> = RwLock::new(Vec::new());
static TOTAL_SWAP_PAGES: AtomicU64 = AtomicU64::new(0);
static USED_SWAP_PAGES: AtomicU64 = AtomicU64::new(0);
static SWAP_IN_COUNT: AtomicU64 = AtomicU64::new(0);
static SWAP_OUT_COUNT: AtomicU64 = AtomicU64::new(0);

// ── Initialization ──────────────────────────────────────────────────────

pub fn init() {
    crate::serial_println!("[swap] swap subsystem initialized (no swap areas yet)");
}

/// Register a swap area (e.g. a disk partition for swapping).
pub fn add_swap_area(area: SwapArea) {
    let pages = area.nr_pages;
    let priority = area.priority;
    let mut areas = SWAP_AREAS.write();

    // Insert sorted by priority (highest priority first)
    let pos = areas
        .iter()
        .position(|a| a.priority < priority)
        .unwrap_or(areas.len());
    areas.insert(pos, area);

    TOTAL_SWAP_PAGES.fetch_add(pages, Ordering::Relaxed);
    crate::serial_println!(
        "[swap] added swap area: {} pages (total: {})",
        pages,
        TOTAL_SWAP_PAGES.load(Ordering::Relaxed)
    );
}

// ── Swap-out ────────────────────────────────────────────────────────────

/// Swap out a page: write its contents to a swap slot and record the mapping.
/// Returns the swap entry on success.
pub fn swap_out(vaddr: u64, page_data: &[u8; PAGE_SIZE]) -> Result<u64, &'static str> {
    let areas = SWAP_AREAS.read();

    // Find the first area with free slots (highest priority first)
    for _area in areas.iter() {
        // We need mutable access to alloc_slot, but we only have a read lock.
        // Use interior mutability via atomic + bitmap under a separate lock.
        // For simplicity, take a write lock.
        drop(areas);
        let mut areas = SWAP_AREAS.write();
        let area = areas.iter_mut().find(|a| a.free_slots() > 0);
        let Some(area) = area else {
            return Err("no free swap space");
        };

        let slot = area.alloc_slot().ok_or("swap allocation failed")?;
        let area_index = area.index;

        area.write(slot, page_data)?;

        let entry = encode_swap_entry(area_index, slot);
        SWAP_CACHE.lock().entries.insert(vaddr, entry);
        USED_SWAP_PAGES.fetch_add(1, Ordering::Relaxed);
        SWAP_OUT_COUNT.fetch_add(1, Ordering::Relaxed);

        return Ok(entry);
    }

    Err("no swap areas available")
}

// ── Swap-in ─────────────────────────────────────────────────────────────

/// Swap in a page: read its contents from the swap slot back into memory.
pub fn swap_in(vaddr: u64, page_data: &mut [u8; PAGE_SIZE]) -> Result<(), &'static str> {
    let entry = {
        let cache = SWAP_CACHE.lock();
        *cache.entries.get(&vaddr).ok_or("page not in swap cache")?
    };

    let (area_index, slot) = decode_swap_entry(entry);

    let areas = SWAP_AREAS.read();
    let area = areas
        .iter()
        .find(|a| a.index == area_index)
        .ok_or("swap area not found")?;

    area.read(slot, page_data)?;

    SWAP_IN_COUNT.fetch_add(1, Ordering::Relaxed);
    Ok(())
}

/// Free a swap entry after a page has been swapped back in.
pub fn swap_free(entry: u64) {
    if !swap_entry_valid(entry) {
        return;
    }

    let (area_index, slot) = decode_swap_entry(entry);
    let mut areas = SWAP_AREAS.write();
    if let Some(area) = areas.iter_mut().find(|a| a.index == area_index) {
        area.free_slot(slot);
        USED_SWAP_PAGES.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Remove a page from the swap cache (after swap-in is complete).
pub fn swap_cache_delete(vaddr: u64) {
    let mut cache = SWAP_CACHE.lock();
    if let Some(entry) = cache.entries.remove(&vaddr) {
        swap_free(entry);
    }
}

// ── Swap cache lookup ───────────────────────────────────────────────────

/// Check if a virtual address is in the swap cache.
pub fn swap_cache_lookup(vaddr: u64) -> Option<u64> {
    SWAP_CACHE.lock().entries.get(&vaddr).copied()
}

// ── Page reclamation (simplified vmscan) ────────────────────────────────

/// LRU-like page list for reclamation.
struct PageReclaim {
    /// List of (vaddr, access_time) pairs, oldest first
    lru: Vec<(u64, u64)>,
    /// Maximum pages before triggering reclaim
    watermark: usize,
}

static PAGE_RECLAIM: Mutex<PageReclaim> = Mutex::new(PageReclaim {
    lru: Vec::new(),
    watermark: 512, // Reclaim when LRU exceeds 512 pages
});

/// Add a page to the LRU list (called when a page is accessed).
pub fn page_add_to_lru(vaddr: u64) {
    let mut reclaim = PAGE_RECLAIM.lock();
    // Remove existing entry if present
    reclaim.lru.retain(|&(v, _)| v != vaddr);
    // Add to end (most recently used)
    reclaim.lru.push((vaddr, crate::time::uptime_ns()));
}

/// Remove a page from the LRU list.
pub fn page_remove_from_lru(vaddr: u64) {
    PAGE_RECLAIM.lock().lru.retain(|&(v, _)| v != vaddr);
}

/// Try to reclaim pages by swapping them out. Returns the number of pages reclaimed.
pub fn try_to_reclaim_pages(target: usize) -> usize {
    let mut reclaimed = 0;
    let mut to_reclaim: Vec<(u64, u64)> = Vec::new();

    {
        let mut reclaim = PAGE_RECLAIM.lock();
        if reclaim.lru.len() <= reclaim.watermark {
            return 0;
        }

        // Take oldest pages up to target
        let count = core::cmp::min(target, reclaim.lru.len() - reclaim.watermark);
        to_reclaim = reclaim.lru.drain(..count).collect();
    }

    for (vaddr, _) in to_reclaim {
        // Read the actual page data from memory via a volatile copy.
        // The page is already mapped at `vaddr` in the kernel's address
        // space, so we can copy it directly.
        let mut page_data = [0u8; PAGE_SIZE];
        unsafe {
            let src = vaddr as *const u8;
            // SAFETY: vaddr comes from the LRU list which tracks mapped
            // kernel pages.  We use copy_nonoverlapping to read PAGE_SIZE
            // bytes from the source into our local buffer.
            core::ptr::copy_nonoverlapping(src, page_data.as_mut_ptr(), PAGE_SIZE);
        }

        if swap_out(vaddr, &page_data).is_ok() {
            reclaimed += 1;
            // Free the physical frame by deallocating the memory region.
            // This returns the physical frames to the allocator.
            use x86_64::VirtAddr;
            let _ = crate::memory::deallocate_memory(VirtAddr::new(vaddr));
        }
    }

    if reclaimed > 0 {
        crate::serial_println!(
            "[swap] reclaimed {} pages (swap_out={})",
            reclaimed,
            SWAP_OUT_COUNT.load(Ordering::Relaxed)
        );
    }

    reclaimed
}

/// Check if we should trigger page reclamation.
pub fn should_reclaim() -> bool {
    let reclaim = PAGE_RECLAIM.lock();
    reclaim.lru.len() > reclaim.watermark
}

// ── Stats ───────────────────────────────────────────────────────────────

pub fn swap_stats() -> SwapStats {
    SwapStats {
        total_pages: TOTAL_SWAP_PAGES.load(Ordering::Relaxed),
        used_pages: USED_SWAP_PAGES.load(Ordering::Relaxed),
        free_pages: TOTAL_SWAP_PAGES.load(Ordering::Relaxed)
            - USED_SWAP_PAGES.load(Ordering::Relaxed),
        swap_in_count: SWAP_IN_COUNT.load(Ordering::Relaxed),
        swap_out_count: SWAP_OUT_COUNT.load(Ordering::Relaxed),
        nr_areas: SWAP_AREAS.read().len(),
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SwapStats {
    pub total_pages: u64,
    pub used_pages: u64,
    pub free_pages: u64,
    pub swap_in_count: u64,
    pub swap_out_count: u64,
    pub nr_areas: usize,
}
