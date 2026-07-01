//! Wayland Tablet Pad protocol implementation.
//!
//! Ported from: meta-wayland-tablet-pad.c/h
//!
//! Implements the zwp_tablet_pad_v2 protocol, representing a tablet pad input device
//! with buttons, dials, and strips. Manages pad focus, button events, and
//! ring/strip feedback.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-tablet-pad.h

use alloc::vec::Vec;

/// Tablet pad button event type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum TabletPadButtonState {
    PRESSED = 0,
    RELEASED = 1,
}

/// Ring/strip feedback event type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum TabletPadFeedback {
    BUTTON_PRESS = 0,
    BUTTON_RELEASE = 1,
    STRIP_START = 2,
    STRIP_STOP = 3,
    RING_START = 4,
    RING_STOP = 5,
}

/// A tablet pad input device.
///
/// Represents a digitizer pad (typically paired with a tablet stylus).
/// Holds pad state, button mappings, focus surface, and protocol resources.
#[derive(Debug)]
pub struct MetaWaylandTabletPad {
    pub tablet_seat: Option<*mut core::ffi::c_void>, // MetaWaylandTabletSeat pointer
    pub device: Option<*mut core::ffi::c_void>,      // ClutterInputDevice pointer
    pub resource_list: Vec<*mut core::ffi::c_void>,
    pub focus_resource_list: Vec<*mut core::ffi::c_void>,
    pub focus_surface: Option<*mut core::ffi::c_void>, // MetaWaylandSurface pointer
    pub focus_serial: u32,
    pub button_count: u32,
    pub ring_count: u32,
    pub strip_count: u32,
    pub group_count: u32,
    /// Current active mode for the pad (per zwp_tablet_pad_v2.mode).
    pub mode: u32,
    /// Index of the currently active button group.
    pub current_group: u32,
}

impl MetaWaylandTabletPad {
    pub fn new() -> Self {
        MetaWaylandTabletPad {
            tablet_seat: None,
            device: None,
            resource_list: Vec::new(),
            focus_resource_list: Vec::new(),
            focus_surface: None,
            focus_serial: 0,
            button_count: 0,
            ring_count: 0,
            strip_count: 0,
            group_count: 0,
            mode: 0,
            current_group: 0,
        }
    }

    pub fn get_button_count(&self) -> u32 {
        self.button_count
    }

    pub fn set_button_count(&mut self, count: u32) {
        self.button_count = count;
    }

    /// Get the number of rings on this pad.
    pub fn get_ring_count(&self) -> u32 {
        self.ring_count
    }

    /// Set the number of rings on this pad.
    pub fn set_ring_count(&mut self, count: u32) {
        self.ring_count = count;
    }

    /// Get the number of strips on this pad.
    pub fn get_strip_count(&self) -> u32 {
        self.strip_count
    }

    /// Set the number of strips on this pad.
    pub fn set_strip_count(&mut self, count: u32) {
        self.strip_count = count;
    }

    /// Get the number of button groups on this pad.
    pub fn get_group_count(&self) -> u32 {
        self.group_count
    }

    /// Set the number of button groups on this pad.
    pub fn set_group_count(&mut self, count: u32) {
        self.group_count = count;
    }

    pub fn has_ring(&self) -> bool {
        self.ring_count > 0
    }

    pub fn has_strip(&self) -> bool {
        self.strip_count > 0
    }

    /// Get the current active mode for the pad.
    pub fn get_mode(&self) -> u32 {
        self.mode
    }

    /// Set the current active mode for the pad.
    /// A full implementation would emit zwp_tablet_pad_v2.mode events
    /// to all bound resources with the serial of the triggering event.
    pub fn set_mode(&mut self, mode: u32) {
        self.mode = mode;
    }

    /// Get the index of the currently active button group.
    pub fn get_current_group(&self) -> u32 {
        self.current_group
    }

    /// Set the currently active button group.
    pub fn set_current_group(&mut self, group: u32) {
        self.current_group = group;
    }
}

impl Default for MetaWaylandTabletPad {
    fn default() -> Self {
        Self::new()
    }
}
