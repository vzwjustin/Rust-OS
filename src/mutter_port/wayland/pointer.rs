//! GNOME src/wayland/meta-wayland-pointer.c
//!
//! MetaWaylandPointer implements the wl_pointer half of a seat: it tracks which
//! surface currently has pointer focus, the implicit (button) grab, the cursor
//! surface/shape, the button count, and the serials used for enter/leave/button
//! events. The C file is ~1670 lines, most of which is Clutter event plumbing,
//! per-client resource bookkeeping and cursor rendering; we model the state
//! machine (focus + implicit grab + button counting) and leave the wire/render
//! bits as stubs.
//!
//! https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-pointer.c

use super::input_device::{next_serial, MetaWaylandInputDevice};

/// Where the cursor image is sourced from (mirrors CursorSource in the C file).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorSource {
    /// Client set a wl_surface as the cursor.
    Surface,
    /// Client requested a named/shape cursor (cursor-shape-v1).
    Shape,
    /// No client cursor; compositor default.
    Default,
}

/// wl_pointer button state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonState {
    Released,
    Pressed,
}

/// MetaWaylandPointer
pub struct MetaWaylandPointer {
    parent: MetaWaylandInputDevice,

    /// Surface that currently owns pointer focus (wl_pointer.enter target).
    focus_surface: Option<u32>,
    /// Serial of the last enter sent to `focus_surface`.
    focus_serial: u32,
    /// Serial of the last button press that could start a click/drag.
    click_serial: u32,

    /// Surface holding the implicit grab (set on first button press, cleared
    /// when the last button is released).
    implicit_grab_surface: Option<u32>,
    /// Serial that opened the implicit grab.
    grab_serial: u32,
    grab_x: f32,
    grab_y: f32,

    /// Surface under the pointer regardless of focus/grab (Clutter "current").
    current: Option<u32>,

    /// Client-provided cursor surface, if any.
    cursor_surface: Option<u32>,
    cursor_source: CursorSource,
    /// Named cursor shape id (cursor-shape-v1) when `cursor_source == Shape`.
    cursor_shape: u32,

    /// Last relative motion coordinates.
    last_rel_x: f32,
    last_rel_y: f32,

    /// Number of currently-pressed buttons; the implicit grab lives while > 0.
    button_count: u32,
}

impl MetaWaylandPointer {
    pub fn new(seat: u32) -> Self {
        MetaWaylandPointer {
            parent: MetaWaylandInputDevice::new(seat),
            focus_surface: None,
            focus_serial: 0,
            click_serial: 0,
            implicit_grab_surface: None,
            grab_serial: 0,
            grab_x: 0.0,
            grab_y: 0.0,
            current: None,
            cursor_surface: None,
            cursor_source: CursorSource::Default,
            cursor_shape: 0,
            last_rel_x: 0.0,
            last_rel_y: 0.0,
            button_count: 0,
        }
    }

    pub fn seat(&self) -> u32 {
        self.parent.get_seat()
    }

    pub fn focus_surface(&self) -> Option<u32> {
        self.focus_surface
    }

    pub fn focus_serial(&self) -> u32 {
        self.focus_serial
    }

    pub fn button_count(&self) -> u32 {
        self.button_count
    }

    pub fn implicit_grab_surface(&self) -> Option<u32> {
        self.implicit_grab_surface
    }

    /// meta_wayland_pointer_set_current(): surface directly under the pointer.
    pub fn set_current(&mut self, surface: Option<u32>) {
        self.current = surface;
    }

    pub fn current(&self) -> Option<u32> {
        self.current
    }

    /// meta_wayland_pointer_set_focus(): move wl_pointer focus to `surface`,
    /// emitting leave on the old focus and enter on the new one. Returns the
    /// enter serial (0 when clearing focus). While an implicit grab is held
    /// focus is pinned to the grab surface, so callers gate on button_count.
    pub fn set_focus(&mut self, surface: Option<u32>) -> u32 {
        if self.focus_surface == surface {
            return self.focus_serial;
        }
        // STUB: wl_pointer.leave on old focus resources.
        self.focus_surface = surface;
        if surface.is_some() {
            self.focus_serial = next_serial();
            // STUB: wl_pointer.enter + frame on new focus resources.
        } else {
            self.focus_serial = 0;
        }
        self.focus_serial
    }

    /// Handle relative motion. Only forwarded while there is a focus surface.
    /// Returns true if an event would be delivered.
    pub fn motion(&mut self, x: f32, y: f32) -> bool {
        self.last_rel_x = x;
        self.last_rel_y = y;
        // STUB: wl_pointer.motion + frame to focus resources.
        self.focus_surface.is_some()
    }

