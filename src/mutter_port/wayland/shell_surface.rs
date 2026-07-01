//! GNOME src/wayland/meta-wayland-shell-surface.c
//!
//! Implements wl_shell_surface protocol (legacy shell interface).
//! Modern applications use xdg_shell instead, but wl_shell_surface
//! is maintained for compatibility.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-shell-surface.c

use alloc::string::String;

/// Shell surface role type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellSurfaceRole {
    TopLevel,
    Transient,
    FullScreen,
    Popup,
}

/// Represents a wl_shell_surface
pub struct ShellSurface {
    pub id: u32,
    pub surface_id: u32,
    pub role: ShellSurfaceRole,
    pub title: String,
    pub class: String,
}

impl ShellSurface {
    pub fn new(id: u32, surface_id: u32) -> Self {
        ShellSurface {
            id,
            surface_id,
            role: ShellSurfaceRole::TopLevel,
            title: String::new(),
            class: String::new(),
        }
    }

    pub fn set_role(&mut self, role: ShellSurfaceRole) {
        self.role = role;
    }

    pub fn get_role(&self) -> ShellSurfaceRole {
        self.role
    }

    pub fn set_title(&mut self, title: impl Into<String>) {
        self.title = title.into();
    }

    pub fn get_title(&self) -> &str {
        &self.title
    }

    pub fn set_class(&mut self, class: impl Into<String>) {
        self.class = class.into();
    }

    pub fn get_class(&self) -> &str {
        &self.class
    }

    /// STUB: Handle set_toplevel request. Requires window manager
    /// integration for surface placement and decoration.
    pub fn set_toplevel(&mut self) {
        self.role = ShellSurfaceRole::TopLevel;
    }

    /// STUB: Handle set_transient request. Requires parent window tracking
    /// and modal dialog behavior.
    pub fn set_transient(&mut self, _parent_surface_id: u32, _x: i32, _y: i32) {
        self.role = ShellSurfaceRole::Transient;
    }

    /// STUB: Handle set_fullscreen request. Requires output selection,
    /// surface scaling, and fullscreen mode handling.
    pub fn set_fullscreen(&mut self, _output_id: Option<u32>) {
        self.role = ShellSurfaceRole::FullScreen;
    }

    /// STUB: Handle set_popup request. Requires grab handling and
    /// input routing to popup surface.
    pub fn set_popup(&mut self, _seat_id: u32, _serial: u32, _x: i32, _y: i32) {
        self.role = ShellSurfaceRole::Popup;
    }
}

/// Manages shell surfaces
pub struct ShellSurfaceManager {
    surfaces: alloc::collections::BTreeMap<u32, ShellSurface>,
    next_id: u32,
}

impl ShellSurfaceManager {
    pub fn new() -> Self {
        ShellSurfaceManager {
            surfaces: alloc::collections::BTreeMap::new(),
            next_id: 1,
        }
    }

    pub fn create_shell_surface(&mut self, surface_id: u32) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        let shell_surface = ShellSurface::new(id, surface_id);
        self.surfaces.insert(id, shell_surface);
        id
    }

    pub fn get_shell_surface(&self, id: u32) -> Option<&ShellSurface> {
        self.surfaces.get(&id)
    }

    pub fn get_shell_surface_mut(&mut self, id: u32) -> Option<&mut ShellSurface> {
        self.surfaces.get_mut(&id)
    }

    pub fn destroy_shell_surface(&mut self, id: u32) -> bool {
        self.surfaces.remove(&id).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shell_surface_creation() {
        let shell = ShellSurface::new(1, 10);
        assert_eq!(shell.id, 1);
        assert_eq!(shell.surface_id, 10);
        assert_eq!(shell.get_role(), ShellSurfaceRole::TopLevel);
    }

    #[test]
    fn test_shell_surface_properties() {
        let mut shell = ShellSurface::new(1, 10);

        shell.set_title("Test Window");
        shell.set_class("test-app");

        assert_eq!(shell.get_title(), "Test Window");
        assert_eq!(shell.get_class(), "test-app");
    }

    #[test]
    fn test_shell_surface_manager() {
        let mut mgr = ShellSurfaceManager::new();
        let id1 = mgr.create_shell_surface(1);
        let id2 = mgr.create_shell_surface(2);

        assert!(mgr.get_shell_surface(id1).is_some());
        assert!(mgr.destroy_shell_surface(id1));
        assert!(mgr.get_shell_surface(id1).is_none());
        assert!(mgr.get_shell_surface(id2).is_some());
    }
}
