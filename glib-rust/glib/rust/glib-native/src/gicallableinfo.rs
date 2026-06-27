//! `gicallableinfo` matching `girepository/gicallableinfo.h`.
//!
//! Callable info: base type for functions, callbacks, signals, vfuncs.
//!
//! Fully `no_std` compatible using `core` and `alloc`.

use crate::giarginfo::ArgInfo;
use crate::gitypes::{GIArgument, GITransfer};
use crate::prelude::*;
use alloc::vec::Vec;

/// Callable info (mirrors `GICallableInfo`).
#[derive(Debug, Clone)]
pub struct CallableInfo {
    pub is_method: bool,
    pub can_throw_gerror: bool,
    pub caller_owns: GITransfer,
    pub may_return_null: bool,
    pub skip_return: bool,
    pub instance_ownership_transfer: GITransfer,
    pub args: Vec<ArgInfo>,
    pub return_attributes: Vec<(String, String)>,
    pub is_async: bool,
}

impl CallableInfo {
    /// Creates a new callable info with defaults.
    pub fn new() -> Self {
        Self {
            is_method: false,
            can_throw_gerror: false,
            caller_owns: GITransfer::Nothing,
            may_return_null: false,
            skip_return: false,
            instance_ownership_transfer: GITransfer::Nothing,
            args: Vec::new(),
            return_attributes: Vec::new(),
            is_async: false,
        }
    }

    /// Returns whether it's a method (mirrors `gi_callable_info_is_method`).
    pub fn is_method(&self) -> bool {
        self.is_method
    }

    /// Returns whether it can throw GError (mirrors `gi_callable_info_can_throw_gerror`).
    pub fn can_throw_gerror(&self) -> bool {
        self.can_throw_gerror
    }

    /// Returns caller ownership (mirrors `gi_callable_info_get_caller_owns`).
    pub fn caller_owns(&self) -> GITransfer {
        self.caller_owns
    }

    /// Returns whether may return null (mirrors `gi_callable_info_may_return_null`).
    pub fn may_return_null(&self) -> bool {
        self.may_return_null
    }

    /// Returns whether to skip return (mirrors `gi_callable_info_skip_return`).
    pub fn skip_return(&self) -> bool {
        self.skip_return
    }

    /// Returns the number of args (mirrors `gi_callable_info_get_n_args`).
    pub fn n_args(&self) -> u32 {
        self.args.len() as u32
    }

    /// Gets an arg by index (mirrors `gi_callable_info_get_arg`).
    pub fn get_arg(&self, n: u32) -> Option<&ArgInfo> {
        self.args.get(n as usize)
    }

    /// Returns instance ownership transfer
    /// (mirrors `gi_callable_info_get_instance_ownership_transfer`).
    pub fn instance_ownership_transfer(&self) -> GITransfer {
        self.instance_ownership_transfer
    }

    /// Returns whether async (mirrors `gi_callable_info_is_async`).
    pub fn is_async(&self) -> bool {
        self.is_async
    }

    /// Invokes the callable (mirrors `gi_callable_info_invoke`).
    /// No-op in our no_std port.
    pub fn invoke(
        &self,
        _function: *mut u8,
        _in_args: &[GIArgument],
        _out_args: &mut [GIArgument],
        _return_value: &mut GIArgument,
    ) -> Result<(), String> {
        Err("invoke not supported in no_std".into())
    }
}

impl Default for CallableInfo {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_defaults() {
        let ci = CallableInfo::new();
        assert!(!ci.is_method());
        assert!(!ci.can_throw_gerror());
        assert_eq!(ci.caller_owns(), GITransfer::Nothing);
        assert_eq!(ci.n_args(), 0);
    }

    #[test]
    fn test_with_args() {
        let mut ci = CallableInfo::new();
        ci.is_method = true;
        ci.args.push(ArgInfo::new());
        ci.args.push(ArgInfo::new());
        assert_eq!(ci.n_args(), 2);
        assert!(ci.get_arg(0).is_some());
        assert!(ci.get_arg(5).is_none());
    }

    #[test]
    fn test_invoke_fails() {
        let ci = CallableInfo::new();
        let mut ret = GIArgument::default();
        assert!(ci
            .invoke(core::ptr::null_mut(), &[], &mut [], &mut ret)
            .is_err());
    }
}
