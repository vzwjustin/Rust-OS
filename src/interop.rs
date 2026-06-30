// SPDX-License-Identifier: GPL-2.0
//! Helpers for interfacing Rust with C-style integer return conventions.
//! Ported from Linux `rust/kernel/interop.rs` (bindings-free version).

#![allow(dead_code, unused_variables, unused_imports)]

use crate::error::{Error, KernelError};
use alloc::alloc::AllocError;
use core::num::NonZeroI32;

// ---------------------------------------------------------------------------
// POSIX errno values used by the interop helpers
// ---------------------------------------------------------------------------

/// Maximum valid errno magnitude (matches Linux MAX_ERRNO = 4095).
pub const MAX_ERRNO: i32 = 4095;

// ---------------------------------------------------------------------------
// Result ↔ i32 conversions
// ---------------------------------------------------------------------------

/// Convert a C-style `i32` return value to a `Result<()>`.
///
/// Returns `Ok(())` for values ≥ 0, and `Err(Error)` for negative values in
/// the range `[-MAX_ERRNO, -1]`.
///
/// Values outside the valid errno range (< -MAX_ERRNO) are mapped to
/// `EINVAL` to avoid constructing an invalid `Error`.
#[inline]
pub fn to_result(val: i32) -> Result<(), LinuxError> {
    if val >= 0 {
        Ok(())
    } else if val >= -MAX_ERRNO {
        // SAFETY: val is in [-MAX_ERRNO, -1], so it is a valid negative errno.
        Err(LinuxError(unsafe { NonZeroI32::new_unchecked(val) }))
    } else {
        // Out-of-range: return EINVAL.
        Err(LinuxError(unsafe { NonZeroI32::new_unchecked(-22) }))
    }
}

/// Convert a `Result<()>` back to a C-style `i32`.
///
/// `Ok(())` maps to `0`.  `Err(e)` maps to the negative errno value.
#[inline]
pub fn from_result(r: Result<(), LinuxError>) -> i32 {
    match r {
        Ok(()) => 0,
        Err(e) => e.0.get(),
    }
}

// ---------------------------------------------------------------------------
// LinuxError — a thin newtype for errno-style errors
// ---------------------------------------------------------------------------

/// A Linux errno value, stored as a negative non-zero `i32`.
///
/// This is a lightweight alternative to the full `Error` type in `error.rs`
/// for use in FFI-boundary helpers.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LinuxError(NonZeroI32);

impl LinuxError {
    /// Creates a `LinuxError` from a raw negative errno value.
    ///
    /// # Safety
    ///
    /// `errno` must be in the range `[-MAX_ERRNO, -1]`.
    #[inline]
    pub const unsafe fn from_errno_unchecked(errno: i32) -> Self {
        // SAFETY: caller guarantees errno is non-zero.
        Self(unsafe { NonZeroI32::new_unchecked(errno) })
    }

    /// Returns the raw errno value (negative).
    #[inline]
    pub const fn to_errno(self) -> i32 {
        self.0.get()
    }
}

// ---------------------------------------------------------------------------
// from_kernel_result! macro
// ---------------------------------------------------------------------------

/// Convert a `Result<i32, LinuxError>` into a plain `i32`, following the
/// Linux convention of returning negative errno values on error.
///
/// # Examples
///
/// ```rust,ignore
/// fn my_ioctl() -> i32 {
///     from_kernel_result! {
///         let n = some_op()?;
///         Ok(n as i32)
///     }
/// }
/// ```
#[macro_export]
macro_rules! from_kernel_result {
    ($($tt:tt)*) => {{
        let __result: Result<i32, $crate::interop::LinuxError> = (|| -> Result<i32, $crate::interop::LinuxError> {
            $($tt)*
        })();
        match __result {
            Ok(v) => v,
            Err(e) => e.to_errno(),
        }
    }};
}
