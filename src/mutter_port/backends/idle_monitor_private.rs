//! Idle Monitor Private ported from GNOME Mutter's src/backends/
//!
//! Tracks user idle time with per-watch callbacks. Supports inhibition
//! (e.g., during video playback) and timeout-based idle detection. Watches can be
//! destroyed via destroy notify callbacks and timeout sources.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-idle-monitor-private.h

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
pub type MetaIdleMonitorDestroyNotify = Option<
    unsafe extern "C" fn(user_data: *mut core::ffi::c_void),
>;

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
    // TODO: watches, current_idle_time, inhibit_count fields
}

impl MetaIdleMonitor {
    /// Create a new idle monitor.
    pub fn new() -> Self {
        MetaIdleMonitor {}
    }

    /// Reset idle time to zero.
    pub fn reset_idletime(&mut self) {
        // TODO: Cancel pending timeouts, reset timer
    }

    /// Get the idle manager owning this monitor.
    pub fn get_manager(&self) -> Option<()> {
        // TODO: Return MetaIdleManager reference
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
