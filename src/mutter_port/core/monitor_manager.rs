//! Base MetaMonitorManager ported from GNOME Mutter's src/core/meta-monitor-manager.c
//!
//! MetaMonitorManager is the central authority on monitor topology. It owns
//! the list of logical monitors, tracks the current monitor configuration
//! (layout, scale, primary, presentation), and drives monitor reconfiguration
//! when hardware changes are detected.
//!
//! In Mutter this is an abstract GObject class with backend-specific subclasses
//! (MetaMonitorManagerNative, MetaMonitorManagerX11, etc.). Here it is a
//! concrete struct; the native subclass (`backends_native/monitor_manager_native.rs`)
//! wraps this and supplies KMS/DRM-specific behavior.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/core/meta-monitor-manager.c

use alloc::string::String;
use alloc::vec::Vec;

use crate::mutter_port::backends::logical_monitor::{LogicalMonitor, MonitorTransform, MtkRectangle};

/// Monitor configuration mode (mirrors MetaMonitorSwitchConfigType).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MonitorSwitchConfig {
    /// All monitors disabled except the primary.
    ExternalOnly,
    /// All monitors show the same content (mirrored).
    Mirror,
    /// Docked: external monitor on, internal off.
    Docked,
    /// Internal monitor on, external off.
    InternalOnly,
    /// No special switch config; use the stored configuration.
    None,
}

impl Default for MonitorSwitchConfig {
    fn default() -> Self {
        MonitorSwitchConfig::None
    }
}

/// The current monitor configuration state. Mirrors the subset of
/// MetaMonitorManager that is backend-independent.
#[derive(Debug)]
pub struct MetaMonitorManager {
    /// All logical monitors currently active.
    logical_monitors: Vec<LogicalMonitor>,
    /// Index of the primary logical monitor.
    primary_index: usize,
    /// The global stage size (bounding box of all monitors).
    stage_size: MtkRectangle,
    /// Whether the monitor configuration is current (not stale).
    config_current: bool,
    /// Current switch config (lid switch / hotplug-driven).
    switch_config: MonitorSwitchConfig,
    /// Whether a lid is present (laptop).
    has_lid: bool,
    /// Whether the lid is currently closed.
    lid_is_closed: bool,
    /// The current global scaling factor (from settings).
    global_scale: f32,
    /// Whether the layout mode is logical (vs physical).
    layout_mode: LayoutMode,
    /// Pending monitor config change notification flag.
    monitors_changed: bool,
}

/// Layout mode, mirrors MetaLogicalMonitorLayoutMode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutMode {
    /// Physical layout: coordinates in physical pixels.
    Physical,
    /// Logical layout: coordinates in logical (scaled) pixels.
    Logical,
}

impl Default for LayoutMode {
    fn default() -> Self {
        LayoutMode::Physical
    }
}

impl MetaMonitorManager {
    /// Create a new monitor manager with no monitors. Mirrors
    /// meta_monitor_manager_new().
    pub fn new() -> Self {
        MetaMonitorManager {
            logical_monitors: Vec::new(),
            primary_index: 0,
            stage_size: MtkRectangle::new(0, 0, 0, 0),
            config_current: true,
            switch_config: MonitorSwitchConfig::default(),
            has_lid: false,
            lid_is_closed: false,
            global_scale: 1.0,
            layout_mode: LayoutMode::default(),
            monitors_changed: false,
        }
    }

    // ── Query API ─────────────────────────────────────────────────────

    /// Get all logical monitors.
    pub fn get_logical_monitors(&self) -> &[LogicalMonitor] {
        &self.logical_monitors
    }

    /// Get the primary logical monitor, if any.
    pub fn get_primary_monitor(&self) -> Option<&LogicalMonitor> {
        self.logical_monitors.get(self.primary_index)
    }

    /// Get the primary logical monitor index.
    pub fn get_primary_monitor_index(&self) -> usize {
        self.primary_index
    }

    /// Get the logical monitor at the given index.
    pub fn get_monitor(&self, index: usize) -> Option<&LogicalMonitor> {
        self.logical_monitors.get(index)
    }

    /// Get mutable reference to the logical monitor at the given index.
    pub fn get_monitor_mut(&mut self, index: usize) -> Option<&mut LogicalMonitor> {
        self.logical_monitors.get_mut(index)
    }

    /// Number of logical monitors.
    pub fn get_num_monitors(&self) -> usize {
        self.logical_monitors.len()
    }

    /// Get the stage size (bounding box of all monitors).
    pub fn get_stage_size(&self) -> MtkRectangle {
        self.stage_size
    }

