//! Stream Monitor — ported from GNOME Mutter
//!
//! MetaStreamMonitor represents a screen capture stream for a single monitor
//! (display output).
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-stream-monitor.h

use super::stream::MetaStream;

/// Logical rectangle describing the monitor's area in stage coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MonitorArea {
    /// X origin of the monitor in stage space.
    pub x: i32,
    /// Y origin of the monitor in stage space.
    pub y: i32,
    /// Width of the monitor in pixels.
    pub width: i32,
    /// Height of the monitor in pixels.
    pub height: i32,
}

impl MonitorArea {
    /// Create a new monitor area.
    pub fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        MonitorArea {
            x,
            y,
            width,
            height,
        }
    }

    /// Check whether the area is empty (zero dimensions).
    pub fn is_empty(&self) -> bool {
        self.width <= 0 || self.height <= 0
    }
}

/// Placeholder for MetaMonitor from monitor manager.
///
/// In upstream Mutter, `MetaMonitor` wraps a logical monitor (a set of
/// outputs tiled or mirrored together) and exposes its geometry, modes
/// and connector list. The full struct requires the monitor manager
/// port; here we track the minimal identifying state needed by the
/// stream monitor: the index into the monitor manager's monitor list
/// and the logical area the stream captures.
pub struct MetaMonitor {
    /// Index of this monitor within the monitor manager's monitor list.
    pub monitor_index: usize,
    /// Logical area occupied by the monitor in stage coordinates.
    pub area: MonitorArea,
    /// Whether the monitor is currently active (enabled and scanned out).
    pub active: bool,
}

impl MetaMonitor {
    /// Create a new monitor placeholder with the given index and area.
    pub fn new(monitor_index: usize, area: MonitorArea) -> Self {
        MetaMonitor {
            monitor_index,
            area,
            active: false,
        }
    }

    /// Mark the monitor as active or inactive.
    pub fn set_active(&mut self, active: bool) {
        self.active = active;
    }

    /// Check whether the monitor is active.
    pub fn is_active(&self) -> bool {
        self.active
    }
}

/// MetaStreamMonitor: Captures a single monitor's output.
/// Extends MetaStream with monitor and logical monitor references for display capture.
pub struct MetaStreamMonitor {
    pub base: MetaStream,
    /// Monitor reference (opaque MetaMonitor*)
    pub monitor: *mut core::ffi::c_void,
    /// Logical monitor reference (opaque MetaLogicalMonitor*)
    pub logical_monitor: *mut core::ffi::c_void,
    /// Index of the captured monitor within the monitor manager.
    pub monitor_index: usize,
    /// Unique identifier for this capture stream.
    pub stream_id: u64,
    /// Logical area of the monitor being captured, in stage coordinates.
    pub area: MonitorArea,
    /// Whether the stream is currently producing frames.
    pub active: bool,
}

impl MetaStreamMonitor {
    /// Create a new, inactive stream monitor.
    pub fn new() -> Self {
        MetaStreamMonitor {
            base: MetaStream::new(),
            monitor: core::ptr::null_mut(),
            logical_monitor: core::ptr::null_mut(),
            monitor_index: 0,
            stream_id: 0,
            area: MonitorArea::new(0, 0, 0, 0),
            active: false,
        }
    }

    /// Configure the stream to capture a specific monitor.
    ///
    /// Stores the monitor index, logical area and assigns a stream id.
    /// The stream is not activated until `set_active(true)` is called.
    pub fn set_monitor(&mut self, monitor_index: usize, stream_id: u64, area: MonitorArea) {
        self.monitor_index = monitor_index;
        self.stream_id = stream_id;
        self.area = area;
    }

    /// Activate or deactivate frame production for this stream.
    pub fn set_active(&mut self, active: bool) {
        self.active = active;
    }

    /// Check whether the stream is currently active.
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Get the captured monitor's index in the monitor manager.
    pub fn get_monitor_index(&self) -> usize {
        self.monitor_index
    }

    /// Get the stream identifier.
    pub fn get_stream_id(&self) -> u64 {
        self.stream_id
    }

    /// Get the captured monitor's logical area.
    pub fn get_area(&self) -> MonitorArea {
        self.area
    }
}

impl Default for MetaStreamMonitor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stream_monitor_defaults() {
        let sm = MetaStreamMonitor::new();
        assert!(!sm.is_active());
        assert_eq!(sm.get_monitor_index(), 0);
        assert_eq!(sm.get_stream_id(), 0);
        assert!(sm.get_area().is_empty());
    }

    #[test]
    fn test_set_monitor_and_activate() {
        let mut sm = MetaStreamMonitor::new();
        sm.set_monitor(2, 12345, MonitorArea::new(0, 0, 1920, 1080));
        assert_eq!(sm.get_monitor_index(), 2);
        assert_eq!(sm.get_stream_id(), 12345);
        assert_eq!(sm.get_area(), MonitorArea::new(0, 0, 1920, 1080));
        assert!(!sm.is_active());
        sm.set_active(true);
        assert!(sm.is_active());
    }

    #[test]
    fn test_meta_monitor_active_state() {
        let mut m = MetaMonitor::new(1, MonitorArea::new(10, 20, 800, 600));
        assert!(!m.is_active());
        m.set_active(true);
        assert!(m.is_active());
    }
}
