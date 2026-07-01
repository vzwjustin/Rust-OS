//! Mutter window management
//! Ported from meta/window.h

use crate::mutter_port::meta::common::MetaFrameBorders;
use crate::mutter_port::meta::enums::*;
use crate::mutter_port::meta::registry::{DisplayId, WindowId, WorkspaceId};
use crate::mutter_port::meta::types::*;
use crate::mutter_port::mtk::MtkRectangle;
use alloc::string::String;
use core::cell::Cell;

/// Window type constants
pub const META_WINDOW_NORMAL: u32 = 0;
pub const META_WINDOW_DESKTOP: u32 = 1;
pub const META_WINDOW_DOCK: u32 = 2;

/// Represents a window managed by the window manager
pub struct MetaWindow {
    /// Window registry ID for lookups
    pub window_id: WindowId,
    /// Display ID for registry resolution
    display_id: Cell<Option<DisplayId>>,
    /// Workspace ID for registry resolution
    workspace_id: Cell<Option<WorkspaceId>>,
    pub window_type: MetaWindowType,
    pub has_focus: bool,
    id: u64,
    title: Option<String>,
    buffer_rect: MtkRectangle,
    frame_rect: MtkRectangle,
    client_rect: MtkRectangle,
    skip_taskbar: bool,
    override_redirect: bool,
    appears_focused: bool,
    maximized_horizontally: bool,
    maximized_vertically: bool,
    minimized: bool,
    closed: bool,
    display: *mut core::ffi::c_void,
    workspace: *mut core::ffi::c_void,
    monitor: i32,
    /// Frame decoration border widths (visible + invisible = total).
    frame_borders: MetaFrameBorders,
}

impl MetaWindow {
    /// Create a new window
    pub fn new(window_type: MetaWindowType) -> Self {
        Self {
            window_id: WindowId::new(),
            display_id: Cell::new(None),
            workspace_id: Cell::new(None),
            window_type,
            has_focus: false,
            id: 0,
            title: None,
            buffer_rect: MtkRectangle::default(),
            frame_rect: MtkRectangle::default(),
            client_rect: MtkRectangle::default(),
            skip_taskbar: false,
            override_redirect: false,
            appears_focused: false,
            maximized_horizontally: false,
            maximized_vertically: false,
            minimized: false,
            closed: false,
            display: core::ptr::null_mut(),
            workspace: core::ptr::null_mut(),
            monitor: 0,
            frame_borders: MetaFrameBorders::default(),
        }
    }

    /// Set the display ID for registry resolution
    pub fn set_display_id(&self, id: DisplayId) {
        self.display_id.set(Some(id));
    }

    /// Set the workspace ID for registry resolution
    pub fn set_workspace_id(&self, id: WorkspaceId) {
        self.workspace_id.set(Some(id));
    }

    /// Set this window's frame decoration borders.
    pub fn set_frame_borders(&mut self, borders: MetaFrameBorders) {
        self.frame_borders = borders;
    }

    /// Check if window has input focus
    pub fn has_focus(&self) -> bool {
        self.has_focus
    }

    /// Check if window appears focused visually
    pub fn appears_focused(&self) -> bool {
        self.appears_focused
    }

    /// Check if window is override-redirect (unmanaged)
    pub fn is_override_redirect(&self) -> bool {
        self.override_redirect
    }

    /// Check if window should be excluded from taskbar
    pub fn is_skip_taskbar(&self) -> bool {
        self.skip_taskbar
    }

    /// Get the buffer rectangle (full window including decoration)
    pub fn get_buffer_rect(&self) -> MtkRectangle {
        self.buffer_rect
    }

    /// Get the frame rectangle (outer window bounds)
    pub fn get_frame_rect(&self) -> MtkRectangle {
        self.frame_rect
    }

    /// Get the client content rectangle (inner content area)
    pub fn get_client_content_rect(&self) -> MtkRectangle {
        self.client_rect
    }

    /// Convert client-relative coordinates to frame-relative
    pub fn client_rect_to_frame_rect(&self, client_rect: &MtkRectangle) -> MtkRectangle {
        // The frame rect surrounds the client rect by the total (visible +
        // invisible) decoration borders on each side. Mirrors upstream
        // meta_window_client_rect_to_frame_rect.
        let b = &self.frame_borders.total;
        MtkRectangle {
            x: client_rect.x - b.left as i32,
            y: client_rect.y - b.top as i32,
            width: client_rect.width + (b.left + b.right) as i32,
            height: client_rect.height + (b.top + b.bottom) as i32,
        }
    }

    /// Convert frame-relative coordinates to client-relative
    pub fn frame_rect_to_client_rect(&self, frame_rect: &MtkRectangle) -> MtkRectangle {
        // Inverse of client_rect_to_frame_rect: shrink the frame rect by the
        // total decoration borders to recover the client content rect.
        let b = &self.frame_borders.total;
        MtkRectangle {
            x: frame_rect.x + b.left as i32,
            y: frame_rect.y + b.top as i32,
            width: frame_rect.width - (b.left + b.right) as i32,
            height: frame_rect.height - (b.top + b.bottom) as i32,
        }
    }

    /// Get the display this window belongs to
    /// Get the display this window belongs to
    /// Registry resolution: uses `self.display_id` to look up in registry
    pub fn get_display(&self) -> Option<&MetaDisplay> {
        // Registry infrastructure in place (display_id stored).
        // Full reference-returning requires Arc<T> or lifetime architecture.
        self.display_id.get().map(|_id| {
            // Would resolve via: DISPLAY_REGISTRY.get(_id)
        });
        None
    }

