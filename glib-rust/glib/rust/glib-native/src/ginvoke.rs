//! `ginvoke` matching `girepository/ginvoke.c`.
//!
//! Function invocation via FFI.
//! Stubbed in no_std since libffi is not available.
//!
//! Fully `no_std` compatible using `core` and `alloc`.

use crate::gifunctioninfo::InvokeError;
use crate::gitypes::GIArgument;

/// Invokes a function by address (mirrors `gi_function_info_invoke` / libffi path).
///
/// Returns `InvokeError::NotSupported` on bare-metal / no_std builds.
pub fn gi_invoke(
    _function: *mut u8,
    _in_args: &[GIArgument],
    _out_args: &mut [GIArgument],
    _return_value: &mut GIArgument,
) -> Result<(), InvokeError> {
    Err(InvokeError::NotSupported)
}

/// Alias for [`gi_invoke`].
pub fn invoke(
    function: *mut u8,
    in_args: &[GIArgument],
    out_args: &mut [GIArgument],
    return_value: &mut GIArgument,
) -> Result<(), InvokeError> {
    gi_invoke(function, in_args, out_args, return_value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gi_invoke_not_supported() {
        let mut ret = GIArgument::default();
        assert_eq!(
            gi_invoke(core::ptr::null_mut(), &[], &mut [], &mut ret),
            Err(InvokeError::NotSupported)
        );
    }

    #[test]
    fn test_invoke_alias() {
        let mut ret = GIArgument::default();
        assert_eq!(
            invoke(core::ptr::null_mut(), &[], &mut [], &mut ret),
            Err(InvokeError::NotSupported)
        );
    }
}
