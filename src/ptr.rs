// SPDX-License-Identifier: GPL-2.0
//! Pointer utilities — ported from Linux `rust/kernel/ptr.rs`.

#![allow(dead_code, unused_variables, unused_imports)]

extern crate alloc;

use core::{
    mem::{align_of, size_of},
    num::NonZero,
    ptr::NonNull,
};

// ---------------------------------------------------------------------------
// Alignment type
// ---------------------------------------------------------------------------

/// An alignment value that is always a power of two.
///
/// This is a temporary local substitute for the `Alignment` nightly type.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Alignment(NonZero<usize>);

impl Alignment {
    /// Validate `ALIGN` at compile time and return an `Alignment`.
    ///
    /// Triggers a build error if `ALIGN` is not a power of two.
    #[inline(always)]
    pub const fn new<const ALIGN: usize>() -> Self {
        assert!(ALIGN.is_power_of_two(), "alignment must be a power of two");
        // SAFETY: power of two implies non-zero.
        Self(unsafe { NonZero::new_unchecked(ALIGN) })
    }

    /// Runtime-checked constructor.  Returns `None` if `align` is not a power of two.
    #[inline(always)]
    pub const fn new_checked(align: usize) -> Option<Self> {
        if align.is_power_of_two() {
            // SAFETY: power of two implies non-zero.
            Some(Self(unsafe { NonZero::new_unchecked(align) }))
        } else {
            None
        }
    }

    /// Returns the alignment of `T`.
    #[inline(always)]
    pub const fn of<T>() -> Self {
        // SAFETY: `align_of::<T>()` is always a power of two.
        unsafe { Self(NonZero::new_unchecked(align_of::<T>())) }
    }

    /// Returns the alignment as a `usize`.
    #[inline(always)]
    pub const fn as_usize(self) -> usize {
        self.0.get()
    }

    /// Returns the alignment as a `NonZero<usize>`.
    #[inline(always)]
    pub const fn as_nonzero(self) -> NonZero<usize> {
        self.0
    }

    /// Returns the base-2 logarithm.
    #[inline(always)]
    pub const fn log2(self) -> u32 {
        self.0.ilog2()
    }

    /// Returns `align - 1` as a bitmask.
    #[inline(always)]
    pub const fn mask(self) -> usize {
        self.0.get() - 1
    }

    /// Returns `true` if `addr` satisfies this alignment.
    #[inline(always)]
    pub const fn is_aligned(self, addr: usize) -> bool {
        (addr & self.mask()) == 0
    }

    /// Round `addr` up to this alignment.
    #[inline(always)]
    pub const fn align_up(self, addr: usize) -> usize {
        (addr + self.mask()) & !self.mask()
    }

    /// Round `addr` down to this alignment.
    #[inline(always)]
    pub const fn align_down(self, addr: usize) -> usize {
        addr & !self.mask()
    }
}

// ---------------------------------------------------------------------------
// ERR_PTR / IS_ERR / PTR_ERR helpers
// ---------------------------------------------------------------------------

/// The maximum errno magnitude — values above this are valid pointers.
///
/// Mirrors Linux's `MAX_ERRNO` (4095).
const MAX_ERRNO: isize = 4095;

/// Check whether a raw pointer encodes a Linux-style error value.
///
/// Linux passes errors back through pointer return values by encoding `errno`
/// as a large negative number near the top of the address space.
#[inline]
pub fn is_err_ptr<T>(ptr: *const T) -> bool {
    (ptr as isize) >= -(MAX_ERRNO) && (ptr as isize) < 0
}

/// Extract the error code from an ERR_PTR value.
///
/// # Safety
///
/// `ptr` must satisfy [`is_err_ptr`].
#[inline]
pub unsafe fn ptr_err<T>(ptr: *const T) -> i32 {
    ptr as i32
}

/// Create an ERR_PTR value from an errno.
///
/// `errno` must be in the range `[-MAX_ERRNO, -1]`.
#[inline]
pub fn err_ptr<T>(errno: i32) -> *mut T {
    errno as *mut T
}

/// Convert an ERR_PTR to a `Result<NonNull<T>>`.
///
/// Returns `Ok(ptr)` for valid (non-error) non-null pointers.
/// Returns `Err(errno)` for ERR_PTR values.
/// Returns `Err(-EINVAL)` for null pointers.
#[inline]
pub fn from_err_ptr<T>(ptr: *mut T) -> Result<NonNull<T>, i32> {
    if is_err_ptr(ptr as *const T) {
        // SAFETY: is_err_ptr returned true.
        Err(unsafe { ptr_err(ptr as *const T) })
    } else if ptr.is_null() {
        Err(-22) // EINVAL
    } else {
        // SAFETY: `ptr` is non-null and not an error value.
        Ok(unsafe { NonNull::new_unchecked(ptr) })
    }
}

// ---------------------------------------------------------------------------
// KBox<T> — thin wrapper around alloc::boxed::Box with kernel allocation semantics
// ---------------------------------------------------------------------------

/// A heap-allocated box with kernel-centric semantics.
///
/// Currently a newtype over [`alloc::boxed::Box`].  When a kernel allocator
/// is integrated, the allocator parameter can be threaded through here.
pub struct KBox<T>(alloc::boxed::Box<T>);

impl<T> KBox<T> {
    /// Allocate a `KBox<T>` containing `val`.
    pub fn new(val: T) -> Self {
        KBox(alloc::boxed::Box::new(val))
    }

    /// Consume the box, returning the inner value.
    pub fn into_inner(self) -> T {
        *self.0
    }

    /// Returns a raw pointer to the contained value.
    pub fn as_ptr(&self) -> *const T {
        &*self.0 as *const T
    }

    /// Returns a mutable raw pointer to the contained value.
    pub fn as_mut_ptr(&mut self) -> *mut T {
        &mut *self.0 as *mut T
    }

    /// Pin this box.
    pub fn into_pin(self) -> core::pin::Pin<alloc::boxed::Box<T>> {
        alloc::boxed::Box::into_pin(self.0)
    }
}

impl<T> core::ops::Deref for KBox<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T> core::ops::DerefMut for KBox<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

// ---------------------------------------------------------------------------
// KnownSize — marker trait
// ---------------------------------------------------------------------------

/// Types whose size is known at compile time and is non-zero.
///
/// Implemented automatically for all `Sized` types with `size_of::<T>() > 0`.
/// This is a marker to enable certain generic constraints.
pub trait KnownSize: Sized {}

// We cannot blanket-impl "only when size > 0" in stable Rust, so provide a
// macro for the cases we care about.
macro_rules! impl_known_size {
    ($($ty:ty),* $(,)?) => { $(impl KnownSize for $ty {})* };
}
impl_known_size!(
    u8, u16, u32, u64, u128, usize,
    i8, i16, i32, i64, i128, isize,
);
