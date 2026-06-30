//! Bit manipulation macros and functions.
//!
//! Ported from Linux `rust/kernel/bits.rs`.
//! C header: `include/linux/bits.h`

use core::ops::RangeInclusive;

macro_rules! impl_bit_fn {
    ($ty:ty, $checked_name:ident, $name:ident) => {
        /// Computes `1 << n` if `n` is in bounds, i.e.: if `n` is smaller than
        /// the maximum number of bits supported by the type.
        ///
        /// Returns `None` otherwise.
        #[inline]
        pub fn $checked_name(n: u32) -> Option<$ty> {
            (1 as $ty).checked_shl(n)
        }

        /// Computes `1 << n` by performing a compile-time assertion that `n` is
        /// in bounds.
        #[inline(always)]
        pub const fn $name(n: u32) -> $ty {
            assert!(n < <$ty>::BITS);
            (1 as $ty) << n
        }
    };
}

impl_bit_fn!(u64, checked_bit_u64, bit_u64);
impl_bit_fn!(u32, checked_bit_u32, bit_u32);
impl_bit_fn!(u16, checked_bit_u16, bit_u16);
impl_bit_fn!(u8, checked_bit_u8, bit_u8);

macro_rules! impl_genmask_fn {
    ($ty:ty, $checked_name:ident, $name:ident, $bit_fn:ident, $checked_bit_fn:ident) => {
        /// Creates a contiguous bitmask for the given range by validating
        /// the range at runtime.
        ///
        /// Returns `None` if the range is invalid.
        #[inline]
        pub fn $checked_name(range: RangeInclusive<u32>) -> Option<$ty> {
            let start = *range.start();
            let end = *range.end();

            if start > end {
                return None;
            }

            let high = $checked_bit_fn(end)?;
            let low = $checked_bit_fn(start)?;
            Some((high | (high - 1)) & !(low - 1))
        }

        /// Creates a compile-time contiguous bitmask for the given range.
        #[inline(always)]
        pub const fn $name(range: RangeInclusive<u32>) -> $ty {
            let start = *range.start();
            let end = *range.end();

            assert!(start <= end);

            let high = $bit_fn(end);
            let low = $bit_fn(start);
            (high | (high - 1)) & !(low - 1)
        }
    };
}

impl_genmask_fn!(u64, genmask_checked_u64, genmask_u64, bit_u64, checked_bit_u64);
impl_genmask_fn!(u32, genmask_checked_u32, genmask_u32, bit_u32, checked_bit_u32);
impl_genmask_fn!(u16, genmask_checked_u16, genmask_u16, bit_u16, checked_bit_u16);
impl_genmask_fn!(u8, genmask_checked_u8, genmask_u8, bit_u8, checked_bit_u8);

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn test_bit_u64() {
        assert_eq!(bit_u64(0), 1);
        assert_eq!(bit_u64(63), 1u64 << 63);
        assert_eq!(checked_bit_u64(64), None);
    }

    #[test_case]
    fn test_genmask_u64() {
        assert_eq!(genmask_u64(0..=0), 0b1);
        assert_eq!(genmask_u64(0..=63), u64::MAX);
        assert_eq!(genmask_u64(21..=39), 0x0000_00ff_ffe0_0000);
        assert_eq!(genmask_checked_u64(21..=80), None);
        assert_eq!(genmask_checked_u64(15..=8), None);
    }

    #[test_case]
    fn test_bit_u32() {
        assert_eq!(bit_u32(0), 1);
        assert_eq!(bit_u32(31), 1u32 << 31);
        assert_eq!(checked_bit_u32(32), None);
    }

    #[test_case]
    fn test_genmask_u32() {
        assert_eq!(genmask_u32(21..=31), 0xffe0_0000);
        assert_eq!(genmask_u32(0..=0), 0b1);
        assert_eq!(genmask_u32(0..=31), u32::MAX);
    }
}
