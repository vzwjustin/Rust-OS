//! GNOME src/wayland/meta-wayland-input.c
//!
//! MetaWaylandInput is the seat's event-dispatch spine. Consumers (the default
//! pointer/keyboard/touch delivery, popup grabs, DnD, window-move/resize, ...)
//! push a MetaWaylandEventHandler onto a stack. Incoming Clutter events are
//! offered to the topmost handler first, walking down the stack until one
//! consumes the event. A handler can be "grabbing", which pins input to it and
//! installs a Clutter grab so events outside the client still route through it.
//!
//! We model the handler stack + the focus/dispatch routing. The concrete
//! handler callbacks are represented by an event-kind enum plus a boolean
//! "consumed" contract, since we can't hold C function pointers; real handler
//! bodies live in the pointer/keyboard/touch/popup modules.
//!
//! https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-input.c

use alloc::vec::Vec;

/// Kinds of event routed through the seat (maps to the MetaWaylandEventInterface
/// vfuncs: focus/motion/press/release/key/other).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    Motion,
    Press,
    Release,
    Key,
    /// Pads / IM / tablet — the `other` vfunc.
    Other,
}

/// A registered event handler (mirrors MetaWaylandEventHandler).
///
/// The C struct carries a `const MetaWaylandEventInterface *iface` of callbacks;
/// here we identify the handler by an opaque `handler_id` that the owning
/// subsystem allocates, plus the `grabbing` flag.
#[derive(Debug, Clone)]
pub struct MetaWaylandEventHandler {
    pub handler_id: u32,
    pub grabbing: bool,
    /// Focus surface this handler last resolved (get_focus_surface result).
    pub focus_surface: Option<u32>,
}

impl MetaWaylandEventHandler {
    pub fn new(handler_id: u32, grabbing: bool) -> Self {
        MetaWaylandEventHandler {
            handler_id,
            grabbing,
            focus_surface: None,
        }
    }
}

/// MetaWaylandInput
pub struct MetaWaylandInput {
    seat: u32,
    /// Handler stack. The *front* is the topmost handler (events tried first);
    /// this mirrors wl_list insertion at the head in the C code.
    handlers: Vec<MetaWaylandEventHandler>,
    next_handler_id: u32,
    /// True when a Clutter grab is installed (a grabbing handler is active).
    grab_active: bool,
}

impl MetaWaylandInput {
    /// meta_wayland_input_new()
    pub fn new(seat: u32) -> Self {
        MetaWaylandInput {
            seat,
            handlers: Vec::new(),
            next_handler_id: 1,
            grab_active: false,
        }
    }

    pub fn seat(&self) -> u32 {
        self.seat
    }

    /// The current (topmost) handler, if any.
    pub fn current_handler(&self) -> Option<&MetaWaylandEventHandler> {
        self.handlers.first()
    }

    /// meta_wayland_input_is_current_handler()
    pub fn is_current_handler(&self, handler_id: u32) -> bool {
        self.current_handler()
            .map(|h| h.handler_id == handler_id)
            .unwrap_or(false)
    }

    /// meta_wayland_input_attach_event_handler(): push a handler onto the top of
    /// the stack. If `grab` is set a Clutter grab is installed. Returns the new
    /// handler id.
    pub fn attach_event_handler(&mut self, grab: bool) -> u32 {
        let id = self.next_handler_id;
        self.next_handler_id += 1;
        self.handlers
            .insert(0, MetaWaylandEventHandler::new(id, grab));
        if grab {
            self.grab_active = true;
            // STUB: install ClutterGrab on the stage.
        }
        id
    }

    /// meta_wayland_input_detach_event_handler(): remove a handler and, if it
    /// held the grab, tear the Clutter grab down (unless another grabbing
    /// handler remains).
    pub fn detach_event_handler(&mut self, handler_id: u32) -> bool {
        let before = self.handlers.len();
        self.handlers.retain(|h| h.handler_id != handler_id);
        let removed = self.handlers.len() != before;
        self.grab_active = self.handlers.iter().any(|h| h.grabbing);
        if !self.grab_active {
            // STUB: dismiss ClutterGrab.
        }
        removed
    }

