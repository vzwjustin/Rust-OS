//! GNOME src/wayland/meta-wayland.c
//!
//! MetaWaylandCompositor is the main Wayland compositor, responsible for
//! managing clients, surfaces, seats, and protocol extensions. It coordinates
//! all Wayland protocol operations and integrates with the desktop shell.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland.c

use alloc::{collections::BTreeMap, string::String, vec::Vec};
use core::sync::atomic::{AtomicU32, Ordering};

use crate::desktop::window_manager::WindowId;

/// Wayland compositor context
pub struct WaylandCompositor {
    pub id: u32,
    pub name: String,
    seats: BTreeMap<u32, WaylandSeat>,
    surfaces: BTreeMap<u32, u32>, // surface_id -> client_id
    frame_callbacks: Vec<u32>,
    next_seat_id: AtomicU32,
}

/// Wayland seat (input device group)
pub struct WaylandSeat {
    pub id: u32,
    pub name: String,
    pub capabilities: u32,
}

impl WaylandCompositor {
    pub fn new(name: impl Into<String>) -> Self {
        WaylandCompositor {
            id: 1,
            name: name.into(),
            seats: BTreeMap::new(),
            surfaces: BTreeMap::new(),
            frame_callbacks: Vec::new(),
            next_seat_id: AtomicU32::new(1),
        }
    }

    /// Create or retrieve a seat for input handling
    pub fn ensure_seat(&mut self, name: impl Into<String>) -> u32 {
        let seat_name = name.into();
        let next_id = self.next_seat_id.fetch_add(1, Ordering::Release);

        if let Some(seat) = self.seats.values().find(|s| s.name == seat_name) {
            return seat.id;
        }

        let seat = WaylandSeat {
            id: next_id,
            name: seat_name,
            capabilities: 7, // WL_SEAT_CAPABILITY_POINTER | KEYBOARD | TOUCH
        };
        self.seats.insert(next_id, seat);
        next_id
    }

    pub fn get_seat(&self, id: u32) -> Option<&WaylandSeat> {
        self.seats.get(&id)
    }

    /// Register a new surface on this compositor
    pub fn add_surface(&mut self, surface_id: u32, client_id: u32) {
        self.surfaces.insert(surface_id, client_id);
    }

    /// Unregister a surface
    pub fn remove_surface(&mut self, surface_id: u32) -> bool {
        self.surfaces.remove(&surface_id).is_some()
    }

    pub fn get_surface_client(&self, surface_id: u32) -> Option<u32> {
        self.surfaces.get(&surface_id).copied()
    }

    /// Request frame callback (vblank-synchronized redraw)
    pub fn request_frame(&mut self, surface_id: u32) -> u32 {
        let callback_id = self.frame_callbacks.len() as u32;
        self.frame_callbacks.push(surface_id);
        callback_id
    }

    /// STUB: Process pending frame callbacks.
    /// Real implementation requires integration with renderer vblank events
    /// and surface repaint scheduling.
    pub fn flush_frame_callbacks(&mut self) -> Vec<u32> {
        core::mem::take(&mut self.frame_callbacks)
    }

    /// STUB: Handle compositor shutdown and client cleanup.
    /// Requires graceful disconnect and resource release.
    pub fn shutdown(&mut self) {
        self.seats.clear();
        self.surfaces.clear();
        self.frame_callbacks.clear();
    }
}

/// Signal types for Wayland compositor events
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaylandSignal {
    /// Emitted before compositor shutdown
    PrepareShutdown,
}

/// STUB: Protocol extensions registry. Real implementation would track
/// protocol versions, wl_registry.bind() operations, and extension capabilities
/// (e.g., wp_viewporter, wp_presentation_time, xdg_shell, xdg_toplevel_drag).
pub struct WaylandProtocolExtensions {
    pub features: u32,
}

impl WaylandProtocolExtensions {
    pub fn new() -> Self {
        WaylandProtocolExtensions { features: 0 }
    }

    pub fn has_xdg_shell(&self) -> bool {
        (self.features & 0x01) != 0
    }

    pub fn has_viewporter(&self) -> bool {
        (self.features & 0x02) != 0
    }

    pub fn has_presentation_time(&self) -> bool {
        (self.features & 0x04) != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compositor_creation() {
        let comp = WaylandCompositor::new("test-wayland");
        assert_eq!(comp.name.as_str(), "test-wayland");
        assert_eq!(comp.id, 1);
    }

    #[test]
    fn test_seat_management() {
        let mut comp = WaylandCompositor::new("test");
        let seat_id = comp.ensure_seat("seat0");

        assert!(comp.get_seat(seat_id).is_some());
        let seat = comp.get_seat(seat_id).unwrap();
        assert_eq!(seat.name.as_str(), "seat0");
    }

    #[test]
    fn test_surface_management() {
        let mut comp = WaylandCompositor::new("test");
        comp.add_surface(1, 100);

        assert_eq!(comp.get_surface_client(1), Some(100));
        assert!(comp.remove_surface(1));
        assert_eq!(comp.get_surface_client(1), None);
    }

    #[test]
    fn test_frame_callbacks() {
        let mut comp = WaylandCompositor::new("test");
        let cb1 = comp.request_frame(1);
        let cb2 = comp.request_frame(1);

        assert_eq!(cb1, 0);
        assert_eq!(cb2, 1);

        let callbacks = comp.flush_frame_callbacks();
        assert_eq!(callbacks.len(), 2);
    }
}
