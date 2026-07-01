//! Wayland Tablet Pad Strip module
//!
//! Handles analog strip input on graphics tablet pads via the zwp_tablet_pad_v2 protocol.
//! Manages strip state, resource bindings, and event delivery to focused clients.
//! Supports feedback (vibration) for strip interactions.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-tablet-pad-strip.h

use alloc::string::String;

/// Represents an analog strip on a graphics tablet pad.
///
/// Tracks the strip's parent pad and group, client resource bindings, and
/// optional haptic feedback string.
#[derive(Debug)]
pub struct MetaWaylandTabletPadStrip {
    /// Parent tablet pad (opaque pointer to MetaWaylandTabletPad).
    pub pad: Option<*mut core::ffi::c_void>,
    /// Parent group within the pad (opaque pointer to MetaWaylandTabletPadGroup).
    pub group: Option<*mut core::ffi::c_void>,
    /// List of all Wayland resource bindings for this strip.
    /// Opaque wl_list equivalent; replaced with None for simplicity in Rust.
    pub resource_list: Option<*mut core::ffi::c_void>,
    /// List of focused resource bindings (clients with input focus).
    /// Opaque wl_list equivalent; replaced with None for simplicity in Rust.
    pub focus_resource_list: Option<*mut core::ffi::c_void>,
    /// Haptic feedback description (e.g., vibration pattern).
    pub feedback: Option<String>,
}

impl MetaWaylandTabletPadStrip {
    /// Create a new tablet pad strip.
    ///
    /// ponytail: register strip with parent pad; real impl initializes protocol
    pub fn new(_pad: *mut core::ffi::c_void) -> Self {
        MetaWaylandTabletPadStrip {
            pad: if _pad.is_null() { None } else { Some(_pad) },
            group: None,
            resource_list: None,
            focus_resource_list: None,
            feedback: None,
        }
    }
}

impl Default for MetaWaylandTabletPadStrip {
    fn default() -> Self {
        MetaWaylandTabletPadStrip {
            pad: None,
            group: None,
            resource_list: None,
            focus_resource_list: None,
            feedback: None,
        }
    }
}

/// Free a tablet pad strip.
///
/// ponytail: cleanup unregisters protocol; stub is no-op (owned by pad)
pub fn meta_wayland_tablet_pad_strip_free(_strip: *mut MetaWaylandTabletPadStrip) {}

/// Set the group for a tablet pad strip.
///
/// ponytail: associate with pad group; real impl wires group signals
pub fn meta_wayland_tablet_pad_strip_set_group(
    strip: *mut MetaWaylandTabletPadStrip,
    group: *mut core::ffi::c_void,
) {
    if !strip.is_null() {
        unsafe {
            (*strip).group = Some(group);
        }
    }
}

/// Create a new Wayland resource for a tablet pad strip.
///
/// ponytail: real impl binds protocol resource; stub returns None
pub fn meta_wayland_tablet_pad_strip_create_new_resource(
    _strip: *mut MetaWaylandTabletPadStrip,
    _client: *mut core::ffi::c_void,
    _group_resource: *mut core::ffi::c_void,
    _id: u32,
) -> Option<*mut core::ffi::c_void> {
    None
}

/// Handle a tablet pad strip event.
///
/// ponytail: real impl processes strip movement and emits events; stub returns false
pub fn meta_wayland_tablet_pad_strip_handle_event(
    _strip: *mut MetaWaylandTabletPadStrip,
    _event: *const core::ffi::c_void,
) -> bool {
    false
}

/// Synchronize focus to the currently focused surface.
///
/// ponytail: real impl updates focus resources when pointer focus changes
pub fn meta_wayland_tablet_pad_strip_sync_focus(_strip: *mut MetaWaylandTabletPadStrip) {}
