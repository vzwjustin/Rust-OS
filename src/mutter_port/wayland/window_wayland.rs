//! GNOME src/wayland/meta-window-wayland.c
//!
//! Represents a MetaWindow for Wayland clients. Bridges between Wayland
//! protocol surfaces and the window manager's window abstraction.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-window-wayland.c

use alloc::string::String;
use alloc::vec::Vec;

/// Wayland window states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowState {
    Normal,
    Maximized,
    Minimized,
    Fullscreen,
    TiledLeft,
    TiledRight,
}

/// Represents a window for a Wayland client
pub struct WaylandWindow {
    pub id: u32,
    pub surface_id: u32,
    pub title: String,
    pub state: WindowState,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub resizable: bool,
    pub focus: bool,
    /// Serials of configures sent to the client but not yet acked
    /// (see window_configuration.rs). Faithful to mutter's
    /// MetaWindowWayland::pending_configurations list, referenced by serial.
    pub pending_configurations: Vec<u32>,
    /// Last serial the client acknowledged via ack_configure.
    pub last_acked_serial: u32,
}

impl WaylandWindow {
    pub fn new(id: u32, surface_id: u32) -> Self {
        WaylandWindow {
            id,
            surface_id,
            title: String::new(),
            state: WindowState::Normal,
            x: 0,
            y: 0,
            width: 800,
            height: 600,
            resizable: true,
            focus: false,
            pending_configurations: Vec::new(),
            last_acked_serial: 0,
        }
    }

    /// Record that a configure with `serial` was sent to the client
    /// (queued until acknowledged).
    pub fn push_configuration(&mut self, serial: u32) {
        self.pending_configurations.push(serial);
    }

    /// Handle ack_configure(serial): drop every queued configure up to and
    /// including `serial`. Returns true if the serial matched a pending entry.
    pub fn acknowledge_configuration(&mut self, serial: u32) -> bool {
        let matched = self.pending_configurations.contains(&serial);
        if matched {
            self.last_acked_serial = serial;
            self.pending_configurations.retain(|&s| s > serial);
        }
        matched
    }

    /// Whether all sent configures have been acknowledged.
    pub fn is_configured(&self) -> bool {
        self.pending_configurations.is_empty()
    }

    pub fn set_title(&mut self, title: impl Into<String>) {
        self.title = title.into();
    }

    pub fn get_title(&self) -> &str {
        &self.title
    }

    pub fn set_position(&mut self, x: i32, y: i32) {
        self.x = x;
        self.y = y;
    }

    pub fn get_position(&self) -> (i32, i32) {
        (self.x, self.y)
    }

    pub fn set_size(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
    }

    pub fn get_size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub fn set_state(&mut self, state: WindowState) {
        self.state = state;
    }

    pub fn get_state(&self) -> WindowState {
        self.state
    }

    pub fn set_resizable(&mut self, resizable: bool) {
        self.resizable = resizable;
    }

    pub fn is_resizable(&self) -> bool {
        self.resizable
    }

    pub fn set_focus(&mut self, focus: bool) {
        self.focus = focus;
    }

    pub fn has_focus(&self) -> bool {
        self.focus
    }

    /// STUB: Maximize window. Requires monitor geometry calculation
    /// and geometry constraints.
    pub fn maximize(&mut self) {
        self.state = WindowState::Maximized;
    }

    /// STUB: Minimize window. Requires window hiding and taskbar updates.
    pub fn minimize(&mut self) {
        self.state = WindowState::Minimized;
    }

    /// STUB: Enter fullscreen. Requires output selection and mode setting.
    pub fn set_fullscreen(&mut self, _output_id: Option<u32>) {
        self.state = WindowState::Fullscreen;
    }

    /// Leave fullscreen
    pub fn unset_fullscreen(&mut self) {
        self.state = WindowState::Normal;
    }

    /// STUB: Update window bounds. Requires constraint application and
    /// frame prediction.
    pub fn update_bounds(&mut self, _configure_serial: u32) {
        // TODO: implement bounds update
    }

    /// STUB: Send geometry configuration to client. Requires
    /// xdg_toplevel/zxdg_toplevel_v6 protocol updates.
    pub fn request_configure(&self) -> Option<(u32, u32)> {
        Some((self.width, self.height))
    }

    pub fn can_close(&self) -> bool {
        true
    }

    pub fn close(&mut self) {
        // TODO: implement close logic
    }
}

/// Manager for Wayland windows
pub struct WindowManager {
    windows: alloc::collections::BTreeMap<u32, WaylandWindow>,
    surface_to_window: alloc::collections::BTreeMap<u32, u32>,
    next_id: u32,
}

impl WindowManager {
    pub fn new() -> Self {
        WindowManager {
            windows: alloc::collections::BTreeMap::new(),
            surface_to_window: alloc::collections::BTreeMap::new(),
            next_id: 1,
        }
    }

    pub fn create_window(&mut self, surface_id: u32) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        let window = WaylandWindow::new(id, surface_id);
        self.windows.insert(id, window);
        self.surface_to_window.insert(surface_id, id);
        id
    }

    pub fn get_window(&self, id: u32) -> Option<&WaylandWindow> {
        self.windows.get(&id)
    }

    pub fn get_window_mut(&mut self, id: u32) -> Option<&mut WaylandWindow> {
        self.windows.get_mut(&id)
    }

    pub fn get_window_for_surface(&self, surface_id: u32) -> Option<&WaylandWindow> {
        self.surface_to_window
            .get(&surface_id)
            .and_then(|id| self.windows.get(id))
    }

    pub fn destroy_window(&mut self, id: u32) -> bool {
        if let Some(window) = self.windows.remove(&id) {
            self.surface_to_window.remove(&window.surface_id);
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_window_creation() {
        let window = WaylandWindow::new(1, 10);
        assert_eq!(window.id, 1);
        assert_eq!(window.surface_id, 10);
        assert_eq!(window.get_state(), WindowState::Normal);
    }

    #[test]
    fn test_window_properties() {
        let mut window = WaylandWindow::new(1, 10);

        window.set_title("Test");
        window.set_position(100, 50);
        window.set_size(1920, 1080);

        assert_eq!(window.get_title(), "Test");
        assert_eq!(window.get_position(), (100, 50));
        assert_eq!(window.get_size(), (1920, 1080));
    }

    #[test]
    fn test_configure_ack_queue() {
        let mut window = WaylandWindow::new(1, 10);
        window.push_configuration(5);
        window.push_configuration(7);
        assert!(!window.is_configured());

        // Acking 7 drops both 5 and 7 (everything <= 7).
        assert!(window.acknowledge_configuration(7));
        assert_eq!(window.last_acked_serial, 7);
        assert!(window.is_configured());

        // Unknown serial does not match.
        assert!(!window.acknowledge_configuration(99));
    }

    #[test]
    fn test_window_manager() {
        let mut mgr = WindowManager::new();
        let wid1 = mgr.create_window(10);
        let wid2 = mgr.create_window(20);

        assert!(mgr.get_window(wid1).is_some());
        assert_eq!(mgr.get_window_for_surface(10).unwrap().id, wid1);

        assert!(mgr.destroy_window(wid1));
        assert!(mgr.get_window(wid1).is_none());
        assert!(mgr.get_window(wid2).is_some());
    }
}
