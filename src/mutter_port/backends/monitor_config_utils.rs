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
pub fn meta_clone_logical_monitor_config_list(
    _configs: &[MetaLogicalMonitorConfig],
) -> Vec<MetaLogicalMonitorConfig> {
    // TODO: deep copy configs
    Vec::new()
}

/// Copy a monitors config.
pub fn meta_monitors_config_copy(config: &MetaMonitorsConfig) -> MetaMonitorsConfig {
    // TODO: deep copy
    MetaMonitorsConfig {
        logical_monitor_configs: Vec::new(),
        disabled_monitor_specs: Vec::new(),
        for_lease_monitor_specs: Vec::new(),
        layout_mode: config.layout_mode,
        flags: config.flags,
        switch_config: config.switch_config,
    }
}

/// Verify logical monitor config list is valid.
pub fn meta_verify_logical_monitor_config_list(
    _configs: &[MetaLogicalMonitorConfig],
    _layout_mode: MetaLogicalMonitorLayoutMode,
    _manager: &MetaMonitorManager,
) -> Result<(), String> {
    // TODO: validate layout constraints
    // - no overlaps
    // - proper gaps/adjacency
    // - at least one primary monitor
    Ok(())
}