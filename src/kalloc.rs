// SPDX-License-Identifier: MIT
// RustOS kernel allocator flags and helpers (ported from Linux rust/kernel/alloc.rs)
// Named `kalloc` to avoid shadowing the built-in `alloc` crate.
// All bindings:: calls replaced with native Rust implementations.

#![allow(dead_code, unused_variables)]

extern crate alloc as alloc_crate;
use alloc_crate::alloc::{alloc, alloc_zeroed, dealloc, realloc, Layout};
use core::ptr;

// ---------------------------------------------------------------------------
// AllocFlags — GFP-style allocation flags
// ---------------------------------------------------------------------------

/// Allocation behaviour flags (mirrors Linux `gfp_t` / GFP_* constants).
///
/// Flags can be combined with `|` and tested with `contains`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct AllocFlags(pub u32);

impl AllocFlags {
    /// Allocation must not sleep (interrupt / atomic context).
    pub const ATOMIC: Self = AllocFlags(1 << 0);
    /// Normal kernel allocation (may sleep, direct reclaim).
    pub const KERNEL: Self = AllocFlags(1 << 1);
    /// Zero the allocated memory.
    pub const ZERO: Self = AllocFlags(1 << 2);
    /// Non-blocking allocation — do not wait on reclaim.
    pub const NOWAIT: Self = AllocFlags(1 << 3);
    /// Allocate from the DMA-capable zone (< 16 MiB on ISA).
    pub const DMA: Self = AllocFlags(1 << 4);
    /// Allocate from the DMA32 zone (< 4 GiB).
    pub const DMA32: Self = AllocFlags(1 << 5);
    /// No filesystem callbacks during reclaim.
    pub const NOFS: Self = AllocFlags(1 << 6);
    /// No I/O during reclaim.
    pub const NOIO: Self = AllocFlags(1 << 7);
    /// User-context allocation (may access user pages).
    pub const USER: Self = AllocFlags(1 << 8);

    /// Combine two flag sets.
    #[inline]
    pub const fn or(self, other: Self) -> Self {
        AllocFlags(self.0 | other.0)
    }

    /// Test whether `self` contains all bits in `flag`.
    #[inline]
    pub const fn contains(self, flag: Self) -> bool {
        (self.0 & flag.0) == flag.0
    }
}

impl core::ops::BitOr for AllocFlags {
    type Output = Self;
    #[inline]
    fn bitor(self, rhs: Self) -> Self {
        AllocFlags(self.0 | rhs.0)
    }
}

impl core::ops::BitAnd for AllocFlags {
    type Output = Self;
    #[inline]
    fn bitand(self, rhs: Self) -> Self {
        AllocFlags(self.0 & rhs.0)
    }
}

impl core::ops::Not for AllocFlags {
    type Output = Self;
    #[inline]
    fn not(self) -> Self {
        AllocFlags(!self.0)
    }
}

// ---------------------------------------------------------------------------
// GFP_* convenience constants
// ---------------------------------------------------------------------------

/// Atomic (non-sleeping) allocation.
pub const GFP_ATOMIC: AllocFlags = AllocFlags::ATOMIC;
/// Normal kernel allocation.
pub const GFP_KERNEL: AllocFlags = AllocFlags::KERNEL;
/// Zero-initialised normal kernel allocation.
pub const GFP_KERNEL_ZERO: AllocFlags = AllocFlags::KERNEL.or(AllocFlags::ZERO);
/// Non-waiting kernel allocation.
pub const GFP_NOWAIT: AllocFlags = AllocFlags::NOWAIT;
/// DMA-capable allocation.
pub const GFP_DMA: AllocFlags = AllocFlags::DMA;
/// DMA32-capable allocation.
pub const GFP_DMA32: AllocFlags = AllocFlags::DMA32;
/// Allocation that must not trigger filesystem I/O.
pub const GFP_NOFS: AllocFlags = AllocFlags::NOFS;
/// Allocation that must not trigger any I/O.
pub const GFP_NOIO: AllocFlags = AllocFlags::NOIO;
/// User-context allocation.
pub const GFP_USER: AllocFlags = AllocFlags::USER;

// ---------------------------------------------------------------------------
// AllocError
// ---------------------------------------------------------------------------

/// Indicates a failed allocation.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AllocError;

impl core::fmt::Display for AllocError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("memory allocation failed")
    }
}

// ---------------------------------------------------------------------------
// kmalloc / kfree family
// ---------------------------------------------------------------------------

const DEFAULT_ALIGN: usize = 16;

