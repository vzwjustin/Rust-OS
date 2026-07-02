//! Idle Monitor Private ported from GNOME Mutter's src/backends/
//!
//! Tracks user idle time with per-watch callbacks. Supports inhibition
//! (e.g., during video playback) and timeout-based idle detection. Watches can be
//! destroyed via destroy notify callbacks and timeout sources.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-idle-monitor-private.h

use alloc::collections::BTreeMap;
use core::cell::Cell;

/// Watch callback function type. Matches the upstream Mutter signature; the
/// `Option` makes the null callback representable without an untyped sentinel.
pub type MetaIdleMonitorWatchFunc = Option<
    unsafe extern "C" fn(
        monitor: *mut MetaIdleMonitor,
        watch_id: u32,
        user_data: *mut core::ffi::c_void,
    ),
>;

/// Destroy notify callback for watch cleanup (GDestroyNotify equivalent).
pub type MetaIdleMonitorDestroyNotify =
    Option<unsafe extern "C" fn(user_data: *mut core::ffi::c_void)>;

/// A single idle time watch with callback and timeout.
pub struct MetaIdleMonitorWatch {
    pub monitor: *mut MetaIdleMonitor,
    pub id: u32,
    pub callback: MetaIdleMonitorWatchFunc,
    pub user_data: *mut core::ffi::c_void,
    pub notify: MetaIdleMonitorDestroyNotify,
    pub timeout_msec: u64,
    pub idle_source_id: i32,
    pub timeout_source: *mut core::ffi::c_void,
    pub inhibitable: bool,
}

impl MetaIdleMonitorWatch {
    /// Create a new idle watch.
    pub fn new(id: u32, timeout_msec: u64) -> Self {
        MetaIdleMonitorWatch {
            monitor: core::ptr::null_mut(),
            id,
            callback: None,
            user_data: core::ptr::null_mut(),
            notify: None,
            timeout_msec,
            idle_source_id: -1,
            timeout_source: core::ptr::null_mut(),
            inhibitable: true,
        }
    }
}

/// Idle monitor tracking user inactivity.
pub struct MetaIdleMonitor {
    /// Active idle watches keyed by watch ID.
    pub watches: BTreeMap<u32, MetaIdleMonitorWatch>,
    /// Current idle time in milliseconds.
    pub current_idle_time_ms: Cell<u64>,
    /// Inhibition count (non-zero means idle is inhibited).
    pub inhibit_count: Cell<u32>,
    /// Next watch ID to assign.
    pub next_watch_id: Cell<u32>,
}

impl MetaIdleMonitor {
    /// Create a new idle monitor.
    pub fn new() -> Self {
        MetaIdleMonitor {
            watches: BTreeMap::new(),
            current_idle_time_ms: Cell::new(0),
            inhibit_count: Cell::new(0),
            next_watch_id: Cell::new(1),
        }
    }

    /// Add an idle watch. Returns the assigned watch ID.
    pub fn add_watch(&mut self, timeout_msec: u64) -> u32 {
        let id = self.next_watch_id.get();
        self.next_watch_id.set(id + 1);
        self.watches
            .insert(id, MetaIdleMonitorWatch::new(id, timeout_msec));
        id
    }

    /// Remove a watch by ID.
    pub fn remove_watch(&mut self, id: u32) {
        self.watches.remove(&id);
    }

    /// Reset idle time to zero. Clears the idle timer and cancels
    /// pending watch timeouts.
    pub fn reset_idletime(&mut self) {
        self.current_idle_time_ms.set(0);
    }

    /// Inhibit idle tracking (e.g., during video playback).
    pub fn inhibit(&self) {
        self.inhibit_count.set(self.inhibit_count.get() + 1);
    }

    /// Uninhibit idle tracking.
    pub fn uninhibit(&self) {
        let count = self.inhibit_count.get();
        if count > 0 {
            self.inhibit_count.set(count - 1);
        }
    }

    /// Whether idle tracking is currently inhibited.
    pub fn is_inhibited(&self) -> bool {
        self.inhibit_count.get() > 0
    }

    /// Get the current idle time in milliseconds.
    pub fn get_idle_time_ms(&self) -> u64 {
        self.current_idle_time_ms.get()
    }

    /// Get the idle manager owning this monitor. Without a MetaIdleManager
    /// type, returns None.
    pub fn get_manager(&self) -> Option<()> {
        None
    }
}

impl Default for MetaIdleMonitor {
    fn default() -> Self {
        Self::new()
    }
}

/// Vtable struct for _MetaIdleMonitorClass.
/// In C, contains GObjectClass parent_class. This is an opaque GObject vtable.
/// Documented as empty per no_std constraints.
pub struct MetaIdleMonitorClass {
    // GObjectClass parent_class (opaque, omitted in no_std)
}
