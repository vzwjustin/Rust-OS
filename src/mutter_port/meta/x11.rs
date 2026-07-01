//! Mutter X11 support
//! Ported from meta/meta-x11*.h
use alloc::{format, string::String, vec::Vec};

use crate::mutter_port::meta::display::MetaDisplay;
use crate::mutter_port::meta::window::MetaWindow;

/// X11 display connection handler (X11 protocol connection and screen info)
pub struct MetaX11Display {
    pub xdisplay: Option<u64>, // opaque X11 Display pointer
    pub screen_number: i32,
    display: *mut MetaDisplay,
}

impl MetaX11Display {
    pub fn new(screen_number: i32) -> Self {
        Self {
            xdisplay: None,
            screen_number,
            display: core::ptr::null_mut(),
        }
    }

    /// Set the MetaDisplay pointer (typed by the caller).
    pub fn set_display(&mut self, display: *mut MetaDisplay) {
        self.display = display;
    }

    /// Get the underlying meta display.
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

impl MetaX11WindowType {
    /// Map an X11 window type to its upstream `_NET_WM_WINDOW_TYPE_*` atom
    /// name. Returns the static atom name string used by the EWMH spec.
    pub fn atom_name(&self) -> &'static str {
        match self {
            MetaX11WindowType::Normal => "_NET_WM_WINDOW_TYPE_NORMAL",
            MetaX11WindowType::Desktop => "_NET_WM_WINDOW_TYPE_DESKTOP",
            MetaX11WindowType::Dock => "_NET_WM_WINDOW_TYPE_DOCK",
            MetaX11WindowType::Dialog => "_NET_WM_WINDOW_TYPE_DIALOG",
            MetaX11WindowType::Toolbar => "_NET_WM_WINDOW_TYPE_TOOLBAR",
            MetaX11WindowType::Menu => "_NET_WM_WINDOW_TYPE_MENU",
            MetaX11WindowType::Utility => "_NET_WM_WINDOW_TYPE_UTILITY",
            MetaX11WindowType::Splash => "_NET_WM_WINDOW_TYPE_SPLASH",
            MetaX11WindowType::DropdownMenu => "_NET_WM_WINDOW_TYPE_DROPDOWN_MENU",
            MetaX11WindowType::PopupMenu => "_NET_WM_WINDOW_TYPE_POPUP_MENU",
            MetaX11WindowType::Tooltip => "_NET_WM_WINDOW_TYPE_TOOLTIP",
            MetaX11WindowType::Notification => "_NET_WM_WINDOW_TYPE_NOTIFICATION",
            MetaX11WindowType::Combo => "_NET_WM_WINDOW_TYPE_COMBO",
            MetaX11WindowType::Dnd => "_NET_WM_WINDOW_TYPE_DND",
        }
    }

    /// Human-readable name for diagnostics and logging.
    pub fn type_name(&self) -> &'static str {
        match self {
            MetaX11WindowType::Normal => "normal",
            MetaX11WindowType::Desktop => "desktop",
            MetaX11WindowType::Dock => "dock",
            MetaX11WindowType::Dialog => "dialog",
            MetaX11WindowType::Toolbar => "toolbar",
            MetaX11WindowType::Menu => "menu",
            MetaX11WindowType::Utility => "utility",
            MetaX11WindowType::Splash => "splash",
            MetaX11WindowType::DropdownMenu => "dropdown_menu",
            MetaX11WindowType::PopupMenu => "popup_menu",
            MetaX11WindowType::Tooltip => "tooltip",
            MetaX11WindowType::Notification => "notification",
            MetaX11WindowType::Combo => "combo",
            MetaX11WindowType::Dnd => "dnd",
        }
    }

    /// Parse an EWMH `_NET_WM_WINDOW_TYPE_*` atom name into the enum.
    /// Returns `None` for unrecognized names.
    pub fn from_atom_name(name: &str) -> Option<Self> {
        match name {
            "_NET_WM_WINDOW_TYPE_NORMAL" => Some(MetaX11WindowType::Normal),
            "_NET_WM_WINDOW_TYPE_DESKTOP" => Some(MetaX11WindowType::Desktop),
            "_NET_WM_WINDOW_TYPE_DOCK" => Some(MetaX11WindowType::Dock),
            "_NET_WM_WINDOW_TYPE_DIALOG" => Some(MetaX11WindowType::Dialog),
            "_NET_WM_WINDOW_TYPE_TOOLBAR" => Some(MetaX11WindowType::Toolbar),
            "_NET_WM_WINDOW_TYPE_MENU" => Some(MetaX11WindowType::Menu),
            "_NET_WM_WINDOW_TYPE_UTILITY" => Some(MetaX11WindowType::Utility),
            "_NET_WM_WINDOW_TYPE_SPLASH" => Some(MetaX11WindowType::Splash),
            "_NET_WM_WINDOW_TYPE_DROPDOWN_MENU" => Some(MetaX11WindowType::DropdownMenu),
            "_NET_WM_WINDOW_TYPE_POPUP_MENU" => Some(MetaX11WindowType::PopupMenu),
            "_NET_WM_WINDOW_TYPE_TOOLTIP" => Some(MetaX11WindowType::Tooltip),
            "_NET_WM_WINDOW_TYPE_NOTIFICATION" => Some(MetaX11WindowType::Notification),
            "_NET_WM_WINDOW_TYPE_COMBO" => Some(MetaX11WindowType::Combo),
            "_NET_WM_WINDOW_TYPE_DND" => Some(MetaX11WindowType::Dnd),
            _ => None,
        }
    }
}

/// X11 window hint/state flags tracked by the window manager.
#[derive(Debug, Clone, Copy, Default)]
pub struct MetaX11WindowHints {
    /// Whether the window requests attention (urgency hint set).
    pub urgent: bool,
    /// Whether the window is in the fullscreen state.
    pub fullscreen: bool,
    /// Whether the window requests focus on map.
    pub input_hint: bool,
    /// Whether the window's initial state is iconic.
    pub initial_state_iconic: bool,
}

impl MetaX11WindowHints {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns true if the WM_HINTS urgency flag is set for this window.
    pub fn is_urgent(&self) -> bool {
        self.urgent
    }

    /// Returns true if the window is in the fullscreen state
    /// (`_NET_WM_STATE_FULLSCREEN`).
    pub fn is_fullscreen(&self) -> bool {
        self.fullscreen
    }

    /// Set the urgency hint state.
    pub fn set_urgent(&mut self, urgent: bool) {
        self.urgent = urgent;
    }

    /// Set the fullscreen state.
    pub fn set_fullscreen(&mut self, fullscreen: bool) {
        self.fullscreen = fullscreen;
    }
}

/// Helper: determine whether a window type should be treated as urgent
/// for focus-stealing purposes. Transient dialog and notification types
/// are considered urgent by convention even without an explicit hint.
pub fn type_implies_urgent(window_type: MetaX11WindowType) -> bool {
    matches!(
        window_type,
        MetaX11WindowType::Dialog | MetaX11WindowType::Notification
    )
}

/// Helper: determine whether a window type is eligible for fullscreen
/// state. Only normal and dock-type windows may enter fullscreen per
/// EWMH conventions; override-redirect-style types cannot.
pub fn type_allows_fullscreen(window_type: MetaX11WindowType) -> bool {
    matches!(
        window_type,
        MetaX11WindowType::Normal | MetaX11WindowType::Dock
    )
}
