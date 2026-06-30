// SPDX-License-Identifier: GPL-2.0
//! Size constants — ported from Linux `include/linux/sizes.h`.

#![allow(dead_code, unused_variables, unused_imports)]

pub const SZ_1:    usize = 0x00000001;
pub const SZ_2:    usize = 0x00000002;
pub const SZ_4:    usize = 0x00000004;
pub const SZ_8:    usize = 0x00000008;
pub const SZ_16:   usize = 0x00000010;
pub const SZ_32:   usize = 0x00000020;
pub const SZ_64:   usize = 0x00000040;
pub const SZ_128:  usize = 0x00000080;
pub const SZ_256:  usize = 0x00000100;
pub const SZ_512:  usize = 0x00000200;
pub const SZ_1K:   usize = 0x00000400;
pub const SZ_2K:   usize = 0x00000800;
pub const SZ_4K:   usize = 0x00001000;
pub const SZ_8K:   usize = 0x00002000;
pub const SZ_16K:  usize = 0x00004000;
pub const SZ_32K:  usize = 0x00008000;
pub const SZ_64K:  usize = 0x00010000;
pub const SZ_128K: usize = 0x00020000;
pub const SZ_256K: usize = 0x00040000;
pub const SZ_512K: usize = 0x00080000;
pub const SZ_1M:   usize = 0x00100000;
pub const SZ_2M:   usize = 0x00200000;
pub const SZ_4M:   usize = 0x00400000;
pub const SZ_8M:   usize = 0x00800000;
pub const SZ_16M:  usize = 0x01000000;
pub const SZ_32M:  usize = 0x02000000;
pub const SZ_64M:  usize = 0x04000000;
pub const SZ_128M: usize = 0x08000000;
pub const SZ_256M: usize = 0x10000000;
pub const SZ_512M: usize = 0x20000000;
pub const SZ_1G:   usize = 0x40000000;
pub const SZ_2G:   usize = 0x80000000;
pub const SZ_4G:   u64  = 0x100000000;
