//! Dnd Private ported from GNOME Mutter's src/backends/
//!
//! Wayland drag-and-drop event handling. Manages modal state during DnD
//! and routes motion events to drop target handlers.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-dnd-private.c

use core::cell::Cell;

/// Private Drag-and-Drop management.
pub struct MetaDnd {
    /// Whether DnD is in modal grab state.
    in_modal: Cell<bool>,
    /// Current drop target x coordinate.
    drop_x: Cell<i32>,
    /// Current drop target y coordinate.
    drop_y: Cell<i32>,
}

impl MetaDnd {
    /// Create a new DnD state tracker.
    pub fn new() -> Self {
        Self {
            in_modal: Cell::new(false),
            drop_x: Cell::new(0),
            drop_y: Cell::new(0),
        }
    }

    /// Begin DnD modal grab (prevent other input).
    pub fn wayland_handle_begin_modal(&self) {
        self.in_modal.set(true);
    }

    /// End DnD modal grab.
    pub fn wayland_handle_end_modal(&self) {
        self.in_modal.set(false);
    }

    /// Whether DnD modal grab is active.
    pub fn is_in_modal(&self) -> bool {
        self.in_modal.get()
    }

    /// Route motion event to DnD handler. Updates the drop target
    /// coordinates. A full implementation would emit the motion event
    /// to the Wayland data device manager.
    pub fn wayland_on_motion_event(&self, x: i32, y: i32) {
        if self.in_modal.get() {
            self.drop_x.set(x);
            self.drop_y.set(y);
        }
    }

    /// Get the current drop target coordinates.
    pub fn get_drop_position(&self) -> (i32, i32) {
        (self.drop_x.get(), self.drop_y.get())
    }
}

impl Default for MetaDnd {
    fn default() -> Self {
        Self::new()
    }
}
