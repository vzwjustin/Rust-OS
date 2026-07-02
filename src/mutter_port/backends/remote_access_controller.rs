//! Remote Access Controller — ported from GNOME Mutter
//!
//! Manages remote access sessions and handles (screen cast + remote desktop).
//! Coordinates between D-Bus session manager and active remote access sessions.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-remote-access-controller-private.h

use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};

/// Handle to an active remote access session (screen cast or remote desktop).
#[derive(Clone)]
pub struct MetaRemoteAccessHandle {
    /// Unique session identifier.
    pub id: u32,
    /// Whether the session has already stopped.
    pub has_stopped: bool,
    /// Whether animations should be disabled during this session.
    pub disable_animations: bool,
    /// Whether the session is actively recording.
    pub is_recording: bool,
}

static NEXT_SESSION_ID: AtomicU32 = AtomicU32::new(1);

impl MetaRemoteAccessHandle {
    /// Create a new remote access handle.
    pub fn new() -> Self {
        MetaRemoteAccessHandle {
            id: NEXT_SESSION_ID.fetch_add(1, Ordering::Relaxed),
            has_stopped: false,
            disable_animations: false,
            is_recording: false,
        }
    }
}

impl Default for MetaRemoteAccessHandle {
    fn default() -> Self {
        Self::new()
    }
}

/// Central controller for all remote access sessions on this display server.
pub struct MetaRemoteAccessController {
    /// List of active remote access handles.
    pub session_managers: Vec<MetaRemoteAccessHandle>,
}

impl MetaRemoteAccessController {
    /// Create a new remote access controller.
    pub fn new() -> Self {
        MetaRemoteAccessController {
            session_managers: Vec::new(),
        }
    }

    /// Register a new active remote access handle.
    pub fn notify_new_handle(&mut self, handle: MetaRemoteAccessHandle) {
        self.session_managers.push(handle);
    }

    /// Mark a handle as stopped.
    pub fn notify_handle_stopped(&mut self, handle: &MetaRemoteAccessHandle) {
        self.session_managers.retain(|h| h.id != handle.id);
    }
}

impl Default for MetaRemoteAccessController {
    fn default() -> Self {
        Self::new()
    }
}
