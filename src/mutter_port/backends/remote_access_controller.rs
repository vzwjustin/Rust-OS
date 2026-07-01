//! Remote Access Controller — ported from GNOME Mutter
//!
//! Manages remote access sessions and handles (screen cast + remote desktop).
//! Coordinates between D-Bus session manager and active remote access sessions.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-remote-access-controller-private.h
//!
//! Note: Upstream header meta-remote-access-controller.h not found; minimal stub based on private header.

use alloc::vec::Vec;

/// Handle to an active remote access session (screen cast or remote desktop).
pub struct MetaRemoteAccessHandle {
    // TODO: Session binding, stop callback from C implementation
}

impl MetaRemoteAccessHandle {
    pub fn new() -> Self {
        MetaRemoteAccessHandle {}
    }
}

impl Default for MetaRemoteAccessHandle {
    fn default() -> Self {
        Self::new()
    }
}

/// Central controller for all remote access sessions on this display server.
pub struct MetaRemoteAccessController {
    active_handles: Vec<MetaRemoteAccessHandle>,
}

impl MetaRemoteAccessController {
    pub fn new() -> Self {
        MetaRemoteAccessController {
            active_handles: Vec::new(),
        }
    }

    /// Register a new active remote access handle.
    pub fn notify_new_handle(&mut self, _handle: MetaRemoteAccessHandle) {
        // TODO: Add handle to tracking, emit signal
    }

    /// Mark a handle as stopped.
    pub fn notify_handle_stopped(&mut self, _handle: &MetaRemoteAccessHandle) {
        // TODO: Remove handle, clean up resources
    }
}

impl Default for MetaRemoteAccessController {
    fn default() -> Self {
        Self::new()
    }
}