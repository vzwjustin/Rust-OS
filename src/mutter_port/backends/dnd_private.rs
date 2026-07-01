//! Dnd Private ported from GNOME Mutter's src/backends/
//!
//! Wayland drag-and-drop event handling. Manages modal state during DnD
//! and routes motion events to drop target handlers.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-dnd-private.c

/// Private Drag-and-Drop management.
pub struct MetaDnd;

impl MetaDnd {
    /// Begin DnD modal grab (prevent other input).
    pub fn wayland_handle_begin_modal() {
        // TODO: Enter modal state, suspend normal input
    }

    /// End DnD modal grab.
    pub fn wayland_handle_end_modal() {
        // TODO: Exit modal state, restore input handling
    }

    /// Route motion event to DnD handler.
    pub fn wayland_on_motion_event() {
        // TODO: Update drop target, emit motion event
    }
}
