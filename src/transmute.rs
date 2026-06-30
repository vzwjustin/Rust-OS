// SPDX-License-Identifier: GPL-2.0
//! Traits for transmuting types — ported from Linux `rust/kernel/transmute.rs`.

#![allow(dead_code, unused_variables, unused_imports)]

use core::mem::size_of;

// ---------------------------------------------------------------------------
// FromBytes
// ---------------------------------------------------------------------------

/// # Safety
///
/// Any bit-pattern must be valid for this type.  The type must not have
/// interior mutability (`UnsafeCell`).
pub unsafe trait FromBytes: Sized {
    /// Interpret a byte slice as a reference to `Self`.
    ///
    /// Returns `None` if `bytes` is not aligned to `Self` or its length does
    /// not equal `size_of::<Self>()`.
    fn from_bytes(bytes: &[u8]) -> Option<&Self> {
        let size = size_of::<Self>();
        if size == 0 || bytes.len() != size {
            return None;
        }
        let ptr = bytes.as_ptr() as *const Self;
        if ptr.align_offset(core::mem::align_of::<Self>()) != 0 {
            return None;
        }
        // SAFETY: alignment and size checked above; all bit-patterns valid per trait contract.
        Some(unsafe { &*ptr })
    }

    /// Like [`from_bytes`] but takes a prefix, returning the remainder.
    fn from_bytes_prefix(bytes: &[u8]) -> Option<(&Self, &[u8])> {
        let size = size_of::<Self>();
        if size == 0 || bytes.len() < size {
            return None;
        }
        let (head, tail) = bytes.split_at(size);
        Self::from_bytes(head).map(|r| (r, tail))
    }

    /// Interpret a mutable byte slice as a mutable reference to `Self`.
    fn from_bytes_mut(bytes: &mut [u8]) -> Option<&mut Self> {
        let size = size_of::<Self>();
        if size == 0 || bytes.len() != size {
            return None;
        }
        let ptr = bytes.as_mut_ptr() as *mut Self;
        if (ptr as usize) % core::mem::align_of::<Self>() != 0 {
            return None;
        }
        // SAFETY: alignment and size checked; all bit-patterns valid.
        Some(unsafe { &mut *ptr })
    }

    /// Like [`from_bytes_mut`] but returns the remainder.
    fn from_bytes_mut_prefix(bytes: &mut [u8]) -> Option<(&mut Self, &mut [u8])> {
        let size = size_of::<Self>();
        if size == 0 || bytes.len() < size {
            return None;
        }
        let (head, tail) = bytes.split_at_mut(size);
        Self::from_bytes_mut(head).map(|r| (r, tail))
    }

    /// Copy `Self` out of a byte slice.
    fn from_bytes_copy(bytes: &[u8]) -> Option<Self> {
        let size = size_of::<Self>();
        if size == 0 || bytes.len() != size {
            return None;
        }
        let mut out = core::mem::MaybeUninit::<Self>::uninit();
        // SAFETY: sizes match; all bit-patterns valid per trait contract.
        unsafe {
            core::ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                out.as_mut_ptr() as *mut u8,
                size,
            );
            Some(out.assume_init())
        }
    }

    /// Like [`from_bytes_copy`] but takes a prefix and returns the remainder.
    fn from_bytes_copy_prefix(bytes: &[u8]) -> Option<(Self, &[u8])> {
        let size = size_of::<Self>();
        if size == 0 || bytes.len() < size {
            return None;
        }
        let (head, tail) = bytes.split_at(size);
        Self::from_bytes_copy(head).map(|v| (v, tail))
    }
}

// ---------------------------------------------------------------------------
// AsBytes
// ---------------------------------------------------------------------------

/// # Safety
///
/// The type must not have any padding bytes (or the caller accepts that padding
/// bytes are unspecified).  The type must not have interior mutability.
pub unsafe trait AsBytes {
    /// View `self` as a byte slice.
    fn as_bytes(&self) -> &[u8] {
        // SAFETY: size_of::<Self>() bytes starting at `self` are valid to read.
        unsafe {
            core::slice::from_raw_parts(
                self as *const Self as *const u8,
                size_of::<Self>(),
            )
        }
    }

    /// View `self` as a mutable byte slice.
    fn as_bytes_mut(&mut self) -> &mut [u8] {
        // SAFETY: size_of::<Self>() bytes starting at `self` are valid to write.
        unsafe {
            core::slice::from_raw_parts_mut(
                self as *mut Self as *mut u8,
                size_of::<Self>(),
            )
        }
    }
}

// ---------------------------------------------------------------------------
// Blanket implementations for primitive types
// ---------------------------------------------------------------------------

macro_rules! impl_from_bytes_and_as_bytes {
    ($($ty:ty),* $(,)?) => {
        $(
            // SAFETY: all bit-patterns are valid for these types.
            unsafe impl FromBytes for $ty {}
            // SAFETY: no padding in primitive types.
            unsafe impl AsBytes for $ty {}
        )*
    };
}

impl_from_bytes_and_as_bytes!(
    u8, u16, u32, u64, u128, usize,
    i8, i16, i32, i64, i128, isize,
);

// ---------------------------------------------------------------------------
// FromZeros / Zeroable
// ---------------------------------------------------------------------------

/// Types whose all-zeros bit-pattern is a valid, initialized value.
///
/// # Safety
///
/// A value of type `Self` consisting entirely of zero bytes must be a valid,
/// fully initialized instance of `Self`.
pub unsafe trait FromZeros: FromBytes {
    /// Returns a zeroed instance of `Self`.
    fn zeroed() -> Self
    where
        Self: Sized,
    {
        // SAFETY: FromBytes guarantees any bit-pattern (including zeros) is valid.
        unsafe { core::mem::zeroed() }
    }
}

macro_rules! impl_from_zeros {
    ($($ty:ty),* $(,)?) => {
        $(
            // SAFETY: zero is a valid value for all integer/float primitive types.
            unsafe impl FromZeros for $ty {}
        )*
    };
}

impl_from_zeros!(
    u8, u16, u32, u64, u128, usize,
    i8, i16, i32, i64, i128, isize,
);
