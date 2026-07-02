//! Mutter workspace management
//! Ported from meta/workspace.h
use alloc::{format, string::String, vec::Vec};

use crate::mutter_port::meta::display::MetaDisplay;
use crate::mutter_port::meta::window::MetaWindow;

/// Represents a virtual workspace/desktop
pub struct MetaWorkspace {
    pub index: u32,
    name: Option<String>,
    display: *mut MetaDisplay,
    width: i32,
    height: i32,
    active_window: *mut MetaWindow,
    is_active: bool,
    windows: Vec<*mut MetaWindow>,
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
            is_active: false,
            windows: Vec::new(),
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

    /// Set the display pointer (typed by the caller).
    pub fn set_display(&mut self, display: *mut MetaDisplay) {
        self.display = display;
    }

    /// Get the display this workspace belongs to.
    /// Resolves the stored typed pointer.
    pub fn get_display(&self) -> Option<&MetaDisplay> {
        if self.display.is_null() {
            None
        } else {
            // SAFETY: The pointer was set by `set_display` with a valid
            // `*mut MetaDisplay`. The caller guarantees the referent
            // outlives this borrow.
            unsafe { Some(&*self.display) }
        }
    }

    /// Add a window to this workspace.
    pub fn add_window(&mut self, window: &MetaWindow) {
        let ptr = window as *const MetaWindow as *mut MetaWindow;
        self.windows.retain(|&w| w != ptr);
        self.windows.push(ptr);
    }

    /// Remove a window from this workspace.
    pub fn remove_window(&mut self, window: &MetaWindow) {
        let ptr = window as *const MetaWindow as *mut MetaWindow;
        self.windows.retain(|&w| w != ptr);
    }

    /// Get all windows on this workspace.
    pub fn get_windows(&self) -> Vec<&MetaWindow> {
        self.windows
            .iter()
            .filter(|&&ptr| !ptr.is_null())
            .map(|&ptr| {
                // SAFETY: Pointers were inserted via `add_window` with
                // valid `&MetaWindow` references. The caller guarantees
                // the windows outlive this borrow.
                unsafe { &*ptr }
            })
            .collect()
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

    /// Activate this workspace. Marks it as the active workspace.
    pub fn activate(&mut self, _timestamp: u32) {
        self.is_active = true;
    }

    /// Deactivate this workspace (called when another workspace is activated).
    pub fn deactivate(&mut self) {
        self.is_active = false;
    }

    /// Whether this workspace is currently active.
    pub fn is_active(&self) -> bool {
        self.is_active
    }

    /// Get active window on this workspace.
    pub fn get_active_window(&self) -> Option<&MetaWindow> {
        if self.active_window.is_null() {
            None
        } else {
            // SAFETY: The pointer was set by `set_active_window` with a
            // valid `*mut MetaWindow`. The caller guarantees the referent
            // outlives this borrow.
            unsafe { Some(&*self.active_window) }
        }
    }

    /// Set active window (typed pointer to the focused window).
    pub fn set_active_window(&mut self, window: *mut MetaWindow) {
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
