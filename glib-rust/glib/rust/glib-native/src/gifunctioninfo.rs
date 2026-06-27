//! `gifunctioninfo` matching `girepository/gifunctioninfo.h`.
//!
//! Function info: describes a function, method, or constructor.
//! Extends `CallableInfo`.
//!
//! Fully `no_std` compatible using `core` and `alloc`.

use crate::gicallableinfo::CallableInfo;
use crate::gitypes::{GIArgument, GIFunctionInfoFlags};
use crate::prelude::*;
use alloc::string::String;

/// Invoke error codes (mirrors `GIInvokeError`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[repr(u32)]
pub enum InvokeError {
    #[default]
    Failed = 0,
    SymbolNotFound = 1,
    ArgumentMismatch = 2,
    NotSupported = 3,
}

/// Function info (mirrors `GIFunctionInfo`).
#[derive(Debug, Clone, Default)]
pub struct FunctionInfo {
    pub callable: CallableInfo,
    pub symbol: String,
    pub flags: GIFunctionInfoFlags,
}

impl FunctionInfo {
    /// Creates a new function info.
    pub fn new() -> Self {
        Self {
            callable: CallableInfo::new(),
            symbol: String::new(),
            flags: GIFunctionInfoFlags::NONE,
        }
    }

    /// Returns the symbol (mirrors `gi_function_info_get_symbol`).
    pub fn symbol(&self) -> &str {
        &self.symbol
    }

    /// Returns the flags (mirrors `gi_function_info_get_flags`).
    pub fn flags(&self) -> GIFunctionInfoFlags {
        self.flags
    }

    /// Invokes the function (mirrors `gi_function_info_invoke`).
    /// No-op in our no_std port.
    pub fn invoke(
        &self,
        _in_args: &[GIArgument],
        _out_args: &mut [GIArgument],
        _return_value: &mut GIArgument,
    ) -> Result<(), InvokeError> {
        Err(InvokeError::SymbolNotFound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let fi = FunctionInfo::new();
        assert_eq!(fi.symbol(), "");
        assert_eq!(fi.flags(), GIFunctionInfoFlags::NONE);
    }

    #[test]
    fn test_custom() {
        let mut fi = FunctionInfo::new();
        fi.symbol = "g_object_new".into();
        fi.flags = GIFunctionInfoFlags::IS_CONSTRUCTOR;
        assert_eq!(fi.symbol(), "g_object_new");
        assert_eq!(fi.flags(), GIFunctionInfoFlags::IS_CONSTRUCTOR);
    }

    #[test]
    fn test_invoke_fails() {
        let fi = FunctionInfo::new();
        let mut ret = GIArgument::default();
        assert_eq!(
            fi.invoke(&[], &mut [], &mut ret),
            Err(InvokeError::SymbolNotFound)
        );
    }
}