    /// meta_wayland_pointer_send_button() path. Maintains button_count and the
    /// implicit grab. Returns the serial assigned to this button event.
    pub fn button(&mut self, state: ButtonState) -> u32 {
        let serial = next_serial();
        match state {
            ButtonState::Pressed => {
                if self.button_count == 0 {
                    // First button down opens the implicit grab on the focus.
                    self.implicit_grab_surface = self.focus_surface;
                    self.grab_serial = serial;
                    self.grab_x = self.last_rel_x;
                    self.grab_y = self.last_rel_y;
                }
                self.button_count += 1;
                self.click_serial = serial;
            }
            ButtonState::Released => {
                if self.button_count > 0 {
                    self.button_count -= 1;
                }
                if self.button_count == 0 {
                    // Last button up ends the implicit grab.
                    self.implicit_grab_surface = None;
                }
            }
        }
        // STUB: wl_pointer.button + frame to focus resources.
        serial
    }

    /// meta_wayland_pointer_set_cursor_surface() via wl_pointer.set_cursor.
    /// A None surface hides the cursor. Only honoured for the client owning the
    /// enter serial in the real code (validated by the caller here).
    pub fn set_cursor_surface(&mut self, surface: Option<u32>) {
        self.cursor_surface = surface;
        self.cursor_source = if surface.is_some() {
            CursorSource::Surface
        } else {
            CursorSource::Default
        };
    }

    /// cursor-shape-v1: request a named cursor shape.
    pub fn set_cursor_shape(&mut self, shape: u32) {
        self.cursor_shape = shape;
        self.cursor_source = CursorSource::Shape;
        self.cursor_surface = None;
    }

    pub fn cursor_source(&self) -> CursorSource {
        self.cursor_source
    }

    /// meta_wayland_pointer_can_grab_surface() / serial validation used to
    /// authorise popups and drags: the serial must match a recent button/enter.
    pub fn can_grab(&self, serial: u32) -> bool {
        serial != 0 && (serial == self.grab_serial || serial == self.click_serial)
    }

    /// Called when a focused/grabbed surface is destroyed.
    pub fn surface_destroyed(&mut self, surface: u32) {
        if self.focus_surface == Some(surface) {
            self.focus_surface = None;
            self.focus_serial = 0;
        }
        if self.implicit_grab_surface == Some(surface) {
            self.implicit_grab_surface = None;
        }
        if self.current == Some(surface) {
            self.current = None;
        }
        if self.cursor_surface == Some(surface) {
            self.cursor_surface = None;
            self.cursor_source = CursorSource::Default;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_focus_assigns_serial() {
        let mut p = MetaWaylandPointer::new(1);
        let s = p.set_focus(Some(42));
        assert_eq!(p.focus_surface(), Some(42));
        assert!(s > 0);
        // Re-focusing same surface is a no-op returning the same serial.
        assert_eq!(p.set_focus(Some(42)), s);
    }

    #[test]
    fn test_implicit_grab_lifecycle() {
        let mut p = MetaWaylandPointer::new(1);
        p.set_focus(Some(10));
        assert_eq!(p.implicit_grab_surface(), None);

        p.button(ButtonState::Pressed);
        assert_eq!(p.button_count(), 1);
        assert_eq!(p.implicit_grab_surface(), Some(10));

        // Second button keeps the grab pinned to the original surface.
        p.set_focus(Some(10));
        p.button(ButtonState::Pressed);
        assert_eq!(p.button_count(), 2);

        p.button(ButtonState::Released);
        assert_eq!(p.implicit_grab_surface(), Some(10));
        p.button(ButtonState::Released);
        assert_eq!(p.button_count(), 0);
        assert_eq!(p.implicit_grab_surface(), None);
    }

    #[test]
    fn test_grab_serial_authorises_popup() {
        let mut p = MetaWaylandPointer::new(1);
        p.set_focus(Some(5));
        let serial = p.button(ButtonState::Pressed);
        assert!(p.can_grab(serial));
        assert!(!p.can_grab(0));
        assert!(!p.can_grab(serial + 999));
    }

    #[test]
    fn test_cursor_source() {
        let mut p = MetaWaylandPointer::new(1);
        p.set_cursor_surface(Some(3));
        assert_eq!(p.cursor_source(), CursorSource::Surface);
        p.set_cursor_shape(2);
        assert_eq!(p.cursor_source(), CursorSource::Shape);
        p.set_cursor_surface(None);
        assert_eq!(p.cursor_source(), CursorSource::Default);
    }

    #[test]
    fn test_surface_destroyed_clears_state() {
        let mut p = MetaWaylandPointer::new(1);
        p.set_focus(Some(9));
        p.button(ButtonState::Pressed);
        p.surface_destroyed(9);
        assert_eq!(p.focus_surface(), None);
        assert_eq!(p.implicit_grab_surface(), None);
    }
}
