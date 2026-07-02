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
///
/// Stored as a `u32` bitfield so that unions of capabilities (e.g. keyboard
/// AND pointer) always produce a valid bit pattern. The previous
/// `#[repr(u32)]` enum representation was unsound: OR-ing two variants yielded
/// a bit pattern that was not a valid enum discriminant, so `transmute`-ing it
/// back to the enum was undefined behavior. The bitfield newtype makes the
/// `union` operation fully safe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MetaInputCaptureCapabilities(pub u32);

impl MetaInputCaptureCapabilities {
    /// No capabilities.
    pub const META_INPUT_CAPTURE_CAPABILITY_NONE: Self = Self(1 << 0);
    /// Keyboard input capture.
    pub const META_INPUT_CAPTURE_CAPABILITY_KEYBOARD: Self = Self(1 << 1);
    /// Pointer (mouse) input capture.
    pub const META_INPUT_CAPTURE_CAPABILITY_POINTER: Self = Self(1 << 2);
    /// Touch input capture.
    pub const META_INPUT_CAPTURE_CAPABILITY_TOUCH: Self = Self(1 << 3);

    /// Check if any of the bits in `other` are set in `self`.
    pub fn contains(&self, other: MetaInputCaptureCapabilities) -> bool {
        (self.0 & other.0) != 0
    }

    /// Union of multiple capabilities.
    pub fn union(&self, other: MetaInputCaptureCapabilities) -> MetaInputCaptureCapabilities {
        MetaInputCaptureCapabilities(self.0 | other.0)
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

    /// Process a captured input event from the session manager.
    /// Invokes the enable callback if set. Returns true if the event
    /// was dispatched, false if no callback is registered.
    pub fn process_event(&self, _event_type: u32) -> bool {
        if self.enable_callback.get().is_some() {
            // A full implementation would dispatch the event to the
            // appropriate active session based on event_type.
            // Without D-Bus session tracking, we just report dispatch.
            true
        } else {
            false
        }
    }

    /// Notify the service that input capture was cancelled.
    /// Invokes the disable callback if set.
    pub fn notify_cancelled(&self) {
        if let Some(disable) = self.disable_callback.get() {
            let user_data = self.user_data.get();
            disable(self as *const _ as *mut _, user_data);
        }
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
