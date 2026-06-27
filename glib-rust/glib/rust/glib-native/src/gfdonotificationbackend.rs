//! GFdoNotificationBackend matching `gio/gfdonotificationbackend.h` /
//! `gio/gfdonotificationbackend.c`.
//!
//! Implements the freedesktop.org notification spec. Sends
//! notifications via D-Bus to `org.freedesktop.Notifications`.
//!
//! In this no_std port we model the D-Bus calls as queued messages
//! and track pending notifications by ID. The actual D-Bus transport
//! is deferred; the queue can be drained by a platform D-Bus layer.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::gnotification::{Notification, NotificationPriority};
use crate::gnotificationbackend::NotificationBackend;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// A pending freedesktop notification (mirrors `FreedesktopNotification`
/// in the C source).
#[derive(Clone, Debug)]
struct FreedesktopNotification {
    id: String,
    notify_id: u32,
    default_action: Option<String>,
}

/// The freedesktop.org notification backend (`GFdoNotificationBackend`).
///
/// This is the fallback backend with the lowest priority. It always
/// reports as supported.
pub struct FdoNotificationBackend {
    inner: NotificationBackend,
    notifications: Mutex<Vec<FreedesktopNotification>>,
    next_notify_id: Mutex<u32>,
}

impl FdoNotificationBackend {
    /// Creates a new freedesktop notification backend.
    pub fn new() -> Self {
        Self {
            inner: NotificationBackend::new(),
            notifications: Mutex::new(Vec::new()),
            next_notify_id: Mutex::new(1),
        }
    }

    /// Always returns `true` — this is the fallback backend.
    ///
    /// Mirrors `g_fdo_notification_backend_is_supported`.
    pub fn is_supported(&self) -> bool {
        true
    }

    /// Sends a notification via the freedesktop.org Notify D-Bus call.
    ///
    /// Mirrors `g_fdo_notification_backend_send_notification`.
    pub fn send_notification(&self, id: &str, notification: Notification) {
        // Extract default action info
        let default_action = notification.default_action().map(|s| s.to_string());

        // Check for existing notification with same id (for replacement)
        let replace_id = {
            let notifs = self.notifications.lock();
            notifs
                .iter()
                .find(|n| n.id == id)
                .map(|n| n.notify_id)
                .unwrap_or(0)
        };

        let notify_id = {
            let mut counter = self.next_notify_id.lock();
            let nid = *counter;
            *counter += 1;
            nid
        };

        // Remove existing notification with same id
        {
            let mut notifs = self.notifications.lock();
            notifs.retain(|n| n.id != id);
            notifs.push(FreedesktopNotification {
                id: id.to_string(),
                notify_id,
                default_action,
            });
        }

        // Queue the notification for D-Bus transport
        self.inner.send_notification(id, notification);
    }

    /// Withdraws a notification by id.
    ///
    /// Mirrors `g_fdo_notification_backend_withdraw_notification`.
    pub fn withdraw_notification(&self, id: &str) -> bool {
        let mut notifs = self.notifications.lock();
        let before = notifs.len();
        notifs.retain(|n| n.id != id);
        let removed = notifs.len() != before;
        drop(notifs);

        self.inner.withdraw_notification(id) || removed
    }

    /// Returns the number of pending notifications.
    pub fn pending_count(&self) -> usize {
        self.notifications.lock().len()
    }

    /// Converts a `NotificationPriority` to a freedesktop urgency level.
    ///
    /// 0 = low, 1 = normal, 2 = critical.
    ///
    /// Mirrors `urgency_from_priority` in the C source.
    pub fn urgency_from_priority(priority: NotificationPriority) -> u8 {
        match priority {
            NotificationPriority::Low => 0,
            NotificationPriority::Normal | NotificationPriority::High => 1,
            NotificationPriority::Urgent => 2,
        }
    }
}

impl Default for FdoNotificationBackend {
    fn default() -> Self {
        Self::new()
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_supported() {
        let b = FdoNotificationBackend::new();
        assert!(b.is_supported());
    }

    #[test]
    fn test_send_and_withdraw() {
        let b = FdoNotificationBackend::new();
        b.send_notification("test1", Notification::new("Title"));
        assert_eq!(b.pending_count(), 1);
        assert!(b.withdraw_notification("test1"));
        assert_eq!(b.pending_count(), 0);
    }

    #[test]
    fn test_withdraw_missing() {
        let b = FdoNotificationBackend::new();
        assert!(!b.withdraw_notification("nonexistent"));
    }

    #[test]
    fn test_replace_notification() {
        let b = FdoNotificationBackend::new();
        b.send_notification("id1", Notification::new("First"));
        b.send_notification("id1", Notification::new("Second"));
        assert_eq!(b.pending_count(), 1);
    }

    #[test]
    fn test_urgency_from_priority() {
        assert_eq!(
            FdoNotificationBackend::urgency_from_priority(NotificationPriority::Low),
            0
        );
        assert_eq!(
            FdoNotificationBackend::urgency_from_priority(NotificationPriority::Normal),
            1
        );
        assert_eq!(
            FdoNotificationBackend::urgency_from_priority(NotificationPriority::High),
            1
        );
        assert_eq!(
            FdoNotificationBackend::urgency_from_priority(NotificationPriority::Urgent),
            2
        );
    }
}
