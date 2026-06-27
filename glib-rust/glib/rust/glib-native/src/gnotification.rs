//! GIO desktop notification matching `gio/gnotification.h` /
//! `gio/gnotification.c`.
//!
//! Upstream `GNotification` is a `GObject` subclass. We port it as a
//! plain `pub struct` with the same fields and API rather than a
//! registered GObject subclass, mirroring upstream semantics with
//! idiomatic Rust. Icons use [`Icon`](crate::gicon::Icon).
//!
//! Provides:
//! - `NotificationPriority` enum (Normal / Low / High / Urgent).
//! - `Notification` struct with title, body, priority, category,
//!   buttons (label + action + optional `Variant` target), default
//!   action + optional `Variant` target, and an optional [`Icon`].
//! - Full setter API matching upstream: `set_title`, `set_body`,
//!   `set_priority`, `set_urgent` (deprecated wrapper), `set_category`,
//!   `add_button`, `add_button_with_target_value`, `set_default_action`,
//!   `set_default_action_with_target_value`, `set_icon`.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::gicon::Icon;
use crate::prelude::*;
use crate::variant::Variant;
use alloc::string::String;
use alloc::vec::Vec;

// ─────────────────────── GNotificationPriority ────────────────────────────

/// Priority for a desktop notification (`GNotificationPriority`).
///
/// Matches the upstream enum order:
/// `Normal = 0`, `Low = 1`, `High = 2`, `Urgent = 3`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum NotificationPriority {
    /// Default priority — majority of notifications (`G_NOTIFICATION_PRIORITY_NORMAL`).
    Normal = 0,
    /// No immediate attention required (`G_NOTIFICATION_PRIORITY_LOW`).
    Low = 1,
    /// Time-sensitive (`G_NOTIFICATION_PRIORITY_HIGH`).
    High = 2,
    /// Urgent / requires prompt response (`G_NOTIFICATION_PRIORITY_URGENT`).
    Urgent = 3,
}

impl Default for NotificationPriority {
    fn default() -> Self {
        NotificationPriority::Normal
    }
}

// ──────────────────────────── Button ──────────────────────────────────────

/// A button attached to a notification. Mirrors the private `Button`
/// struct in `gnotification.c`: label, action name, optional target
/// variant.
#[derive(Clone, Debug)]
pub struct NotificationButton {
    /// Button label shown to the user.
    pub label: String,
    /// Action name (e.g. `"app.quit"`).
    pub action_name: String,
    /// Optional target parameter for the action (a `GVariant`).
    pub target: Option<Variant>,
}

// ────────────────────────── GNotification ─────────────────────────────────

/// A desktop notification (`GNotification`).
///
/// Plain struct port of the upstream GObject subclass. Fields match
/// upstream 1:1 (title, body, icon, priority, category, buttons,
/// default_action, default_action_target). Ref counting is handled by
/// Rust's ownership model — pass `Notification` by value or wrap in
/// `Arc<Notification>` for shared ownership.
#[derive(Clone, Debug)]
pub struct Notification {
    title: String,
    body: String,
    icon: Option<Icon>,
    priority: NotificationPriority,
    category: Option<String>,
    buttons: Vec<NotificationButton>,
    default_action: Option<String>,
    default_action_target: Option<Variant>,
}

impl Notification {
    /// Create a new notification with the given title
    /// (`g_notification_new`).
    pub fn new(title: &str) -> Self {
        Self {
            title: title.to_owned(),
            body: String::new(),
            icon: None,
            priority: NotificationPriority::Normal,
            category: None,
            buttons: Vec::new(),
            default_action: None,
            default_action_target: None,
        }
    }

    /// Set the title (`g_notification_set_title`).
    pub fn set_title(&mut self, title: &str) {
        self.title = title.to_owned();
    }

    /// Set the body (`g_notification_set_body`).
    pub fn set_body(&mut self, body: &str) {
        self.body = body.to_owned();
    }

    /// Set the icon (`g_notification_set_icon`).
    pub fn set_icon(&mut self, icon: Icon) {
        self.icon = Some(icon);
    }

    /// Set the priority (`g_notification_set_priority`).
    pub fn set_priority(&mut self, priority: NotificationPriority) {
        self.priority = priority;
    }

