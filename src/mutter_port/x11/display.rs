//! X11 display management.
//!
//! Ported from GNOME Mutter's src/x11/meta-x11-display.c/.h.
//! Manages the X11 connection, atom tables, event dispatch, and top-level window registry.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/x11/meta-x11-display.c

use crate::mutter_port::core::DisplayId;
use crate::mutter_port::x11::atoms::AtomNames;
use alloc::boxed::Box;
use alloc::vec::Vec;

/// Opaque handle to an X11 window ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct XWindow(pub u64);

/// Type alias for an X11 event callback function.
///
/// A full implementation would receive a parsed XEvent and the MetaX11Display.
/// In this no_std port we carry the boxed callback so callers can register
/// handlers without depending on XLib types. The first argument is the opaque
/// X event serial, the second is the event window id, and the third is the
/// event code.
pub type EventFunc = Box<dyn Fn(u64, XWindow, u8) + Send + Sync>;

/// Primary X11 display management structure.
/// Corresponds to struct _MetaX11Display in meta-x11-display-private.h.
pub struct MetaX11Display {
    /// Parent display reference.
    pub display_id: DisplayId,

    /// X display name (e.g., ":0" or ":0.0").
    pub name: Option<alloc::string::String>,
    pub screen_name: Option<alloc::string::String>,

    /// Root window and connection state.
    pub xroot: XWindow,
    pub xdisplay_handle: u64, // Opaque Display* pointer

    /// Timestamp of the last X11 server event.
    pub server_timestamp: u32,

    /// Interned X11 atom names.
    pub atoms: AtomNames,

    /// Window registry: XWindow -> MetaWindow mapping.
    pub xid_to_window: Option<alloc::collections::BTreeMap<u64, u64>>,

    /// Alarm registry for sync events.
    pub alarms: Option<alloc::collections::BTreeMap<u64, u64>>,

    /// Event function callbacks.
    pub event_funcs: Vec<EventFunc>,

    /// Focus tracking.
    pub server_focus_window: XWindow,
    pub server_focus_serial: u64,
    pub focus_xwindow: XWindow,
    pub focus_serial: u64,
    pub no_focus_window: XWindow,

    /// Guard window for unmapped windows in compositing mode.
    pub guard_window: XWindow,

    /// WM selection window.
    pub wm_sn_selection_window: XWindow,
    pub wm_sn_atom: u64,
    pub wm_sn_timestamp: u32,

    /// Composite manager state.
    pub composite_overlay_window: XWindow,
    pub wm_cm_selection_window: XWindow,

    /// X extension support flags and event/error base codes.
    pub have_xsync: bool,
    pub have_shape: bool,
    pub have_composite: bool,
    pub have_damage: bool,

    pub composite_event_base: i32,
    pub composite_error_base: i32,
    pub damage_event_base: i32,
    pub xfixes_event_base: i32,
    pub xfixes_error_base: i32,
    pub xinput_error_base: i32,
    pub xinput_event_base: i32,
    pub xsync_event_base: i32,
    pub xsync_error_base: i32,
    pub shape_event_base: i32,
    pub shape_error_base: i32,

    /// Focus state.
    pub focused_by_us: bool,
    pub is_server_focus: bool,
    pub closing: bool,

    /// Last bell time for photosensitivity throttling.
    pub last_bell_time: u32,
}

impl MetaX11Display {
    /// Create a new X11 display structure and perform port initialization.
    ///
    /// Mirrors meta_x11_display_new(): it constructs the display, interns all
    /// EWMH/ICCCM atoms via `AtomNames::intern_all`, and initializes the
    /// window and alarm registries. A full implementation would additionally
    /// open the X connection (XOpenDisplay), query the root window, install
    /// the WM selection, and query extension event/error bases; those steps
    /// require a live X server and are left to the platform backend.
    pub fn new(display_id: DisplayId) -> Self {
        let mut atoms = AtomNames::new();
        atoms.intern_all(display_id);

        Self {
            display_id,
            name: None,
            screen_name: None,
            xroot: XWindow(0),
            xdisplay_handle: 0,
            server_timestamp: 0,
            atoms,
            xid_to_window: Some(alloc::collections::BTreeMap::new()),
            alarms: Some(alloc::collections::BTreeMap::new()),
            event_funcs: Vec::new(),
            server_focus_window: XWindow(0),
            server_focus_serial: 0,
            focus_xwindow: XWindow(0),
            focus_serial: 0,
            no_focus_window: XWindow(0),
            guard_window: XWindow(0),
            wm_sn_selection_window: XWindow(0),
            wm_sn_atom: 0,
            wm_sn_timestamp: 0,
            composite_overlay_window: XWindow(0),
            wm_cm_selection_window: XWindow(0),
            have_xsync: false,
            have_shape: false,
            have_composite: false,
            have_damage: false,
            composite_event_base: 0,
            composite_error_base: 0,
            damage_event_base: 0,
            xfixes_event_base: 0,
            xfixes_error_base: 0,
            xinput_error_base: 0,
            xinput_event_base: 0,
            xsync_event_base: 0,
            xsync_error_base: 0,
            shape_event_base: 0,
            shape_error_base: 0,
            focused_by_us: false,
            is_server_focus: false,
            closing: false,
            last_bell_time: 0,
        }
    }

    /// Look up a MetaWindow by X window ID.
    pub fn lookup_x_window(&self, xwindow: XWindow) -> Option<u64> {
        self.xid_to_window
            .as_ref()
            .and_then(|map| map.get(&xwindow.0).copied())
    }

    /// Register an X window -> MetaWindow mapping.
    pub fn register_x_window(&mut self, xwindow: XWindow, meta_window_id: u64) {
        if let Some(ref mut map) = self.xid_to_window {
            map.insert(xwindow.0, meta_window_id);
        }
    }

    /// Unregister an X window mapping.
    pub fn unregister_x_window(&mut self, xwindow: XWindow) {
        if let Some(ref mut map) = self.xid_to_window {
            map.remove(&xwindow.0);
        }
    }

    /// Register an event callback to be invoked during event dispatch.
    pub fn add_event_func(&mut self, func: EventFunc) {
        self.event_funcs.push(func);
    }

    /// Dispatch an event to all registered event callbacks.
    pub fn dispatch_event(&self, serial: u64, window: XWindow, code: u8) {
        for func in &self.event_funcs {
            func(serial, window, code);
        }
    }

    /// Restore the active workspace (called on startup).
    /// A full implementation would read the _NET_CURRENT_DESKTOP
    /// property from the root window and set the active workspace
    /// index accordingly. Without an X connection, this is a no-op.
    pub fn restore_active_workspace(&self) {
        // Would read _NET_CURRENT_DESKTOP from root window via
        // XGetWindowProperty and set the active workspace on the
        // MetaDisplay. Requires an X connection.
    }
}
