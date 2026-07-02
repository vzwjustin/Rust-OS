//! Wayland XDG Shell module
//!
//! Implements xdg_shell protocol (xdg-shell-v6+). Modern shell interface
//! providing application windows (xdg_toplevel) and popups (xdg_popup).
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-xdg-shell.h

use alloc::{collections::BTreeMap, string::String, vec::Vec};

/// The role bound to an xdg_surface via get_toplevel / get_popup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XdgSurfaceRole {
    /// No role assigned yet (xdg_surface created but not specialized).
    None,
    /// Bound as an xdg_toplevel (id).
    Toplevel(u32),
    /// Bound as an xdg_popup (id).
    Popup(u32),
}

/// Window geometry set via xdg_surface.set_window_geometry: the region of the
/// buffer the client considers "the window" (excludes client-side shadows).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WindowGeometry {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// An xdg_surface: the shared base for both toplevels and popups. It owns the
/// configure/ack serial state machine.
///
/// The compositor sends configure events (each with a fresh serial); the client
/// must reply with ack_configure(serial). Only an acked configure may be applied
/// on the next wl_surface.commit.
pub struct XdgSurface {
    pub id: u32,
    pub surface_id: u32,
    pub role: XdgSurfaceRole,
    pub geometry: Option<WindowGeometry>,
    /// True once the client has committed at least one buffer.
    pub configured: bool,
    /// Serial of the most recent configure we sent (0 = none pending).
    pub pending_serial: u32,
    /// Serial of the last configure the client acknowledged.
    pub acked_serial: u32,
}

impl XdgSurface {
    pub fn new(id: u32, surface_id: u32) -> Self {
        XdgSurface {
            id,
            surface_id,
            role: XdgSurfaceRole::None,
            geometry: None,
            configured: false,
            pending_serial: 0,
            acked_serial: 0,
        }
    }

    pub fn set_window_geometry(&mut self, x: i32, y: i32, width: i32, height: i32) {
        self.geometry = Some(WindowGeometry {
            x,
            y,
            width,
            height,
        });
    }

    /// STUB: emit an xdg_surface.configure wire event. Returns the serial the
    /// client must ack. Real impl serializes toplevel/popup state first.
    pub fn send_configure(&mut self, serial: u32) -> u32 {
        self.pending_serial = serial;
        self.configured = true;
        serial
    }

    /// Handle xdg_surface.ack_configure(serial).
    pub fn ack_configure(&mut self, serial: u32) {
        self.acked_serial = serial;
    }

    /// Whether the latest sent configure has been acknowledged.
    pub fn is_acked(&self) -> bool {
        self.pending_serial != 0 && self.acked_serial >= self.pending_serial
    }
}

/// XDG toplevel window state flags
pub mod toplevel_state {
    pub const RESIZING: u32 = 1 << 0;
    pub const ACTIVATED: u32 = 1 << 1;
    pub const MAXIMIZED: u32 = 1 << 2;
    pub const FULLSCREEN: u32 = 1 << 3;
    pub const TILED_LEFT: u32 = 1 << 4;
    pub const TILED_RIGHT: u32 = 1 << 5;
    pub const TILED_TOP: u32 = 1 << 6;
    pub const TILED_BOTTOM: u32 = 1 << 7;
}

/// Represents an xdg_toplevel surface
pub struct XdgToplevel {
    pub id: u32,
    pub surface_id: u32,
    pub title: String,
    pub app_id: String,
    pub state: u32,
    pub min_width: u32,
    pub min_height: u32,
    pub max_width: u32,
    pub max_height: u32,
}

impl XdgToplevel {
    pub fn new(id: u32, surface_id: u32) -> Self {
        XdgToplevel {
            id,
            surface_id,
            title: String::new(),
            app_id: String::new(),
            state: 0,
            min_width: 0,
            min_height: 0,
            max_width: 0,
            max_height: 0,
        }
    }

    pub fn set_title(&mut self, title: impl Into<String>) {
        self.title = title.into();
    }

    pub fn set_app_id(&mut self, app_id: impl Into<String>) {
        self.app_id = app_id.into();
    }

    pub fn set_min_size(&mut self, width: u32, height: u32) {
        self.min_width = width;
        self.min_height = height;
    }

    pub fn set_max_size(&mut self, width: u32, height: u32) {
        self.max_width = width;
        self.max_height = height;
    }

    pub fn add_state(&mut self, state: u32) {
        self.state |= state;
    }

    pub fn remove_state(&mut self, state: u32) {
        self.state &= !state;
    }

    pub fn has_state(&self, state: u32) -> bool {
        (self.state & state) != 0
    }

    pub fn is_activated(&self) -> bool {
        self.has_state(toplevel_state::ACTIVATED)
    }

    pub fn is_maximized(&self) -> bool {
        self.has_state(toplevel_state::MAXIMIZED)
    }

    pub fn is_fullscreen(&self) -> bool {
        self.has_state(toplevel_state::FULLSCREEN)
    }

    /// STUB: Request move operation. Requires grab handling and
    /// pointer tracking for window dragging.
    pub fn request_move(&self, _seat_id: u32, _serial: u32) {
        // ponytail: real impl requires seat grab and pointer tracking
    }

    /// STUB: Request resize operation. Requires edge detection and
    /// size constraint application.
    pub fn request_resize(&self, _seat_id: u32, _serial: u32, _edges: u32) {
        // ponytail: real impl requires edge detection and size constraint logic
    }
}

/// Represents an xdg_popup surface
pub struct XdgPopup {
    pub id: u32,
    pub surface_id: u32,
    pub parent_surface_id: u32,
    pub positioner_id: u32,
    pub geometry: PopupGeometry,
}

