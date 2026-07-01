//! X11 atom definitions and name constants.
//!
//! Ported from GNOME Mutter's x11/atomnames.h and x11/mutter-Xatomtype.h.
//! Atoms are interned X11 identifiers used for properties, window types, and other protocol messaging.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/x11/atomnames.h

use crate::mutter_port::core::DisplayId;

/// X11 atom type used throughout the display and window management.
/// In Rust, we represent this as an opaque u64 handle rather than XLib's Atom type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Atom(pub u64);

/// Standard X11 atom names. Each corresponds to an interned atom in the X server.
/// These are typically used as property names for inter-client communication.
#[allow(non_snake_case)]
pub struct AtomNames {
    pub WM_PROTOCOLS: Atom,
    pub WM_TAKE_FOCUS: Atom,
    pub WM_DELETE_WINDOW: Atom,
    pub WM_STATE: Atom,
    pub WM_CHANGE_STATE: Atom,
    pub WM_CLIENT_LEADER: Atom,
    pub WM_COLORMAPS: Atom,
    pub WM_WINDOW_ROLE: Atom,
    pub NET_WM_CONTEXT_HELP: Atom,
    pub NET_WM_WINDOW_TYPE: Atom,
    pub NET_WM_WINDOW_TYPE_DESKTOP: Atom,
    pub NET_WM_WINDOW_TYPE_DOCK: Atom,
    pub NET_WM_WINDOW_TYPE_TOOLBAR: Atom,
    pub NET_WM_WINDOW_TYPE_MENU: Atom,
    pub NET_WM_WINDOW_TYPE_UTILITY: Atom,
    pub NET_WM_WINDOW_TYPE_SPLASH: Atom,
    pub NET_WM_WINDOW_TYPE_DIALOG: Atom,
    pub NET_WM_WINDOW_TYPE_DROPDOWN_MENU: Atom,
    pub NET_WM_WINDOW_TYPE_POPUP_MENU: Atom,
    pub NET_WM_WINDOW_TYPE_TOOLTIP: Atom,
    pub NET_WM_WINDOW_TYPE_NOTIFICATION: Atom,
    pub NET_WM_WINDOW_TYPE_COMBO: Atom,
    pub NET_WM_WINDOW_TYPE_DND: Atom,
    pub NET_WM_NAME: Atom,
    pub NET_WM_VISIBLE_NAME: Atom,
    pub NET_WM_ICON_NAME: Atom,
    pub NET_WM_VISIBLE_ICON_NAME: Atom,
    pub NET_WM_ICON: Atom,
    pub NET_WM_PID: Atom,
    pub NET_WM_ALLOWED_ACTIONS: Atom,
    pub NET_WM_ACTION_MOVE: Atom,
    pub NET_WM_ACTION_RESIZE: Atom,
    pub NET_WM_ACTION_MINIMIZE: Atom,
    pub NET_WM_ACTION_SHADE: Atom,
    pub NET_WM_ACTION_STICK: Atom,
    pub NET_WM_ACTION_MAXIMIZE_HORZ: Atom,
    pub NET_WM_ACTION_MAXIMIZE_VERT: Atom,
    pub NET_WM_ACTION_FULLSCREEN: Atom,
    pub NET_WM_ACTION_CHANGE_DESKTOP: Atom,
    pub NET_WM_ACTION_CLOSE: Atom,
    pub NET_WM_ACTION_ABOVE: Atom,
    pub NET_WM_ACTION_BELOW: Atom,
    pub NET_WM_STATE: Atom,
    pub NET_WM_STATE_MODAL: Atom,
    pub NET_WM_STATE_STICKY: Atom,
    pub NET_WM_STATE_MAXIMIZED_VERT: Atom,
    pub NET_WM_STATE_MAXIMIZED_HORZ: Atom,
    pub NET_WM_STATE_SHADED: Atom,
    pub NET_WM_STATE_SKIP_TASKBAR: Atom,
    pub NET_WM_STATE_SKIP_PAGER: Atom,
    pub NET_WM_STATE_HIDDEN: Atom,
    pub NET_WM_STATE_FULLSCREEN: Atom,
    pub NET_WM_STATE_ABOVE: Atom,
    pub NET_WM_STATE_BELOW: Atom,
    pub NET_WM_STATE_DEMANDS_ATTENTION: Atom,
    pub NET_WM_STATE_FOCUSED: Atom,
}

