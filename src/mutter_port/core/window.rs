//! Window abstraction ported from GNOME Mutter's src/core/window.c / window-private.h
//!
//! Implements the core MetaWindow class representing managed windows. Each window
//! has geometry, constraints, focus state, and can transition between various states
//! (maximized, fullscreen, minimized, etc.).
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/core/window.c

use crate::desktop::window_manager::WindowId;
use alloc::string::String;
use alloc::vec::Vec;

/// Window type classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetaWindowType {
    /// Normal top-level application window.
    Normal,
    /// Desktop window (background).
    Desktop,
    /// Dock/panel window.
    Dock,
    /// Toolbar floating window.
    Toolbar,
    /// Menu window.
    Menu,
    /// Dialog window.
    Dialog,
    /// Modal dialog.
    ModalDialog,
    /// Splash screen.
    Splash,
    /// Utility window.
    Utility,
    /// Drop-down/pop-up menu.
    Dropdown,
    /// Pop-up menu.
    Popup,
    /// Tooltip.
    Tooltip,
    /// Notification.
    Notification,
    /// Combo box popup.
    Combo,
    /// Drag and drop visual.
    DND,
}

/// Window state flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowState {
    pub maximized_horizontally: bool,
    pub maximized_vertically: bool,
    pub hidden: bool,
    pub fullscreen: bool,
    pub sticky: bool,
    pub demands_attention: bool,
    pub urgent: bool,
    pub modal: bool,
    pub skip_taskbar: bool,
    pub skip_pager: bool,
    pub above: bool,
    pub below: bool,
    pub focused: bool,
    pub tiled_left: bool,
    pub tiled_right: bool,
}

impl Default for WindowState {
    fn default() -> Self {
        WindowState {
            maximized_horizontally: false,
            maximized_vertically: false,
            hidden: false,
            fullscreen: false,
            sticky: false,
            demands_attention: false,
            urgent: false,
            modal: false,
            skip_taskbar: false,
            skip_pager: false,
            above: false,
            below: false,
            focused: false,
            tiled_left: false,
            tiled_right: false,
        }
    }
}

/// Simple rectangle for window geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    /// Create a new rectangle.
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    /// Get the right edge coordinate.
    pub fn right(&self) -> i32 {
        self.x + self.width as i32
    }

    /// Get the bottom edge coordinate.
    pub fn bottom(&self) -> i32 {
        self.y + self.height as i32
    }

    /// Check if this rectangle contains a point.
    pub fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x && px < self.right() && py >= self.y && py < self.bottom()
    }

    /// Get the intersection of two rectangles.
    pub fn intersect(&self, other: &Rect) -> Option<Rect> {
        let left = self.x.max(other.x);
        let top = self.y.max(other.y);
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());

        if left < right && top < bottom {
            Some(Rect {
                x: left,
                y: top,
                width: (right - left) as u32,
                height: (bottom - top) as u32,
            })
        } else {
            None
        }
    }
}

/// Represents a managed window.
#[derive(Debug)]
pub struct MetaWindow {
    /// Unique identifier for this window.
    id: WindowId,

    /// Unique stable identifier (never recycled).
    stamp: u64,

    /// Window type (normal, dialog, dock, etc.).
    window_type: MetaWindowType,

    /// Window title.
    title: String,

    /// Resource class (X11).
    res_class: String,

    /// Resource name (X11).
    res_name: String,

    /// Window role (X11).
    role: String,

    /// Application ID (Wayland).
    app_id: String,

    /// Current window state.
    state: WindowState,

    /// Current frame rectangle (in screen coordinates).
    frame_rect: Rect,

    /// Geometry from client (in buffer coordinates).
    buffer_rect: Rect,

    /// Saved geometry when unmaximizing.
    saved_rect: Rect,

    /// Saved geometry when unfullscreening.
    saved_rect_fullscreen: Rect,

    /// Unconstrained rectangle (before constraints applied).
    unconstrained_rect: Rect,

    /// Opacity (0-255).
    opacity: u8,

    /// Whether this is an override-redirect window.
    override_redirect: bool,

    /// Whether this window has user-set position.
    user_positioned: bool,

    /// Client process ID.
    client_pid: i32,

    /// Window's stable sequence number (for ordering).
    stable_sequence: u32,

    /// Last user interaction timestamp.
    net_wm_user_time: u32,

    /// Transient parent window (if any).
    transient_for: Option<WindowId>,

    /// Focus prevention timestamp.
    last_focus_prevent_time: u32,
}

