//! Wayland Commit Timing — handles presentation timing and frame statistics.
//!
//! Manages commit feedback, sync timing, and frame delivery statistics for
//! Wayland surfaces using the presentation-time protocol.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-commit-timing.h

/// Commit timing state for a surface.
///
/// In the C original, `MetaWaylandCommitTiming` tracks the last commit
/// timestamp, the expected frame interval, and feeds these into the
/// wp_presentation_feedback events so clients can synchronize their
/// rendering pipeline. A full implementation would hook into the DRM
/// backend's vblank and page-flip completion events to populate the
/// timing data.
#[derive(Debug, Clone, Copy)]
pub struct MetaWaylandCommitTiming {
    /// Timestamp of the last committed frame, in nanoseconds since
    /// some compositor epoch (typically CLOCK_MONOTONIC).
    pub last_commit_time: u64,
    /// Expected interval between frames in nanoseconds (derived from
    /// the output refresh rate). 0 means unset.
    pub frame_interval: u64,
    /// Number of frames committed since timing tracking started.
    pub frame_count: u64,
}

impl MetaWaylandCommitTiming {
    /// Create a new commit timing state with zeroed values.
    pub fn new() -> Self {
        MetaWaylandCommitTiming {
            last_commit_time: 0,
            frame_interval: 0,
            frame_count: 0,
        }
    }

    /// Record a new commit at the given timestamp.
    /// Updates the last commit time and increments the frame counter.
    /// A full implementation would also emit wp_presentation_feedback
    /// events with the sync and presentation timestamps.
    pub fn record_commit(&mut self, timestamp_ns: u64) {
        self.last_commit_time = timestamp_ns;
        self.frame_count = self.frame_count.saturating_add(1);
    }

    /// Get the timestamp of the last committed frame.
    pub fn get_last_commit_time(&self) -> u64 {
        self.last_commit_time
    }

    /// Set the expected frame interval in nanoseconds.
    /// Typically computed as 1e9 / refresh_rate_hz.
    pub fn set_frame_interval(&mut self, interval_ns: u64) {
        self.frame_interval = interval_ns;
    }

    /// Get the expected frame interval in nanoseconds.
    pub fn get_frame_interval(&self) -> u64 {
        self.frame_interval
    }

    /// Get the total number of committed frames.
    pub fn get_frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Compute the refresh rate in Hz from the frame interval.
    /// Returns 0.0 if the interval is unset.
    pub fn get_refresh_rate_hz(&self) -> f64 {
        if self.frame_interval == 0 {
            0.0
        } else {
            1_000_000_000.0 / self.frame_interval as f64
        }
    }

    /// Reset all timing state to zero.
    pub fn reset(&mut self) {
        self.last_commit_time = 0;
        self.frame_interval = 0;
        self.frame_count = 0;
    }
}

impl Default for MetaWaylandCommitTiming {
    fn default() -> Self {
        Self::new()
    }
}

/// Initialize commit timing support for the compositor.
///
/// Sets up presentation-time protocol handlers. A full implementation
/// would register the wp_presentation global and hook into the DRM
/// backend's vblank/page-flip completion callbacks to feed timing data
/// to surface commit timing state.
pub fn meta_wayland_commit_timing_init(compositor: *mut core::ffi::c_void) {
    if compositor.is_null() {
        return;
    }
    // Protocol global registration requires libwayland-server.
}
