//! X11 window management.
//!
//! Ported from GNOME Mutter's src/x11/window-x11.c/.h.
//! Manages MetaWindow objects backed by X11 windows with properties, sync counters, input regions, etc.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/x11/window-x11.c

use crate::mutter_port::x11::display::XWindow;

/// Represents an X11 window that has been wrapped as a MetaWindow.
/// Corresponds to MetaWindowX11 in window-x11-private.h.
pub struct MetaWindowX11 {
    /// The underlying X window ID.
    pub xwindow: XWindow,

    /// Toplevel X window (for client-side decorations).
    pub toplevel_xwindow: XWindow,

    /// WM_SYNC_REQUEST counter ID (if supported).
    pub sync_request_counter: Option<u64>,

    /// WM_SYNC_REQUEST alarm (if XSync is available).
    pub sync_request_alarm: Option<u64>,

    /// Frozen commit depth (for frame commit synchronization).
    pub frozen_commits: u32,

    /// Whether we should thaw after paint.
    pub thaw_after_paint: bool,

    /// Input region (for click-through handling).
    pub input_region: Option<u64>,

    /// Shape region (for shaped windows).
    pub shape_region: Option<u64>,

    /// Window icon from WM_HINTS or NET_WM_ICON.
    pub wm_hints_icon: Option<u64>,

    /// Support flags for various WM protocols.
    pub wm_take_focus: bool,
    pub wm_delete_window: bool,
    pub wm_ping: bool,

    /// Client-side decoration (CSD) frame.
    pub frame: Option<u64>,

    /// Current WM_STATE (NormalState=1, IconicState=3, WithdrawnState=0).
    pub wm_state: u32,

    /// Bitmask of _NET_WM_STATE flags currently set.
    pub net_wm_state: u64,

    /// Bitmask of _NET_WM_ALLOWED_ACTIONS currently set.
    pub allowed_actions: u64,

    /// Current window type from _NET_WM_WINDOW_TYPE.
    pub window_type: u32,

    /// Whether shape should always be updated (certain window types).
    pub always_update_shape_flag: bool,
}

/// _NET_WM_STATE bit flags.
pub const NET_WM_STATE_MODAL: u64 = 1 << 0;
pub const NET_WM_STATE_STICKY: u64 = 1 << 1;
pub const NET_WM_STATE_MAXIMIZED_VERT: u64 = 1 << 2;
pub const NET_WM_STATE_MAXIMIZED_HORZ: u64 = 1 << 3;
pub const NET_WM_STATE_SHADED: u64 = 1 << 4;
pub const NET_WM_STATE_SKIP_TASKBAR: u64 = 1 << 5;
pub const NET_WM_STATE_SKIP_PAGER: u64 = 1 << 6;
pub const NET_WM_STATE_HIDDEN: u64 = 1 << 7;
pub const NET_WM_STATE_FULLSCREEN: u64 = 1 << 8;
pub const NET_WM_STATE_ABOVE: u64 = 1 << 9;
pub const NET_WM_STATE_BELOW: u64 = 1 << 10;
pub const NET_WM_STATE_DEMANDS_ATTENTION: u64 = 1 << 11;
pub const NET_WM_STATE_FOCUSED: u64 = 1 << 12;

/// _NET_WM_ALLOWED_ACTIONS bit flags.
pub const NET_WM_ACTION_MOVE: u64 = 1 << 0;
pub const NET_WM_ACTION_RESIZE: u64 = 1 << 1;
pub const NET_WM_ACTION_MINIMIZE: u64 = 1 << 2;
pub const NET_WM_ACTION_SHADE: u64 = 1 << 3;
pub const NET_WM_ACTION_STICK: u64 = 1 << 4;
pub const NET_WM_ACTION_MAXIMIZE_HORZ: u64 = 1 << 5;
pub const NET_WM_ACTION_MAXIMIZE_VERT: u64 = 1 << 6;
pub const NET_WM_ACTION_FULLSCREEN: u64 = 1 << 7;
pub const NET_WM_ACTION_CHANGE_DESKTOP: u64 = 1 << 8;
pub const NET_WM_ACTION_CLOSE: u64 = 1 << 9;
pub const NET_WM_ACTION_ABOVE: u64 = 1 << 10;
pub const NET_WM_ACTION_BELOW: u64 = 1 << 11;

