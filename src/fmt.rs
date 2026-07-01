// SPDX-License-Identifier: GPL-2.0
//! Formatting utilities — ported from Linux `rust/kernel/fmt.rs`.

#![allow(dead_code, unused_variables, unused_imports)]

// Re-export the core formatting primitives so callers can `use crate::fmt::*`.
pub use core::fmt::{
    Arguments,
    Debug,
    Error,
    Formatter,
    Result,
    Write,
};

/// A display format trait that mirrors [`core::fmt::Display`] but is defined
/// in this crate, allowing implementations for foreign types without the
/// orphan rule getting in the way.
pub trait Display {
    /// Format `self` into `f`.
    fn fmt(&self, f: &mut Formatter<'_>) -> Result;
}

// Blanket: references to Display types are also Display.
impl<T: ?Sized + Display> Display for &T {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        Display::fmt(*self, f)
    }
}

// Forward Display for common primitive types.
macro_rules! impl_display_forward {
    ($($ty:ty),* $(,)?) => {
        $(
            impl Display for $ty {
                fn fmt(&self, f: &mut Formatter<'_>) -> Result {
                    core::fmt::Display::fmt(self, f)
                }
            }
        )*
    };
}

impl_display_forward!(
    bool, char,
    i8, i16, i32, i64, i128, isize,
    u8, u16, u32, u64, u128, usize,
    str,
    core::fmt::Arguments<'_>,
);

/// Convenience re-export of `format_args!` so modules that `use crate::fmt`
/// can write `fmt::format_args!(...)`.
#[macro_export]
macro_rules! fmt_args {
    ($($tt:tt)*) => { ::core::format_args!($($tt)*) };
}
