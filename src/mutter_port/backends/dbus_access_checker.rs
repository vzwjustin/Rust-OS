//! dbus_access_checker - allow-list of D-Bus senders permitted to call a service.
//!
//! Ported from GNOME Mutter's src/backends/meta-dbus-access-checker.c. The C type
//! is a GObject that watches each allowed bus name and records its current owner,
//! then checks incoming senders against those owners. The allow-list data model and
//! the is-sender-allowed logic (including the unsafe-mode bypass) are preserved. The
//! actual bus-name watching (g_bus_watch_name_on_connection) is stubbed, so owners
//! must be updated via the name_appeared/name_vanished helpers.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-dbus-access-checker.c

use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// An allowed bus name plus its currently-watched owner.
/// Mirrors `AllowedSender`.
#[derive(Debug, Clone)]
pub struct AllowedSender {
    /// Well-known name we allow (e.g. "org.gnome.Shell").
    pub name: String,
    /// The unique owner of `name`, if currently present on the bus.
    pub name_owner: Option<String>,
    /// Stub for the g_bus_watch_name watch id.
    pub watch_id: u32,
}

impl AllowedSender {
    /// Mirrors `allowed_sender_new`. The real code registers a bus-name watch;
    /// that is stubbed, so `watch_id` is a caller-supplied placeholder.
    pub fn new(name: &str, watch_id: u32) -> Self {
        AllowedSender {
            name: name.to_string(),
            name_owner: None,
            watch_id,
        }
    }

    /// Mirrors `name_appeared_cb`: record the unique owner.
    pub fn name_appeared(&mut self, name_owner: &str) {
        self.name_owner = Some(name_owner.to_string());
    }

    /// Mirrors `name_vanished_cb`: clear the owner.
    pub fn name_vanished(&mut self) {
        self.name_owner = None;
    }
}

/// Checks whether D-Bus senders are permitted. Mirrors `MetaDbusAccessChecker`.
///
/// The GObject `connection` and `context` properties are represented as plain
/// fields; `unsafe_mode` stands in for `meta_context_get_unsafe_mode`.
#[derive(Debug, Default)]
pub struct DbusAccessChecker {
    allowed_senders: Vec<AllowedSender>,
    /// When true, every sender is allowed (unsafe-mode bypass).
    pub unsafe_mode: bool,
}

impl DbusAccessChecker {
    /// Mirrors `meta_dbus_access_checker_new` / `_init`.
    pub fn new() -> Self {
        DbusAccessChecker {
            allowed_senders: Vec::new(),
            unsafe_mode: false,
        }
    }

    /// Mirrors `meta_dbus_access_checker_allow_sender`.
    pub fn allow_sender(&mut self, name: &str, watch_id: u32) {
        self.allowed_senders
            .push(AllowedSender::new(name, watch_id));
    }

    /// Mirrors `meta_dbus_access_checker_is_sender_allowed`.
    ///
    /// Returns true in unsafe mode, or if `sender_name` matches the current
    /// owner of any allowed name.
    pub fn is_sender_allowed(&self, sender_name: Option<&str>) -> bool {
        if self.unsafe_mode {
            return true;
        }

        let sender_name = match sender_name {
            Some(s) => s,
            None => return false,
        };

        self.allowed_senders
            .iter()
            .any(|s| s.name_owner.as_deref() == Some(sender_name))
    }

    /// Mutable access to a watched sender by name (used by the stubbed watch
    /// callbacks to update owners).
    pub fn sender_mut(&mut self, name: &str) -> Option<&mut AllowedSender> {
        self.allowed_senders.iter_mut().find(|s| s.name == name)
    }

    /// Number of allowed senders.
    pub fn len(&self) -> usize {
        self.allowed_senders.len()
    }

    /// Whether the allow-list is empty.
    pub fn is_empty(&self) -> bool {
        self.allowed_senders.is_empty()
    }
}
