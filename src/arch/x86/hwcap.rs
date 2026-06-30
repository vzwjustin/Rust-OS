//! x86 HWCAP2 constants.
//!
//! Ported from Linux `arch/x86/include/uapi/asm/hwcap2.h`.

/// MONITOR/MWAIT enabled in Ring 3.
pub const HWCAP2_RING3MWAIT: u64 = 1 << 0;

/// Kernel allows FSGSBASE instructions available in Ring 3.
pub const HWCAP2_FSGSBASE: u64 = 1 << 1;
