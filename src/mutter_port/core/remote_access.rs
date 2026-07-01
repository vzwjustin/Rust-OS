//! MetaRemoteAccessController ported from GNOME Mutter's
//! src/core/meta-remote-access-controller.c
//!
//! MetaRemoteAccessController manages remote access to the compositor: it
//! tracks active remote desktop sessions and screen cast sessions, and
//! notifies the shell when remote access starts/stops so it can show an
//! indicator. In Mutter this is a GObject that emits "remote-access-started"
//! and "remote-access-ended" signals.
//!
//! In the kernel, GObject signals are replaced by a pending-events queue
//! that callers drain after each event loop iteration.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/core/meta-remote-access-controller.c

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};

/// Type of remote access session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteAccessType {
    /// Remote desktop control (input + output).
    RemoteDesktop,
    /// Screen cast (output only).
    ScreenCast,
}

/// A remote access session. Mirrors MetaRemoteAccessHandle.
#[derive(Debug, Clone)]
pub struct RemoteAccessHandle {
    /// Unique session id.
    pub id: u32,
    /// Type of remote access.
    pub access_type: RemoteAccessType,
    /// Client requesting the session.
    pub client: String,
    /// Whether the session is active.
    pub active: bool,
    /// Whether the user has been notified of this session.
    pub notified: bool,
}

/// Events emitted by the controller, replacing GObject signals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteAccessEvent {
    /// A remote access session has started.
    Started,
    /// A remote access session has ended.
    Ended,
}

static HANDLE_ID: AtomicU32 = AtomicU32::new(0);

fn next_handle_id() -> u32 {
    HANDLE_ID.fetch_add(1, Ordering::Relaxed) + 1
}

/// The remote access controller. Mirrors MetaRemoteAccessController.
#[derive(Debug)]
pub struct MetaRemoteAccessController {
    /// Active remote access sessions.
    handles: Vec<RemoteAccessHandle>,
    /// Pending events (replaces GObject signal emission).
    pending_events: Vec<RemoteAccessEvent>,
}

impl MetaRemoteAccessController {
    /// Create a new controller. Mirrors meta_remote_access_controller_new().
    pub fn new() -> Self {
        MetaRemoteAccessController {
            handles: Vec::new(),
            pending_events: Vec::new(),
        }
    }

    /// Register a new remote access session. Mirrors
    /// meta_remote_access_controller_register().
    ///
    /// If this is the first session, emits a "remote-access-started" event.
    pub fn register(&mut self, access_type: RemoteAccessType, client: &str) -> u32 {
        let was_empty = self.handles.is_empty();

        let id = next_handle_id();
        let handle = RemoteAccessHandle {
            id,
            access_type,
            client: String::from(client),
            active: true,
            notified: false,
        };
        self.handles.push(handle);

        if was_empty {
            self.pending_events.push(RemoteAccessEvent::Started);
        }

        id
    }

    /// End a remote access session. Mirrors
    /// meta_remote_access_controller_unregister().
    ///
    /// If this was the last session, emits a "remote-access-ended" event.
    pub fn unregister(&mut self, id: u32) -> bool {
        let before = self.handles.len();
        self.handles.retain(|h| h.id != id);

        if self.handles.len() == before {
            return false;
        }

        if self.handles.is_empty() {
            self.pending_events.push(RemoteAccessEvent::Ended);
        }

        true
    }

    /// Get all active handles.
    pub fn get_handles(&self) -> &[RemoteAccessHandle] {
        &self.handles
    }

    /// Number of active sessions.
    pub fn handle_count(&self) -> usize {
        self.handles.len()
    }

    /// Whether any remote access is active.
    pub fn is_active(&self) -> bool {
        !self.handles.is_empty()
    }

    /// Drain pending events.
    pub fn take_pending_events(&mut self) -> Vec<RemoteAccessEvent> {
        core::mem::take(&mut self.pending_events)
    }

    /// Mark all handles as notified (the shell has shown the indicator).
    pub fn mark_all_notified(&mut self) {
        for h in &mut self.handles {
            h.notified = true;
        }
    }
}

impl Default for MetaRemoteAccessController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_controller() {
        let ctrl = MetaRemoteAccessController::new();
        assert!(!ctrl.is_active());
        assert_eq!(ctrl.handle_count(), 0);
    }

    #[test]
    fn test_register_emits_started() {
        let mut ctrl = MetaRemoteAccessController::new();
        let _id = ctrl.register(RemoteAccessType::ScreenCast, "test");

        assert!(ctrl.is_active());
        assert_eq!(ctrl.handle_count(), 1);

        let events = ctrl.take_pending_events();
        assert!(events.contains(&RemoteAccessEvent::Started));
    }

    #[test]
    fn test_unregister_emits_ended() {
        let mut ctrl = MetaRemoteAccessController::new();
        let id = ctrl.register(RemoteAccessType::RemoteDesktop, "test");
        let _ = ctrl.take_pending_events();

        assert!(ctrl.unregister(id));
        assert!(!ctrl.is_active());

        let events = ctrl.take_pending_events();
        assert!(events.contains(&RemoteAccessEvent::Ended));
    }

    #[test]
    fn test_multiple_sessions_one_event() {
        let mut ctrl = MetaRemoteAccessController::new();
        let id1 = ctrl.register(RemoteAccessType::ScreenCast, "a");
        let _id2 = ctrl.register(RemoteAccessType::RemoteDesktop, "b");
        let _ = ctrl.take_pending_events();

        // Unregister one — should NOT emit Ended (still have one active).
        assert!(ctrl.unregister(id1));
        let events = ctrl.take_pending_events();
        assert!(!events.contains(&RemoteAccessEvent::Ended));
    }

    #[test]
    fn test_unregister_unknown_fails() {
        let mut ctrl = MetaRemoteAccessController::new();
        assert!(!ctrl.unregister(999));
    }

    #[test]
    fn test_mark_all_notified() {
        let mut ctrl = MetaRemoteAccessController::new();
        ctrl.register(RemoteAccessType::ScreenCast, "a");
        ctrl.register(RemoteAccessType::RemoteDesktop, "b");

        ctrl.mark_all_notified();
        for h in ctrl.get_handles() {
            assert!(h.notified);
        }
    }
}