impl AtomNames {
    /// Create a new AtomNames structure with all atoms initialized to zero.
    pub fn new() -> Self {
        Self {
            WM_PROTOCOLS: Atom(0),
            WM_TAKE_FOCUS: Atom(0),
            WM_DELETE_WINDOW: Atom(0),
            WM_STATE: Atom(0),
            WM_CHANGE_STATE: Atom(0),
            WM_CLIENT_LEADER: Atom(0),
            WM_COLORMAPS: Atom(0),
            WM_WINDOW_ROLE: Atom(0),
            NET_WM_CONTEXT_HELP: Atom(0),
            NET_WM_WINDOW_TYPE: Atom(0),
            NET_WM_WINDOW_TYPE_DESKTOP: Atom(0),
            NET_WM_WINDOW_TYPE_DOCK: Atom(0),
            NET_WM_WINDOW_TYPE_TOOLBAR: Atom(0),
            NET_WM_WINDOW_TYPE_MENU: Atom(0),
            NET_WM_WINDOW_TYPE_UTILITY: Atom(0),
            NET_WM_WINDOW_TYPE_SPLASH: Atom(0),
            NET_WM_WINDOW_TYPE_DIALOG: Atom(0),
            NET_WM_WINDOW_TYPE_DROPDOWN_MENU: Atom(0),
            NET_WM_WINDOW_TYPE_POPUP_MENU: Atom(0),
            NET_WM_WINDOW_TYPE_TOOLTIP: Atom(0),
            NET_WM_WINDOW_TYPE_NOTIFICATION: Atom(0),
            NET_WM_WINDOW_TYPE_COMBO: Atom(0),
            NET_WM_WINDOW_TYPE_DND: Atom(0),
            NET_WM_NAME: Atom(0),
            NET_WM_VISIBLE_NAME: Atom(0),
            NET_WM_ICON_NAME: Atom(0),
            NET_WM_VISIBLE_ICON_NAME: Atom(0),
            NET_WM_ICON: Atom(0),
            NET_WM_PID: Atom(0),
            NET_WM_ALLOWED_ACTIONS: Atom(0),
            NET_WM_ACTION_MOVE: Atom(0),
            NET_WM_ACTION_RESIZE: Atom(0),
            NET_WM_ACTION_MINIMIZE: Atom(0),
            NET_WM_ACTION_SHADE: Atom(0),
            NET_WM_ACTION_STICK: Atom(0),
            NET_WM_ACTION_MAXIMIZE_HORZ: Atom(0),
            NET_WM_ACTION_MAXIMIZE_VERT: Atom(0),
            NET_WM_ACTION_FULLSCREEN: Atom(0),
            NET_WM_ACTION_CHANGE_DESKTOP: Atom(0),
            NET_WM_ACTION_CLOSE: Atom(0),
            NET_WM_ACTION_ABOVE: Atom(0),
            NET_WM_ACTION_BELOW: Atom(0),
            NET_WM_STATE: Atom(0),
            NET_WM_STATE_MODAL: Atom(0),
            NET_WM_STATE_STICKY: Atom(0),
            NET_WM_STATE_MAXIMIZED_VERT: Atom(0),
            NET_WM_STATE_MAXIMIZED_HORZ: Atom(0),
            NET_WM_STATE_SHADED: Atom(0),
            NET_WM_STATE_SKIP_TASKBAR: Atom(0),
            NET_WM_STATE_SKIP_PAGER: Atom(0),
            NET_WM_STATE_HIDDEN: Atom(0),
            NET_WM_STATE_FULLSCREEN: Atom(0),
            NET_WM_STATE_ABOVE: Atom(0),
            NET_WM_STATE_BELOW: Atom(0),
            NET_WM_STATE_DEMANDS_ATTENTION: Atom(0),
            NET_WM_STATE_FOCUSED: Atom(0),
        }
    }

