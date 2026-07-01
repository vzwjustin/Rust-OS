//! Window manager preferences and settings store.
//!
//! Core preferences data model for window management, focus behavior, animations,
//! and workspace configuration. Ported from GNOME Mutter:
//! Source: mutter-main/src/core/prefs.c (GNU GPL 2+)
//!
//! This module replaces GSettings/dconf backend with in-memory storage and defaults.

use alloc::string::String;
use alloc::vec::Vec;

/// Focus mode for window focus behavior
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusMode {
    /// Click to focus
    Click,
    /// Focus follows mouse
    Sloppy,
    /// Focus on mouse motion after delay
    MousePrecision,
}

/// Focus policy for new windows
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusNewWindows {
    /// Smart: new windows grab focus unless something else is focused
    Smart,
    /// Strict: don't focus new windows automatically
    Strict,
}

/// Action triggered by titlebar double-click
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TitlebarAction {
    /// Toggle maximize
    ToggleMaximize,
    /// Toggle maximize vertically only
    ToggleMaximizeVertically,
    /// Toggle maximize horizontally only
    ToggleMaximizeHorizontally,
    /// Minimize window
    Minimize,
    /// Lower window
    Lower,
    /// Open window menu
    Menu,
    /// No action
    None,
}

/// Visual bell effect type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisualBellType {
    /// Flash the entire screen
    FullscreenFlash,
    /// Flash the active window
    WindowFlash,
}

/// Window manager preferences and settings
#[derive(Debug, Clone)]
pub struct WmPrefs {
    /// Focus mode (click or sloppy)
    pub focus_mode: FocusMode,
    /// Focus policy for new windows
    pub focus_new_windows: FocusNewWindows,
    /// Raise window on click
    pub raise_on_click: bool,
    /// Auto-raise window after delay
    pub auto_raise: bool,
    /// Auto-raise delay in milliseconds
    pub auto_raise_delay: i32,
    /// Number of workspaces
    pub num_workspaces: i32,
    /// Workspace names (empty string = use default name)
    pub workspace_names: Vec<String>,
    /// Button layout string (e.g. "menu:minimize,maximize,close")
    pub button_layout: String,
    /// Mouse button modifier for window operations
    pub mouse_button_modifier: String,
    /// Cursor theme name
    pub cursor_theme: String,
    /// Cursor size in pixels
    pub cursor_size: i32,
    /// Draggable border width in pixels
    pub draggable_border_width: i32,
    /// Drag threshold in pixels
    pub drag_threshold: i32,
    /// Double-click titlebar action
    pub action_double_click_titlebar: TitlebarAction,
    /// Middle-click titlebar action
    pub action_middle_click_titlebar: TitlebarAction,
    /// Right-click titlebar action
    pub action_right_click_titlebar: TitlebarAction,
    /// Enable visual bell
    pub visual_bell: bool,
    /// Visual bell type
    pub visual_bell_type: VisualBellType,
    /// Enable audible bell
    pub audible_bell: bool,
    /// Attach modal dialogs to parent window
    pub attach_modal_dialogs: bool,
    /// Center new windows on screen
    pub center_new_windows: bool,
    /// Enable animations
    pub enable_animations: bool,
    /// Enable edge tiling
    pub edge_tiling: bool,
    /// Only allow workspaces on primary monitor
    pub workspaces_only_on_primary: bool,
    /// Auto-maximize windows on drag to top
    pub auto_maximize: bool,
    /// Focus changes when pointer stops moving
    pub focus_change_on_pointer_rest: bool,
    /// Check alive timeout in milliseconds
    pub check_alive_timeout: u32,
}

impl Default for WmPrefs {
    fn default() -> Self {
        Self {
            focus_mode: FocusMode::Click,
            focus_new_windows: FocusNewWindows::Smart,
            raise_on_click: true,
            auto_raise: false,
            auto_raise_delay: 500,
            num_workspaces: 4,
            workspace_names: Vec::new(),
            button_layout: String::new(),
            mouse_button_modifier: String::new(),
            cursor_theme: String::new(),
            cursor_size: 24,
            draggable_border_width: 10,
            drag_threshold: 8,
            action_double_click_titlebar: TitlebarAction::ToggleMaximize,
            action_middle_click_titlebar: TitlebarAction::Lower,
            action_right_click_titlebar: TitlebarAction::Menu,
            visual_bell: false,
            visual_bell_type: VisualBellType::FullscreenFlash,
            audible_bell: false,
            attach_modal_dialogs: false,
            center_new_windows: false,
            enable_animations: true,
            edge_tiling: true,
            workspaces_only_on_primary: false,
            auto_maximize: false,
            focus_change_on_pointer_rest: false,
            check_alive_timeout: 5000,
        }
    }
}