impl MetaWindow {
    /// Create a new window.
    pub fn new(id: WindowId, stamp: u64, window_type: MetaWindowType, title: String) -> Self {
        MetaWindow {
            id,
            stamp,
            window_type,
            title,
            res_class: String::new(),
            res_name: String::new(),
            role: String::new(),
            app_id: String::new(),
            state: WindowState::default(),
            frame_rect: Rect::new(0, 0, 800, 600),
            buffer_rect: Rect::new(0, 0, 800, 600),
            saved_rect: Rect::new(0, 0, 800, 600),
            saved_rect_fullscreen: Rect::new(0, 0, 800, 600),
            unconstrained_rect: Rect::new(0, 0, 800, 600),
            opacity: 255,
            override_redirect: false,
            user_positioned: false,
            client_pid: -1,
            stable_sequence: 0,
            net_wm_user_time: 0,
            transient_for: None,
            last_focus_prevent_time: 0,
        }
    }

    /// Get this window's unique identifier.
    pub fn id(&self) -> WindowId {
        self.id
    }

    /// Get this window's stable stamp (never recycled).
    pub fn stamp(&self) -> u64 {
        self.stamp
    }

    /// Get window type.
    pub fn window_type(&self) -> MetaWindowType {
        self.window_type
    }

    /// Get window title.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Set window title.
    pub fn set_title(&mut self, title: String) {
        self.title = title;
    }

    /// Get window state.
    pub fn state(&self) -> WindowState {
        self.state
    }

    /// Update window state.
    pub fn set_state(&mut self, state: WindowState) {
        self.state = state;
    }

    /// Get frame rectangle (screen coordinates).
    pub fn frame_rect(&self) -> Rect {
        self.frame_rect
    }

    /// Set frame rectangle.
    pub fn set_frame_rect(&mut self, rect: Rect) {
        self.frame_rect = rect;
    }

    /// Move window to position.
    pub fn move_to(&mut self, x: i32, y: i32) {
        self.frame_rect.x = x;
        self.frame_rect.y = y;
    }

    /// Resize window.
    pub fn resize_to(&mut self, width: u32, height: u32) {
        self.frame_rect.width = width;
        self.frame_rect.height = height;
    }

    /// Get buffer rectangle (buffer coordinates).
    pub fn buffer_rect(&self) -> Rect {
        self.buffer_rect
    }

    /// Set buffer rectangle.
    pub fn set_buffer_rect(&mut self, rect: Rect) {
        self.buffer_rect = rect;
    }

    /// Maximize horizontally and/or vertically.
    pub fn maximize(&mut self, horizontal: bool, vertical: bool) {
        if horizontal {
            self.saved_rect.x = self.frame_rect.x;
            self.saved_rect.width = self.frame_rect.width;
            self.state.maximized_horizontally = true;
        }
        if vertical {
            self.saved_rect.y = self.frame_rect.y;
            self.saved_rect.height = self.frame_rect.height;
            self.state.maximized_vertically = true;
        }
    }

    /// Unmaximize and restore to saved geometry.
    pub fn unmaximize(&mut self, horizontal: bool, vertical: bool) {
        if horizontal && self.state.maximized_horizontally {
            self.frame_rect.x = self.saved_rect.x;
            self.frame_rect.width = self.saved_rect.width;
            self.state.maximized_horizontally = false;
        }
        if vertical && self.state.maximized_vertically {
            self.frame_rect.y = self.saved_rect.y;
            self.frame_rect.height = self.saved_rect.height;
            self.state.maximized_vertically = false;
        }
    }

    /// Enter fullscreen mode.
    pub fn fullscreen(&mut self) {
        self.saved_rect_fullscreen = self.frame_rect;
        self.state.fullscreen = true;
    }

    /// Exit fullscreen mode.
    pub fn unfullscreen(&mut self) {
        if self.state.fullscreen {
            self.frame_rect = self.saved_rect_fullscreen;
            self.state.fullscreen = false;
        }
    }

    /// Check if window is maximized (either direction).
    pub fn is_maximized(&self) -> bool {
        self.state.maximized_horizontally || self.state.maximized_vertically
    }

    /// Check if window is fullscreen.
    pub fn is_fullscreen(&self) -> bool {
        self.state.fullscreen
    }

    /// Check if window is hidden.
    pub fn is_hidden(&self) -> bool {
        self.state.hidden
    }

    /// Hide window.
    pub fn hide(&mut self) {
        self.state.hidden = true;
    }

    /// Show window.
    pub fn show(&mut self) {
        self.state.hidden = false;
    }

    /// Check if window has focus.
    pub fn is_focused(&self) -> bool {
        self.state.focused
    }

    /// Set focus state.
    pub fn set_focused(&mut self, focused: bool) {
        self.state.focused = focused;
    }

    /// Get opacity (0-255).
    pub fn opacity(&self) -> u8 {
        self.opacity
    }

