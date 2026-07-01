//! Monitor Private — ported from GNOME Mutter
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-monitor-private.h









use crate::mutter_port::backends::common_types::*;
use crate::mutter_port::backends::common_types::*;


use alloc::string::String;

/// MetaMonitorSpec
#[derive(Debug, Clone)]
pub struct MetaMonitorSpec {
    // TODO: Add fields from C struct
}

impl MetaMonitorSpec {
    /// TODO: port logic from meta_monitor_tiled_new
    pub fn monitor_tiled_new(&self) {
        todo!()
    }

    /// TODO: port logic from meta_monitor_normal_new
    pub fn monitor_normal_new(&self) {
        todo!()
    }

}

/// MetaMonitorModeSpec
#[derive(Debug, Clone)]
pub struct MetaMonitorModeSpec {
    pub width: i32,
    pub height: i32,
    pub refresh_rate: f32,
    pub refresh_rate_mode: MetaCrtcRefreshRateMode,
    pub flags: MetaCrtcModeFlag,
}

impl MetaMonitorModeSpec {
    /// TODO: port logic from meta_monitor_tiled_new
    pub fn monitor_tiled_new(&self) {
        todo!()
    }

    /// TODO: port logic from meta_monitor_normal_new
    pub fn monitor_normal_new(&self) {
        todo!()
    }

}

/// MetaMonitorCrtcMode
#[derive(Debug, Clone)]
pub struct MetaMonitorCrtcMode {
    // TODO: Add fields from C struct
}

impl MetaMonitorCrtcMode {
    /// TODO: port logic from meta_monitor_tiled_new
    pub fn monitor_tiled_new(&self) {
        todo!()
    }

    /// TODO: port logic from meta_monitor_normal_new
    pub fn monitor_normal_new(&self) {
        todo!()
    }

}
