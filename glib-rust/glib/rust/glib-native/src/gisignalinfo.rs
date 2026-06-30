//! `gisignalinfo` matching `girepository/gisignalinfo.h`.
//!
//! Signal info: describes a GObject signal.
//!
//! Fully `no_std` compatible using `core` and `alloc`.

use crate::givfuncinfo::VFuncInfo;
use alloc::boxed::Box;

/// Signal flags (mirrors `GSignalFlags`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct SignalFlags(pub u32);

impl SignalFlags {
    pub const NONE: Self = Self(0);
    pub const RUN_FIRST: Self = Self(1 << 0);
    pub const RUN_LAST: Self = Self(1 << 1);
    pub const RUN_CLEANUP: Self = Self(1 << 2);
    pub const NO_RECURSE: Self = Self(1 << 3);
    pub const DETAILED: Self = Self(1 << 4);
    pub const ACTION: Self = Self(1 << 5);
    pub const NO_HOOKS: Self = Self(1 << 6);
    pub const MUST_COLLECT: Self = Self(1 << 7);
}

/// Signal info (mirrors `GISignalInfo`).
#[derive(Debug, Clone, Default)]
pub struct SignalInfo {
    pub flags: SignalFlags,
    pub class_closure: Option<Box<VFuncInfo>>,
    pub true_stops_emit: bool,
}

impl SignalInfo {
    /// Creates a new signal info.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the flags (mirrors `gi_signal_info_get_flags`).
    pub fn flags(&self) -> SignalFlags {
        self.flags
    }

    /// Returns the class closure (mirrors `gi_signal_info_get_class_closure`).
    pub fn class_closure(&self) -> Option<&VFuncInfo> {
        self.class_closure.as_deref()
    }

    /// Returns whether true stops emit (mirrors `gi_signal_info_true_stops_emit`).
    pub fn true_stops_emit(&self) -> bool {
        self.true_stops_emit
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let si = SignalInfo::new();
        assert_eq!(si.flags(), SignalFlags::NONE);
        assert!(!si.true_stops_emit());
    }
}
