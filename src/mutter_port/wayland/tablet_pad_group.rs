//! Wayland Tablet Pad Group module
//!
//! Represents a mode group on a tablet pad, with buttons, strips, rings,
//! and dials organized by mode. Handles mode switching and input events.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-tablet-pad-group.h

use alloc::vec::Vec;

/// A logical group of tablet pad controls (buttons, strips, rings, dials)
/// organized by mode. Modes switch via mode-switch buttons.
pub struct MetaWaylandTabletPadGroup {
    pub pad: Option<*mut core::ffi::c_void>, // MetaWaylandTabletPad pointer
    pub n_modes: u32,
    pub current_mode: u32,
    pub resource_list: Vec<*mut core::ffi::c_void>,
    pub focus_resource_list: Vec<*mut core::ffi::c_void>,
    pub mode_switch_serial: u32,
    pub strips: Vec<*mut core::ffi::c_void>, // GList of strips
    pub rings: Vec<*mut core::ffi::c_void>,  // GList of rings
    pub dials: Vec<*mut core::ffi::c_void>,  // GList of dials
}

impl MetaWaylandTabletPadGroup {
    /// Create a new tablet pad group (stub).
    pub fn new() -> Self {
        MetaWaylandTabletPadGroup {
            pad: None,
            n_modes: 1,
            current_mode: 0,
            resource_list: Vec::new(),
            focus_resource_list: Vec::new(),
            mode_switch_serial: 0,
            strips: Vec::new(),
            rings: Vec::new(),
            dials: Vec::new(),
        }
    }
}

impl Default for MetaWaylandTabletPadGroup {
    fn default() -> Self {
        Self::new()
    }
}
