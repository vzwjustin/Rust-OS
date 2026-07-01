//! Remote Access Controller Private — ported from GNOME Mutter
//!
//! Private API for remote access controller internal state and session manager integration.
//! Handles D-Bus registration, handle lifecycle, and animation disable callbacks.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-remote-access-controller-private.h

use super::remote_access_controller::{MetaRemoteAccessController, MetaRemoteAccessHandle};

impl MetaRemoteAccessHandle {
    /// Mark this handle as stopped and trigger cleanup.
    pub fn notify_stopped(&mut self) {
        // TODO: Call stop callback, emit signals
    }

    /// Set whether animations should be disabled while this session is active.
    pub fn set_disable_animations(&mut self, _disable: bool) {
        // TODO: Update animation disable flag, notify display server
    }
}

impl MetaRemoteAccessController {
    /// Add a session manager to track new sessions created via D-Bus.
    pub fn add_session_manager(&mut self, _session_manager: &()) {
        // TODO: Register D-Bus interface handlers, connect signals
    }

    /// Register a new handle with the controller (internal use).
    pub fn register_handle(&mut self, handle: MetaRemoteAccessHandle) {
        // TODO: Add to active_handles, emit new-handle signal
        self.notify_new_handle(handle);
    }
}