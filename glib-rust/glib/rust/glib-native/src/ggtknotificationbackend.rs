//! GGtkNotificationBackend matching `gio/ggtknotificationbackend.h` /
//! `gio/ggtknotificationbackend.c`.
//!
//! Sends notifications via the `org.gtk.Notifications` D-Bus interface.
//! This backend checks whether the GTK notification server is running
//! before declaring itself supported.
//!
//! In this no_std port we model the D-Bus availability check with a
//! configurable flag and queue notifications for transport.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::gnotification::Notification;
use crate::gnotificationbackend::NotificationBackend;
use alloc::string::{String, ToString};
use spin::Mutex;

/// The GTK notification backend (`GGtkNotificationBackend`).
///
/// Sends notifications via `org.gtk.Notifications` D-Bus service.
pub struct GtkNotificationBackend {
    inner: NotificationBackend,
    /// Whether the GTK notification server is available.
    server_available: Mutex<bool>,
    /// The application ID to use when sending notifications.
    application_id: Mutex<Option<String>>,
}

impl GtkNotificationBackend {
    /// Creates a new GTK notification backend.
    pub fn new() -> Self {
        Self {
            inner: NotificationBackend::new(),
            server_available: Mutex::new(false),
            application_id: Mutex::new(None),
        }
    }

    /// Returns whether this backend is supported.
    ///
    /// Mirrors `g_gtk_notification_backend_is_supported`. In the C
    /// source this does a synchronous D-Bus call to check if
    /// `org.gtk.Notifications` is owned. In this port we use a flag.
    pub fn is_supported(&self) -> bool {
        *self.server_available.lock()
    }

    /// Sets whether the GTK notification server is available.
    ///
    /// In a real system this would be determined by a D-Bus
    /// `GetNameOwner` call. In this port it is set externally.
    pub fn set_server_available(&self, available: bool) {
        *self.server_available.lock() = available;
    }

    /// Sets the application ID used when sending notifications.
    pub fn set_application_id(&self, app_id: &str) {
        *self.application_id.lock() = Some(app_id.to_string());
    }

    /// Returns the application ID, if set.
    pub fn application_id(&self) -> Option<String> {
        self.application_id.lock().clone()
    }

    /// Sends a notification via the GTK Notifications D-Bus interface.
    ///
    /// Mirrors `g_gtk_notification_backend_send_notification`.
    pub fn send_notification(&self, id: &str, notification: Notification) {
        self.inner.send_notification(id, notification);
    }

    /// Withdraws a notification by id.
    ///
    /// Mirrors `g_gtk_notification_backend_withdraw_notification`.
    pub fn withdraw_notification(&self, id: &str) -> bool {
        self.inner.withdraw_notification(id)
    }

    /// Returns the number of pending notifications.
    pub fn pending_count(&self) -> usize {
        self.inner.pending_count()
    }
}

impl Default for GtkNotificationBackend {
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
        let b = GtkNotificationBackend::new();
        assert!(!b.is_supported());
    }

    #[test]
    fn test_set_server_available() {
        let b = GtkNotificationBackend::new();
        b.set_server_available(true);
        assert!(b.is_supported());
    }

    #[test]
    fn test_send_and_withdraw() {
        let b = GtkNotificationBackend::new();
        b.set_server_available(true);
        b.send_notification("test1", Notification::new("Title"));
        assert_eq!(b.pending_count(), 1);
        assert!(b.withdraw_notification("test1"));
        assert_eq!(b.pending_count(), 0);
    }

    #[test]
    fn test_application_id() {
        let b = GtkNotificationBackend::new();
        assert!(b.application_id().is_none());
        b.set_application_id("org.example.App");
        assert_eq!(b.application_id().unwrap(), "org.example.App");
    }
}