impl WmPrefs {
    /// Create a new preferences struct with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Set focus mode
    pub fn set_focus_mode(&mut self, mode: FocusMode) {
        self.focus_mode = mode;
    }

    /// Set focus policy for new windows
    pub fn set_focus_new_windows(&mut self, policy: FocusNewWindows) {
        self.focus_new_windows = policy;
    }

    /// Set raise on click behavior
    pub fn set_raise_on_click(&mut self, raise: bool) {
        self.raise_on_click = raise;
    }

    /// Set auto-raise behavior
    pub fn set_auto_raise(&mut self, auto_raise: bool) {
        self.auto_raise = auto_raise;
    }

    /// Set auto-raise delay
    pub fn set_auto_raise_delay(&mut self, delay: i32) {
        self.auto_raise_delay = delay;
    }

    /// Set number of workspaces
    pub fn set_num_workspaces(&mut self, count: i32) {
        if count > 0 {
            self.num_workspaces = count;
        }
    }

    /// Set workspace names
    pub fn set_workspace_names(&mut self, names: Vec<String>) {
        self.workspace_names = names;
    }

    /// Set button layout
    pub fn set_button_layout(&mut self, layout: String) {
        self.button_layout = layout;
    }

    /// Set mouse button modifier
    pub fn set_mouse_button_modifier(&mut self, modifier: String) {
        self.mouse_button_modifier = modifier;
    }

    /// Set cursor theme
    pub fn set_cursor_theme(&mut self, theme: String) {
        self.cursor_theme = theme;
    }

    /// Set cursor size
    pub fn set_cursor_size(&mut self, size: i32) {
        if size > 0 {
            self.cursor_size = size;
        }
    }

    /// Set draggable border width
    pub fn set_draggable_border_width(&mut self, width: i32) {
        if width >= 0 {
            self.draggable_border_width = width;
        }
    }

    /// Set drag threshold
    pub fn set_drag_threshold(&mut self, threshold: i32) {
        if threshold >= 0 {
            self.drag_threshold = threshold;
        }
    }

    /// Set double-click titlebar action
    pub fn set_action_double_click_titlebar(&mut self, action: TitlebarAction) {
        self.action_double_click_titlebar = action;
    }

    /// Set middle-click titlebar action
    pub fn set_action_middle_click_titlebar(&mut self, action: TitlebarAction) {
        self.action_middle_click_titlebar = action;
    }

    /// Set right-click titlebar action
    pub fn set_action_right_click_titlebar(&mut self, action: TitlebarAction) {
        self.action_right_click_titlebar = action;
    }

    /// Set visual bell enabled state
    pub fn set_visual_bell(&mut self, enabled: bool) {
        self.visual_bell = enabled;
    }

    /// Set visual bell type
    pub fn set_visual_bell_type(&mut self, bell_type: VisualBellType) {
        self.visual_bell_type = bell_type;
    }

    /// Set audible bell enabled state
    pub fn set_audible_bell(&mut self, enabled: bool) {
        self.audible_bell = enabled;
    }

    /// Set attach modal dialogs behavior
    pub fn set_attach_modal_dialogs(&mut self, attach: bool) {
        self.attach_modal_dialogs = attach;
    }

    /// Set center new windows behavior
    pub fn set_center_new_windows(&mut self, center: bool) {
        self.center_new_windows = center;
    }

    /// Set animations enabled state
    pub fn set_enable_animations(&mut self, enabled: bool) {
        self.enable_animations = enabled;
    }

    /// Set edge tiling behavior
    pub fn set_edge_tiling(&mut self, enabled: bool) {
        self.edge_tiling = enabled;
    }

    /// Set workspaces only on primary monitor
    pub fn set_workspaces_only_on_primary(&mut self, only_primary: bool) {
        self.workspaces_only_on_primary = only_primary;
    }

    /// Set auto-maximize behavior
    pub fn set_auto_maximize(&mut self, auto_max: bool) {
        self.auto_maximize = auto_max;
    }

    /// Set focus change on pointer rest
    pub fn set_focus_change_on_pointer_rest(&mut self, enabled: bool) {
        self.focus_change_on_pointer_rest = enabled;
    }

    /// Set check alive timeout
    pub fn set_check_alive_timeout(&mut self, timeout: u32) {
        self.check_alive_timeout = timeout;
    }
}