    pub fn grab_active(&self) -> bool {
        self.grab_active
    }

    /// Update the focus surface resolved by the topmost handler
    /// (get_focus_surface). Lower handlers "chain up" to this value.
    pub fn set_current_focus(&mut self, surface: Option<u32>) {
        if let Some(h) = self.handlers.first_mut() {
            h.focus_surface = surface;
        }
    }

    /// meta_wayland_event_handler_chain_up_get_focus_surface(): the focus
    /// surface of the handler directly below `handler_id`, i.e. what a handler
    /// that does not itself resolve focus should defer to.
    pub fn chain_up_focus_surface(&self, handler_id: u32) -> Option<u32> {
        let idx = self
            .handlers
            .iter()
            .position(|h| h.handler_id == handler_id)?;
        self.handlers.get(idx + 1).and_then(|h| h.focus_surface)
    }

    /// meta_wayland_input_handle_event(): offer an event to the handler stack.
    ///
    /// `consume_at` lets a caller model which handler (by id) will consume the
    /// event; the real dispatch invokes the per-kind vfunc and stops at the
    /// first that returns TRUE. A grabbing handler always terminates the walk
    /// even if it does not consume, so events never fall through a grab.
    ///
    /// Returns true if the event was consumed.
    pub fn handle_event(&self, _kind: EventKind, consume_at: Option<u32>) -> bool {
        for handler in &self.handlers {
            if consume_at == Some(handler.handler_id) {
                return true;
            }
            if handler.grabbing {
                // A grab swallows the event even when the handler declines it.
                return false;
            }
        }
        false
    }

    /// meta_wayland_input_invalidate_focus(): force the topmost handler to
    /// re-resolve focus (e.g. after the pointer moved onto a new surface).
    /// Modeled as clearing the cached focus so the next set_current_focus wins.
    pub fn invalidate_focus(&mut self) {
        if let Some(h) = self.handlers.first_mut() {
            h.focus_surface = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_attach_detach_stack_order() {
        let mut input = MetaWaylandInput::new(1);
        let a = input.attach_event_handler(false);
        let b = input.attach_event_handler(false);
        // Most recently attached is on top.
        assert!(input.is_current_handler(b));
        assert!(!input.is_current_handler(a));

        assert!(input.detach_event_handler(b));
        assert!(input.is_current_handler(a));
        assert!(!input.detach_event_handler(999));
    }

    #[test]
    fn test_grab_tracking() {
        let mut input = MetaWaylandInput::new(1);
        assert!(!input.grab_active());
        let g = input.attach_event_handler(true);
        assert!(input.grab_active());
        input.detach_event_handler(g);
        assert!(!input.grab_active());
    }

    #[test]
    fn test_grab_swallows_event() {
        let mut input = MetaWaylandInput::new(1);
        let bottom = input.attach_event_handler(false);
        let _grab = input.attach_event_handler(true);
        // The grab is on top and declines; the bottom handler must not see it.
        assert!(!input.handle_event(EventKind::Motion, Some(bottom)));
    }

    #[test]
    fn test_event_consumed_by_top() {
        let mut input = MetaWaylandInput::new(1);
        let top = input.attach_event_handler(false);
        assert!(input.handle_event(EventKind::Press, Some(top)));
    }

    #[test]
    fn test_chain_up_focus() {
        let mut input = MetaWaylandInput::new(1);
        let bottom = input.attach_event_handler(false);
        let top = input.attach_event_handler(false);
        input.set_current_focus(Some(0)); // sets on `top`
                                          // bottom handler's cached focus:
        input
            .handlers
            .iter_mut()
            .find(|h| h.handler_id == bottom)
            .unwrap()
            .focus_surface = Some(77);
        assert_eq!(input.chain_up_focus_surface(top), Some(77));
    }
}
