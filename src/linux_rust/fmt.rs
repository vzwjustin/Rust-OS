//! Formatting utilities.
//!
//! Ported from Linux `rust/kernel/fmt.rs`.
//! Re-exports `core::fmt` types and provides a `Display` trait wrapper
//! for foreign types.

pub use core::fmt::{
    Arguments,
    Debug,
    Error,
    Formatter,
    Result,
    Write,
};

/// Internal adapter used to route formatting trait implementations for foreign types.
#[doc(hidden)]
pub struct Adapter<T>(pub T);

macro_rules! impl_fmt_adapter_forward {
    ($($trait:ident),* $(,)?) => {
        $(
            impl<T: $trait> $trait for Adapter<T> {
                fn fmt(&self, f: &mut Formatter<'_>) -> Result {
                    let Self(t) = self;
                    $trait::fmt(t, f)
                }
            }
        )*
    };
}

use core::fmt::{
    Binary,
    LowerExp,
    LowerHex,
    Octal,
    Pointer,
    UpperExp,
    UpperHex,
};
impl_fmt_adapter_forward!(Debug, LowerHex, UpperHex, Octal, Binary, Pointer, LowerExp, UpperExp);

/// A copy of `core::fmt::Display` that allows implementing it for foreign types.
pub trait Display {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result;
}

impl<T: ?Sized + Display> Display for &T {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        Display::fmt(*self, f)
    }
}

impl<T: ?Sized + Display> core::fmt::Display for Adapter<&T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        let Self(t) = self;
        Display::fmt(t, f)
    }
}

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
    bool,
    char,
    core::panic::PanicInfo<'_>,
    Arguments<'_>,
    i128,
    i16,
    i32,
    i64,
    i8,
    isize,
    str,
    u128,
    u16,
    u32,
    u64,
    u8,
    usize,
);
