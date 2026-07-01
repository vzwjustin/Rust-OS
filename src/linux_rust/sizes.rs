//! Commonly used size constants.
//!
//! Ported from Linux `rust/kernel/sizes.rs`.
//! C header: `include/linux/sizes.h`

/// Size constants trait for device address spaces.
///
/// Implemented for `u32`, `u64`, and `usize` so drivers can choose
/// the width that matches their hardware.
pub trait SizeConstants {
    const SZ_1K: Self;
    const SZ_2K: Self;
    const SZ_4K: Self;
    const SZ_8K: Self;
    const SZ_16K: Self;
    const SZ_32K: Self;
    const SZ_64K: Self;
    const SZ_128K: Self;
    const SZ_256K: Self;
    const SZ_512K: Self;
    const SZ_1M: Self;
    const SZ_2M: Self;
    const SZ_4M: Self;
    const SZ_8M: Self;
    const SZ_16M: Self;
    const SZ_32M: Self;
    const SZ_64M: Self;
    const SZ_128M: Self;
    const SZ_256M: Self;
    const SZ_512M: Self;
    const SZ_1G: Self;
    const SZ_2G: Self;
}

macro_rules! impl_size_constants {
    ($($type:ty),* $(,)?) => {
        $(
        impl SizeConstants for $type {
            const SZ_1K: Self = 0x0000_0400;
            const SZ_2K: Self = 0x0000_0800;
            const SZ_4K: Self = 0x0000_1000;
            const SZ_8K: Self = 0x0000_2000;
            const SZ_16K: Self = 0x0000_4000;
            const SZ_32K: Self = 0x0000_8000;
            const SZ_64K: Self = 0x0001_0000;
            const SZ_128K: Self = 0x0002_0000;
            const SZ_256K: Self = 0x0004_0000;
            const SZ_512K: Self = 0x0008_0000;
            const SZ_1M: Self = 0x0010_0000;
            const SZ_2M: Self = 0x0020_0000;
            const SZ_4M: Self = 0x0040_0000;
            const SZ_8M: Self = 0x0080_0000;
            const SZ_16M: Self = 0x0100_0000;
            const SZ_32M: Self = 0x0200_0000;
            const SZ_64M: Self = 0x0400_0000;
            const SZ_128M: Self = 0x0800_0000;
            const SZ_256M: Self = 0x1000_0000;
            const SZ_512M: Self = 0x2000_0000;
            const SZ_1G: Self = 0x4000_0000;
            const SZ_2G: Self = 0x8000_0000;
        }
        )*
    };
}

impl_size_constants!(u32, u64, usize);

// Top-level `usize`-typed constants for convenience in kernel page arithmetic.
pub const SZ_1K: usize = 0x0000_0400;
pub const SZ_2K: usize = 0x0000_0800;
pub const SZ_4K: usize = 0x0000_1000;
pub const SZ_8K: usize = 0x0000_2000;
pub const SZ_16K: usize = 0x0000_4000;
pub const SZ_32K: usize = 0x0000_8000;
pub const SZ_64K: usize = 0x0001_0000;
pub const SZ_128K: usize = 0x0002_0000;
pub const SZ_256K: usize = 0x0004_0000;
pub const SZ_512K: usize = 0x0008_0000;
pub const SZ_1M: usize = 0x0010_0000;
pub const SZ_2M: usize = 0x0020_0000;
pub const SZ_4M: usize = 0x0040_0000;
pub const SZ_8M: usize = 0x0080_0000;
pub const SZ_16M: usize = 0x0100_0000;
pub const SZ_32M: usize = 0x0200_0000;
pub const SZ_64M: usize = 0x0400_0000;
pub const SZ_128M: usize = 0x0800_0000;
pub const SZ_256M: usize = 0x1000_0000;
pub const SZ_512M: usize = 0x2000_0000;
pub const SZ_1G: usize = 0x4000_0000;
pub const SZ_2G: usize = 0x8000_0000;
