//! Monitor Config Manager — ported from GNOME Mutter
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-monitor-config-manager.h









use crate::mutter_port::backends::common_types::*;
use crate::mutter_port::backends::common_types::*;


use alloc::string::String;

/// MetaMonitorConfig
#[derive(Debug, Clone)]
pub struct MetaMonitorConfig {
    pub enable_underscanning: bool,
    pub has_max_bpc: bool,
    pub max_bpc: i32,
    pub rgb_range: MetaOutputRGBRange,
    pub color_mode: MetaColorMode,
}

impl MetaMonitorConfig {
    /// TODO: port logic from meta_monitor_config_manager_new
    pub fn _new(&self) {
        todo!()
    }

    /// TODO: port logic from meta_monitor_config_manager_get_store
    pub fn _get_store(&self) {
        todo!()
    }

}

/// MetaLogicalMonitorConfig
#[derive(Debug, Clone)]
pub struct MetaLogicalMonitorConfig {
    pub layout: MtkRectangle,
    pub transform: MtkMonitorTransform,
    pub scale: f32,
    pub is_primary: bool,
    pub is_presentation: bool,
}

impl MetaLogicalMonitorConfig {
    /// TODO: port logic from meta_monitor_config_manager_new
    pub fn _new(&self) {
        todo!()
    }

    /// TODO: port logic from meta_monitor_config_manager_get_store
    pub fn _get_store(&self) {
        todo!()
    }

}

/// MetaMonitorsConfigKey
#[derive(Debug, Clone)]
pub struct MetaMonitorsConfigKey {
    pub layout_mode: MetaLogicalMonitorLayoutMode,
}

impl MetaMonitorsConfigKey {
    /// TODO: port logic from meta_monitor_config_manager_new
    pub fn _new(&self) {
        todo!()
    }

    /// TODO: port logic from meta_monitor_config_manager_get_store
    pub fn _get_store(&self) {
        todo!()
    }

}
