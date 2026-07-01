//! X11 property handling.
//!
//! Ported from GNOME Mutter's src/x11/xprops.c/.h, src/x11/window-props.c/.h, and src/x11/group-props.c/.h.
//! Provides utilities for reading/writing X11 window and group properties,
//! with special handling for ICCCM and EWMH properties.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/x11/xprops.c

use crate::mutter_port::x11::display::XWindow;
use alloc::vec::Vec;

/// Property data type (e.g., cardinal, window, atom).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyType {
    Cardinal,
    Window,
    Atom,
    String,
    Geometry,
    Other(u32),
}

/// Property value wrapper for different data types.
#[derive(Debug, Clone)]
pub enum PropertyValue {
    Cardinal(Vec<u32>),
    Window(Vec<XWindow>),
    Atom(Vec<u64>),
    String(alloc::string::String),
    Raw(Vec<u8>),
}

/// Property hook for window properties (WM_HINTS, NET_WM_ICON, etc.).
pub struct WindowPropertyHook {
    pub name: &'static str,
    pub callback: fn(xwindow: XWindow, value: &PropertyValue),
}

/// Property hook for group properties.
pub struct GroupPropertyHook {
    pub name: &'static str,
    pub callback: fn(value: &PropertyValue),
}

/// Get a property from an X window.
/// # TODO: port logic from meta_prop_get_list() / meta_prop_get_object()
pub fn get_property(
    _xwindow: XWindow,
    _atom: u64,
    _property_type: PropertyType,
) -> Option<PropertyValue> {
    // TODO: XGetWindowProperty call and type conversion
    None
}

/// Set a property on an X window.
/// # TODO: port logic from property setting functions
pub fn set_property(
    _xwindow: XWindow,
    _atom: u64,
    _value: &PropertyValue,
) -> bool {
    // TODO: XChangeProperty call
    true
}

/// Delete a property from an X window.
/// # TODO: port logic from property deletion
pub fn delete_property(_xwindow: XWindow, _atom: u64) -> bool {
    // TODO: XDeleteProperty call
    true
}

/// Read WM_HINTS structure from window.
/// # TODO: port logic from meta_prop_get_wm_hints()
pub fn get_wm_hints(_xwindow: XWindow) -> Option<WmHints> {
    // TODO: parse WM_HINTS property
    None
}

/// Read ICCCM size hints from window.
/// # TODO: port logic from meta_prop_get_size_hints()
pub fn get_size_hints(_xwindow: XWindow) -> Option<SizeHints> {
    // TODO: parse WM_NORMAL_HINTS property
    None
}

/// Read _NET_WM_NAME from window.
/// # TODO: port logic from meta_prop_get_utf8_string()
pub fn get_net_wm_name(_xwindow: XWindow) -> Option<alloc::string::String> {
    // TODO: read _NET_WM_NAME property
    None
}

/// Read _NET_WM_ICON_GEOMETRY from window.
/// # TODO: port logic from meta_prop_get_box()
pub fn get_net_wm_icon_geometry(_xwindow: XWindow) -> Option<(i32, i32, i32, i32)> {
    // TODO: read icon geometry
    None
}

/// WM_HINTS structure.
#[derive(Debug, Clone)]
pub struct WmHints {
    pub flags: u32,
    pub input: bool,
    pub initial_state: i32,
    pub icon_pixmap: u64,
    pub icon_mask: u64,
    pub icon_window: XWindow,
    pub icon_x: i32,
    pub icon_y: i32,
    pub window_group: XWindow,
}

/// ICCCM size hints.
#[derive(Debug, Clone)]
pub struct SizeHints {
    pub flags: u32,
    pub min_width: i32,
    pub min_height: i32,
    pub max_width: i32,
    pub max_height: i32,
    pub width_inc: i32,
    pub height_inc: i32,
    pub aspect_x: i32,
    pub aspect_y: i32,
    pub base_width: i32,
    pub base_height: i32,
}
