//! idle_manager - owns the core idle monitor and exports it over D-Bus.
//!
//! Ported from GNOME Mutter's src/backends/meta-idle-manager.c. In Mutter this
//! object owns the backend's core `MetaIdleMonitor`, exports an
//! `org.gnome.Mutter.IdleMonitor` D-Bus service, and manages per-client idle
//! watches. The idle-tracking bookkeeping (core monitor, per-client watches,
//! reset) is preserved; the entire D-Bus surface (skeletons, method handlers,
//! g_bus_own_name, name watching) and the Clutter integration are stubbed, since
//! they are unavailable in the kernel.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-idle-manager.c

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};

/// Kind of idle watch requested by a client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdleWatchKind {
    /// Fires after the given idle interval elapses (add_idle_watch).
    Idle,
    /// Fires when the user becomes active again (add_user_active_watch).
    UserActive,
}

/// A per-client idle watch tracked over D-Bus. Mirrors the C `DBusWatch`.
///
/// The `dbus_monitor` skeleton, GObject refs, and name-watcher id are stubbed;
/// the client bus name and watch identity are kept.
#[derive(Debug, Clone)]
pub struct DBusWatch {
    /// Unique id of this watch within its monitor.
    pub watch_id: u32,
    /// Bus name of the requesting client (g_dbus_method_invocation_get_sender).
    pub dbus_name: String,
    /// Kind of watch.
    pub kind: IdleWatchKind,
    /// Idle interval in milliseconds (only meaningful for `Idle`).
    pub interval: u64,
    /// Stub for the g_bus_watch_name name-watcher id.
    pub name_watcher_id: u32,
}

/// Tracks cumulative idle time and per-client watches. Mirrors `MetaIdleMonitor`
/// insofar as this file interacts with it.
#[derive(Debug, Default)]
pub struct IdleMonitor {
    /// Accumulated idle time in milliseconds.
    idletime: u64,
    /// Active watches keyed by their id.
    watches: Vec<DBusWatch>,
    /// Source of monotonically increasing watch ids.
    next_watch_id: AtomicU32,
}

impl IdleMonitor {
    pub fn new() -> Self {
        IdleMonitor {
            idletime: 0,
            watches: Vec::new(),
            next_watch_id: AtomicU32::new(1),
        }
    }

    /// Mirrors `meta_idle_monitor_get_idletime`.
    pub fn get_idletime(&self) -> u64 {
        self.idletime
    }

    /// Advance the accumulated idle time (would be driven by input events).
    pub fn accumulate_idle(&mut self, ms: u64) {
        self.idletime = self.idletime.saturating_add(ms);
    }

    /// Mirrors `meta_idle_monitor_reset_idletime`.
    pub fn reset_idletime(&mut self) {
        self.idletime = 0;
    }

    /// Mirrors `meta_idle_monitor_add_idle_watch`.
    pub fn add_idle_watch(&mut self, interval: u64, dbus_name: &str, name_watcher_id: u32) -> u32 {
        let id = self.next_watch_id.fetch_add(1, Ordering::Relaxed);
        self.watches.push(DBusWatch {
            watch_id: id,
            dbus_name: dbus_name.to_string(),
            kind: IdleWatchKind::Idle,
            interval,
            name_watcher_id,
        });
        id
    }

    /// Mirrors `meta_idle_monitor_add_user_active_watch`.
    pub fn add_user_active_watch(&mut self, dbus_name: &str, name_watcher_id: u32) -> u32 {
        let id = self.next_watch_id.fetch_add(1, Ordering::Relaxed);
        self.watches.push(DBusWatch {
            watch_id: id,
            dbus_name: dbus_name.to_string(),
            kind: IdleWatchKind::UserActive,
            interval: 0,
            name_watcher_id,
        });
        id
    }

    /// Mirrors `meta_idle_monitor_remove_watch`.
    pub fn remove_watch(&mut self, id: u32) -> bool {
        let before = self.watches.len();
        self.watches.retain(|w| w.watch_id != id);
        self.watches.len() != before
    }

    /// Number of active watches.
    pub fn watch_count(&self) -> usize {
        self.watches.len()
    }
}

/// Owns the core idle monitor for a backend. Mirrors `MetaIdleManager`.
///
/// The `backend` pointer and D-Bus name ownership (`dbus_name_id`) are stubbed.
#[derive(Debug, Default)]
pub struct IdleManager {
    /// The core monitor, created lazily like the C `core_monitor`.
    core_monitor: Option<IdleMonitor>,
}

impl IdleManager {
    /// Mirrors `meta_idle_manager_new`. The D-Bus name acquisition
    /// (g_bus_own_name for "org.gnome.Mutter.IdleMonitor") is stubbed out.
    pub fn new() -> Self {
        IdleManager { core_monitor: None }
    }

    /// Mirrors `meta_idle_manager_get_core_monitor`: lazily creates the core
    /// monitor. It is never cleared, so it cumulates idle time from all devices.
    pub fn get_core_monitor(&mut self) -> &mut IdleMonitor {
        self.core_monitor.get_or_insert_with(IdleMonitor::new)
    }

    /// Mirrors `meta_idle_manager_reset_idle_time`.
    pub fn reset_idle_time(&mut self) {
        self.get_core_monitor().reset_idletime();
    }
}
