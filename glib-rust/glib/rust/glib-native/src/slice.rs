//! Slice allocator matching `gslice.h` (deprecated).
//!
//! Deprecated in GLib 2.34 in favor of `g_malloc`/`g_free`.
//! This implementation simply delegates to the `mem` module.
//! Fully `no_std` compatible using `alloc`.

use alloc::alloc::{alloc, alloc_zeroed, dealloc, Layout};
use core::ptr;

/// Slice config (`GSliceConfig`). Deprecated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SliceConfig {
    AlwaysMalloc,
    BypassMagazines,
    WorkingSetMsecs,
    ColorIncrement,
    ChunkSizes,
    ContentionCounter,
}

/// Allocate a block (`g_slice_alloc`). Deprecated.
///
/// In Rust, use `Box` or `Vec`. This is a compatibility wrapper.
pub fn slice_alloc(block_size: usize) -> *mut u8 {
    if block_size == 0 {
        return ptr::null_mut();
    }
    let layout = match Layout::from_size_align(block_size, 8) {
        Ok(l) => l,
        Err(_) => return ptr::null_mut(),
    };
    unsafe { alloc(layout) }
}

/// Allocate and zero a block (`g_slice_alloc0`). Deprecated.
pub fn slice_alloc0(block_size: usize) -> *mut u8 {
    if block_size == 0 {
        return ptr::null_mut();
    }
    let layout = match Layout::from_size_align(block_size, 8) {
        Ok(l) => l,
        Err(_) => return ptr::null_mut(),
    };
    unsafe { alloc_zeroed(layout) }
}

/// Copy a block (`g_slice_copy`). Deprecated.
pub fn slice_copy(block_size: usize, mem_block: *const u8) -> *mut u8 {
    if block_size == 0 || mem_block.is_null() {
        return ptr::null_mut();
    }
    let dst = slice_alloc(block_size);
    if !dst.is_null() {
        unsafe {
            ptr::copy_nonoverlapping(mem_block, dst, block_size);
        }
    }
    dst
}

/// Free a block (`g_slice_free1`). Deprecated.
pub fn slice_free1(block_size: usize, mem_block: *mut u8) {
    if block_size == 0 || mem_block.is_null() {
        return;
    }
    let layout = match Layout::from_size_align(block_size, 8) {
        Ok(l) => l,
        Err(_) => return,
    };
    unsafe {
        dealloc(mem_block, layout);
    }
}

/// Set config (`g_slice_set_config`). Deprecated, no-op.
pub fn slice_set_config(_ckey: SliceConfig, _value: i64) {
    // Deprecated no-op
}

/// Get config (`g_slice_get_config`). Deprecated, returns 0.
pub fn slice_get_config(_ckey: SliceConfig) -> i64 {
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alloc_and_free() {
        let p = slice_alloc(64);
        assert!(!p.is_null());
        slice_free1(64, p);
    }

    #[test]
    fn alloc0_zeroed() {
        let p = slice_alloc0(32);
        assert!(!p.is_null());
        for i in 0..32 {
            assert_eq!(unsafe { *p.add(i) }, 0);
        }
        slice_free1(32, p);
    }

    #[test]
    fn copy() {
        let src = slice_alloc(16);
        assert!(!src.is_null());
        for i in 0..16 {
            unsafe {
                *src.add(i) = i as u8;
            }
        }
        let dst = slice_copy(16, src);
        assert!(!dst.is_null());
        for i in 0..16 {
            assert_eq!(unsafe { *dst.add(i) }, i as u8);
        }
        slice_free1(16, src);
        slice_free1(16, dst);
    }

    #[test]
    fn zero_size_returns_null() {
        assert!(slice_alloc(0).is_null());
        assert!(slice_alloc0(0).is_null());
    }
}