    /// Set opacity.
    pub fn set_opacity(&mut self, opacity: u8) {
        self.opacity = opacity;
    }

    /// Get client PID.
    pub fn client_pid(&self) -> i32 {
        self.client_pid
    }

    /// Set client PID.
    pub fn set_client_pid(&mut self, pid: i32) {
        self.client_pid = pid;
    }

    /// Get stable sequence number.
    pub fn stable_sequence(&self) -> u32 {
        self.stable_sequence
    }

    /// Set stable sequence number.
    pub fn set_stable_sequence(&mut self, seq: u32) {
        self.stable_sequence = seq;
    }

    /// Get last user interaction timestamp.
    pub fn net_wm_user_time(&self) -> u32 {
        self.net_wm_user_time
    }

    /// Update last user interaction timestamp.
    pub fn update_user_time(&mut self, timestamp: u32) {
        self.net_wm_user_time = timestamp;
    }

    /// Set transient parent window.
    pub fn set_transient_for(&mut self, parent: Option<WindowId>) {
        self.transient_for = parent;
    }

    /// Get transient parent window.
    pub fn transient_for(&self) -> Option<WindowId> {
        self.transient_for
    }

    /// Check if window is a transient dialog.
    pub fn is_transient(&self) -> bool {
        self.transient_for.is_some()
    }

    /// Set application ID.
    pub fn set_app_id(&mut self, app_id: String) {
        self.app_id = app_id;
    }

    /// Get application ID.
    pub fn app_id(&self) -> &str {
        &self.app_id
    }

    /// Check if window is override-redirect.
    pub fn is_override_redirect(&self) -> bool {
        self.override_redirect
    }

    /// Set override-redirect flag.
    pub fn set_override_redirect(&mut self, override_redirect: bool) {
        self.override_redirect = override_redirect;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_window_creation() {
        let window = MetaWindow::new(
            WindowId(1),
            42,
            MetaWindowType::Normal,
            "Test Window".into(),
        );

        assert_eq!(window.id(), WindowId(1));
        assert_eq!(window.stamp(), 42);
        assert_eq!(window.window_type(), MetaWindowType::Normal);
        assert_eq!(window.title(), "Test Window");
        assert!(!window.is_maximized());
        assert!(!window.is_fullscreen());
    }

    #[test]
    fn test_rect_operations() {
        let rect = Rect::new(10, 20, 100, 50);
        assert_eq!(rect.right(), 110);
        assert_eq!(rect.bottom(), 70);
        assert!(rect.contains(50, 40));
        assert!(!rect.contains(0, 0));
    }

    #[test]
    fn test_rect_intersection() {
        let rect1 = Rect::new(0, 0, 100, 100);
        let rect2 = Rect::new(50, 50, 100, 100);

        if let Some(intersection) = rect1.intersect(&rect2) {
            assert_eq!(intersection.x, 50);
            assert_eq!(intersection.y, 50);
            assert_eq!(intersection.width, 50);
            assert_eq!(intersection.height, 50);
        } else {
            panic!("Rects should intersect");
        }
    }

    #[test]
    fn test_maximize() {
        let mut window = MetaWindow::new(WindowId(1), 42, MetaWindowType::Normal, "Test".into());

        window.set_frame_rect(Rect::new(10, 20, 800, 600));
        window.maximize(true, true);

        assert!(window.is_maximized());
        assert_eq!(window.saved_rect.x, 10);
        assert_eq!(window.saved_rect.width, 800);
    }

    #[test]
    fn test_fullscreen() {
        let mut window = MetaWindow::new(WindowId(1), 42, MetaWindowType::Normal, "Test".into());

        window.set_frame_rect(Rect::new(10, 20, 800, 600));
        window.fullscreen();

        assert!(window.is_fullscreen());
        assert_eq!(window.saved_rect_fullscreen, Rect::new(10, 20, 800, 600));

        window.unfullscreen();
        assert!(!window.is_fullscreen());
        assert_eq!(window.frame_rect(), Rect::new(10, 20, 800, 600));
    }

    #[test]
    fn test_opacity() {
        let mut window = MetaWindow::new(WindowId(1), 42, MetaWindowType::Normal, "Test".into());

        assert_eq!(window.opacity(), 255);
        window.set_opacity(128);
        assert_eq!(window.opacity(), 128);
    }

    #[test]
    fn test_transient() {
        let mut window = MetaWindow::new(WindowId(1), 42, MetaWindowType::Dialog, "Dialog".into());

        assert!(!window.is_transient());
        window.set_transient_for(Some(WindowId(2)));
        assert!(window.is_transient());
        assert_eq!(window.transient_for(), Some(WindowId(2)));
    }
}
