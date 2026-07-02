//! Logical Monitor Private — ported from GNOME Mutter
//!
//! Represents a logical (desktop) monitor combining one or more physical outputs.
//! Manages transform, scale, primary/presentation state, and spatial layout.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-logical-monitor-private.h

use alloc::vec::Vec;

/// Rectangle with integer coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MtkRectangle {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl MtkRectangle {
    /// Create a new rectangle.
    pub fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        MtkRectangle {
            x,
            y,
            width,
            height,
        }
    }
}

/// Monitor transform (rotation/flip).
pub type MtkMonitorTransform = u32;

/// Opaque monitor type.
pub struct MetaMonitor;

/// Opaque monitor manager type.
pub struct MetaMonitorManager;

/// Opaque output type.
pub struct MetaOutput;

/// Opaque CRTC type.
pub struct MetaCrtc;

/// Opaque logical monitor ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MetaLogicalMonitorId(u32);

/// Logical monitor configuration.
pub struct MetaLogicalMonitorConfig;

/// Desktop-visible logical monitor combining physical outputs.
pub struct MetaLogicalMonitor {
    pub number: i32,
    pub rect: MtkRectangle,
    pub is_primary: bool,
    pub is_presentation: bool,
    pub in_fullscreen: bool,
    pub scale: f32,
    pub transform: MtkMonitorTransform,
    pub monitors: Vec<*const MetaMonitor>,
    /// Unique identifier for this logical monitor.
    id: Option<MetaLogicalMonitorId>,
    /// Monitor manager that owns this logical monitor.
    monitor_manager: *const MetaMonitorManager,
}

impl MetaLogicalMonitor {
    /// Create a new logical monitor.
    pub fn new(number: i32) -> Self {
        MetaLogicalMonitor {
            number,
            rect: MtkRectangle::new(0, 0, 0, 0),
            is_primary: false,
            is_presentation: false,
            in_fullscreen: false,
            scale: 1.0,
            transform: 0,
            monitors: Vec::new(),
            id: None,
            monitor_manager: core::ptr::null(),
        }
    }

    /// Add a monitor to this logical monitor.
    pub fn add_monitor(&mut self, monitor: &MetaMonitor) {
        let ptr = monitor as *const MetaMonitor;
        self.monitors.retain(|&m| m != ptr);
        self.monitors.push(ptr);
    }

    /// Check if this is the primary logical monitor.
    pub fn is_primary(&self) -> bool {
        self.is_primary
    }

    /// Mark this as the primary logical monitor.
    pub fn make_primary(&mut self) {
        self.is_primary = true;
    }

    /// Get the display scale factor.
    pub fn get_scale(&self) -> f32 {
        self.scale
    }

    /// Get the monitor transform.
    pub fn get_transform(&self) -> MtkMonitorTransform {
        self.transform
    }

    /// Get the layout rectangle.
    pub fn get_layout(&self) -> MtkRectangle {
        self.rect
    }

    /// Check if monitor is a neighbor in the given direction.
    /// Direction: 1=left, 2=right, 3=up, 4=down.
    /// Checks spatial adjacency based on layout rectangles.
    pub fn has_neighbor(&self, neighbor: &MetaLogicalMonitor, dir: u32) -> bool {
        let r = &self.rect;
        let n = &neighbor.rect;
        match dir {
            1 => n.x + n.width == r.x,  // neighbor is to the left
            2 => r.x + r.width == n.x,  // neighbor is to the right
            3 => n.y + n.height == r.y, // neighbor is above
            4 => r.y + r.height == n.y, // neighbor is below
            _ => false,
        }
    }

    /// Set the monitor manager pointer.
    pub fn set_monitor_manager(&mut self, manager: *const MetaMonitorManager) {
        self.monitor_manager = manager;
    }

    /// Get monitor manager.
    pub fn get_monitor_manager(&self) -> Option<&MetaMonitorManager> {
        if self.monitor_manager.is_null() {
            None
        } else {
            // SAFETY: The pointer was set by `set_monitor_manager` with
            // a valid reference. The caller guarantees the referent
            // outlives this borrow.
            unsafe { Some(&*self.monitor_manager) }
        }
    }

    /// Set the logical monitor ID.
    pub fn set_id(&mut self, id: MetaLogicalMonitorId) {
        self.id = Some(id);
    }

    /// Get logical monitor ID.
    pub fn get_id(&self) -> Option<&MetaLogicalMonitorId> {
        self.id.as_ref()
    }

    /// Duplicate logical monitor ID.
    pub fn dup_id(&self) -> Option<MetaLogicalMonitorId> {
        self.id
    }

    /// Update from new config. Updates the monitor number; a full
    /// implementation would apply geometry, scale, and transform from
    /// the config struct.
    pub fn update(&mut self, _config: &MetaLogicalMonitorConfig, number: i32) -> bool {
        self.number = number;
        true
    }
}

impl Default for MetaLogicalMonitor {
    fn default() -> Self {
        Self::new(0)
    }
}

/// Logical monitor ID management.
impl MetaLogicalMonitorId {
    /// Create a new logical monitor ID.
    pub fn new(value: u32) -> Self {
        Self(value)
    }

    /// Free a logical monitor ID. Since the ID is a Copy type, this
    /// is a no-op (no heap allocation to free).
    pub fn free(&mut self) {
        // No-op: ID is a stack-allocated Copy type.
    }

    /// Duplicate a logical monitor ID.
    pub fn dup(&self) -> MetaLogicalMonitorId {
        *self
    }

    /// Check if two IDs are equal.
    pub fn equal(&self, other: &MetaLogicalMonitorId) -> bool {
        self == other
    }
}