    /// Whether the configuration is current.
    pub fn is_config_current(&self) -> bool {
        self.config_current
    }

    /// Get the current switch config.
    pub fn get_switch_config(&self) -> MonitorSwitchConfig {
        self.switch_config
    }

    /// Set the switch config (e.g. when lid opens/closes).
    pub fn set_switch_config(&mut self, config: MonitorSwitchConfig) {
        self.switch_config = config;
    }

    /// Whether a lid is present.
    pub fn has_lid(&self) -> bool {
        self.has_lid
    }

    pub fn set_has_lid(&mut self, has_lid: bool) {
        self.has_lid = has_lid;
    }

    /// Whether the lid is closed.
    pub fn lid_is_closed(&self) -> bool {
        self.lid_is_closed
    }

    pub fn set_lid_is_closed(&mut self, closed: bool) {
        self.lid_is_closed = closed;
    }

    /// Get the global scaling factor.
    pub fn get_global_scale(&self) -> f32 {
        self.global_scale
    }

    pub fn set_global_scale(&mut self, scale: f32) {
        self.global_scale = scale;
    }

    /// Get the layout mode.
    pub fn get_layout_mode(&self) -> LayoutMode {
        self.layout_mode
    }

    pub fn set_layout_mode(&mut self, mode: LayoutMode) {
        self.layout_mode = mode;
    }

    // ── Configuration API ─────────────────────────────────────────────

    /// Set the logical monitors and recompute the stage size. Mirrors
    /// meta_monitor_manager_read_current_state() which populates the
    /// logical_monitors list and computes the bounding box.
    pub fn set_logical_monitors(&mut self, monitors: Vec<LogicalMonitor>) {
        self.logical_monitors = monitors;
        self.recompute_stage_size();
        self.config_current = true;
        self.monitors_changed = true;
    }

    /// Replace all logical monitors (used by backend-specific read_current).
    pub fn take_logical_monitors(&mut self, monitors: Vec<LogicalMonitor>) {
        self.logical_monitors = monitors;
        self.recompute_stage_size();
    }

    /// Set the primary monitor by index.
    pub fn set_primary_monitor(&mut self, index: usize) -> bool {
        if index >= self.logical_monitors.len() {
            return false;
        }
        // Clear previous primary.
        for m in &mut self.logical_monitors {
            m.is_primary = false;
        }
        self.logical_monitors[index].is_primary = true;
        self.primary_index = index;
        true
    }

    /// Add a logical monitor.
    pub fn add_logical_monitor(&mut self, monitor: LogicalMonitor) {
        self.logical_monitors.push(monitor);
        self.recompute_stage_size();
        self.monitors_changed = true;
    }

    /// Remove a logical monitor by index.
    pub fn remove_logical_monitor(&mut self, index: usize) -> bool {
        if index >= self.logical_monitors.len() {
            return false;
        }
        self.logical_monitors.remove(index);
        if self.primary_index >= self.logical_monitors.len() && !self.logical_monitors.is_empty() {
            self.primary_index = 0;
            self.logical_monitors[0].is_primary = true;
        }
        self.recompute_stage_size();
        self.monitors_changed = true;
        true
    }

    // ── Internal helpers ──────────────────────────────────────────────

    /// Recompute the stage size as the bounding box of all logical monitors.
    /// Mirrors the layout recalculation in meta_monitor_manager_read_current_state().
    fn recompute_stage_size(&mut self) {
        if self.logical_monitors.is_empty() {
            self.stage_size = MtkRectangle::new(0, 0, 0, 0);
            return;
        }
        let mut min_x = i32::MAX;
        let mut min_y = i32::MAX;
        let mut max_x = i32::MIN;
        let mut max_y = i32::MIN;
        for m in &self.logical_monitors {
            min_x = min_x.min(m.rect.x);
            min_y = min_y.min(m.rect.y);
            max_x = max_x.max(m.rect.x + m.rect.width);
            max_y = max_y.max(m.rect.y + m.rect.height);
        }
        self.stage_size = MtkRectangle::new(min_x, min_y, max_x - min_x, max_y - min_y);
    }

    // ── Signal handling ───────────────────────────────────────────────

    /// Called by the backend when monitors have changed. Marks the config
    /// as stale so the backend will re-read it.
    pub fn on_monitors_changed(&mut self) {
        self.config_current = false;
        self.monitors_changed = true;
    }

