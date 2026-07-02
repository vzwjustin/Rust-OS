//! Input Settings Private — ported from GNOME Mutter
//!
//! Private virtual methods and internal types for input device settings.
//! Defines the backend-specific configuration methods for different device types.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-input-settings-private.h

use core::ffi::c_void;

/// Keyboard accessibility settings structure.
#[derive(Debug, Clone, Copy)]
pub struct MetaKbdA11ySettings {
    /// Keyboard a11y control flags bitmask.
    pub controls: u32,
    /// Slow keys activation delay in milliseconds.
    pub slowkeys_delay: i32,
    /// Debounce delay for key repeats in milliseconds.
    pub debounce_delay: i32,
    /// Timeout before sticky keys release in milliseconds.
    pub timeout_delay: i32,
    /// Initial delay before mouse keys start moving in milliseconds.
    pub mousekeys_init_delay: i32,
    /// Maximum mouse keys movement speed in pixels/sec.
    pub mousekeys_max_speed: i32,
    /// Acceleration time for mouse keys in milliseconds.
    pub mousekeys_accel_time: i32,
}

/// Virtual method table for input settings backends.
///
/// GObject class structure with virtual methods for backend-specific
/// input device configuration (e.g., libinput properties, keyboard repeat).
pub struct InputSettingsClass {
    /// Virtual method: set device send-events mode.
    pub set_send_events: Option<unsafe extern "C" fn(*mut c_void, *mut c_void, u32) -> ()>,
    /// Virtual method: set device transformation matrix.
    pub set_matrix: Option<unsafe extern "C" fn(*mut c_void, *mut c_void, *const f32) -> ()>,
    /// Virtual method: set device pointer speed.
    pub set_speed: Option<unsafe extern "C" fn(*mut c_void, *mut c_void, f64) -> ()>,
    /// Virtual method: set left-handed mode for device.
    pub set_left_handed: Option<unsafe extern "C" fn(*mut c_void, *mut c_void, bool) -> ()>,
    /// Virtual method: enable/disable tap-to-click on touchpad.
    pub set_tap_enabled: Option<unsafe extern "C" fn(*mut c_void, *mut c_void, bool) -> ()>,
    /// Virtual method: set keyboard repeat rate.
    pub set_keyboard_repeat: Option<unsafe extern "C" fn(*mut c_void, bool, u32, u32) -> ()>,
}
