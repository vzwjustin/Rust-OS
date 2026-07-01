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
    /// Create a new monitor config with default values.
    pub fn _new(&self) -> Self {
        Self {
            enable_underscanning: false,
            has_max_bpc: false,
            max_bpc: 0,
            rgb_range: self.rgb_range,
            color_mode: self.color_mode,
        }
    }

    /// Get the config store reference (none without persistent storage).
    pub fn _get_store(&self) -> Option<()> {
        None
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
    /// Create a new logical monitor config with default values.
    pub fn _new(&self) -> Self {
        Self {
            layout: self.layout,
            transform: self.transform,
            scale: self.scale,
            is_primary: false,
            is_presentation: false,
        }
    }

    /// Get the config store reference (none without persistent storage).
    pub fn _get_store(&self) -> Option<()> {
        None
    }
}

/// MetaMonitorsConfigKey
#[derive(Debug, Clone)]
pub struct MetaMonitorsConfigKey {
    pub layout_mode: MetaLogicalMonitorLayoutMode,
}

impl MetaMonitorsConfigKey {
    /// Create a new monitors config key with the same layout mode.
    pub fn _new(&self) -> Self {
        Self {
            layout_mode: self.layout_mode,
        }
    }

    /// Get the config store reference (none without persistent storage).
    pub fn _get_store(&self) -> Option<()> {
        None
    }
}