/// WM_STATE values (from ICCCM).
pub const WITHDRAWN_STATE: u32 = 0;
pub const NORMAL_STATE: u32 = 1;
pub const ICONIC_STATE: u32 = 3;

/// Window types from _NET_WM_WINDOW_TYPE.
pub const WINDOW_TYPE_DESKTOP: u32 = 1;
pub const WINDOW_TYPE_DOCK: u32 = 2;
pub const WINDOW_TYPE_TOOLBAR: u32 = 3;
pub const WINDOW_TYPE_MENU: u32 = 4;
pub const WINDOW_TYPE_UTILITY: u32 = 5;
pub const WINDOW_TYPE_SPLASH: u32 = 6;
pub const WINDOW_TYPE_DIALOG: u32 = 7;
pub const WINDOW_TYPE_NORMAL: u32 = 8;

impl MetaWindowX11 {
    /// Create a new MetaWindowX11 wrapping an X window.
    pub fn new(xwindow: XWindow) -> Self {
        Self {
            xwindow,
            toplevel_xwindow: xwindow,
            sync_request_counter: None,
            sync_request_alarm: None,
            frozen_commits: 0,
            thaw_after_paint: false,
            input_region: None,
            shape_region: None,
            wm_hints_icon: None,
            wm_take_focus: false,
            wm_delete_window: false,
            wm_ping: false,
            frame: None,
            wm_state: WITHDRAWN_STATE,
            net_wm_state: 0,
            allowed_actions: 0,
            window_type: WINDOW_TYPE_NORMAL,
            always_update_shape_flag: false,
        }
    }

    /// Set the WM_STATE property on the window. Updates the internal
    /// state tracking. A full implementation would call XChangeProperty
    /// to write the WM_STATE atom to the X server.
    pub fn set_wm_state(&mut self, state: u32) {
        self.wm_state = state;
    }

    /// Get the current WM_STATE.
    pub fn get_wm_state(&self) -> u32 {
        self.wm_state
    }

    /// Set NET_WM_STATE properties based on window state. Updates the
    /// internal bitmask. A full implementation would write the
    /// _NET_WM_STATE X atom list to the server.
    pub fn set_net_wm_state(&mut self, state_mask: u64) {
        self.net_wm_state = state_mask;
    }

    /// Add a _NET_WM_STATE flag.
    pub fn add_net_wm_state(&mut self, flag: u64) {
        self.net_wm_state |= flag;
    }

    /// Remove a _NET_WM_STATE flag.
    pub fn remove_net_wm_state(&mut self, flag: u64) {
        self.net_wm_state &= !flag;
    }

    /// Check if a _NET_WM_STATE flag is set.
    pub fn has_net_wm_state(&self, flag: u64) -> bool {
        (self.net_wm_state & flag) != 0
    }

    /// Set WM_TAKE_FOCUS capability hint. Updates the flag.
    /// A full implementation would send a WM_PROTOCOLS ClientMessage
    /// to the client window.
    pub fn set_wm_take_focus(&mut self, take_focus: bool) {
        self.wm_take_focus = take_focus;
    }

    /// Set WM_PING capability hint. Updates the flag.
    pub fn set_wm_ping(&mut self, ping: bool) {
        self.wm_ping = ping;
    }

    /// Set WM_DELETE_WINDOW capability hint. Updates the flag.
    pub fn set_wm_delete_window(&mut self, delete_window: bool) {
        self.wm_delete_window = delete_window;
    }

    /// Set the _NET_WM_ALLOWED_ACTIONS hint. Updates the internal
    /// bitmask. A full implementation would write the atom list to
    /// the X server.
    pub fn set_allowed_actions_hint(&mut self, actions: u64) {
        self.allowed_actions = actions;
    }

    /// Add an allowed action flag.
    pub fn add_allowed_action(&mut self, action: u64) {
        self.allowed_actions |= action;
    }

    /// Check if an action is allowed.
    pub fn is_action_allowed(&self, action: u64) -> bool {
        (self.allowed_actions & action) != 0
    }

