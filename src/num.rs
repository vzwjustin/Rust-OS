// SPDX-License-Identifier: GPL-2.0
//! Numeric utilities — ported from Linux `rust/kernel/num.rs`.

#![allow(dead_code, unused_variables, unused_imports)]

use core::ops;

// ---------------------------------------------------------------------------
// Integer trait
// ---------------------------------------------------------------------------

/// Designates unsigned primitive integer types.
pub enum Unsigned {}

/// Designates signed primitive integer types.
pub enum Signed {}

/// Describes core properties shared by all primitive integer types.
pub trait Integer:
    Sized
    + Copy
    + Clone
    + PartialEq
    + Eq
    + PartialOrd
    + Ord
    + ops::Add<Output = Self>
    + ops::AddAssign
    + ops::Sub<Output = Self>
    + ops::SubAssign
    + ops::Mul<Output = Self>
    + ops::MulAssign
    + ops::Div<Output = Self>
    + ops::DivAssign
    + ops::Rem<Output = Self>
    + ops::RemAssign
    + ops::BitAnd<Output = Self>
    + ops::BitAndAssign
    + ops::BitOr<Output = Self>
    + ops::BitOrAssign
    + ops::BitXor<Output = Self>
    + ops::BitXorAssign
    + ops::Shl<u32, Output = Self>
    + ops::ShlAssign<u32>
    + ops::Shr<u32, Output = Self>
    + ops::ShrAssign<u32>
    + ops::Not
{
    /// Whether this type is [`Signed`] or [`Unsigned`].
    type Signedness;

    /// Number of bits used for the value representation.
    const BITS: u32;
}

macro_rules! impl_integer {
    ($($ty:ty: $sign:ty),* $(,)?) => {
        $(
            impl Integer for $ty {
                type Signedness = $sign;
                const BITS: u32 = <$ty>::BITS;
            }
        )*
    };
}

impl_integer!(
    u8:    Unsigned,
    u16:   Unsigned,
    u32:   Unsigned,
    u64:   Unsigned,
    u128:  Unsigned,
    usize: Unsigned,
    i8:    Signed,
    i16:   Signed,
    i32:   Signed,
    i64:   Signed,
    i128:  Signed,
    isize: Signed,
);

// ---------------------------------------------------------------------------
// Arithmetic helpers — const where possible
// ---------------------------------------------------------------------------

/// Integer ceiling division: `ceil(a / b)`.
///
/// Panics on division by zero (same as `/`).
#[inline(always)]
pub const fn div_ceil_usize(a: usize, b: usize) -> usize {
    (a + b - 1) / b
}

/// Integer floor division (identical to `/` for non-negative values, here for symmetry).
#[inline(always)]
pub const fn div_floor_usize(a: usize, b: usize) -> usize {
    a / b
}

/// Ceiling division for `u64`.
#[inline(always)]
pub const fn div_ceil_u64(a: u64, b: u64) -> u64 {
    (a + b - 1) / b
}

/// Floor division for `u64`.
#[inline(always)]
pub const fn div_floor_u64(a: u64, b: u64) -> u64 {
    a / b
}

// ---------------------------------------------------------------------------
// Power-of-two helpers
// ---------------------------------------------------------------------------

/// Returns `true` if `n` is a power of two (including 1).
#[inline(always)]
pub const fn is_power_of_two(n: usize) -> bool {
    n != 0 && (n & (n - 1)) == 0
}

/// Returns the smallest power of two ≥ `n`.  Returns 0 for `n == 0`.
#[inline(always)]
pub const fn next_power_of_two(n: usize) -> usize {
    if n == 0 {
        return 1;
    }
    if is_power_of_two(n) {
        return n;
    }
    let mut v = n - 1;
    v |= v >> 1;
    v |= v >> 2;
    v |= v >> 4;
    v |= v >> 8;
    v |= v >> 16;
    #[cfg(target_pointer_width = "64")]
    { v |= v >> 32; }
    v + 1
}

/// Returns the largest power of two ≤ `n`.  Returns 0 for `n == 0`.
#[inline(always)]
pub const fn prev_power_of_two(n: usize) -> usize {
    if n == 0 {
        return 0;
    }
    let mut v = n;
    v |= v >> 1;
    v |= v >> 2;
    v |= v >> 4;
    v |= v >> 8;
    v |= v >> 16;
    #[cfg(target_pointer_width = "64")]
    { v |= v >> 32; }
    v - (v >> 1)
}

// ---------------------------------------------------------------------------
// Alignment helpers
// ---------------------------------------------------------------------------

/// Round `val` up to the nearest multiple of `align` (must be a power of two).
#[inline(always)]
pub const fn align_up(val: usize, align: usize) -> usize {
    debug_assert!(is_power_of_two(align), "align must be a power of two");
    (val + align - 1) & !(align - 1)
}

/// Round `val` down to the nearest multiple of `align` (must be a power of two).
#[inline(always)]
pub const fn align_down(val: usize, align: usize) -> usize {
    debug_assert!(is_power_of_two(align), "align must be a power of two");
    val & !(align - 1)
}
