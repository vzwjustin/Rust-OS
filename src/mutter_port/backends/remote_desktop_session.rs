//! Remote Desktop Session — ported from GNOME Mutter
//!
//! Combines screen casting and remote desktop input control in a unified session.
//! Allows clients to both view and control a desktop remotely.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-remote-desktop-session.h

use alloc::string::String;

/// Opaque Eis (Event Input Source) object for input injection (mirrors C type).
pub struct MetaEis;

/// Handle to a remote desktop session linking screen cast and input control.
pub struct MetaRemoteDesktopSessionHandle {
    // TODO: Handle state, callback binding from C implementation
}

impl MetaRemoteDesktopSessionHandle {
    pub fn new() -> Self {
        MetaRemoteDesktopSessionHandle {}
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
    eis: Option<MetaEis>,
    // TODO: Screen cast reference, input state from C implementation
}

impl MetaRemoteDesktopSession {
    pub fn new() -> Self {
        MetaRemoteDesktopSession { eis: None }
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