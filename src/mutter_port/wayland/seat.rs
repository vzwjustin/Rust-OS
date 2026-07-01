//! GNOME src/wayland/meta-wayland-seat.c
//!
//! MetaWaylandSeat is the wl_seat: it owns the pointer, keyboard and touch
//! devices, advertises seat capabilities to clients, tracks the keyboard input
//! focus surface, and routes incoming events into the MetaWaylandInput handler
//! stack. It also owns the data-device/primary-selection and tablet/text-input
//! machinery, which is stubbed here.
//!
//! https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-seat.c

use super::input::MetaWaylandInput;
use super::keyboard::MetaWaylandKeyboard;
use super::pointer::MetaWaylandPointer;
use super::touch::MetaWaylandTouch;

/// wl_seat capability bits (wayland-server WL_SEAT_CAPABILITY_*).
pub const CAP_POINTER: u32 = 1 << 0;
pub const CAP_KEYBOARD: u32 = 1 << 1;
pub const CAP_TOUCH: u32 = 1 << 2;

/// MetaWaylandSeat
pub struct MetaWaylandSeat {
    pub id: u32,

    pointer: MetaWaylandPointer,
    keyboard: MetaWaylandKeyboard,
    touch: MetaWaylandTouch,
    input: MetaWaylandInput,

    /// Bitmask of CAP_* currently advertised to clients.
    capabilities: u32,

    /// Keyboard input focus surface (meta_wayland_seat_set_input_focus).
    input_focus: Option<u32>,
}

impl MetaWaylandSeat {
    /// meta_wayland_seat_init()
    pub fn new(id: u32) -> Self {
        MetaWaylandSeat {
            id,
            pointer: MetaWaylandPointer::new(id),
            keyboard: MetaWaylandKeyboard::new(id),
            touch: MetaWaylandTouch::new(id),
            input: MetaWaylandInput::new(id),
            capabilities: 0,
            input_focus: None,
        }
    }

    pub fn pointer(&self) -> &MetaWaylandPointer {
        &self.pointer
    }
    pub fn pointer_mut(&mut self) -> &mut MetaWaylandPointer {
        &mut self.pointer
    }
    pub fn keyboard(&self) -> &MetaWaylandKeyboard {
        &self.keyboard
    }
    pub fn keyboard_mut(&mut self) -> &mut MetaWaylandKeyboard {
        &mut self.keyboard
    }
    pub fn touch(&self) -> &MetaWaylandTouch {
        &self.touch
    }
    pub fn touch_mut(&mut self) -> &mut MetaWaylandTouch {
        &mut self.touch
    }
    pub fn input(&self) -> &MetaWaylandInput {
        &self.input
    }
    pub fn input_mut(&mut self) -> &mut MetaWaylandInput {
        &mut self.input
    }

    pub fn capabilities(&self) -> u32 {
        self.capabilities
    }

    /// meta_wayland_seat_has_pointer()
    pub fn has_pointer(&self) -> bool {
        self.capabilities & CAP_POINTER != 0
    }
    /// meta_wayland_seat_has_keyboard()
    pub fn has_keyboard(&self) -> bool {
        self.capabilities & CAP_KEYBOARD != 0
    }
    /// meta_wayland_seat_has_touch()
    pub fn has_touch(&self) -> bool {
        self.capabilities & CAP_TOUCH != 0
    }

    /// meta_wayland_seat_set_capabilities(): advertise a new capability set.
    /// When a capability is dropped, the corresponding device's focus/state is
    /// released (mirrors the *_set_focus(NULL) calls in the C code).
    pub fn set_capabilities(&mut self, flags: u32) {
        let prev = self.capabilities;
        self.capabilities = flags;

        if prev & CAP_POINTER != 0 && flags & CAP_POINTER == 0 {
            self.pointer.set_focus(None);
        }
        if prev & CAP_KEYBOARD != 0 && flags & CAP_KEYBOARD == 0 {
            self.keyboard.set_focus(None);
            self.input_focus = None;
        }
        if prev & CAP_TOUCH != 0 && flags & CAP_TOUCH == 0 {
            self.touch.cancel();
        }
        // STUB: wl_seat.send_capabilities to bound seat resources.
    }

