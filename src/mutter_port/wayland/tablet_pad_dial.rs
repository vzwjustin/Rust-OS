//! Wayland Tablet Pad Dial module
//!
//! Represents a rotary dial control on a Wayland tablet pad device.
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-tablet-pad-dial.h

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::ffi::c_void;

/// Tablet pad dial control for multi-touch tablet pad devices.
pub struct MetaWaylandTabletPadDial {
    /// Parent tablet pad
    pub pad: Option<*mut c_void>,
    /// Parent dial group
    pub group: Option<*mut c_void>,
    /// List of bound resource handles
    pub resource_list: Vec<*mut c_void>,
    /// List of focused client resource handles
    pub focus_resource_list: Vec<*mut c_void>,
    /// Feedback string for dial
    pub feedback: Option<String>,
}

impl MetaWaylandTabletPadDial {
    /// Create a new tablet pad dial
    /// ponytail: register dial with parent pad; real impl initializes protocol
    pub fn new(pad: *mut c_void) -> Option<*mut c_void> {
        let dial = Box::new(MetaWaylandTabletPadDial {
            pad: Some(pad),
            group: None,
            resource_list: Vec::new(),
            focus_resource_list: Vec::new(),
            feedback: None,
        });
        Some(Box::into_raw(dial) as *mut c_void)
    }

    /// Free a tablet pad dial
    /// ponytail: cleanup unregisters protocol; stub just deallocates
    pub fn free(dial: *mut c_void) {
        if !dial.is_null() {
            unsafe {
                let _ = Box::from_raw(dial as *mut MetaWaylandTabletPadDial);
            }
        }
    }

    /// Set the group for this dial
    /// ponytail: associate with pad group; real impl wires group signals
    pub fn set_group(&mut self, group: *mut c_void) {
        self.group = Some(group);
    }

    /// Handle dial event from input device
    /// ponytail: real impl processes rotation and emits client events; stub returns false
    pub fn handle_event(&mut self, _event: *mut c_void) -> bool {
        false
    }

    /// Sync focus state with seat pointer
    /// ponytail: real impl updates focus resources when pointer focus changes
    pub fn sync_focus(&mut self) {
        self.focus_resource_list.clear();
    }
}

impl Default for MetaWaylandTabletPadDial {
    fn default() -> Self {
        Self {
            pad: None,
            group: None,
            resource_list: Vec::new(),
            focus_resource_list: Vec::new(),
            feedback: None,
        }
    }
}
