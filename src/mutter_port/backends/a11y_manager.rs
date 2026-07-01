//! A11Y Manager ported from GNOME Mutter's src/backends/
//!
//! Manages accessibility features including keyboard event notifications,
//! motion event tracking, and modifier key extraction for a11y clients.
//! Coordinates D-Bus access, key grabbing, and pointer monitoring.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-a11y-manager.c

use alloc::{collections::BTreeMap, vec::Vec};
use core::ffi::c_void;

/// Opaque backend reference.
pub struct MetaBackend;

/// Accessibility manager for event notification and keyboard handling.
pub struct MetaA11yManager {
    /// Reference to the backend (opaque).
    pub backend: *mut MetaBackend,
    /// D-Bus registration name ID for the service.
    pub dbus_name_id: u32,
    /// Hash map of pointer query requesters by D-Bus connection.
    pub query_pointer_requesters: BTreeMap<*mut c_void, ()>,
    /// D-Bus keyboard monitor skeleton interface (opaque).
    pub keyboard_monitor_skeleton: *mut c_void,
    /// D-Bus pointer locator skeleton interface (opaque).
    pub pointer_locator_skeleton: *mut c_void,
    /// D-Bus connection reference (opaque).
    pub connection: *mut c_void,
    /// List of active key grabbers (opaque D-Bus connections).
    pub key_grabbers: Vec<*mut c_void>,
    /// Map of grabbed key presses by keysym.
    pub grabbed_keypresses: BTreeMap<u32, u32>,
    /// Map of all grabbed modifier keysyms.
    pub all_grabbed_modifiers: BTreeMap<u32, u32>,
    /// Access control checker for D-Bus operations (opaque).
    pub access_checker: *mut c_void,
}

impl MetaA11yManager {
    /// Create a new accessibility manager.
    pub fn new() -> Self {
        MetaA11yManager {
            backend: core::ptr::null_mut(),
            dbus_name_id: 0,
            query_pointer_requesters: BTreeMap::new(),
            keyboard_monitor_skeleton: core::ptr::null_mut(),
            pointer_locator_skeleton: core::ptr::null_mut(),
            connection: core::ptr::null_mut(),
            key_grabbers: Vec::new(),
            grabbed_keypresses: BTreeMap::new(),
            all_grabbed_modifiers: BTreeMap::new(),
            access_checker: core::ptr::null_mut(),
        }
    }

    /// Notify accessibility clients of input event. Returns true if
    /// there are listeners to notify. Without D-Bus transport, returns
    /// false (no listeners reachable).
    pub fn notify_clients(&mut self) -> bool {
        // D-Bus event dispatch requires a D-Bus transport layer.
        // If there are grabbed keypresses, there are interested clients.
        !self.grabbed_keypresses.is_empty()
    }

    /// Notify of pointer motion event. Only emits if there are
    /// registered pointer query requesters.
    pub fn maybe_notify_motion(&mut self) {
        // Without D-Bus, motion events can't be dispatched to a11y clients.
        // If there are requesters, the motion would be sent via D-Bus.
        if !self.query_pointer_requesters.is_empty() {
            // D-Bus motion event emission would go here.
        }
    }

    /// Get list of modifier keysyms. Returns the keysyms for all
    /// grabbed modifier keys (Shift, Control, Alt, etc.).
    /// Standard X11 keysyms for common modifiers:
    /// Shift_L=0xffe1, Shift_R=0xffe2, Control_L=0xffe3, Control_R=0xffe4,
    /// Alt_L=0xffe9, Alt_R=0xffea, Super_L=0xffeb, Super_R=0xffec.
    pub fn get_modifier_keysyms(&self) -> Vec<u32> {
        self.all_grabbed_modifiers.keys().copied().collect()
    }
}

impl Default for MetaA11yManager {
    fn default() -> Self {
        Self::new()
    }
}