    /// Set whether the notification is urgent (`g_notification_set_urgent`).
    ///
    /// Deprecated upstream in favour of `set_priority`; maps `true` to
    /// `Urgent` and `false` to `Normal`, matching upstream behaviour.
    pub fn set_urgent(&mut self, urgent: bool) {
        self.priority = if urgent {
            NotificationPriority::Urgent
        } else {
            NotificationPriority::Normal
        };
    }

    /// Set the category (`g_notification_set_category`).
    pub fn set_category(&mut self, category: &str) {
        self.category = Some(category.to_owned());
    }

    /// Add a button with a detailed action spec
    /// (`g_notification_add_button`).
    ///
    /// `detailed_action` is the action name optionally followed by
    /// target data in the `"action::target"` form. We store it
    /// verbatim (no parsing of the detailed-action syntax, matching
    /// the upstream `add_button_with_target_value` path which takes
    /// separate action + target).
    pub fn add_button(&mut self, label: &str, detailed_action: &str) {
        self.buttons.push(NotificationButton {
            label: label.to_owned(),
            action_name: detailed_action.to_owned(),
            target: None,
        });
    }

    /// Add a button with an action name and a `Variant` target
    /// (`g_notification_add_button_with_target_value`).
    pub fn add_button_with_target_value(&mut self, label: &str, action: &str, target: Variant) {
        self.buttons.push(NotificationButton {
            label: label.to_owned(),
            action_name: action.to_owned(),
            target: Some(target),
        });
    }

    /// Set the default action (`g_notification_set_default_action`).
    pub fn set_default_action(&mut self, detailed_action: &str) {
        self.default_action = Some(detailed_action.to_owned());
        self.default_action_target = None;
    }

    /// Set the default action with a `Variant` target
    /// (`g_notification_set_default_action_and_target_value`).
    pub fn set_default_action_with_target_value(&mut self, action: &str, target: Variant) {
        self.default_action = Some(action.to_owned());
        self.default_action_target = Some(target);
    }

    // ── accessors (for testing / smoke check; not in upstream public API
    //   but useful for introspection and the kernel smoke check) ───────

    /// Current title.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Current body.
    pub fn body(&self) -> &str {
        &self.body
    }

    /// Current priority.
    pub fn priority(&self) -> NotificationPriority {
        self.priority
    }

    /// Current category, if set.
    pub fn category(&self) -> Option<&str> {
        self.category.as_deref()
    }

    /// Number of buttons attached.
    pub fn n_buttons(&self) -> usize {
        self.buttons.len()
    }

    /// Borrow the buttons slice.
    pub fn buttons(&self) -> &[NotificationButton] {
        &self.buttons
    }

    /// Default action (name), if set.
    pub fn default_action(&self) -> Option<&str> {
        self.default_action.as_deref()
    }

    /// Default action target, if set.
    pub fn default_action_target(&self) -> Option<&Variant> {
        self.default_action_target.as_ref()
    }

    /// Borrow the icon, if set.
    pub fn icon(&self) -> Option<&Icon> {
        self.icon.as_ref()
    }
}

