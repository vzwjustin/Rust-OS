//! Mutter workspace management
//! Ported from meta/workspace.h
use alloc::{string::String, vec::Vec, format};

use crate::mutter_port::meta::types::*;

/// Represents a virtual workspace/desktop
pub struct MetaWorkspace {
    pub index: u32,
    name: Option<String>,
    display: *mut core::ffi::c_void,
    width: i32,
    height: i32,
    active_window: *mut core::ffi::c_void,
}

impl MetaWorkspace {
    /// Create a new workspace
    pub fn new(index: u32) -> Self {
        Self {
            index,
            name: None,
            display: core::ptr::null_mut(),
            width: 0,
            height: 0,
            active_window: core::ptr::null_mut(),
        }
    }

    /// Get workspace index
    pub fn get_index(&self) -> u32 {
        self.index
    }

    /// Get workspace name
    pub fn get_name(&self) -> Option<&str> {
        self.name.as_ref().map(|s| s.as_str())
    }

    /// Get the display this workspace belongs to
    pub fn get_display(&self) -> Option<&MetaDisplay> {
        // TODO: implement
        None
    }

    /// Get all windows on this workspace
    pub fn get_windows(&self) -> Vec<&MetaWindow> {
        // TODO: implement
        Vec::new()
    }

    /// Get workspace geometry
    pub fn get_width(&self) -> i32 {
        self.width
    }

    pub fn get_height(&self) -> i32 {
        self.height
    }

    /// Set workspace geometry
    pub fn set_geometry(&mut self, width: i32, height: i32) {
        self.width = width;
        self.height = height;
    }

    /// Activate this workspace
    pub fn activate(&mut self, _timestamp: u32) {
        // TODO: implement
    }

    /// Get active window on this workspace.
    pub fn get_active_window(&self) -> Option<&MetaWindow> {
        // TODO: the active window is held as an opaque pointer; resolving it to a
        // typed &MetaWindow needs the window registry, so leave unimplemented.
        None
    }

    /// Set active window (opaque pointer to the focused window).
    pub fn set_active_window(&mut self, window: *mut core::ffi::c_void) {
        self.active_window = window;
    }

    /// Set workspace name
    pub fn set_name(&mut self, name: Option<String>) {
        self.name = name;
    }
}

impl Default for MetaWorkspace {
    fn default() -> Self {
        Self::new(0)
    }
}
