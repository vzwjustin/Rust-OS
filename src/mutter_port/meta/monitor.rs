//! Mutter monitor/output management
//! Ported from meta/meta-monitor*.h
use alloc::{format, string::String, vec::Vec};

use crate::mutter_port::meta::types::*;
use crate::mutter_port::mtk::MtkRectangle;

/// Represents a monitor/output device (physical display with resolution, position, and properties)
pub struct MetaMonitor {
    pub index: u32,
    pub name: Option<String>,
    pub geometry: MtkRectangle,
    pub work_area: MtkRectangle,
    pub is_primary: bool,
    pub is_connected: bool,
    pub refresh_rate: f32,
    pub physical_width: u32,
    pub physical_height: u32,
}

impl MetaMonitor {
    pub fn new(index: u32) -> Self {
        Self {
            index,
            name: None,
            geometry: MtkRectangle::default(),
            work_area: MtkRectangle::default(),
            is_primary: false,
            is_connected: false,
            refresh_rate: 60.0,
            physical_width: 0,
            physical_height: 0,
        }
    }

    /// Get monitor name
    pub fn get_name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Get monitor geometry
    pub fn get_geometry(&self) -> MtkRectangle {
        self.geometry
    }

    /// Get workarea (minus panels)
    pub fn get_work_area(&self) -> MtkRectangle {
        self.work_area
    }

    /// Check if monitor is primary
    pub fn is_primary(&self) -> bool {
        self.is_primary
    }

    /// Check if monitor is connected
    pub fn is_connected(&self) -> bool {
        self.is_connected
    }

    /// Get refresh rate
    pub fn get_refresh_rate(&self) -> f32 {
        self.refresh_rate
    }

    /// Get physical dimensions in mm
    pub fn get_physical_width(&self) -> u32 {
        self.physical_width
    }

    pub fn get_physical_height(&self) -> u32 {
        self.physical_height
    }
}

impl Default for MetaMonitor {
    fn default() -> Self {
        Self::new(0)
    }
}

/// Logical monitor grouping physical monitors (display with consistent scale and orientation)
pub struct MetaLogicalMonitor {
    pub index: u32,
    pub geometry: MtkRectangle,
    pub scale: i32,
    pub monitors: Vec<u32>, // indices into physical monitors
    pub is_primary: bool,
}

impl MetaLogicalMonitor {
    pub fn new(index: u32) -> Self {
        Self {
            index,
            geometry: MtkRectangle::default(),
            scale: 1,
            monitors: Vec::new(),
            is_primary: false,
        }
    }

    /// Get logical monitor geometry
    pub fn get_geometry(&self) -> MtkRectangle {
        self.geometry
    }

    /// Get monitor scale factor
    pub fn get_scale(&self) -> i32 {
        self.scale
    }

    /// Get all physical monitors in this logical monitor
    pub fn get_monitors(&self) -> Vec<&MetaMonitor> {
        Vec::new()
    }

    /// Check if monitor is primary
    pub fn is_primary(&self) -> bool {
        self.is_primary
    }
}

impl Default for MetaLogicalMonitor {
    fn default() -> Self {
        Self::new(0)
    }
}

/// Manages monitors and display configuration (collection of physical and logical monitors)
pub struct MetaMonitorManager {
    pub monitors: Vec<MetaMonitor>,
    pub logical_monitors: Vec<MetaLogicalMonitor>,
    pub primary_monitor_index: Option<u32>,
}

impl MetaMonitorManager {
    pub fn new() -> Self {
        Self {
            monitors: Vec::new(),
            logical_monitors: Vec::new(),
            primary_monitor_index: None,
        }
    }

    /// Get all monitors
    pub fn get_monitors(&self) -> Vec<&MetaMonitor> {
        self.monitors.iter().collect()
    }

    /// Get all logical monitors
    pub fn get_logical_monitors(&self) -> Vec<&MetaLogicalMonitor> {
        self.logical_monitors.iter().collect()
    }

    /// Get primary monitor
    pub fn get_primary_monitor(&self) -> Option<&MetaMonitor> {
        self.primary_monitor_index
            .and_then(|idx| self.monitors.iter().find(|m| m.index == idx))
    }

    /// Get monitor by index
    pub fn get_monitor_by_index(&self, index: u32) -> Option<&MetaMonitor> {
        self.monitors.iter().find(|m| m.index == index)
    }

    /// Apply new configuration. Rebuilds logical monitors from the
    /// current physical monitor list: each connected monitor becomes
    /// a logical monitor at scale 1, and the primary monitor is set
    /// to the first connected monitor if not already set.
    pub fn apply_configuration(&mut self) {
        // Rebuild logical monitors from physical monitors.
        self.logical_monitors.clear();
        let mut primary_idx: Option<u32> = None;
        for m in &self.monitors {
            if !m.is_connected {
                continue;
            }
            let mut logical = MetaLogicalMonitor::new(m.index);
            logical.geometry = m.geometry;
            logical.is_primary = m.is_primary;
            logical.monitors.push(m.index);
            if m.is_primary && primary_idx.is_none() {
                primary_idx = Some(m.index);
            }
            self.logical_monitors.push(logical);
        }
        // If no primary was set, default to the first connected monitor.
        if primary_idx.is_none() {
            if let Some(first) = self.monitors.iter().find(|m| m.is_connected) {
                primary_idx = Some(first.index);
                if let Some(ref mut logical) = self.logical_monitors.first_mut() {
                    logical.is_primary = true;
                }
            }
        }
        self.primary_monitor_index = primary_idx;
    }

    /// Get the number of physical monitors known to the manager
    /// (including disconnected ones).
    pub fn get_monitor_count(&self) -> usize {
        self.monitors.len()
    }

    /// Get the number of logical monitors currently configured.
    pub fn get_logical_monitor_count(&self) -> usize {
        self.logical_monitors.len()
    }

    /// Find a physical monitor by its connector name (e.g. "DP-1",
    /// "HDMI-A-1"). Returns the first connected monitor whose `name`
    /// matches `connector`, falling back to disconnected monitors if no
    /// connected match is found.
    pub fn find_monitor_by_connector(&self, connector: &str) -> Option<&MetaMonitor> {
        // Prefer a connected monitor with the requested connector name.
        if let Some(m) = self
            .monitors
            .iter()
            .find(|m| m.is_connected && m.name.as_deref() == Some(connector))
        {
            return Some(m);
        }
        // Fall back to any monitor (connected or not) with that name.
        self.monitors
            .iter()
            .find(|m| m.name.as_deref() == Some(connector))
    }

    /// Find a mutable physical monitor by its connector name.
    pub fn find_monitor_by_connector_mut(&mut self, connector: &str) -> Option<&mut MetaMonitor> {
        self.monitors
            .iter_mut()
            .find(|m| m.name.as_deref() == Some(connector))
    }

    /// Get the number of currently connected physical monitors.
    pub fn get_connected_monitor_count(&self) -> usize {
        self.monitors.iter().filter(|m| m.is_connected).count()
    }

    /// Get the logical monitor that owns a given physical monitor index.
    pub fn get_logical_monitor_for_index(&self, index: u32) -> Option<&MetaLogicalMonitor> {
        self.logical_monitors
            .iter()
            .find(|lm| lm.monitors.contains(&index))
    }
}

impl Default for MetaMonitorManager {
    fn default() -> Self {
        Self::new()
    }
}
