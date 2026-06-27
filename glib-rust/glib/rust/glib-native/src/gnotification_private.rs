//! `gnotification-private` matching `gio/gnotification-private.h`.
//!
//! Private notification API: getters for id, title, body, category,
//! icon, priority, buttons, and serialization.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::gnotification::{Notification, NotificationButton, NotificationPriority};
use crate::variant::Variant;
use alloc::vec::Vec;

/// Returns the notification's id (mirrors `g_notification_get_id`).
///
/// In the C code, the id is set externally by the backend. In our port,
/// there is no id field on the struct, so we return `None`.
pub fn get_id(_notification: &Notification) -> Option<&str> {
    None
}

/// Returns the notification's title (mirrors `g_notification_get_title`).
pub fn get_title(notification: &Notification) -> &str {
    notification.title()
}

/// Returns the notification's body (mirrors `g_notification_get_body`).
pub fn get_body(notification: &Notification) -> &str {
    notification.body()
}

/// Returns the notification's category (mirrors `g_notification_get_category`).
pub fn get_category(notification: &Notification) -> Option<&str> {
    notification.category()
}

/// Returns the notification's priority (mirrors `g_notification_get_priority`).
pub fn get_priority(notification: &Notification) -> NotificationPriority {
    notification.priority()
}

/// Returns the number of buttons (mirrors `g_notification_get_n_buttons`).
pub fn get_n_buttons(notification: &Notification) -> usize {
    notification.n_buttons()
}

/// Returns a button by index (mirrors `g_notification_get_button`).
pub fn get_button(notification: &Notification, index: usize) -> Option<&NotificationButton> {
    notification.buttons().get(index)
}

/// Finds a button by action name (mirrors `g_notification_get_button_with_action`).
pub fn get_button_with_action(notification: &Notification, action: &str) -> Option<usize> {
    notification
        .buttons()
        .iter()
        .position(|b| b.action_name == action)
}

/// Returns the default action (mirrors `g_notification_get_default_action`).
pub fn get_default_action(notification: &Notification) -> Option<(&str, Option<&Variant>)> {
    notification
        .default_action()
        .map(|a| (a, notification.default_action_target()))
}

/// Serializes a notification to a `Variant` (mirrors `g_notification_serialize`).
pub fn serialize(notification: &Notification) -> Variant {
    let title = notification.title();
    let body = notification.body();
    let category = notification.category();
    let priority = notification.priority();

    let entries: Vec<(&str, Variant)> = {
        let mut v = vec![
            ("type", Variant::new_string("notification")),
            ("title", Variant::new_string(title)),
            ("priority", Variant::new_int32(priority as i32)),
        ];
        if !body.is_empty() {
            v.push(("body", Variant::new_string(body)));
        }
        if let Some(cat) = category {
            v.push(("category", Variant::new_string(cat)));
        }
        v
    };

    Variant::new_dict_entry(
        Variant::new_string("notification"),
        Variant::new_tuple(entries.iter().map(|(_, v)| v.clone()).collect()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_getters() {
        let mut n = Notification::new("Test Title");
        n.set_body("Test body");
        n.set_category("test");

        assert_eq!(get_title(&n), "Test Title");
        assert_eq!(get_body(&n), "Test body");
        assert_eq!(get_id(&n), None);
        assert_eq!(get_category(&n), Some("test"));
    }

    #[test]
    fn test_buttons() {
        let mut n = Notification::new("Test");
        n.add_button("OK", "ok.action");
        n.add_button("Cancel", "cancel.action");

        assert_eq!(get_n_buttons(&n), 2);
        assert_eq!(get_button(&n, 0).unwrap().label, "OK");
        assert_eq!(get_button_with_action(&n, "cancel.action"), Some(1));
        assert_eq!(get_button_with_action(&n, "nonexistent"), None);
    }

    #[test]
    fn test_default_action() {
        let mut n = Notification::new("Test");
        n.set_default_action("app.open");
        let action = get_default_action(&n);
        let (name, target) = action.unwrap();
        assert_eq!(name, "app.open");
        assert!(target.is_none());
    }
}