/// Popup geometry constraints
#[derive(Debug, Clone, Copy)]
pub struct PopupGeometry {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl XdgPopup {
    pub fn new(id: u32, surface_id: u32, parent_surface_id: u32, positioner_id: u32) -> Self {
        XdgPopup {
            id,
            surface_id,
            parent_surface_id,
            positioner_id,
            geometry: PopupGeometry {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            },
        }
    }

    pub fn set_geometry(&mut self, x: i32, y: i32, width: u32, height: u32) {
        self.geometry = PopupGeometry {
            x,
            y,
            width,
            height,
        };
    }

    /// STUB: Grab input. Requires seat integration and popup dismiss handling.
    pub fn grab(&self, _seat_id: u32, _serial: u32) {
        // ponytail: real impl requires seat grab and popup dismiss logic
    }
}

/// xdg_positioner for popup constraint calculation
#[derive(Debug, Clone)]
pub struct XdgPositioner {
    pub id: u32,
    pub anchor_rect: (i32, i32, u32, u32),
    pub anchor: u32,
    pub gravity: u32,
    pub constraint_adjustment: u32,
    pub offset_x: i32,
    pub offset_y: i32,
    pub width: u32,
    pub height: u32,
}

impl XdgPositioner {
    pub fn new(id: u32) -> Self {
        XdgPositioner {
            id,
            anchor_rect: (0, 0, 0, 0),
            anchor: 0,
            gravity: 0,
            constraint_adjustment: 0,
            offset_x: 0,
            offset_y: 0,
            width: 0,
            height: 0,
        }
    }

    /// STUB: Calculate popup position. Requires monitor geometry,
    /// constraint application, and gravity calculation.
    pub fn calculate_position(&self) -> (i32, i32) {
        (self.offset_x, self.offset_y)
    }
}

/// Manages xdg_shell surfaces
pub struct XdgShellManager {
    toplevels: BTreeMap<u32, XdgToplevel>,
    popups: BTreeMap<u32, XdgPopup>,
    positioners: BTreeMap<u32, XdgPositioner>,
    next_toplevel_id: u32,
    next_popup_id: u32,
    next_positioner_id: u32,
}

impl XdgShellManager {
    pub fn new() -> Self {
        XdgShellManager {
            toplevels: BTreeMap::new(),
            popups: BTreeMap::new(),
            positioners: BTreeMap::new(),
            next_toplevel_id: 1,
            next_popup_id: 1,
            next_positioner_id: 1,
        }
    }

    pub fn create_toplevel(&mut self, surface_id: u32) -> u32 {
        let id = self.next_toplevel_id;
        self.next_toplevel_id += 1;
        let toplevel = XdgToplevel::new(id, surface_id);
        self.toplevels.insert(id, toplevel);
        id
    }

    pub fn create_popup(
        &mut self,
        surface_id: u32,
        parent_surface_id: u32,
        positioner_id: u32,
    ) -> u32 {
        let id = self.next_popup_id;
        self.next_popup_id += 1;
        let popup = XdgPopup::new(id, surface_id, parent_surface_id, positioner_id);
        self.popups.insert(id, popup);
        id
    }

    pub fn create_positioner(&mut self) -> u32 {
        let id = self.next_positioner_id;
        self.next_positioner_id += 1;
        let positioner = XdgPositioner::new(id);
        self.positioners.insert(id, positioner);
        id
    }

    pub fn get_toplevel(&self, id: u32) -> Option<&XdgToplevel> {
        self.toplevels.get(&id)
    }

    pub fn get_toplevel_mut(&mut self, id: u32) -> Option<&mut XdgToplevel> {
        self.toplevels.get_mut(&id)
    }

    pub fn get_popup(&self, id: u32) -> Option<&XdgPopup> {
        self.popups.get(&id)
    }

    pub fn get_positioner(&self, id: u32) -> Option<&XdgPositioner> {
        self.positioners.get(&id)
    }

    pub fn destroy_toplevel(&mut self, id: u32) -> bool {
        self.toplevels.remove(&id).is_some()
    }

    pub fn destroy_popup(&mut self, id: u32) -> bool {
        self.popups.remove(&id).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xdg_toplevel() {
        let mut tl = XdgToplevel::new(1, 10);

        tl.set_title("App Window");
        tl.set_app_id("com.example.app");
        tl.add_state(toplevel_state::ACTIVATED);

        assert_eq!(tl.title.as_str(), "App Window");
        assert!(tl.is_activated());
    }

    #[test]
    fn test_xdg_popup() {
        let popup = XdgPopup::new(1, 11, 10, 50);
        assert_eq!(popup.surface_id, 11);
        assert_eq!(popup.parent_surface_id, 10);
    }

    #[test]
    fn test_xdg_surface_configure_ack() {
        let mut xs = XdgSurface::new(1, 10);
        assert!(!xs.configured);
        xs.set_window_geometry(0, 0, 640, 480);
        assert_eq!(xs.geometry.unwrap().width, 640);

        xs.send_configure(42);
        assert!(xs.configured);
        assert!(!xs.is_acked());

        xs.ack_configure(42);
        assert!(xs.is_acked());
    }

    #[test]
    fn test_xdg_shell_manager() {
        let mut mgr = XdgShellManager::new();
        let tlid = mgr.create_toplevel(10);
        let posid = mgr.create_positioner();
        let popid = mgr.create_popup(11, 10, posid);

        assert!(mgr.get_toplevel(tlid).is_some());
        assert!(mgr.get_popup(popid).is_some());
        assert!(mgr.get_positioner(posid).is_some());
    }
}