    /// Whether a monitors-changed notification is pending.
    pub fn take_monitors_changed(&mut self) -> bool {
        let changed = self.monitors_changed;
        self.monitors_changed = false;
        changed
    }

    // ── Lookups ───────────────────────────────────────────────────────

    /// Find the logical monitor that contains the point (x, y).
    /// Mirrors meta_monitor_manager_get_logical_monitor_at().
    pub fn get_monitor_at(&self, x: i32, y: i32) -> Option<usize> {
        self.logical_monitors.iter().position(|m| {
            x >= m.rect.x
                && x < m.rect.x + m.rect.width
                && y >= m.rect.y
                && y < m.rect.y + m.rect.height
        })
    }

    /// Find the logical monitor whose rectangle is closest to the given
    /// point. Mirrors meta_monitor_manager_get_logical_monitor_neighbor().
    pub fn get_monitor_in_direction(
        &self,
        source_index: usize,
        direction: crate::mutter_port::backends::logical_monitor::DisplayDirection,
    ) -> Option<usize> {
        let source = self.logical_monitors.get(source_index)?;
        for (i, m) in self.logical_monitors.iter().enumerate() {
            if i == source_index {
                continue;
            }
            if source.has_neighbor(m, direction) {
                return Some(i);
            }
        }
        None
    }
}

impl Default for MetaMonitorManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use crate::mutter_port::backends::logical_monitor::{
        LogicalMonitor, Monitor, MonitorTransform, MtkRectangle,
    };
    use super::*;

    fn make_monitor(x: i32, y: i32, w: i32, h: i32, number: i32) -> LogicalMonitor {
        LogicalMonitor::new(
            number,
            1.0,
            MonitorTransform::Normal,
            MtkRectangle::new(x, y, w, h),
            vec![Monitor::new(String::from("DP-1"))],
        )
    }

    #[test]
    fn test_empty_manager() {
        let mgr = MetaMonitorManager::new();
        assert_eq!(mgr.get_num_monitors(), 0);
        assert!(mgr.get_primary_monitor().is_none());
        assert_eq!(mgr.get_stage_size(), MtkRectangle::new(0, 0, 0, 0));
    }

    #[test]
    fn test_add_monitors() {
        let mut mgr = MetaMonitorManager::new();
        mgr.add_logical_monitor(make_monitor(0, 0, 1920, 1080, 0));
        mgr.add_logical_monitor(make_monitor(1920, 0, 1920, 1080, 1));

        assert_eq!(mgr.get_num_monitors(), 2);
        assert_eq!(mgr.get_stage_size(), MtkRectangle::new(0, 0, 3840, 1080));
    }

    #[test]
    fn test_set_primary() {
        let mut mgr = MetaMonitorManager::new();
        mgr.add_logical_monitor(make_monitor(0, 0, 1920, 1080, 0));
        mgr.add_logical_monitor(make_monitor(1920, 0, 1920, 1080, 1));

        assert!(mgr.set_primary_monitor(1));
        assert!(mgr.get_monitor(1).unwrap().is_primary());
        assert!(!mgr.get_monitor(0).unwrap().is_primary());
    }

    #[test]
    fn test_monitor_at_point() {
        let mut mgr = MetaMonitorManager::new();
        mgr.add_logical_monitor(make_monitor(0, 0, 1920, 1080, 0));
        mgr.add_logical_monitor(make_monitor(1920, 0, 1920, 1080, 1));

        assert_eq!(mgr.get_monitor_at(500, 500), Some(0));
        assert_eq!(mgr.get_monitor_at(2000, 500), Some(1));
        assert_eq!(mgr.get_monitor_at(5000, 5000), None);
    }

    #[test]
    fn test_remove_monitor() {
        let mut mgr = MetaMonitorManager::new();
        mgr.add_logical_monitor(make_monitor(0, 0, 1920, 1080, 0));
        mgr.add_logical_monitor(make_monitor(1920, 0, 1920, 1080, 1));

        assert!(mgr.remove_logical_monitor(1));
        assert_eq!(mgr.get_num_monitors(), 1);
        assert_eq!(mgr.get_stage_size(), MtkRectangle::new(0, 0, 1920, 1080));
    }

    #[test]
    fn test_monitors_changed_flag() {
        let mut mgr = MetaMonitorManager::new();
        mgr.add_logical_monitor(make_monitor(0, 0, 1920, 1080, 0));
        assert!(mgr.take_monitors_changed());

        mgr.on_monitors_changed();
        assert!(mgr.take_monitors_changed());
        assert!(!mgr.take_monitors_changed());
    }
}