    /// Intern all atom names with the X11 display. Assigns sequential
    /// atom IDs starting from 1 (X server typically starts at 1).
    /// A full implementation would call XInternAtom for each name,
    /// which returns the server-assigned atom ID.
    pub fn intern_all(&mut self, _display_id: DisplayId) {
        // Assign sequential IDs. In a real X server, atom IDs are
        // server-assigned and typically start at 1. We use the same
        // convention here for consistency.
        let mut next_id: u64 = 1;
        macro_rules! intern {
            ($field:ident) => {
                self.$field = Atom(next_id);
                next_id += 1;
            };
        }
        intern!(WM_PROTOCOLS);
        intern!(WM_TAKE_FOCUS);
        intern!(WM_DELETE_WINDOW);
        intern!(WM_STATE);
        intern!(WM_CHANGE_STATE);
        intern!(WM_CLIENT_LEADER);
        intern!(WM_COLORMAPS);
        intern!(WM_WINDOW_ROLE);
        intern!(NET_WM_CONTEXT_HELP);
        intern!(NET_WM_WINDOW_TYPE);
        intern!(NET_WM_WINDOW_TYPE_DESKTOP);
        intern!(NET_WM_WINDOW_TYPE_DOCK);
        intern!(NET_WM_WINDOW_TYPE_TOOLBAR);
        intern!(NET_WM_WINDOW_TYPE_MENU);
        intern!(NET_WM_WINDOW_TYPE_UTILITY);
        intern!(NET_WM_WINDOW_TYPE_SPLASH);
        intern!(NET_WM_WINDOW_TYPE_DIALOG);
        intern!(NET_WM_WINDOW_TYPE_DROPDOWN_MENU);
        intern!(NET_WM_WINDOW_TYPE_POPUP_MENU);
        intern!(NET_WM_WINDOW_TYPE_TOOLTIP);
        intern!(NET_WM_WINDOW_TYPE_NOTIFICATION);
        intern!(NET_WM_WINDOW_TYPE_COMBO);
        intern!(NET_WM_WINDOW_TYPE_DND);
        intern!(NET_WM_NAME);
        intern!(NET_WM_VISIBLE_NAME);
        intern!(NET_WM_ICON_NAME);
        intern!(NET_WM_VISIBLE_ICON_NAME);
        intern!(NET_WM_ICON);
        intern!(NET_WM_PID);
        intern!(NET_WM_ALLOWED_ACTIONS);
        intern!(NET_WM_ACTION_MOVE);
        intern!(NET_WM_ACTION_RESIZE);
        intern!(NET_WM_ACTION_MINIMIZE);
        intern!(NET_WM_ACTION_SHADE);
        intern!(NET_WM_ACTION_STICK);
        intern!(NET_WM_ACTION_MAXIMIZE_HORZ);
        intern!(NET_WM_ACTION_MAXIMIZE_VERT);
        intern!(NET_WM_ACTION_FULLSCREEN);
        intern!(NET_WM_ACTION_CHANGE_DESKTOP);
        intern!(NET_WM_ACTION_CLOSE);
        intern!(NET_WM_ACTION_ABOVE);
        intern!(NET_WM_ACTION_BELOW);
        intern!(NET_WM_STATE);
        intern!(NET_WM_STATE_MODAL);
        intern!(NET_WM_STATE_STICKY);
        intern!(NET_WM_STATE_MAXIMIZED_VERT);
        intern!(NET_WM_STATE_MAXIMIZED_HORZ);
        intern!(NET_WM_STATE_SHADED);
        intern!(NET_WM_STATE_SKIP_TASKBAR);
        intern!(NET_WM_STATE_SKIP_PAGER);
        intern!(NET_WM_STATE_HIDDEN);
        intern!(NET_WM_STATE_FULLSCREEN);
        intern!(NET_WM_STATE_ABOVE);
        intern!(NET_WM_STATE_BELOW);
        intern!(NET_WM_STATE_DEMANDS_ATTENTION);
        intern!(NET_WM_STATE_FOCUSED);
    }
}

impl Default for AtomNames {
    fn default() -> Self {
        Self::new()
    }
}
