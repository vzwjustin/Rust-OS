// SPDX-License-Identifier: GPL-2.0
//! Safety-related helpers — ported from Linux `rust/kernel/safety.rs`.

#![allow(dead_code, unused_variables, unused_imports)]

/// Assert a precondition of an unsafe function at runtime when debug assertions are enabled.
///
/// At runtime this is equivalent to `debug_assert!`.  It is a no-op in release builds,
/// so there is no run-time cost in production kernels.
///
/// # Examples
///
/// ```rust,ignore
/// unsafe fn set_index(slice: &mut [u8], idx: usize, val: u8) {
///     unsafe_precondition_assert!(idx < slice.len(), "index {idx} out of bounds (len={})", slice.len());
///     unsafe { *slice.get_unchecked_mut(idx) = val; }
/// }
/// ```
#[macro_export]
macro_rules! unsafe_precondition_assert {
    ($cond:expr $(,)?) => {
        ::core::debug_assert!($cond, "unsafe precondition violated: {}", stringify!($cond))
    };
    ($cond:expr, $($arg:tt)+) => {
        ::core::debug_assert!($cond, "unsafe precondition violated: {}", format_args!($($arg)+))
    };
}

// ---------------------------------------------------------------------------
// Zero-cost safety token types
// ---------------------------------------------------------------------------

/// A zero-sized token that proves the caller holds the kernel lock associated
/// with a particular resource.  Passed by reference so the token cannot be
/// copied out of the locked scope.
///
/// Usage pattern:
/// ```rust,ignore
/// fn protected_op(token: &CallerMustHoldLock) { ... }
/// ```
#[derive(Debug)]
pub struct CallerMustHoldLock(());

impl CallerMustHoldLock {
    /// Construct the token.
    ///
    /// # Safety
    ///
    /// The caller must actually hold the relevant lock before constructing this
    /// token and must not allow the token to escape the locked region.
    #[inline(always)]
    pub const unsafe fn new() -> Self {
        Self(())
    }
}

/// A zero-sized token proving that IRQs are disabled on the current CPU.
///
/// Usage pattern:
/// ```rust,ignore
/// fn irq_safe_op(token: &IrqDisabled) { ... }
/// ```
#[derive(Debug)]
pub struct IrqDisabled(());

impl IrqDisabled {
    /// Construct the token.
    ///
    /// # Safety
    ///
    /// The caller must have disabled IRQs (e.g. via `cli`) before constructing
    /// this token and must not re-enable them while the token is live.
    #[inline(always)]
    pub const unsafe fn new() -> Self {
        Self(())
    }
}
