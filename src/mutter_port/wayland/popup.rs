//! GNOME src/wayland/meta-wayland-popup.c
//!
//! Implements popup surface handling for menus, dropdowns, and other
//! temporary surfaces. Manages popup grabs and positioning.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-popup.c

use alloc::vec::Vec;

/// Represents a popup surface
pub struct PopupSurface {
    pub id: u32,
    pub surface_id: u32,
    pub parent_surface_id: u32,
    pub x: i32,
    pub y: i32,
    pub grab_active: bool,
}

impl PopupSurface {
    pub fn new(id: u32, surface_id: u32, parent_surface_id: u32, x: i32, y: i32) -> Self {
        PopupSurface {
            id,
            surface_id,
            parent_surface_id,
            x,
            y,
            grab_active: false,
        }
    }

    pub fn set_position(&mut self, x: i32, y: i32) {
        self.x = x;
        self.y = y;
    }

    pub fn get_position(&self) -> (i32, i32) {
        (self.x, self.y)
    }

    pub fn activate_grab(&mut self) {
        self.grab_active = true;
    }

    pub fn deactivate_grab(&mut self) {
        self.grab_active = false;
    }

    pub fn is_grab_active(&self) -> bool {
        self.grab_active
    }

    /// STUB: Reposition popup. Requires anchor rect, gravity, constraint
    /// adjustment, and screen boundary checking.
    pub fn reposition(&mut self) {
        // TODO: implement popup repositioning logic
    }

    /// STUB: Grab keyboard/pointer input. Requires seat integration and
    /// input routing to this popup surface.
    pub fn grab_input(&mut self, _seat_id: u32) {
        self.grab_active = true;
    }

    /// Release input grab
    pub fn ungrab_input(&mut self) {
        self.grab_active = false;
    }
}

/// Popup grab state (dismissal and input routing)
pub struct PopupGrab {
    pub id: u32,
    pub seat_id: u32,
    pub popup_id: u32,
    pub serial: u32,
}

impl PopupGrab {
    pub fn new(id: u32, seat_id: u32, popup_id: u32, serial: u32) -> Self {
        PopupGrab {
            id,
            seat_id,
            popup_id,
            serial,
        }
    }

    /// STUB: Check if click should dismiss popup. Requires surface
    /// under-pointer calculation and grab interaction logic.
    pub fn should_dismiss_on_click(&self, _x: i32, _y: i32) -> bool {
        true
    }

    /// STUB: Handle input events during popup grab. Requires event
    /// routing and pointer tracking.
    pub fn handle_event(&self) {
        // TODO: implement event handling during popup grab
    }
}

/// Manages popup surfaces
pub struct PopupManager {
    popups: alloc::collections::BTreeMap<u32, PopupSurface>,
    grabs: alloc::collections::BTreeMap<u32, PopupGrab>,
    next_popup_id: u32,
    next_grab_id: u32,
}

impl PopupManager {
    pub fn new() -> Self {
        PopupManager {
            popups: alloc::collections::BTreeMap::new(),
            grabs: alloc::collections::BTreeMap::new(),
            next_popup_id: 1,
            next_grab_id: 1,
        }
    }

    pub fn create_popup(&mut self, surface_id: u32, parent_surface_id: u32, x: i32, y: i32) -> u32 {
        let id = self.next_popup_id;
        self.next_popup_id += 1;
        let popup = PopupSurface::new(id, surface_id, parent_surface_id, x, y);
        self.popups.insert(id, popup);
        id
    }

    pub fn create_grab(&mut self, seat_id: u32, popup_id: u32, serial: u32) -> u32 {
        let id = self.next_grab_id;
        self.next_grab_id += 1;
        let grab = PopupGrab::new(id, seat_id, popup_id, serial);
        self.grabs.insert(id, grab);
        id
    }

    pub fn get_popup(&self, id: u32) -> Option<&PopupSurface> {
        self.popups.get(&id)
    }

    pub fn get_popup_mut(&mut self, id: u32) -> Option<&mut PopupSurface> {
        self.popups.get_mut(&id)
    }

    pub fn destroy_popup(&mut self, id: u32) -> bool {
        self.popups.remove(&id).is_some()
    }

    pub fn destroy_grab(&mut self, id: u32) -> bool {
        self.grabs.remove(&id).is_some()
    }

    pub fn get_grabs_for_popup(&self, popup_id: u32) -> Vec<u32> {
        self.grabs
            .values()
            .filter(|g| g.popup_id == popup_id)
            .map(|g| g.id)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_popup_creation() {
        let popup = PopupSurface::new(1, 10, 5, 100, 50);
        assert_eq!(popup.surface_id, 10);
        assert_eq!(popup.parent_surface_id, 5);
        assert_eq!(popup.get_position(), (100, 50));
        assert!(!popup.is_grab_active());
    }

    #[test]
    fn test_popup_grab() {
        let mut popup = PopupSurface::new(1, 10, 5, 100, 50);
        popup.activate_grab();
        assert!(popup.is_grab_active());

        popup.deactivate_grab();
        assert!(!popup.is_grab_active());
    }

    #[test]
    fn test_popup_manager() {
        let mut mgr = PopupManager::new();
        let pid1 = mgr.create_popup(10, 5, 100, 50);
        let pid2 = mgr.create_popup(11, 5, 150, 50);

        assert!(mgr.get_popup(pid1).is_some());
        assert_eq!(mgr.get_popup(pid1).unwrap().x, 100);

        let gid1 = mgr.create_grab(1, pid1, 0);
        let grabs = mgr.get_grabs_for_popup(pid1);
        assert_eq!(grabs.len(), 1);

        mgr.destroy_popup(pid1);
        assert!(mgr.get_popup(pid1).is_none());
    }
}
