// SPDX-License-Identifier: MIT
// RustOS page management abstractions (ported from Linux rust/kernel/page.rs)
// All bindings:: calls replaced with native Rust + x86_64 implementations.

#![allow(dead_code, unused_variables)]

use core::ptr;

// ---------------------------------------------------------------------------
// Page constants
// ---------------------------------------------------------------------------

pub const PAGE_SHIFT: usize = 12;
pub const PAGE_SIZE: usize = 1 << PAGE_SHIFT;
pub const PAGE_MASK: usize = !(PAGE_SIZE - 1);

/// Round `addr` up to the next page boundary.
/// Returns `None` on overflow.
#[inline]
pub fn page_align(addr: usize) -> Option<usize> {
    addr.checked_add(PAGE_SIZE - 1).map(|a| a & PAGE_MASK)
}

/// Round `addr` down to the page boundary below it.
#[inline]
pub fn page_align_down(addr: usize) -> usize {
    addr & PAGE_MASK
}

// ---------------------------------------------------------------------------
// Physical-to-virtual mapping
// ---------------------------------------------------------------------------

/// Convert a physical address to a kernel virtual address.
/// On x86_64 with identity-mapped or direct-map regions, physical memory
/// is accessible at PHYS_OFFSET + phys. We use the direct-map offset typical
/// for a RustOS setup (identity map; adjust if your kernel uses 0xffff888000000000).
#[inline]
pub fn phys_to_virt(phys: u64) -> *mut u8 {
    // RustOS uses identity-mapped physical memory (phys == virt in early boot).
    // If a higher-half direct map is established, change the constant below.
    const DIRECT_MAP_OFFSET: u64 = 0;
    (DIRECT_MAP_OFFSET + phys) as *mut u8
}

// ---------------------------------------------------------------------------
// Page — a single physical page frame
// ---------------------------------------------------------------------------

/// A single physical page frame (4 KiB on x86_64).
pub struct Page {
    phys_addr: u64,
}

impl Page {
    /// Construct a `Page` from a physical address.
    #[inline]
    pub fn from_phys(phys: u64) -> Self {
        Page { phys_addr: phys }
    }

    /// Return the physical address of the page.
    #[inline]
    pub fn phys_addr(&self) -> u64 {
        self.phys_addr
    }

    /// Return a pointer to the kernel virtual address of this page.
    #[inline]
    pub fn virt_addr(&self) -> *mut u8 {
        phys_to_virt(self.phys_addr)
    }

    /// Read a value of type `T` from `offset` bytes into this page.
    ///
    /// # Safety
    /// - `offset + size_of::<T>()` must not exceed `PAGE_SIZE`.
    /// - The page must be mapped and readable.
    #[inline]
    pub unsafe fn read<T: Copy>(&self, offset: usize) -> T {
        debug_assert!(offset + core::mem::size_of::<T>() <= PAGE_SIZE);
        unsafe { ptr::read_volatile(self.virt_addr().add(offset) as *const T) }
    }

    /// Write a value of type `T` to `offset` bytes into this page.
    ///
    /// # Safety
    /// - `offset + size_of::<T>()` must not exceed `PAGE_SIZE`.
    /// - The page must be mapped and writable.
    #[inline]
    pub unsafe fn write<T: Copy>(&self, offset: usize, val: T) {
        debug_assert!(offset + core::mem::size_of::<T>() <= PAGE_SIZE);
        unsafe { ptr::write_volatile(self.virt_addr().add(offset) as *mut T, val) }
    }

    /// Zero the entire page.
    ///
    /// # Safety
    /// The page must be mapped and writable.
    #[inline]
    pub unsafe fn zero(&self) {
        unsafe { ptr::write_bytes(self.virt_addr(), 0u8, PAGE_SIZE) }
    }

    /// Copy `src` bytes into this page at `offset`.
    ///
    /// # Safety
    /// - `offset + src.len()` must not exceed `PAGE_SIZE`.
    /// - The page must be mapped and writable.
    pub unsafe fn copy_from_user_slice(&self, offset: usize, src: &[u8]) -> Result<(), i32> {
        if offset + src.len() > PAGE_SIZE {
            return Err(EINVAL);
        }
        unsafe {
            ptr::copy_nonoverlapping(src.as_ptr(), self.virt_addr().add(offset), src.len());
        }
        Ok(())
    }

    /// Read `len` bytes from `offset` into `dst`.
    pub unsafe fn read_raw(&self, dst: &mut [u8], offset: usize) -> Result<(), i32> {
        let len = dst.len();
        if offset + len > PAGE_SIZE {
            return Err(EINVAL);
        }
        unsafe {
            ptr::copy_nonoverlapping(self.virt_addr().add(offset), dst.as_mut_ptr(), len);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// PageRange — physically contiguous pages
// ---------------------------------------------------------------------------

/// A range of physically contiguous page frames.
pub struct PageRange {
    start: u64,
    count: usize,
}

impl PageRange {
    /// Allocate `count` contiguous pages.
    ///
    /// `order` is the power-of-two order (Linux buddy allocator convention).
    /// Returns `None` if allocation fails.
    pub fn alloc(count: usize, order: u32) -> Option<Self> {
        // Delegate to the global frame allocator in memory.rs when available.
        // For now, provide a stub that fails gracefully.
        let _ = order;
        None
    }

    /// Free the page range.
    pub fn free(self) {
        // Stub: return frames to global allocator.
    }

    /// Return the physical address of the first page.
    #[inline]
    pub fn start_phys(&self) -> u64 {
        self.start
    }

    /// Number of pages in this range.
    #[inline]
    pub fn len(&self) -> usize {
        self.count
    }

    /// Whether the range is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Iterate over pages in the range.
    pub fn iter(&self) -> PageRangeIter<'_> {
        PageRangeIter {
            range: self,
            idx: 0,
        }
    }
}

/// Iterator over a `PageRange`.
pub struct PageRangeIter<'a> {
    range: &'a PageRange,
    idx: usize,
}

impl<'a> Iterator for PageRangeIter<'a> {
    type Item = Page;

    fn next(&mut self) -> Option<Page> {
        if self.idx >= self.range.count {
            return None;
        }
        let phys = self.range.start + (self.idx * PAGE_SIZE) as u64;
        self.idx += 1;
        Some(Page::from_phys(phys))
    }
}

// ---------------------------------------------------------------------------
// BorrowedPage — non-owning reference to a page (mirrors Linux BorrowedPage)
// ---------------------------------------------------------------------------

/// A non-owning reference to a physical page.
pub struct BorrowedPage<'a> {
    page: Page,
    _marker: core::marker::PhantomData<&'a Page>,
}

impl<'a> BorrowedPage<'a> {
    /// Construct from a physical address without taking ownership.
    ///
    /// # Safety
    /// The physical page at `phys` must remain valid for lifetime `'a`.
    pub unsafe fn from_phys(phys: u64) -> Self {
        BorrowedPage {
            page: Page::from_phys(phys),
            _marker: core::marker::PhantomData,
        }
    }
}

impl<'a> core::ops::Deref for BorrowedPage<'a> {
    type Target = Page;
    fn deref(&self) -> &Self::Target {
        &self.page
    }
}

// ---------------------------------------------------------------------------
// Error constant (matches EINVAL = 22)
// ---------------------------------------------------------------------------

// Defined here to avoid a circular dep; the real definition lives in error.rs.
const EINVAL: i32 = 22;