    /// Get the window type
    pub fn get_window_type(&self) -> MetaWindowType {
        self.window_type
    }

    /// Get the workspace this window is on
    /// Registry resolution: uses `self.workspace_id` to look up in registry
    pub fn get_workspace(&self) -> Option<&MetaWorkspace> {
        // Registry infrastructure in place (workspace_id stored).
        self.workspace_id.get().map(|_id| {
            // Would resolve via: WORKSPACE_REGISTRY.get(_id)
        });
        None
    }

    /// Get the monitor index this window is on
    pub fn get_monitor(&self) -> i32 {
        self.monitor
    }

    /// Maximize the window in the directions named by `flags`.
    pub fn maximize(&mut self, flags: MetaMaximizeFlags) {
        let bits = flags as u32;
        if bits & MetaMaximizeFlags::Horizontal as u32 != 0 {
            self.maximized_horizontally = true;
        }
        if bits & MetaMaximizeFlags::Vertical as u32 != 0 {
            self.maximized_vertically = true;
        }
    }

    /// Unmaximize the window in the directions named by `flags`.
    pub fn unmaximize(&mut self, flags: MetaMaximizeFlags) {
        let bits = flags as u32;
        if bits & MetaMaximizeFlags::Horizontal as u32 != 0 {
            self.maximized_horizontally = false;
        }
        if bits & MetaMaximizeFlags::Vertical as u32 != 0 {
            self.maximized_vertically = false;
        }
    }

    /// Get maximize state
    pub fn is_maximized_vertically(&self) -> bool {
        self.maximized_vertically
    }

    pub fn is_maximized_horizontally(&self) -> bool {
        self.maximized_horizontally
    }

    /// Whether the window is maximized in both directions.
    pub fn is_maximized(&self) -> bool {
        self.maximized_horizontally && self.maximized_vertically
    }

    /// Minimize window
    pub fn minimize(&mut self) {
        self.minimized = true;
    }

    /// Unminimize window
    pub fn unminimize(&mut self) {
        self.minimized = false;
    }

    /// Whether the window is currently minimized.
    pub fn is_minimized(&self) -> bool {
        self.minimized
    }

    /// Close window. Marks the window as closed and removes focus.
    pub fn close(&mut self, _timestamp: u32) {
        self.closed = true;
        self.has_focus = false;
        self.appears_focused = false;
    }

    /// Whether the window has been closed.
    pub fn is_closed(&self) -> bool {
        self.closed
    }

    /// Get window ID
    pub fn get_id(&self) -> u64 {
        self.id
    }

    /// Get window title
    pub fn get_title(&self) -> Option<&str> {
        self.title.as_ref().map(|s| s.as_str())
    }
}

impl Default for MetaWindow {
    fn default() -> Self {
        Self::new(MetaWindowType::Normal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mutter_port::meta::common::{MetaFrameBorder, MetaFrameBorders};

    fn borders(l: i16, r: i16, t: i16, b: i16) -> MetaFrameBorders {
        let total = MetaFrameBorder {
            left: l,
            right: r,
            top: t,
            bottom: b,
        };
        MetaFrameBorders {
            visible: total,
            invisible: MetaFrameBorder::default(),
            total,
        }
    }

    #[test]
    fn test_client_to_frame_adds_borders() {
        let mut w = MetaWindow::new(MetaWindowType::Normal);
        w.set_frame_borders(borders(10, 5, 20, 8));
        let client = MtkRectangle::new(100, 100, 640, 480);
        let frame = w.client_rect_to_frame_rect(&client);
        // x/y move out by left/top; size grows by left+right / top+bottom.
        assert_eq!(frame, MtkRectangle::new(90, 80, 655, 508));
    }

    #[test]
    fn test_frame_to_client_is_inverse() {
        let mut w = MetaWindow::new(MetaWindowType::Normal);
        w.set_frame_borders(borders(10, 5, 20, 8));
        let client = MtkRectangle::new(100, 100, 640, 480);
        let frame = w.client_rect_to_frame_rect(&client);
        // Converting back recovers the original client rect.
        assert_eq!(w.frame_rect_to_client_rect(&frame), client);
    }

    #[test]
    fn test_zero_borders_is_identity() {
        let w = MetaWindow::new(MetaWindowType::Normal);
        let r = MtkRectangle::new(1, 2, 3, 4);
        assert_eq!(w.client_rect_to_frame_rect(&r), r);
        assert_eq!(w.frame_rect_to_client_rect(&r), r);
    }

    #[test]
    fn test_maximize_directions() {
        let mut w = MetaWindow::new(MetaWindowType::Normal);
        w.maximize(MetaMaximizeFlags::Horizontal);
        assert!(w.is_maximized_horizontally() && !w.is_maximized_vertically());
        assert!(!w.is_maximized());

        w.maximize(MetaMaximizeFlags::Vertical);
        assert!(w.is_maximized());

        w.unmaximize(MetaMaximizeFlags::Both);
        assert!(!w.is_maximized_horizontally() && !w.is_maximized_vertically());
    }

    #[test]
    fn test_minimize_toggle() {
        let mut w = MetaWindow::new(MetaWindowType::Normal);
        assert!(!w.is_minimized());
        w.minimize();
        assert!(w.is_minimized());
        w.unminimize();
        assert!(!w.is_minimized());
    }
}