    /// meta_wayland_seat_get_input_focus()
    pub fn get_input_focus(&self) -> Option<u32> {
        self.input_focus
    }

    /// meta_wayland_seat_set_input_focus(): move keyboard focus to `surface`.
    /// Only meaningful when the seat has a keyboard.
    pub fn set_input_focus(&mut self, surface: Option<u32>) {
        self.input_focus = surface;
        if self.has_keyboard() {
            self.keyboard.set_focus(surface);
        }
        // STUB: notify text-input / data-device of the focus change.
    }

    /// meta_wayland_seat_can_popup(): a popup grab is allowed if the serial
    /// matches a recent pointer button or touch down.
    pub fn can_popup(&self, serial: u32) -> bool {
        self.pointer.can_grab(serial) || self.touch.can_grab(serial)
    }

    /// meta_wayland_seat_get_grab_info(): resolve the surface/coords that own a
    /// grab for `serial`. Returns (surface, x, y) when found. Simplified to the
    /// pointer implicit-grab path.
    pub fn get_grab_info(&self, serial: u32) -> Option<(u32, f32, f32)> {
        if self.pointer.can_grab(serial) {
            self.pointer.implicit_grab_surface().map(|s| (s, 0.0, 0.0))
        } else {
            None
        }
    }

    /// meta_wayland_seat_handle_event(): dispatch a Clutter event through the
    /// input handler stack. STUB: real code decodes ClutterEvent into the
    /// per-device motion/button/key calls; here it forwards to the input stack.
    pub fn handle_event(&self) -> bool {
        // STUB: translate ClutterEvent and route via self.input.handle_event.
        false
    }

    /// Propagate a surface destruction to every device and the input focus.
    pub fn surface_destroyed(&mut self, surface: u32) {
        self.pointer.surface_destroyed(surface);
        self.keyboard.surface_destroyed(surface);
        self.touch.surface_destroyed(surface);
        if self.input_focus == Some(surface) {
            self.input_focus = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::pointer::ButtonState;
    use super::*;

    #[test]
    fn test_capabilities() {
        let mut seat = MetaWaylandSeat::new(1);
        assert!(!seat.has_pointer());
        seat.set_capabilities(CAP_POINTER | CAP_KEYBOARD);
        assert!(seat.has_pointer());
        assert!(seat.has_keyboard());
        assert!(!seat.has_touch());
    }

    #[test]
    fn test_input_focus_requires_keyboard() {
        let mut seat = MetaWaylandSeat::new(1);
        seat.set_capabilities(CAP_KEYBOARD);
        seat.set_input_focus(Some(42));
        assert_eq!(seat.get_input_focus(), Some(42));
        assert_eq!(seat.keyboard().focus_surface(), Some(42));
    }

    #[test]
    fn test_dropping_keyboard_clears_focus() {
        let mut seat = MetaWaylandSeat::new(1);
        seat.set_capabilities(CAP_KEYBOARD);
        seat.set_input_focus(Some(7));
        seat.set_capabilities(0);
        assert_eq!(seat.get_input_focus(), None);
        assert_eq!(seat.keyboard().focus_surface(), None);
    }

    #[test]
    fn test_can_popup_via_pointer() {
        let mut seat = MetaWaylandSeat::new(1);
        seat.set_capabilities(CAP_POINTER);
        seat.pointer_mut().set_focus(Some(3));
        let serial = seat.pointer_mut().button(ButtonState::Pressed);
        assert!(seat.can_popup(serial));
        assert!(!seat.can_popup(0));
        assert_eq!(seat.get_grab_info(serial), Some((3, 0.0, 0.0)));
    }

    #[test]
    fn test_surface_destroyed_propagates() {
        let mut seat = MetaWaylandSeat::new(1);
        seat.set_capabilities(CAP_KEYBOARD | CAP_POINTER);
        seat.set_input_focus(Some(5));
        seat.pointer_mut().set_focus(Some(5));
        seat.surface_destroyed(5);
        assert_eq!(seat.get_input_focus(), None);
        assert_eq!(seat.pointer().focus_surface(), None);
    }
}
