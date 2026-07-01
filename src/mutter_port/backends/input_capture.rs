//! Input Capture ported from GNOME Mutter's src/backends/
//!
//! Top-level input capture service managing multiple capture sessions and capability negotiation.
//! Provides D-Bus interface for applications to capture keyboard, pointer, and touch input
//! with per-capability enable/disable callbacks.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-input-capture.c

use core::cell::Cell;

/// D-Bus Session Manager skeleton base type (opaque, hardware/D-Bus I/O bound).
pub struct DbusSessionManager;

/// Input capture capability flags (keyboard, pointer, touch).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MetaInputCaptureCapabilities {
    /// No capabilities.
    META_INPUT_CAPTURE_CAPABILITY_NONE = 1 << 0,
    /// Keyboard input capture.
    META_INPUT_CAPTURE_CAPABILITY_KEYBOARD = 1 << 1,
    /// Pointer (mouse) input capture.
    META_INPUT_CAPTURE_CAPABILITY_POINTER = 1 << 2,
    /// Touch input capture.
    META_INPUT_CAPTURE_CAPABILITY_TOUCH = 1 << 3,
}

impl MetaInputCaptureCapabilities {
    /// Check if a specific capability is set.
    pub fn contains(&self, other: MetaInputCaptureCapabilities) -> bool {
        (*self as u32 & other as u32) != 0
    }

    /// Union of multiple capabilities.
    pub fn union(&self, other: MetaInputCaptureCapabilities) -> MetaInputCaptureCapabilities {
        // SAFETY: All representable u32 bit patterns are valid enum variants
        unsafe {
            core::mem::transmute::<u32, MetaInputCaptureCapabilities>(
                *self as u32 | other as u32,
            )
        }
    }
}

/// Callback invoked when input capture is enabled for a session (D-Bus/hardware bound).
///
/// The callback receives:
/// - The MetaInputCapture instance
/// - User-provided context data (opaque `*mut void` in C)
pub type InputCaptureEnable = fn(*mut MetaInputCapture, *mut core::ffi::c_void);

/// Callback invoked when input capture is disabled for a session (D-Bus/hardware bound).
///
/// The callback receives:
/// - The MetaInputCapture instance
/// - User-provided context data (opaque `*mut void` in C)
pub type InputCaptureDisable = fn(*mut MetaInputCapture, *mut core::ffi::c_void);

/// Input capture service managing active capture sessions.
///
/// Coordinates capability negotiation and routes events to active sessions.
/// Maintains callbacks for enable/disable transitions and manages session lifecycle.
pub struct MetaInputCapture {
    /// D-Bus skeleton (opaque).
    pub dbus: DbusSessionManager,
    /// Enable callback (invoked when session is activated).
    pub enable_callback: Cell<Option<InputCaptureEnable>>,
    /// Disable callback (invoked when session is deactivated).
    pub disable_callback: Cell<Option<InputCaptureDisable>>,
    /// User-provided context for callbacks.
    pub user_data: Cell<*mut core::ffi::c_void>,
}

impl MetaInputCapture {
    /// Create a new input capture service.
    pub fn new() -> Self {
        MetaInputCapture {
            dbus: DbusSessionManager,
            enable_callback: Cell::new(None),
            disable_callback: Cell::new(None),
            user_data: Cell::new(core::ptr::null_mut()),
        }
    }

    /// Set the enable/disable event router callbacks (D-Bus/hardware bound).
    pub fn set_event_router(
        &self,
        enable: Option<InputCaptureEnable>,
        disable: Option<InputCaptureDisable>,
        user_data: *mut core::ffi::c_void,
    ) {
        self.enable_callback.set(enable);
        self.disable_callback.set(disable);
        self.user_data.set(user_data);
    }

    /// Process a captured input event from the session manager (D-Bus/hardware bound).
    pub fn process_event(&self, _event_type: u32) -> bool {
        // TODO: Implement event dispatch to active sessions
        false
    }

    /// Notify the service that input capture was cancelled (D-Bus/hardware bound).
    pub fn notify_cancelled(&self) {
        // TODO: Implement D-Bus cancellation broadcast to sessions
    }

    /// Activate input capture for a session (invokes enable_callback).
    pub fn activate_session(&self) {
        if let Some(enable) = self.enable_callback.get() {
            let user_data = self.user_data.get();
            enable(self as *const _ as *mut _, user_data);
        }
    }

    /// Deactivate input capture for a session (invokes disable_callback).
    pub fn deactivate_session(&self) {
        if let Some(disable) = self.disable_callback.get() {
            let user_data = self.user_data.get();
            disable(self as *const _ as *mut _, user_data);
        }
    }
}

impl Default for MetaInputCapture {
    fn default() -> Self {
        Self::new()
    }
}
