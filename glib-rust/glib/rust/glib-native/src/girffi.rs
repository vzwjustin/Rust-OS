//! `girffi` matching `girepository/girffi.h`.
//!
//! FFI integration for GObject introspection.
//! Stubbed in no_std since libffi is not available.
//!
//! Fully `no_std` compatible using `core` and `alloc`.

use crate::gitypes::GIArgument;
use crate::prelude::*;
use alloc::string::String;

/// FFI closure callback type (mirrors `GIFFIClosureCallback`).
pub type FfiClosureCallback =
    extern "C" fn(cif: *mut u8, ret: *mut u8, args: *mut *mut u8, user_data: *mut u8);

/// Function invoker (mirrors `GIFunctionInvoker`).
#[derive(Debug, Default)]
pub struct FunctionInvoker {
    pub native_address: *mut u8,
}

/// FFI return value (mirrors `GIFFIReturnValue`).
pub type FfiReturnValue = GIArgument;

/// Gets the FFI type for a type tag (mirrors `gi_type_tag_get_ffi_type`).
/// Returns a placeholder since libffi is not available.
pub fn type_tag_get_ffi_type(_type_tag: u32, _is_pointer: bool) -> *mut u8 {
    core::ptr::null_mut()
}

/// Gets the FFI type for a type info (mirrors `gi_type_info_get_ffi_type`).
pub fn type_info_get_ffi_type() -> *mut u8 {
    core::ptr::null_mut()
}

/// Prepares a function invoker (mirrors `gi_function_info_prep_invoker`).
/// No-op in our no_std port.
pub fn function_info_prep_invoker(
    _info: &str,
    _invoker: &mut FunctionInvoker,
) -> Result<(), String> {
    Err("FFI not supported in no_std".into())
}

/// Clears a function invoker (mirrors `gi_function_invoker_clear`).
pub fn function_invoker_clear(invoker: &mut FunctionInvoker) {
    invoker.native_address = core::ptr::null_mut();
}

/// Creates a closure (mirrors `gi_callable_info_create_closure`).
/// No-op in our no_std port.
pub fn callable_info_create_closure(
    _callable_info: &str,
    _callback: FfiClosureCallback,
    _user_data: *mut u8,
) -> *mut u8 {
    core::ptr::null_mut()
}

/// Destroys a closure (mirrors `gi_callable_info_destroy_closure`).
pub fn callable_info_destroy_closure(_closure: *mut u8) {}

/// Gets the native address of a closure
/// (mirrors `gi_callable_info_get_closure_native_address`).
pub fn callable_info_get_closure_native_address(_closure: *mut u8) -> *mut u8 {
    core::ptr::null_mut()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_tag_get_ffi_type() {
        assert!(type_tag_get_ffi_type(0, false).is_null());
    }

    #[test]
    fn test_function_invoker_clear() {
        let mut invoker = FunctionInvoker::default();
        function_invoker_clear(&mut invoker);
        assert!(invoker.native_address.is_null());
    }

    #[test]
    fn test_prep_invoker_fails() {
        let mut invoker = FunctionInvoker::default();
        assert!(function_info_prep_invoker("test", &mut invoker).is_err());
    }
}
