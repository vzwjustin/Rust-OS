//! Input Settings — ported from GNOME Mutter
//!
//! Global input device settings and configuration. Manages mouse, touchpad, trackball,
//! pointing stick, and keyboard settings through GSettings integration.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-input-settings.c

/// Keyboard accessibility flags (bitmask). A type alias + consts (rather than
/// an `enum`) so the values can be combined with bitwise OR, matching upstream.
pub type MetaKeyboardA11yFlags = u32;

pub const META_KEYBOARD_A11Y_SLOWKEYS_ENABLED: MetaKeyboardA11yFlags = 1 << 0;
pub const META_KEYBOARD_A11Y_DEBOUNCE_ENABLED: MetaKeyboardA11yFlags = 1 << 1;
pub const META_KEYBOARD_A11Y_TIMEOUT_ENABLED: MetaKeyboardA11yFlags = 1 << 2;
pub const META_KEYBOARD_A11Y_MOUSEKEYS_ENABLED: MetaKeyboardA11yFlags = 1 << 3;
pub const META_KEYBOARD_A11Y_TOGGLE_ENABLED: MetaKeyboardA11yFlags = 1 << 4;
pub const META_KEYBOARD_A11Y_STICKY_KEYS_ENABLED: MetaKeyboardA11yFlags = 1 << 5;

/// Keyboard accessibility settings structure.
#[derive(Debug, Clone)]
pub struct MetaKbdA11ySettings {
    pub controls: u32,
    pub slowkeys_delay: i32,
    pub debounce_delay: i32,
    pub timeout_delay: i32,
    pub mousekeys_init_delay: i32,
    pub mousekeys_max_speed: i32,
    pub mousekeys_accel_time: i32,
}

impl MetaKbdA11ySettings {
    pub fn new() -> Self {
        MetaKbdA11ySettings {
            controls: 0,
            slowkeys_delay: 0,
            debounce_delay: 0,
            timeout_delay: 0,
            mousekeys_init_delay: 0,
            mousekeys_max_speed: 0,
            mousekeys_accel_time: 0,
        }
    }
}

impl Default for MetaKbdA11ySettings {
    fn default() -> Self {
        Self::new()
    }
}

/// Main input settings manager.
/// Manages input device settings through GSettings and backend integration.
pub struct InputSettings {
    /// Backend reference (opaque MetaBackend*)
    pub backend: *mut core::ffi::c_void,
    /// Clutter seat reference (opaque ClutterSeat*)
    pub seat: *mut core::ffi::c_void,
    /// Mouse settings (opaque GSettings*)
    pub mouse_settings: *mut core::ffi::c_void,
    /// Touchpad settings (opaque GSettings*)
    pub touchpad_settings: *mut core::ffi::c_void,
    /// Trackball settings (opaque GSettings*)
    pub trackball_settings: *mut core::ffi::c_void,
    /// Pointing stick settings (opaque GSettings*)
    pub pointing_stick_settings: *mut core::ffi::c_void,
    /// Keyboard settings (opaque GSettings*)
    pub keyboard_settings: *mut core::ffi::c_void,
    /// Keyboard accessibility settings (opaque GSettings*)
    pub keyboard_a11y_settings: *mut core::ffi::c_void,
    /// Current keyboard a11y configuration
    pub kbd_a11y_settings: MetaKbdA11ySettings,
}

impl InputSettings {
    pub fn new() -> Self {
        InputSettings {
            backend: core::ptr::null_mut(),
            seat: core::ptr::null_mut(),
            mouse_settings: core::ptr::null_mut(),
            touchpad_settings: core::ptr::null_mut(),
            trackball_settings: core::ptr::null_mut(),
            pointing_stick_settings: core::ptr::null_mut(),
            keyboard_settings: core::ptr::null_mut(),
            keyboard_a11y_settings: core::ptr::null_mut(),
            kbd_a11y_settings: MetaKbdA11ySettings::new(),
        }
    }
}

impl Default for InputSettings {
    fn default() -> Self {
        Self::new()
    }
}
