//! Idle Monitor Private ported from GNOME Mutter's src/backends/
//!
//! Tracks user idle time with per-watch callbacks. Supports inhibition
//! (e.g., during video playback) and timeout-based idle detection.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-idle-monitor-private.c

/// Watch callback function type. Matches the upstream Mutter signature; the
/// `Option` makes the null callback representable without an untyped sentinel.
pub type MetaIdleMonitorWatchFunc = Option<
    unsafe extern "C" fn(
        monitor: *mut MetaIdleMonitor,
        watch_id: u32,
        user_data: *mut core::ffi::c_void,
    ),
>;

/// A single idle time watch with callback and timeout.
pub struct MetaIdleMonitorWatch {
    pub monitor_id: u32,
    pub watch_id: u32,
    pub callback: MetaIdleMonitorWatchFunc,
    pub timeout_msec: u64,
    pub idle_source_id: i32,
    pub inhibitable: bool,
}

impl MetaIdleMonitorWatch {
    /// Create a new idle watch.
    pub fn new(id: u32, timeout_msec: u64) -> Self {
        MetaIdleMonitorWatch {
            monitor_id: 0,
            watch_id: id,
            callback: None,
            timeout_msec,
            idle_source_id: -1,
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
