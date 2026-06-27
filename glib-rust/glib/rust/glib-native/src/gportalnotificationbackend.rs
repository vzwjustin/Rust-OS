//! GPortalNotificationBackend matching `gio/gportalnotificationbackend.h` /
//! `gio/gportalnotificationbackend.c`.
//!
//! Sends notifications via the Flatpak/portal D-Bus interface
//! (`org.freedesktop.portal.Notification`). This backend is used
//! inside sandboxed applications that communicate through the portal.
//!
//! In this no_std port we model portal availability with a flag
//! (delegating to `PortalSupport`) and queue notifications for transport.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::gnotification::Notification;
use crate::gnotificationbackend::NotificationBackend;
use crate::gportalsupport::PortalSupport;
use crate::prelude::*;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// The portal notification backend (`GPortalNotificationBackend`).
///
/// Sends notifications via `org.freedesktop.portal.Notification` D-Bus
/// interface. Used in sandboxed environments (Flatpak).
pub struct PortalNotificationBackend {
    inner: NotificationBackend,
    portal: PortalSupport,
    /// Sent notification IDs (for withdrawal tracking).
    sent_ids: Mutex<Vec<String>>,
}

impl PortalNotificationBackend {
    /// Creates a new portal notification backend.
    pub fn new() -> Self {
        Self {
            inner: NotificationBackend::new(),
            portal: PortalSupport::new(),
            sent_ids: Mutex::new(Vec::new()),
        }
    }

    /// Returns whether this backend is supported.
    ///
    /// Mirrors `g_portal_notification_backend_is_supported`.
    /// Checks `glib_should_use_portal()` — in this port we delegate
    /// to `PortalSupport::is_available()`.
    pub fn is_supported(&self) -> bool {
        self.portal.is_available()
    }

    /// Sets portal availability (for testing/configuration).
    pub fn set_portal_available(&self, available: bool) {
        self.portal.set_available(available);
    }

    /// Sends a notification via the portal D-Bus interface.
    ///
    /// Mirrors `g_portal_notification_backend_send_notification`.
    /// Calls `org.freedesktop.portal.Notification.AddNotification`
    /// with the notification serialized as `a{sv}`.
    pub fn send_notification(&self, id: &str, notification: Notification) {
        self.sent_ids.lock().push(id.to_string());
        self.inner.send_notification(id, notification);
    }

    /// Withdraws a notification by id.
    ///
    /// Mirrors `g_portal_notification_backend_withdraw_notification`.
    /// Calls `org.freedesktop.portal.Notification.RemoveNotification`.
    pub fn withdraw_notification(&self, id: &str) -> bool {
        let mut sent = self.sent_ids.lock();
        let before = sent.len();
        sent.retain(|s| s != id);
        let removed = sent.len() != before;
        drop(sent);

        self.inner.withdraw_notification(id) || removed
    }

    /// Returns the number of pending notifications.
    pub fn pending_count(&self) -> usize {
        self.inner.pending_count()
    }
}

impl Default for PortalNotificationBackend {
    fn default() -> Self {
        Self::new()
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_not_supported_by_default() {
        let b = PortalNotificationBackend::new();
        assert!(!b.is_supported());
    }

    #[test]
    fn test_set_portal_available() {
        let b = PortalNotificationBackend::new();
        b.set_portal_available(true);
        assert!(b.is_supported());
    }

    #[test]
    fn test_send_and_withdraw() {
        let b = PortalNotificationBackend::new();
        b.set_portal_available(true);
        b.send_notification("test1", Notification::new("Title"));
        assert_eq!(b.pending_count(), 1);
        assert!(b.withdraw_notification("test1"));
        assert_eq!(b.pending_count(), 0);
    }

    #[test]
    fn test_withdraw_missing() {
        let b = PortalNotificationBackend::new();
        assert!(!b.withdraw_notification("nonexistent"));
    }
}
