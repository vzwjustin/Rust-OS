//! X11 event handling and dispatch.
//!
//! Ported from GNOME Mutter's src/x11/events.c/.h.
//! Handles X11 protocol events (ConfigureNotify, PropertyNotify, ClientMessage, etc.)
//! and routes them to appropriate window/display handlers.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/x11/events.c

use crate::mutter_port::x11::display::MetaX11Display;

/// Opaque X11 event type (corresponds to XEvent union in Xlib).
#[derive(Debug, Clone, Copy)]
pub struct XEvent {
    pub event_type: u32,
    pub xwindow: u64,
    pub serial: u64,
}

/// Event processing callback signature.
/// # TODO: port callback signatures from mutter x11/events.c
pub type EventFunc = fn(&XEvent) -> bool;

impl MetaX11Display {
    /// Initialize X11 event handling.
    /// # TODO: port logic from meta_x11_display_init_events()
    pub fn init_events(&mut self) {
        // TODO: register event handlers
        // TODO: create event source for main loop
        // TODO: select event masks on root window
    }

    /// Free event handling resources.
    /// # TODO: port logic from meta_x11_display_free_events()
    pub fn free_events(&mut self) {
        self.event_funcs.clear();
    }

    /// Process an X11 event from the server.
    /// # TODO: port full event dispatch logic from events.c
    pub fn process_event(&mut self, event: &XEvent) -> bool {
        match event.event_type {
            2 => {
                // KeyPress
                // TODO: dispatch to key handler
                true
            }
            3 => {
                // KeyRelease
                // TODO: dispatch to key handler
                true
            }
            4 => {
                // ButtonPress
                // TODO: dispatch to button handler
                true
            }
            5 => {
                // ButtonRelease
                // TODO: dispatch to button handler
                true
            }
            6 => {
                // MotionNotify
                // TODO: dispatch to motion handler
                true
            }
            12 => {
                // ConfigureNotify
                // TODO: handle configure notify
                true
            }
            13 => {
                // CreateNotify
                // TODO: handle create notify
                true
            }
            17 => {
                // ClientMessage
                // TODO: handle client message
                true
            }
            25 => {
                // PropertyNotify
                // TODO: handle property notify
                true
            }
            _ => {
                // Other events (XSync, Damage, XFixes, etc.)
                // TODO: dispatch to extension handlers
                false
            }
        }
    }
}
