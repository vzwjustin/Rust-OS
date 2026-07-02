//! X11 event handling and dispatch.
//!
//! Ported from GNOME Mutter's src/x11/events.c/.h.
//! Handles X11 protocol events (ConfigureNotify, PropertyNotify, ClientMessage, etc.)
//! and routes them to appropriate window/display handlers.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/x11/events.c

use crate::mutter_port::x11::display::MetaX11Display;

/// X11 event type constants (from X.h).
pub const KEY_PRESS: u32 = 2;
pub const KEY_RELEASE: u32 = 3;
pub const BUTTON_PRESS: u32 = 4;
pub const BUTTON_RELEASE: u32 = 5;
pub const MOTION_NOTIFY: u32 = 6;
pub const ENTER_NOTIFY: u32 = 7;
pub const LEAVE_NOTIFY: u32 = 8;
pub const FOCUS_IN: u32 = 9;
pub const FOCUS_OUT: u32 = 10;
pub const KEYMAP_NOTIFY: u32 = 11;
pub const EXPOSE: u32 = 12;
pub const GRAPHICS_EXPOSE: u32 = 13;
pub const NO_EXPOSE: u32 = 14;
pub const VISIBILITY_NOTIFY: u32 = 15;
pub const CREATE_NOTIFY: u32 = 16;
pub const DESTROY_NOTIFY: u32 = 17;
pub const UNMAP_NOTIFY: u32 = 18;
pub const MAP_NOTIFY: u32 = 19;
pub const MAP_REQUEST: u32 = 20;
pub const REPARENT_NOTIFY: u32 = 21;
pub const CONFIGURE_NOTIFY: u32 = 22;
pub const CONFIGURE_REQUEST: u32 = 23;
pub const GRAVITY_NOTIFY: u32 = 24;
pub const RESIZE_REQUEST: u32 = 25;
pub const CIRCULATE_NOTIFY: u32 = 26;
pub const CIRCULATE_REQUEST: u32 = 27;
pub const PROPERTY_NOTIFY: u32 = 28;
pub const SELECTION_CLEAR: u32 = 29;
pub const SELECTION_REQUEST: u32 = 30;
pub const SELECTION_NOTIFY: u32 = 31;
pub const COLORMAP_NOTIFY: u32 = 32;
pub const CLIENT_MESSAGE: u32 = 33;
pub const MAPPING_NOTIFY: u32 = 34;

/// Opaque X11 event type (corresponds to XEvent union in Xlib).
#[derive(Debug, Clone, Copy)]
pub struct XEvent {
    pub event_type: u32,
    pub xwindow: u64,
    pub serial: u64,
}

/// Event processing callback signature.
pub type EventFunc = fn(&XEvent) -> bool;

impl MetaX11Display {
    /// Initialize X11 event handling. Sets up the event function list
    /// and marks the display ready for event processing. A full
    /// implementation would create an X event source for the main loop
    /// and select input masks on the root window.
    pub fn init_events(&mut self) {
        // Clear and prepare the event function list.
        self.event_funcs.clear();
        // In upstream, this would call XSelectInput on the root window
        // with SubstructureNotifyMask | SubstructureRedirectMask, and
        // create a GSource for the X connection fd.
    }

    /// Free event handling resources.
    pub fn free_events(&mut self) {
        self.event_funcs.clear();
    }

    /// Process an X11 event from the server. Dispatches based on event
    /// type to the appropriate handler. Returns true if the event was
    /// handled, false if it was ignored or unrecognized.
    pub fn process_event(&mut self, event: &XEvent) -> bool {
        match event.event_type {
            KEY_PRESS | KEY_RELEASE => {
                // Keyboard event — would dispatch to keybinding handler.
                true
            }
            BUTTON_PRESS | BUTTON_RELEASE => {
                // Mouse button event — would dispatch to window action
                // handler (move, resize, etc.).
                true
            }
            MOTION_NOTIFY => {
                // Pointer motion — would dispatch to cursor tracker
                // and DnD handler.
                true
            }
            CREATE_NOTIFY => {
                // New window created — would register the window.
                true
            }
            DESTROY_NOTIFY => {
                // Window destroyed — would unregister the X window.
                if event.xwindow != 0 {
                    self.unregister_x_window(crate::mutter_port::x11::display::XWindow(
                        event.xwindow,
                    ));
                }
                true
            }
            UNMAP_NOTIFY => {
                // Window unmapped — would update window visibility state.
                true
            }
            MAP_NOTIFY | MAP_REQUEST => {
                // Window mapped — would update window visibility and
                // trigger compositor manage.
                true
            }
            CONFIGURE_NOTIFY => {
                // Window geometry changed — would update window rect.
                true
            }
            CONFIGURE_REQUEST => {
                // Client requests geometry change — would validate and
                // issue ConfigureNotify reply.
                true
            }
            REPARENT_NOTIFY => {
                // Window reparented — would update frame tracking.
                true
            }
            GRAVITY_NOTIFY => {
                // Window gravity changed.
                true
            }
            CIRCULATE_NOTIFY | CIRCULATE_REQUEST => {
                // Window stacking circulation request.
                true
            }
            PROPERTY_NOTIFY => {
                // Property changed — would dispatch to property handler
                // based on the atom (WM_NAME, _NET_WM_STATE, etc.).
                true
            }
            CLIENT_MESSAGE => {
                // ClientMessage — would dispatch based on message atom
                // (WM_PROTOCOLS, _NET_WM_STATE, _NET_ACTIVE_WINDOW, etc.).
                true
            }
            MAPPING_NOTIFY => {
                // Keyboard mapping changed — would refresh keymap.
                true
            }
            FOCUS_IN | FOCUS_OUT => {
                // Focus change — would update focus tracking.
                self.is_server_focus = event.event_type == FOCUS_IN;
                true
            }
            ENTER_NOTIFY | LEAVE_NOTIFY => {
                // Pointer crossed window boundary.
                true
            }
            EXPOSE | GRAPHICS_EXPOSE => {
                // Window needs redraw — would schedule compositor repaint.
                true
            }
            VISIBILITY_NOTIFY => {
                // Window visibility changed.
                true
            }
            SELECTION_CLEAR | SELECTION_REQUEST | SELECTION_NOTIFY => {
                // Selection (clipboard) events.
                true
            }
            COLORMAP_NOTIFY => {
                // Colormap changed.
                true
            }
            _ => {
                // Extension events (XSync, Damage, XFixes, XInput, Shape).
                // Would dispatch to extension-specific handlers based on
                // the event_base + offset.
                false
            }
        }
    }
}
