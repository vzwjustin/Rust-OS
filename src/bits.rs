// SPDX-License-Identifier: GPL-2.0
//! Bit manipulation primitives — pure Rust port of Linux kernel bits.h
#![allow(dead_code, unused_macros)]

/// Returns a bitmask with bit `n` set (u64).
#[inline(always)]
pub const fn bit(n: u32) -> u64 {
    1u64 << n
}

/// Returns a bitmask with bit `n` set (u32).
#[inline(always)]
pub const fn bit_u32(n: u32) -> u32 {
    1u32 << n
}

/// Returns a bitmask with bit `n` set (u16).
#[inline(always)]
pub const fn bit_u16(n: u32) -> u16 {
    1u16 << n
}

/// Returns a bitmask with bit `n` set (u8).
#[inline(always)]
pub const fn bit_u8(n: u32) -> u8 {
    1u8 << n
}

/// Generates a contiguous bitmask spanning bits `low` through `high` (inclusive) in a u64.
///
/// # Panics (debug)
/// Panics if `high < low` or `high >= 64`.
#[inline(always)]
pub const fn genmask(high: u32, low: u32) -> u64 {
    debug_assert!(high >= low, "genmask: high must be >= low");
    debug_assert!(high < 64, "genmask: high must be < 64");
    let width = high - low + 1;
    let mask = if width == 64 { u64::MAX } else { (1u64 << width) - 1 };
    mask << low
}

/// Generates a contiguous bitmask spanning bits `low` through `high` (inclusive) in a u32.
#[inline(always)]
pub const fn genmask_u32(high: u32, low: u32) -> u32 {
    debug_assert!(high >= low, "genmask_u32: high must be >= low");
    debug_assert!(high < 32, "genmask_u32: high must be < 32");
    let width = high - low + 1;
    let mask = if width == 32 { u32::MAX } else { (1u32 << width) - 1 };
    mask << low
}

/// Generates a contiguous bitmask spanning bits `low` through `high` (inclusive) in a u16.
#[inline(always)]
pub const fn genmask_u16(high: u32, low: u32) -> u16 {
    debug_assert!(high >= low);
    debug_assert!(high < 16);
    let width = high - low + 1;
    let mask = if width == 16 { u16::MAX } else { (1u16 << width) - 1 };
    mask << low
}

/// Extracts a field value defined by `mask` from `val`.
/// Equivalent to Linux's `FIELD_GET(mask, val)`.
#[inline(always)]
pub const fn field_get(mask: u64, val: u64) -> u64 {
    (val & mask) >> mask.trailing_zeros()
}

/// Prepares a field value: shifts `val` into position defined by `mask`.
/// Equivalent to Linux's `FIELD_PREP(mask, val)`.
#[inline(always)]
pub const fn field_prep(mask: u64, val: u64) -> u64 {
    (val << mask.trailing_zeros()) & mask
}

/// Extracts a u32 field value defined by `mask` from `val`.
#[inline(always)]
pub const fn field_get_u32(mask: u32, val: u32) -> u32 {
    (val & mask) >> mask.trailing_zeros()
}

/// Prepares a u32 field value into position defined by `mask`.
#[inline(always)]
pub const fn field_prep_u32(mask: u32, val: u32) -> u32 {
    (val << mask.trailing_zeros()) & mask
}

/// Returns true if `val` has bit `n` set.
#[inline(always)]
pub const fn test_bit(val: u64, n: u32) -> bool {
    (val >> n) & 1 == 1
}

/// Returns true if `val` (u32) has bit `n` set.
#[inline(always)]
pub const fn test_bit_u32(val: u32, n: u32) -> bool {
    (val >> n) & 1 == 1
}

/// Rotate `val` left by `n` bits (u64).
#[inline(always)]
pub const fn rol64(val: u64, n: u32) -> u64 {
    val.rotate_left(n)
}

/// Rotate `val` right by `n` bits (u64).
#[inline(always)]
pub const fn ror64(val: u64, n: u32) -> u64 {
    val.rotate_right(n)
}

/// Rotate `val` left by `n` bits (u32).
#[inline(always)]
pub const fn rol32(val: u32, n: u32) -> u32 {
    val.rotate_left(n)
}

/// Rotate `val` right by `n` bits (u32).
#[inline(always)]
pub const fn ror32(val: u32, n: u32) -> u32 {
    val.rotate_right(n)
}

/// Returns the number of set bits (population count) in `val`.
#[inline(always)]
pub const fn hweight64(val: u64) -> u32 {
    val.count_ones()
}

/// Returns the number of set bits (population count) in `val` (u32).
#[inline(always)]
pub const fn hweight32(val: u32) -> u32 {
    val.count_ones()
}

/// Returns the position of the lowest set bit, or `None` if `val == 0`.
#[inline(always)]
pub const fn ffs(val: u64) -> Option<u32> {
    if val == 0 { None } else { Some(val.trailing_zeros()) }
}

/// Returns the position of the highest set bit, or `None` if `val == 0`.
#[inline(always)]
pub const fn fls(val: u64) -> Option<u32> {
    if val == 0 { None } else { Some(63 - val.leading_zeros()) }
}

/// Aligns `val` up to the next multiple of `align` (must be a power of two).
#[inline(always)]
pub const fn align_up(val: usize, align: usize) -> usize {
    debug_assert!(align.is_power_of_two());
    (val + align - 1) & !(align - 1)
}

/// Aligns `val` down to `align` (must be a power of two).
#[inline(always)]
pub const fn align_down(val: usize, align: usize) -> usize {
    debug_assert!(align.is_power_of_two());
    val & !(align - 1)
}

/// Returns `true` if `val` is aligned to `align` (power of two).
#[inline(always)]
pub const fn is_aligned(val: usize, align: usize) -> bool {
    val & (align - 1) == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_genmask() {
        assert_eq!(genmask(3, 0), 0xf);
        assert_eq!(genmask(7, 4), 0xf0);
    }
    #[test]
    fn test_field_get_prep() {
        let mask = genmask(7, 4);
        let val = field_prep(mask, 0xb);
        assert_eq!(field_get(mask, val), 0xb);
    }
}
