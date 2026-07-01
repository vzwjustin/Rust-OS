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
/// Hardware I/O is TODO.
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
        }
    }

    pub fn get_button_count(&self) -> u32 {
        self.button_count
    }

    pub fn set_button_count(&mut self, count: u32) {
        self.button_count = count;
    }

    pub fn has_ring(&self) -> bool {
        self.ring_count > 0
    }

    pub fn has_strip(&self) -> bool {
        self.strip_count > 0
    }
}

impl Default for MetaWaylandTabletPad {
    fn default() -> Self {
        Self::new()
    }
}
