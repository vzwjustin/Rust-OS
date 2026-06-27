//! `givfuncinfo` matching `girepository/givfuncinfo.h`.
//!
//! VFunc info: describes a virtual function slot.
//!
//! Fully `no_std` compatible using `core` and `alloc`.

use crate::gicallableinfo::CallableInfo;
use crate::gifunctioninfo::FunctionInfo;
use crate::gisignalinfo::SignalInfo;
use crate::gitypes::{GIArgument, GIVFuncInfoFlags};
use crate::prelude::*;
use alloc::boxed::Box;

/// VFunc info (mirrors `GIVFuncInfo`).
#[derive(Debug, Clone, Default)]
pub struct VFuncInfo {
    pub callable: CallableInfo,
    pub flags: GIVFuncInfoFlags,
    pub offset: usize,
    pub signal: Option<Box<SignalInfo>>,
    pub invoker: Option<FunctionInfo>,
}

impl VFuncInfo {
    /// Creates a new vfunc info.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the flags (mirrors `gi_vfunc_info_get_flags`).
    pub fn flags(&self) -> GIVFuncInfoFlags {
        self.flags
    }

    /// Returns the offset (mirrors `gi_vfunc_info_get_offset`).
    pub fn offset(&self) -> usize {
        self.offset
    }

    /// Returns the signal (mirrors `gi_vfunc_info_get_signal`).
    pub fn signal(&self) -> Option<&SignalInfo> {
        self.signal.as_deref()
    }

    /// Returns the invoker (mirrors `gi_vfunc_info_get_invoker`).
    pub fn invoker(&self) -> Option<&FunctionInfo> {
        self.invoker.as_ref()
    }

    /// Gets the address (mirrors `gi_vfunc_info_get_address`).
    /// No-op in our no_std port.
    pub fn address(&self, _implementor_gtype: u64) -> Option<*mut u8> {
        None
    }

    /// Invokes the vfunc (mirrors `gi_vfunc_info_invoke`).
    /// No-op in our no_std port.
    pub fn invoke(
        &self,
        _implementor: u64,
        _in_args: &[GIArgument],
        _out_args: &mut [GIArgument],
        _return_value: &mut GIArgument,
    ) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let vi = VFuncInfo::new();
        assert_eq!(vi.flags(), GIVFuncInfoFlags::NONE);
        assert_eq!(vi.offset(), 0);
        assert!(vi.signal().is_none());
    }
}
