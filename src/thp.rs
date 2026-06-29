//! Transparent Huge Pages (THP) — khugepaged-style collapse and madvise hooks.
//!
//! Tracks per-range `MADV_HUGEPAGE` / `MADV_NOHUGEPAGE` advice and periodically
//! attempts to collapse aligned 2 MiB regions of 4 KiB mappings into a single
//! PDE huge-page entry.

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use spin::RwLock;
use x86_64::structures::paging::PageTableFlags;
use x86_64::{PhysAddr, VirtAddr};

use crate::linux_compat::{LinuxError, LinuxResult};
use crate::memory::{self, HUGEPAGE_4K_PAGES, HUGEPAGE_SIZE};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ThpAdvice {
    /// User requested transparent huge pages for this range.
    Huge,
    /// User forbade transparent huge pages for this range.
    NoHuge,
}

#[derive(Debug, Clone, Copy)]
struct AdviceRange {
    start: usize,
    end: usize,
    advice: ThpAdvice,
}

static INITIALIZED: AtomicBool = AtomicBool::new(false);
static ENABLED: AtomicBool = AtomicBool::new(true);
static ADVICE: RwLock<Vec<AdviceRange>> = RwLock::new(Vec::new());
static COLLAPSED: AtomicUsize = AtomicUsize::new(0);
static SCAN_ATTEMPTS: AtomicUsize = AtomicUsize::new(0);

/// Initialize THP state and run an initial khugepaged-style scan.
pub fn init() {
    if INITIALIZED.swap(true, Ordering::SeqCst) {
        return;
    }
    ADVICE.write().clear();
    khugepaged_scan();
    crate::serial_println!(
        "[thp] initialized (collapsed={})",
        COLLAPSED.load(Ordering::Relaxed)
    );
}

/// Global enable/disable (mirrors `/sys/kernel/mm/transparent_hugepage/enabled`).
pub fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
}

pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

pub fn collapsed_count() -> usize {
    COLLAPSED.load(Ordering::Relaxed)
}

/// Record madvise advice for `[addr, addr+len)`.
pub fn set_advice(addr: usize, len: usize, want_huge: bool) -> LinuxResult<()> {
    if addr == 0 || len == 0 {
        return Err(LinuxError::EINVAL);
    }
    let start = addr & !(HUGEPAGE_SIZE as usize - 1);
    let end = addr.saturating_add(len);
    let advice = if want_huge {
        ThpAdvice::Huge
    } else {
        ThpAdvice::NoHuge
    };

    let mut ranges = ADVICE.write();
    ranges.retain(|r| r.end <= start || r.start >= end);
    ranges.push(AdviceRange { start, end, advice });
    ranges.sort_by_key(|r| r.start);

    if want_huge && is_enabled() {
        let mut virt = start;
        while virt + HUGEPAGE_SIZE as usize <= end {
            let _ = try_collapse_region(virt);
            virt += HUGEPAGE_SIZE as usize;
        }
    }
    Ok(())
}

fn advice_for(addr: usize) -> Option<ThpAdvice> {
    ADVICE
        .read()
        .iter()
        .find(|r| addr >= r.start && addr < r.end)
        .map(|r| r.advice)
}

/// khugepaged-style scan over all MADV_HUGEPAGE ranges.
pub fn khugepaged_scan() -> usize {
    if !is_enabled() {
        return 0;
    }
    let candidates: Vec<usize> = ADVICE
        .read()
        .iter()
        .filter(|r| r.advice == ThpAdvice::Huge)
        .flat_map(|r| {
            let mut addrs = Vec::new();
            let mut virt = r.start & !(HUGEPAGE_SIZE as usize - 1);
            while virt + HUGEPAGE_SIZE as usize <= r.end {
                addrs.push(virt);
                virt += HUGEPAGE_SIZE as usize;
            }
            addrs
        })
        .collect();

    let mut collapsed = 0usize;
    for virt in candidates {
        SCAN_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
        if try_collapse_region(virt).is_ok() {
            collapsed += 1;
        }
    }
    collapsed
}

/// Attempt to collapse one 2 MiB aligned region starting at `virt`.
pub fn try_collapse_region(virt: usize) -> Result<(), &'static str> {
    if virt as u64 % HUGEPAGE_SIZE != 0 {
        return Err("not huge-page aligned");
    }
    if !is_enabled() {
        return Err("thp disabled");
    }
    if advice_for(virt) != Some(ThpAdvice::Huge) {
        return Err("range not marked MADV_HUGEPAGE");
    }

    let mm = memory::get_memory_manager().ok_or("memory manager not initialized")?;
    let page_size = 4096usize;

    // Verify every 4 KiB page in the 2 MiB window is mapped with uniform flags.
    let mut first_flags: Option<PageTableFlags> = None;
    for i in 0..HUGEPAGE_4K_PAGES {
        let va = virt + i * page_size;
        if memory::translate_addr(VirtAddr::new(va as u64)).is_none() {
            return Err("missing 4K mapping");
        }
        let page = x86_64::structures::paging::Page::containing_address(VirtAddr::new(va as u64));
        let flags = mm
            .page_table_manager
            .lock()
            .get_flags(page)
            .ok_or("missing page flags")?;
        if flags.contains(PageTableFlags::HUGE_PAGE) {
            return Err("already huge");
        }
        match first_flags {
            None => first_flags = Some(flags),
            Some(expected) if expected == flags => {}
            _ => return Err("inconsistent flags"),
        }
    }

    let flags = first_flags.ok_or("no pages")?;
    let huge_phys = memory::allocate_huge_frame().ok_or("no huge frame")?;

    // Copy contents from existing 4 KiB mappings into the new huge frame.
    unsafe {
        let huge_ptr = (mm.physical_memory_offset() + huge_phys.as_u64()).as_mut_ptr::<u8>();
        for i in 0..HUGEPAGE_4K_PAGES {
            let va = virt + i * page_size;
            let phys =
                memory::translate_addr(VirtAddr::new(va as u64)).ok_or("translate failed")?;
            let src = (mm.physical_memory_offset() + phys.as_u64()).as_ptr::<u8>();
            core::ptr::copy_nonoverlapping(src, huge_ptr.add(i * page_size), page_size);
        }
    }

    // Drop old 4 KiB mappings (returns frames to the buddy allocator).
    for i in 0..HUGEPAGE_4K_PAGES {
        memory::unmap_user_page(virt + i * page_size)?;
    }

    memory::map_user_huge_page(virt, huge_phys, flags)?;
    COLLAPSED.fetch_add(1, Ordering::Relaxed);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advice_validation() {
        assert!(set_advice(0, 4096, true).is_err());
        assert!(set_advice(0x1000, 0, true).is_err());
    }
}
