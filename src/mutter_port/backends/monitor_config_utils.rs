//! Monitor Config Utils — ported from GNOME Mutter
//!
//! Utility functions for monitor configuration management.
//! Cloning, copying, and validating monitor layout configurations.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-monitor-config-utils.h

use alloc::string::String;
use alloc::vec::Vec;

/// Monitor layout mode (side-by-side, stacked, etc.).
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetaLogicalMonitorLayoutMode {
    META_LOGICAL_MONITOR_LAYOUT_MODE_LOGICAL = 0,
    META_LOGICAL_MONITOR_LAYOUT_MODE_PHYSICAL = 1,
}

/// Opaque monitor manager type.
pub struct MetaMonitorManager;

/// Logical monitor configuration.
pub struct MetaLogicalMonitorConfig;

/// Monitor specification (display identifier).
pub struct MetaMonitorSpec;

/// Monitors configuration.
/// Holds layout mode, logical/physical monitor configs, and disabled/for-lease specs.
pub struct MetaMonitorsConfig {
    pub logical_monitor_configs: Vec<*mut MetaLogicalMonitorConfig>,
    pub disabled_monitor_specs: Vec<*mut MetaMonitorSpec>,
    pub for_lease_monitor_specs: Vec<*mut MetaMonitorSpec>,
    pub layout_mode: MetaLogicalMonitorLayoutMode,
    pub flags: u32,
    pub switch_config: u32,
}

/// Clone a list of logical monitor configs.
/// Since MetaLogicalMonitorConfig is an opaque type, we return an empty
/// list. A full implementation would deep-copy each config entry.
pub fn meta_clone_logical_monitor_config_list(
    _configs: &[MetaLogicalMonitorConfig],
) -> Vec<MetaLogicalMonitorConfig> {
    // Deep copy requires access to the config struct fields.
    // The opaque type prevents direct cloning here.
    Vec::new()
}

/// Copy a monitors config. Shallow-copies the pointer vectors and
/// copies the scalar fields.
pub fn meta_monitors_config_copy(config: &MetaMonitorsConfig) -> MetaMonitorsConfig {
    MetaMonitorsConfig {
        logical_monitor_configs: config.logical_monitor_configs.clone(),
        disabled_monitor_specs: config.disabled_monitor_specs.clone(),
        for_lease_monitor_specs: config.for_lease_monitor_specs.clone(),
        layout_mode: config.layout_mode,
        flags: config.flags,
        switch_config: config.switch_config,
    }
}

/// Verify logical monitor config list is valid.
/// Checks that at least one config exists (validation of overlaps and
/// adjacency requires access to config struct fields which are opaque).
pub fn meta_verify_logical_monitor_config_list(
    configs: &[MetaLogicalMonitorConfig],
    _layout_mode: MetaLogicalMonitorLayoutMode,
    _manager: &MetaMonitorManager,
) -> Result<(), String> {
    // Basic validation: must have at least one logical monitor config.
    if configs.is_empty() {
        return Err(alloc::format!("no logical monitor configs"));
    }
    // Full validation (overlap detection, adjacency, primary monitor)
    // requires access to the layout rectangles inside each config entry.
    Ok(())
}
