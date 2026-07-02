//! Remote Access Controller Private — ported from GNOME Mutter
//!
//! Private API for remote access controller internal state and session manager integration.
//! Handles D-Bus registration, handle lifecycle, and animation disable callbacks.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-remote-access-controller-private.h

use super::remote_access_controller::{MetaRemoteAccessController, MetaRemoteAccessHandle};

impl MetaRemoteAccessHandle {
    /// Mark this handle as stopped and trigger cleanup.
    /// Sets the `has_stopped` flag to prevent double-stop.
    pub fn notify_stopped(&mut self) {
        if !self.has_stopped {
            self.has_stopped = true;
            // A full implementation would call the stop callback and
            // emit the "stopped" signal to D-Bus clients.
        }
    }

    /// Set whether animations should be disabled while this session is active.
    pub fn set_disable_animations(&mut self, disable: bool) {
        self.disable_animations = disable;
        // A full implementation would notify the display server to
        // suspend or resume Clutter animations.
    }
}

impl MetaRemoteAccessController {
    /// Add a session manager to track new sessions created via D-Bus.
    /// Without D-Bus transport, this is a no-op placeholder.
    pub fn add_session_manager(&mut self, _session_manager: &()) {
        // D-Bus session manager registration requires a D-Bus transport
        // layer. The session_managers list is used for handle tracking.
    }

    /// Register a new handle with the controller.
    /// Adds the handle to the active list.
    pub fn register_handle(&mut self, handle: MetaRemoteAccessHandle) {
        self.notify_new_handle(handle);
    }
}