// ───────────────────────────── tests ──────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn priority_values_match_upstream() {
        assert_eq!(NotificationPriority::Normal as i32, 0);
        assert_eq!(NotificationPriority::Low as i32, 1);
        assert_eq!(NotificationPriority::High as i32, 2);
        assert_eq!(NotificationPriority::Urgent as i32, 3);
    }

    #[test]
    fn priority_default_is_normal() {
        assert_eq!(
            NotificationPriority::default(),
            NotificationPriority::Normal
        );
    }

    #[test]
    fn new_sets_title_and_defaults() {
        let n = Notification::new("Hello");
        assert_eq!(n.title(), "Hello");
        assert_eq!(n.body(), "");
        assert_eq!(n.priority(), NotificationPriority::Normal);
        assert_eq!(n.category(), None);
        assert_eq!(n.n_buttons(), 0);
        assert_eq!(n.default_action(), None);
        assert!(n.default_action_target().is_none());
        assert!(n.icon().is_none());
    }

    #[test]
    fn set_title_and_body() {
        let mut n = Notification::new("Hello");
        n.set_title("World");
        n.set_body("Body text");
        assert_eq!(n.title(), "World");
        assert_eq!(n.body(), "Body text");
    }

    #[test]
    fn set_priority() {
        let mut n = Notification::new("Hello");
        n.set_priority(NotificationPriority::High);
        assert_eq!(n.priority(), NotificationPriority::High);
    }

    #[test]
    fn set_urgent_maps_to_priority() {
        let mut n = Notification::new("Hello");
        n.set_urgent(true);
        assert_eq!(n.priority(), NotificationPriority::Urgent);
        n.set_urgent(false);
        assert_eq!(n.priority(), NotificationPriority::Normal);
    }

    #[test]
    fn set_category() {
        let mut n = Notification::new("Hello");
        n.set_category("email.arrived");
        assert_eq!(n.category(), Some("email.arrived"));
    }

    #[test]
    fn add_button_without_target() {
        let mut n = Notification::new("Hello");
        n.add_button("Reply", "app.reply");
        n.add_button("Forward", "app.forward::plaintext");
        assert_eq!(n.n_buttons(), 2);
        assert_eq!(n.buttons()[0].label, "Reply");
        assert_eq!(n.buttons()[0].action_name, "app.reply");
        assert!(n.buttons()[0].target.is_none());
        assert_eq!(n.buttons()[1].action_name, "app.forward::plaintext");
        assert!(n.buttons()[1].target.is_none());
    }

    #[test]
    fn add_button_with_target_value() {
        let mut n = Notification::new("Hello");
        n.add_button_with_target_value("Open", "app.open", Variant::new_string("file.txt"));
        assert_eq!(n.n_buttons(), 1);
        let btn = &n.buttons()[0];
        assert_eq!(btn.label, "Open");
        assert_eq!(btn.action_name, "app.open");
        assert!(btn.target.is_some());
        assert_eq!(btn.target.as_ref().unwrap().get_string(), "file.txt");
    }

    #[test]
    fn set_default_action() {
        let mut n = Notification::new("Hello");
        n.set_default_action("app.activate");
        assert_eq!(n.default_action(), Some("app.activate"));
        assert!(n.default_action_target().is_none());
    }

    #[test]
    fn set_default_action_with_target_value() {
        let mut n = Notification::new("Hello");
        n.set_default_action_with_target_value("app.open", Variant::new_int32(42));
        assert_eq!(n.default_action(), Some("app.open"));
        let target = n.default_action_target().unwrap();
        assert_eq!(target.get_int32(), 42);
    }

    #[test]
    fn set_default_action_overwrites_previous_target() {
        let mut n = Notification::new("Hello");
        n.set_default_action_with_target_value("app.open", Variant::new_int32(1));
        n.set_default_action("app.activate");
        assert_eq!(n.default_action(), Some("app.activate"));
        // set_default_action (without target) clears the target.
        assert!(n.default_action_target().is_none());
    }

    #[test]
    fn set_icon_stores_icon() {
        use crate::gthemedicon::ThemedIcon;

        let mut n = Notification::new("Hello");
        let icon = Icon::Themed(ThemedIcon::new("folder"));
        n.set_icon(icon.clone());
        let stored = n.icon().unwrap();
        assert!(stored.equal(&icon));
    }

    #[test]
    fn clone_preserves_all_fields() {
        let mut n = Notification::new("Hello");
        n.set_body("World");
        n.set_priority(NotificationPriority::High);
        n.set_category("test");
        n.add_button_with_target_value("Btn", "app.btn", Variant::new_string("x"));
        n.set_default_action_with_target_value("app.default", Variant::new_int32(7));

        let cloned = n.clone();
        assert_eq!(cloned.title(), "Hello");
        assert_eq!(cloned.body(), "World");
        assert_eq!(cloned.priority(), NotificationPriority::High);
        assert_eq!(cloned.category(), Some("test"));
        assert_eq!(cloned.n_buttons(), 1);
        assert_eq!(cloned.buttons()[0].label, "Btn");
        assert_eq!(cloned.default_action(), Some("app.default"));
        assert_eq!(cloned.default_action_target().unwrap().get_int32(), 7);
    }

    #[test]
    fn buttons_slice_is_accessible() {
        let mut n = Notification::new("Hello");
        n.add_button("A", "app.a");
        n.add_button("B", "app.b");
        n.add_button("C", "app.c");
        let labels: Vec<&str> = n.buttons().iter().map(|b| b.label.as_str()).collect();
        assert_eq!(labels, vec!["A", "B", "C"]);
    }

    #[test]
    fn empty_notification_has_zero_buttons() {
        let n = Notification::new("Empty");
        assert_eq!(n.n_buttons(), 0);
        assert!(n.buttons().is_empty());
    }
}
