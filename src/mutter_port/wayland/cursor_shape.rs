//! Wayland Cursor Shape protocol implementation.
//!
//! Ported from: meta-wayland-cursor-shape.c/h
//!
//! Implements the wp_cursor_shape_manager_v1 and wp_cursor_shape_device_v1 protocols,
//! allowing clients to request named cursor shapes instead of providing bitmap data.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-cursor-shape.h

use alloc::{string::String, vec::Vec};

/// Named cursor shape enumeration (mirrors wp_cursor_shape_device_v1 shape values).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum WpCursorShape {
    // Core shapes
    DEFAULT = 1,
    CONTEXT_MENU = 2,
    HELP = 3,
    POINTER = 4,
    PROGRESS = 5,
    WAIT = 6,
    CELL = 7,
    CROSSHAIR = 8,
    TEXT = 9,
    VERTICAL_TEXT = 10,
    ALIAS = 11,
    COPY = 12,
    MOVE = 13,
    NO_DROP = 14,
    NOT_ALLOWED = 15,
    GRAB = 16,
    GRABBING = 17,
    // Resize/edge shapes
    E_RESIZE = 18,
    N_RESIZE = 19,
    NE_RESIZE = 20,
    NW_RESIZE = 21,
    S_RESIZE = 22,
    SE_RESIZE = 23,
    SW_RESIZE = 24,
    W_RESIZE = 25,
    EW_RESIZE = 26,
    NS_RESIZE = 27,
    NESW_RESIZE = 28,
    NWSE_RESIZE = 29,
    COL_RESIZE = 30,
    ROW_RESIZE = 31,
    // Zoom shapes
    ALL_SCROLL = 32,
    ZOOM_IN = 33,
    ZOOM_OUT = 34,
}

/// Cursor shape manager for a Wayland compositor.
///
/// Maintains the wp_cursor_shape_manager_v1 global resource and per-device
/// cursor shape state. A full implementation would call
/// `wl_global_create` to advertise `wp_cursor_shape_manager_v1` and,
/// on bind, hand out `wp_cursor_shape_device_v1` resources. The device
/// resource's `set_shape` and `destroy` request handlers would update
/// the per-device state tracked here. Without libwayland, this struct
/// holds the data model so the compositor can query and mutate the
/// active cursor shape.
#[derive(Debug)]
pub struct MetaWaylandCursorShape {
    pub compositor: Option<*mut core::ffi::c_void>, // MetaWaylandCompositor pointer
    /// Per-device cursor shape state, keyed by the wl_pointer resource
    /// (or seat device pointer). Linear lookup; the C code uses a
    /// GHashTable keyed by ClutterInputDevice.
    pub devices: Vec<CursorShapeDeviceState>,
}

/// Per-device cursor shape state: the currently-set shape, its scale,
/// and the hotspot the client specified.
#[derive(Debug, Clone, Copy)]
pub struct CursorShapeDeviceState {
    /// Device/pointer resource this state belongs to.
    pub device: *mut core::ffi::c_void,
    /// Currently requested cursor shape (a WpCursorShape value).
    pub shape_id: u32,
    /// Scale applied to the cursor shape (for HiDPI).
    pub scale: u32,
    /// Hotspot X offset within the cursor image.
    pub hotspot_x: i32,
    /// Hotspot Y offset within the cursor image.
    pub hotspot_y: i32,
}

impl CursorShapeDeviceState {
    /// Create a new device state with the default cursor shape.
    pub fn new(device: *mut core::ffi::c_void) -> Self {
        Self {
            device,
            shape_id: WpCursorShape::DEFAULT as u32,
            scale: 1,
            hotspot_x: 0,
            hotspot_y: 0,
        }
    }
}

impl MetaWaylandCursorShape {
    pub fn new(compositor: *mut core::ffi::c_void) -> Self {
        MetaWaylandCursorShape {
            compositor: if compositor.is_null() {
                None
            } else {
                Some(compositor)
            },
            devices: Vec::new(),
        }
    }

    /// Get the cursor shape state for a device, if any.
    pub fn get_shape(&self, device: *mut core::ffi::c_void) -> Option<&CursorShapeDeviceState> {
        self.devices
            .iter()
            .find(|d| core::ptr::eq(d.device, device))
    }

    /// Set the cursor shape for a device. Creates a new entry if the
    /// device has no prior state, otherwise updates the existing entry.
    /// A full implementation would also render the named cursor shape
    /// via the cursor theme and update the hardware cursor plane.
    pub fn set_shape(
        &mut self,
        device: *mut core::ffi::c_void,
        shape_id: u32,
        scale: u32,
        hotspot_x: i32,
        hotspot_y: i32,
    ) {
        if let Some(state) = self
            .devices
            .iter_mut()
            .find(|d| core::ptr::eq(d.device, device))
        {
            state.shape_id = shape_id;
            state.scale = scale;
            state.hotspot_x = hotspot_x;
            state.hotspot_y = hotspot_y;
        } else {
            self.devices.push(CursorShapeDeviceState {
                device,
                shape_id,
                scale,
                hotspot_x,
                hotspot_y,
            });
        }
    }

    /// Get the shape id for a device, defaulting to `DEFAULT` if unset.
    pub fn get_shape_id(&self, device: *mut core::ffi::c_void) -> u32 {
        self.get_shape(device)
            .map(|s| s.shape_id)
            .unwrap_or(WpCursorShape::DEFAULT as u32)
    }

    /// Get the scale for a device, defaulting to 1.
    pub fn get_scale(&self, device: *mut core::ffi::c_void) -> u32 {
        self.get_shape(device).map(|s| s.scale).unwrap_or(1)
    }

    /// Get the hotspot for a device, defaulting to (0, 0).
    pub fn get_hotspot(&self, device: *mut core::ffi::c_void) -> (i32, i32) {
        self.get_shape(device)
            .map(|s| (s.hotspot_x, s.hotspot_y))
            .unwrap_or((0, 0))
    }

    /// Remove the cursor shape state for a device (e.g. when the
    /// pointer resource is destroyed). Returns true if it was present.
    pub fn remove_device(&mut self, device: *mut core::ffi::c_void) -> bool {
        let before = self.devices.len();
        self.devices.retain(|d| !core::ptr::eq(d.device, device));
        self.devices.len() != before
    }

    /// Number of devices with cursor shape state.
    pub fn device_count(&self) -> usize {
        self.devices.len()
    }
}

impl Default for MetaWaylandCursorShape {
    fn default() -> Self {
        MetaWaylandCursorShape {
            compositor: None,
            devices: Vec::new(),
        }
    }
}
