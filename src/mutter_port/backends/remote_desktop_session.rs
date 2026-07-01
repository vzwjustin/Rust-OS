//! Remote Desktop Session — ported from GNOME Mutter
//!
//! Combines screen casting and remote desktop input control in a unified session.
//! Allows clients to both view and control a desktop remotely via D-Bus RDP protocol.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-remote-desktop-session.h

use alloc::string::String;
use core::ffi::c_void;

/// Opaque Eis (Event Input Source) object for input injection (mirrors C type).
pub struct MetaEis;

/// Handle to a remote desktop session linking screen cast and input control.
pub struct MetaRemoteDesktopSessionHandle {
    /// Reference to parent MetaRemoteAccessHandle.
    pub parent: *mut c_void,
    /// Weak reference back to session (to avoid circular ownership).
    pub session: *mut c_void,
}

impl MetaRemoteDesktopSessionHandle {
    pub fn new() -> Self {
        MetaRemoteDesktopSessionHandle {
            parent: core::ptr::null_mut(),
            session: core::ptr::null_mut(),
        }
    }
}

impl Default for MetaRemoteDesktopSessionHandle {
    fn default() -> Self {
        Self::new()
    }
}

/// A remote desktop session combining screen capture and input injection.
///
/// Sessions bind a screen cast session (for video/audio output) with input
/// devices (keyboard, pointer, touchscreen) for full remote control capability.
pub struct MetaRemoteDesktopSession {
    /// EIS context for input injection.
    pub eis: Option<MetaEis>,
    /// Object path for D-Bus service.
    pub object_path: String,
    /// Reference to linked screen cast session (opaque MetaScreenCastSession).
    pub screen_cast_session: *mut c_void,
    /// Input device records (opaque MetaRemoteDesktopSessionInputSource collection).
    pub input_sources: *mut c_void,
    /// Whether session is active and accepting input.
    pub active: bool,
}

impl MetaRemoteDesktopSession {
    pub fn new() -> Self {
        MetaRemoteDesktopSession {
            eis: None,
            object_path: String::new(),
            screen_cast_session: core::ptr::null_mut(),
            input_sources: core::ptr::null_mut(),
            active: false,
        }
    }

    pub fn get_eis(&self) -> Option<&MetaEis> {
        self.eis.as_ref()
    }

    pub fn set_eis(&mut self, eis: MetaEis) {
        self.eis = Some(eis);
    }

    /// Register a screen cast session with this remote desktop session.
    /// Returns an error if incompatible states are detected.
    pub fn register_screen_cast_session(&mut self, _screen_cast: &()) -> Result<(), String> {
        // TODO: Validate session type, bind screen cast streams
        Ok(())
    }
}

impl Default for MetaRemoteDesktopSession {
    fn default() -> Self {
        Self::new()
    }
}