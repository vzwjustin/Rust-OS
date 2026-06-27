//! GNotificationBackend matching `gio/gnotificationbackend.h`.
//! A backend for delivering notifications. In this no_std port we model
//! it with a queue of pending notifications.
//! Fully `no_std` compatible using `alloc`.

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

use crate::gnotification::Notification;

/// A notification backend (`GNotificationBackend`).
pub struct NotificationBackend {
    notifications: Mutex<Vec<(String, Notification)>>,
}

impl NotificationBackend {
    pub fn new() -> Self {
        Self {
            notifications: Mutex::new(Vec::new()),
        }
    }

    pub fn send_notification(&self, id: &str, notification: Notification) {
        self.notifications
            .lock()
            .push((id.to_string(), notification));
    }

    pub fn withdraw_notification(&self, id: &str) -> bool {
        let mut notifs = self.notifications.lock();
        let before = notifs.len();
        notifs.retain(|(nid, _)| nid != id);
        notifs.len() != before
    }

    pub fn pending_count(&self) -> usize {
        self.notifications.lock().len()
    }
}

impl Default for NotificationBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gnotification::NotificationPriority;

    #[test]
    fn test_send_withdraw() {
        let b = NotificationBackend::new();
        b.send_notification("test", Notification::new("Title"));
        assert_eq!(b.pending_count(), 1);
        assert!(b.withdraw_notification("test"));
        assert_eq!(b.pending_count(), 0);
    }

    #[test]
    fn test_withdraw_missing() {
        let b = NotificationBackend::new();
        assert!(!b.withdraw_notification("nonexistent"));
    }
}
