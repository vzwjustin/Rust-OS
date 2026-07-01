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
        MtkRectangle { x, y, width, height }
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
pub struct MetaLogicalMonitorId;

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
        }
    }

    /// Add a monitor to this logical monitor.
    pub fn add_monitor(&mut self, _monitor: &MetaMonitor) {
        // TODO: add to monitors list
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
    pub fn has_neighbor(&self, _neighbor: &MetaLogicalMonitor, _dir: u32) -> bool {
        // TODO: check spatial adjacency
        false
    }

    /// Get monitor manager.
    pub fn get_monitor_manager(&self) -> Option<&MetaMonitorManager> {
        // TODO: return manager reference
        None
    }

    /// Get logical monitor ID.
    pub fn get_id(&self) -> Option<&MetaLogicalMonitorId> {
        // TODO: return id reference
        None
    }

    /// Duplicate logical monitor ID.
    pub fn dup_id(&self) -> Option<MetaLogicalMonitorId> {
        // TODO: return duplicated id
        None
    }

    /// Update from new config.
    pub fn update(&mut self, _config: &MetaLogicalMonitorConfig, number: i32) -> bool {
        self.number = number;
        // TODO: apply config changes
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
    /// Free a logical monitor ID.
    pub fn free(&mut self) {
        // TODO: cleanup
    }

    /// Duplicate a logical monitor ID.
    pub fn dup(&self) -> MetaLogicalMonitorId {
        // TODO: copy
        MetaLogicalMonitorId
    }

    /// Check if two IDs are equal.
    pub fn equal(&self, _other: &MetaLogicalMonitorId) -> bool {
        // TODO: compare
        false
    }
}