//! Backlight Private ported from GNOME Mutter's src/backends/
//!
//! Private GObject class structure for MetaBacklight. Defines vfunc hooks
//! for async brightness setting via hardware-specific backends.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-backlight-private.h

use core::ffi::c_void;

/// Opaque cancellable type for async operations.
pub struct GCancellable;

/// Opaque async result type.
pub struct GAsyncResult;

/// Opaque error type (GError*).
pub type GError = core::ffi::c_void;

/// Async callback function pointer type.
pub type GAsyncReadyCallback =
    Option<unsafe extern "C" fn(*mut c_void, *mut GAsyncResult, *mut c_void)>;

/// Virtual method function pointers for MetaBacklight.
///
/// Provides async hooks for brightness adjustment:
/// - `set_brightness`: Initiate async brightness change
/// - `set_brightness_finish`: Complete async operation and return result
#[repr(C)]
pub struct MetaBacklightClass {
    /// VTable hook: start async brightness setting.
    /// Signature: set_brightness(backlight, brightness_target, cancellable, callback, user_data)
    pub set_brightness: Option<
        unsafe extern "C" fn(
            *mut c_void,         // MetaBacklight *backlight
            i32,                 // brightness_target
            *mut GCancellable,   // cancellable
            GAsyncReadyCallback, // callback
            *mut c_void,         // user_data
        ),
    >,
    /// VTable hook: finish async brightness operation, return int result.
    /// Signature: set_brightness_finish(backlight, result, error) -> int
    pub set_brightness_finish: Option<
        unsafe extern "C" fn(
            *mut c_void,       // MetaBacklight *backlight
            *mut GAsyncResult, // result
            *mut *mut GError,  // error (out param)
        ) -> i32,
    >,
}