/// Allocate `size` bytes with at least `DEFAULT_ALIGN` alignment.
///
/// Returns a null pointer on failure.  Analogous to Linux `kmalloc`.
///
/// # Notes
/// The `flags` parameter is advisory on RustOS (no zone policy yet).
pub fn kmalloc(size: usize, flags: AllocFlags) -> *mut u8 {
    if size == 0 {
        return ptr::null_mut();
    }
    let layout = match Layout::from_size_align(size, DEFAULT_ALIGN) {
        Ok(l) => l,
        Err(_) => return ptr::null_mut(),
    };
    // SAFETY: layout is valid.
    let ptr = if flags.contains(AllocFlags::ZERO) {
        unsafe { alloc_zeroed(layout) }
    } else {
        unsafe { alloc(layout) }
    };
    ptr
}

/// Free a pointer previously returned by `kmalloc` / `kzalloc`.
///
/// Passing a null pointer is safe (no-op).
///
/// # Safety
/// `ptr` must have been allocated by `kmalloc` or `kzalloc` with the same
/// size, or be null.
pub unsafe fn kfree(ptr: *mut u8, size: usize) {
    if ptr.is_null() || size == 0 {
        return;
    }
    let layout = match Layout::from_size_align(size, DEFAULT_ALIGN) {
        Ok(l) => l,
        Err(_) => return,
    };
    // SAFETY: caller guarantees ptr came from kmalloc/kzalloc with this size.
    unsafe { dealloc(ptr, layout) };
}

/// Allocate `size` bytes of zeroed memory.  Analogous to Linux `kzalloc`.
pub fn kzalloc(size: usize, flags: AllocFlags) -> *mut u8 {
    kmalloc(size, flags | AllocFlags::ZERO)
}

/// Reallocate `ptr` to `new_size` bytes.  Analogous to Linux `krealloc`.
///
/// # Safety
/// `ptr` must have been allocated by `kmalloc` / `kzalloc` with `old_size`.
pub unsafe fn krealloc(
    ptr: *mut u8,
    old_size: usize,
    new_size: usize,
    flags: AllocFlags,
) -> *mut u8 {
    if new_size == 0 {
        // SAFETY: caller guarantees ptr validity.
        unsafe { kfree(ptr, old_size) };
        return ptr::null_mut();
    }
    if ptr.is_null() {
        return kmalloc(new_size, flags);
    }
    let old_layout = match Layout::from_size_align(old_size, DEFAULT_ALIGN) {
        Ok(l) => l,
        Err(_) => return ptr::null_mut(),
    };
    let new_ptr = unsafe { realloc(ptr, old_layout, new_size) };
    if !new_ptr.is_null() && flags.contains(AllocFlags::ZERO) && new_size > old_size {
        unsafe {
            ptr::write_bytes(new_ptr.add(old_size), 0u8, new_size - old_size);
        }
    }
    new_ptr
}

/// Allocate virtually-contiguous (but possibly physically-discontiguous) memory.
/// Analogous to Linux `vmalloc`.
///
/// On RustOS, vmalloc falls back to the global heap allocator.
pub fn vmalloc(size: usize) -> *mut u8 {
    kmalloc(size, GFP_KERNEL)
}

/// Free memory allocated by `vmalloc`.
///
/// # Safety
/// `ptr` must have been returned by `vmalloc` with `size`.
pub unsafe fn vfree(ptr: *mut u8, size: usize) {
    unsafe { kfree(ptr, size) };
}

// ---------------------------------------------------------------------------
// Typed allocation helpers
// ---------------------------------------------------------------------------

/// Allocate and zero-initialise a value of type `T`.
/// Returns `None` on allocation failure.
pub fn kzalloc_typed<T>(_flags: AllocFlags) -> Option<alloc_crate::boxed::Box<T>> {
    // This cannot be implemented as a raw alloc without zeroing tricks;
    // just use Box::try_new_zeroed via the global allocator.
    // For now stub out: allocate via Box which uses the global allocator.
    // Box::try_new_zeroed is nightly-only in no_std; fall back to unsafe.
    let size = core::mem::size_of::<T>();
    if size == 0 {
        // ZST
        return Some(unsafe {
            alloc_crate::boxed::Box::from_raw(core::mem::align_of::<T>() as *mut T)
        });
    }
    let layout = Layout::new::<T>();
    let ptr = unsafe { alloc_zeroed(layout) } as *mut T;
    if ptr.is_null() {
        None
    } else {
        Some(unsafe { alloc_crate::boxed::Box::from_raw(ptr) })
    }
}

// ---------------------------------------------------------------------------
// NumaNode stub (mirrors Linux NumaNode without bindings)
// ---------------------------------------------------------------------------

/// NUMA node identifier.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct NumaNode(pub i32);

impl NumaNode {
    /// Sentinel value: no node preference.
    pub const NO_NODE: NumaNode = NumaNode(-1);

    /// Construct from a non-negative node id.
    pub fn new(id: i32) -> Result<Self, i32> {
        if id < 0 {
            Err(22 /* EINVAL */)
        } else {
            Ok(NumaNode(id))
        }
    }
}
