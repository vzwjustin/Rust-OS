//! x86-specific mmap flags.
//!
//! Ported from Linux `arch/x86/include/uapi/asm/mman.h`.

/// Only give out 32-bit addresses (x86_64 specific).
pub const MAP_32BIT: u32 = 0x40;

/// Only map above 4GB (x86_64 specific).
pub const MAP_ABOVE4G: u32 = 0x80;
