//! Mutter X11 support
//! Ported from meta/meta-x11*.h
use alloc::{string::String, vec::Vec, format};

use crate::mutter_port::meta::types::*;

/// X11 display connection handler (X11 protocol connection and screen info)
pub struct MetaX11Display {
    pub xdisplay: Option<u64>, // opaque X11 Display pointer
    pub screen_number: i32,
}

impl MetaX11Display {
    pub fn new(screen_number: i32) -> Self {
        Self {
            xdisplay: None,
            screen_number,
        }
    }

    /// Get the underlying meta display
    pub fn get_display(&self) -> Option<&MetaDisplay> {
        // TODO: implement
        None
    }

    /// Get X11 display pointer
    pub fn get_xdisplay(&self) -> Option<u64> {
        self.xdisplay
    }

    /// Get X11 screen number
    pub fn get_screen_number(&self) -> i32 {
        self.screen_number
    }
}

impl Default for MetaX11Display {
    fn default() -> Self {
        Self::new(0)
    }
}

/// X11 window group/class (group leader and member windows)
pub struct MetaX11Group {
    pub leader: Option<u64>, // X11 window ID
    pub windows: Vec<u32>,   // indices to member windows
}

impl MetaX11Group {
    pub fn new() -> Self {
        Self {
            leader: None,
            windows: Vec::new(),
        }
    }

    /// Get group leader window
    pub fn get_leader(&self) -> Option<u64> {
        self.leader
    }

    /// Get all windows in group
    pub fn get_windows(&self) -> Vec<&MetaWindow> {
        Vec::new()
    }
}

impl Default for MetaX11Group {
    fn default() -> Self {
        Self::new()
    }
}

/// X11 type constants
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MetaX11WindowType {
    Normal = 0,
    Desktop = 1,
    Dock = 2,
    Dialog = 3,
    Toolbar = 4,
    Menu = 5,
    Utility = 6,
    Splash = 7,
    DropdownMenu = 8,
    PopupMenu = 9,
    Tooltip = 10,
    Notification = 11,
    Combo = 12,
    Dnd = 13,
}

// TODO: port remaining X11 functions
