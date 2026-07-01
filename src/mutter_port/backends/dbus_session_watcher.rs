//! Dbus Session Watcher ported from GNOME Mutter
//!
//! MetaDbusSessionWatcher monitors D-Bus sessions for lifecycle events,
//! tracking session creation and destruction to coordinate with screen cast.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-dbus-session-watcher.h

use alloc::collections::BTreeMap;

pub struct MetaDbusSession {
    // TODO: port from meta-dbus-session.h
}

/// Property enum constants for MetaDbusSession
pub const META_DBUS_SESSION_PROP_SESSION_MANAGER: u32 = 0;
pub const META_DBUS_SESSION_PROP_PEER_NAME: u32 = 1;
pub const META_DBUS_SESSION_PROP_ID: u32 = 2;

pub struct MetaDbusSessionInterface {
    // TODO: port interface vtable from upstream
}

/// MetaDbusSessionWatcher: Monitors D-Bus session lifecycle events.
/// Tracks active clients and their sessions via a hash table.
pub struct MetaDbusSessionWatcher {
    pub clients: BTreeMap<usize, *mut core::ffi::c_void>,
}

impl MetaDbusSessionWatcher {
    pub fn new() -> Self {
        MetaDbusSessionWatcher {
            clients: BTreeMap::new(),
        }
    }
}

impl Default for MetaDbusSessionWatcher {
    fn default() -> Self {
        Self::new()
    }
}