    /// Create a WM_SYNC_REQUEST alarm for this window. Generates a
    /// unique alarm ID from the counter ID. A full implementation
    /// would call XSyncCreateAlarm.
    pub fn create_sync_request_alarm(&mut self) {
        if let Some(counter) = self.sync_request_counter {
            // Generate a synthetic alarm ID from the counter ID.
            // In upstream, XSyncCreateAlarm returns an XID.
            self.sync_request_alarm = Some(counter + 1);
        }
    }

    /// Destroy the WM_SYNC_REQUEST alarm.
    pub fn destroy_sync_request_alarm(&mut self) {
        self.sync_request_alarm = None;
    }

    /// Update the input region based on shape and decorations.
    /// When a shape region is set, the input region follows it.
    /// When no shape is set, the input region is cleared (full window).
    pub fn update_input_region(&mut self) {
        self.input_region = self.shape_region;
    }

    /// Set the shape region from a region handle.
    pub fn set_shape_region(&mut self, region: u64) {
        self.shape_region = Some(region);
    }

    /// Update the shape region from _NET_WM_WINDOW_SHAPE.
    /// A full implementation would query the XShape extension.
    /// Marks the shape as updated.
    pub fn update_shape_region(&mut self) {
        // Shape region is updated via set_shape_region by the caller.
        // The XShape query would happen here with a real X connection.
    }

    /// Recalculate window type from _NET_WM_WINDOW_TYPE.
    /// Updates the internal window_type field. A full implementation
    /// would query the _NET_WM_WINDOW_TYPE atom list from the X server.
    pub fn recalc_window_type(&mut self, window_type: u32) {
        self.window_type = window_type;
        // Desktop and dock windows always update shape.
        self.always_update_shape_flag =
            window_type == WINDOW_TYPE_DESKTOP || window_type == WINDOW_TYPE_DOCK;
    }

    /// Get the current window type.
    pub fn get_window_type(&self) -> u32 {
        self.window_type
    }

    /// Process a ConfigureRequest from the client. Returns true if
    /// the request was accepted. A full implementation would validate
    /// the requested geometry against workspace constraints and issue
    /// a ConfigureNotify reply.
    pub fn configure_request(&mut self) -> bool {
        // Accept all configure requests; geometry validation would
        // happen here with access to the workspace and frame extents.
        true
    }

    /// Process a property notify event. A full implementation would
    /// dispatch to specific property handlers based on the atom.
    pub fn property_notify(&mut self) {
        // Property-specific handling would dispatch to handlers for
        // WM_NAME, _NET_WM_STATE, _NET_WM_WINDOW_TYPE, etc.
    }

    /// Process a ClientMessage event. Returns true if handled.
    /// A full implementation would dispatch based on the message atom
    /// (WM_PROTOCOLS, _NET_WM_STATE, _NET_ACTIVE_WINDOW, etc.).
    pub fn client_message(&mut self) -> bool {
        // Client message dispatch would check the message type atom
        // and route to the appropriate state change handler.
        true
    }

    /// Process a ConfigureNotify event. A full implementation would
    /// update the window geometry from the event coordinates.
    pub fn configure_notify(&mut self) {
        // Geometry update from the event would happen here.
    }

    /// Get the toplevel X window.
    pub fn get_toplevel_xwindow(&self) -> XWindow {
        self.toplevel_xwindow
    }

    /// Freeze commits on this window (for batching).
    pub fn freeze_commits(&mut self) {
        self.frozen_commits += 1;
    }

    /// Thaw commits on this window.
    pub fn thaw_commits(&mut self) {
        if self.frozen_commits > 0 {
            self.frozen_commits -= 1;
        }
    }

    /// Set whether to thaw after paint.
    pub fn set_thaw_after_paint(&mut self, thaw: bool) {
        self.thaw_after_paint = thaw;
    }

    /// Check if we should thaw after paint.
    pub fn should_thaw_after_paint(&self) -> bool {
        self.thaw_after_paint
    }

    /// Check if shape should always be updated. Certain window types
    /// (desktop, dock) require always-updating shape regions.
    pub fn always_update_shape(&self) -> bool {
        self.always_update_shape_flag
    }
}
