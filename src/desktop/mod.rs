//! # RustOS Desktop Environment Module
//!
//! This module provides a complete desktop environment for RustOS, including
//! window management, graphics rendering, and user interface components.

pub mod window_manager;

use crate::graphics::framebuffer::{self, Color, FramebufferInfo, Rect};
use heapless::Vec;

// Re-export commonly used types
pub use window_manager::{ButtonId, DesktopEvent, MouseButton, WindowId, WindowManager};

/// Simplified desktop environment configuration
#[derive(Debug, Clone, Copy)]
pub struct DesktopConfig {
    pub preferred_width: u16,
    pub preferred_height: u16,
    pub preferred_bpp: u16,
    pub double_buffered: bool,
    pub hardware_acceleration: bool,
    pub show_splash: bool,
    pub background_color: Color,
}

impl Default for DesktopConfig {
    fn default() -> Self {
        Self {
            preferred_width: 1024,
            preferred_height: 768,
            preferred_bpp: 32,
            double_buffered: true,
            hardware_acceleration: false,
            show_splash: true,
            background_color: Color::rgb(28, 34, 54),
        }
    }
}

/// Desktop environment status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopStatus {
    Uninitialized,
    Initializing,
    Running,
    Error,
}

/// Simplified desktop environment structure
pub struct Desktop {
    status: DesktopStatus,
    config: DesktopConfig,
    frame_counter: usize,
    event_queue: Vec<DesktopEvent, 32>,
    framebuffer_info: Option<FramebufferInfo>,
    video_mode: Option<u16>,
    window_manager: Option<WindowManager>,
}

impl Desktop {
    /// Create a new desktop environment
    pub fn new(config: DesktopConfig) -> Self {
        Self {
            status: DesktopStatus::Uninitialized,
            config,
            frame_counter: 0,
            event_queue: Vec::new(),
            framebuffer_info: None,
            video_mode: None,
            window_manager: None,
        }
    }

    /// Initialize the desktop environment
    pub fn init(&mut self) -> Result<(), &'static str> {
        unsafe { crate::early_serial_write_str("desktop:init begin\r\n") };
        self.status = DesktopStatus::Initializing;

        // Clear screen with background color
        framebuffer::clear_screen(self.config.background_color);
        unsafe { crate::early_serial_write_str("desktop:screen cleared\r\n") };

        // Get actual screen dimensions from graphics system
        let (width, height) = if let Some((w, h)) = crate::graphics::get_screen_dimensions() {
            (w, h)
        } else {
            // Fall back to configured dimensions
            (
                self.config.preferred_width as usize,
                self.config.preferred_height as usize,
            )
        };

        // Initialize window manager with actual screen size
        self.window_manager = Some(WindowManager::new(width, height));
        unsafe { crate::early_serial_write_str("desktop:wm new\r\n") };

        if self.config.show_splash {
            self.show_splash_screen();
            unsafe { crate::early_serial_write_str("desktop:splash done\r\n") };
        }

        // Create real shell/status windows for the initial desktop.
        if let Some(ref mut wm) = self.window_manager {
            unsafe { crate::early_serial_write_str("desktop:create windows\r\n") };
            let welcome = wm.create_window("RustOS Desktop", 88, 56, 400, 260);
            wm.set_window_content(
                welcome,
                &[
                    "RustOS framebuffer desktop",
                    "",
                    "This is the kernel UI shell.",
                    "It renders from real framebuffer",
                    "and window-manager state.",
                    "",
                    "No userspace app registry",
                    "is mounted yet.",
                    "",
                    "Use Activities for open",
                    "window overview.",
                ],
            );

            let status = wm.create_window("Runtime Status", 188, 152, 350, 220);
            wm.set_window_content(
                status,
                &[
                    "Runtime-backed values:",
                    "",
                    "Uptime: panel top-right",
                    "Windows: panel task list",
                    "Focus: launcher indicator",
                    "",
                    "Network: quick settings",
                    "Audio: unavailable",
                ],
            );

            let sysinfo = wm.create_window("Kernel Info", 288, 248, 300, 200);
            wm.set_window_content(
                sysinfo,
                &[
                    "RustOS",
                    "",
                    "Kernel: x86_64 no_std",
                    "Display: 32-bit framebuffer",
                    "Input: PS/2 keyboard/mouse",
                    "",
                    "Subsystem status is shown",
                    "only when wired to kernel",
                    "state.",
                ],
            );

            wm.create_shell_window(112, 96, 440, 260);
            wm.force_redraw();
            unsafe { crate::early_serial_write_str("desktop:windows done\r\n") };
        }

        self.status = DesktopStatus::Running;
        unsafe { crate::early_serial_write_str("desktop:init done\r\n") };
        Ok(())
    }

    /// Show startup splash screen
    fn show_splash_screen(&self) {
        let (width, height) = if let Some((w, h)) = crate::graphics::get_screen_dimensions() {
            (w, h)
        } else {
            (
                self.config.preferred_width as usize,
                self.config.preferred_height as usize,
            )
        };

        let center_x = width / 2;
        let center_y = height / 2;

        let logo_rect = Rect::new(
            center_x.saturating_sub(200),
            center_y.saturating_sub(100),
            400,
            200,
        );

        // Modern gradient-style splash screen
        framebuffer::fill_rect(logo_rect, Color::rgb(45, 52, 73));
        framebuffer::draw_rect(logo_rect, Color::rgb(100, 160, 220), 3);

        let inner_rect = Rect::new(
            logo_rect.x + 20,
            logo_rect.y + 20,
            logo_rect.width - 40,
            logo_rect.height - 40,
        );
        framebuffer::fill_rect(inner_rect, Color::rgb(65, 75, 100));
        framebuffer::draw_rect(inner_rect, Color::rgb(120, 180, 240), 2);

        // Add a title bar effect
        let title_rect = Rect::new(logo_rect.x, logo_rect.y, logo_rect.width, 30);
        framebuffer::fill_rect(title_rect, Color::rgb(80, 120, 180));
    }

    /// Get framebuffer info
    pub fn framebuffer_info(&self) -> Option<&FramebufferInfo> {
        self.framebuffer_info.as_ref()
    }

    /// Get video mode
    pub fn video_mode(&self) -> Option<u16> {
        self.video_mode
    }

    /// Add event to queue
    pub fn add_event(&mut self, event: DesktopEvent) {
        let _ = self.event_queue.push(event);
    }

    /// Process events
    pub fn process_events(&mut self) {
        while let Some(event) = self.event_queue.pop() {
            self.handle_event(event);
        }
    }

    /// Handle a single event
    fn handle_event(&mut self, event: DesktopEvent) {
        if let Some(ref mut wm) = self.window_manager {
            match event {
                DesktopEvent::MouseMove { x, y } => {
                    wm.handle_mouse_move(x, y);
                }
                DesktopEvent::MouseDown { x, y, button } => {
                    wm.handle_mouse_down(x, y, button);
                }
                DesktopEvent::MouseUp { x, y, button } => {
                    wm.handle_mouse_up(x, y, button);
                }
                DesktopEvent::KeyDown { key } => {
                    wm.handle_key_down(key);
                }
                DesktopEvent::KeyUp { key: _ } => {
                    // Handle key up - simplified
                }
                DesktopEvent::Scroll { x, y, delta } => {
                    wm.handle_scroll(x, y, delta);
                }
                DesktopEvent::WindowClose { window_id } => {
                    wm.close_window(window_id);
                }
                DesktopEvent::WindowFocus { window_id } => {
                    wm.focus_window(window_id);
                }
                DesktopEvent::WindowResize {
                    window_id,
                    width,
                    height,
                } => {
                    wm.resize_window(window_id, width, height);
                }
                DesktopEvent::WindowMove { window_id, x, y } => {
                    wm.move_window(window_id, x, y);
                }
            }
        }
    }

    /// Update desktop state
    pub fn update(&mut self) {
        self.frame_counter = self.frame_counter.wrapping_add(1);

        if let Some(ref mut wm) = self.window_manager {
            if wm.needs_redraw() {
                wm.render();
            }
        }
    }

    /// Get desktop status
    pub fn status(&self) -> DesktopStatus {
        self.status
    }

    /// Get desktop configuration
    pub fn config(&self) -> &DesktopConfig {
        &self.config
    }

    /// Get mutable window manager reference
    pub fn window_manager_mut(&mut self) -> Option<&mut WindowManager> {
        self.window_manager.as_mut()
    }

    /// Get window manager reference
    pub fn window_manager(&self) -> Option<&WindowManager> {
        self.window_manager.as_ref()
    }
}

use lazy_static::lazy_static;
use spin::Mutex;

// Global desktop state (production)
lazy_static! {
    static ref GLOBAL_DESKTOP: Mutex<Option<Desktop>> = Mutex::new(None);
}

/// Initialize the desktop environment
pub fn init_default_desktop() -> Result<(), &'static str> {
    unsafe { crate::early_serial_write_str("desktop:default begin\r\n") };
    let config = DesktopConfig::default();
    let mut desktop = Desktop::new(config);
    unsafe { crate::early_serial_write_str("desktop:constructed\r\n") };
    desktop.init()?;
    unsafe { crate::early_serial_write_str("desktop:locking global\r\n") };

    let mut global = GLOBAL_DESKTOP.lock();
    *global = Some(desktop);
    unsafe { crate::early_serial_write_str("desktop:stored global\r\n") };
    Ok(())
}

/// Set up full desktop environment
pub fn setup_full_desktop() -> Result<(), &'static str> {
    init_default_desktop()
}

/// Update desktop
pub fn update_desktop() {
    let mut global = GLOBAL_DESKTOP.lock();
    if let Some(ref mut desktop) = *global {
        desktop.update();
    }
}

/// Get desktop status
pub fn get_desktop_status() -> DesktopStatus {
    let global = GLOBAL_DESKTOP.lock();
    global
        .as_ref()
        .map_or(DesktopStatus::Uninitialized, |d| d.status())
}

/// Create a window using the global window manager
pub fn create_window(
    title: &'static str,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
) -> WindowId {
    let mut global = GLOBAL_DESKTOP.lock();
    if let Some(ref mut desktop) = *global {
        if let Some(ref mut wm) = desktop.window_manager_mut() {
            return wm.create_window(title, x, y, width, height);
        }
    }
    WindowId(0) // Failed
}

/// Close a window
pub fn close_window(window_id: WindowId) -> bool {
    let mut global = GLOBAL_DESKTOP.lock();
    if let Some(ref mut desktop) = *global {
        if let Some(ref mut wm) = desktop.window_manager_mut() {
            return wm.close_window(window_id);
        }
    }
    false
}

/// Focus a window
pub fn focus_window(window_id: WindowId) -> bool {
    let mut global = GLOBAL_DESKTOP.lock();
    if let Some(ref mut desktop) = *global {
        if let Some(ref mut wm) = desktop.window_manager_mut() {
            return wm.focus_window(window_id);
        }
    }
    false
}

/// Replace a window's text content.
pub fn set_window_content(window_id: WindowId, lines: &[&'static str]) -> bool {
    let mut global = GLOBAL_DESKTOP.lock();
    if let Some(ref mut desktop) = *global {
        if let Some(ref mut wm) = desktop.window_manager_mut() {
            return wm.set_window_content(window_id, lines);
        }
    }
    false
}

/// Create a simple message window.
pub fn show_message_window(title: &'static str, lines: &[&'static str]) -> WindowId {
    let id = create_window(title, 96, 96, 360, 180);
    if id.0 != 0 {
        let _ = set_window_content(id, lines);
    }
    id
}

/// Move a window to an absolute desktop position.
pub fn move_window(window_id: WindowId, x: usize, y: usize) -> bool {
    let mut global = GLOBAL_DESKTOP.lock();
    if let Some(ref mut desktop) = *global {
        if let Some(ref mut wm) = desktop.window_manager_mut() {
            return wm.move_window(window_id, x, y);
        }
    }
    false
}

/// Resize a window, clamped by the window manager.
pub fn resize_window(window_id: WindowId, width: usize, height: usize) -> bool {
    let mut global = GLOBAL_DESKTOP.lock();
    if let Some(ref mut desktop) = *global {
        if let Some(ref mut wm) = desktop.window_manager_mut() {
            return wm.resize_window(window_id, width, height);
        }
    }
    false
}

/// Get currently focused window.
pub fn focused_window() -> Option<WindowId> {
    let global = GLOBAL_DESKTOP.lock();
    global
        .as_ref()
        .and_then(|desktop| desktop.window_manager())
        .and_then(|wm| wm.get_focused_window())
}

/// Bring a window to the front and focus it.
pub fn bring_window_to_front(window_id: WindowId) -> bool {
    let mut global = GLOBAL_DESKTOP.lock();
    if let Some(ref mut desktop) = *global {
        if let Some(ref mut wm) = desktop.window_manager_mut() {
            return wm.bring_to_front(window_id);
        }
    }
    false
}

/// Center a window on the desktop.
pub fn center_window(window_id: WindowId) -> bool {
    let mut global = GLOBAL_DESKTOP.lock();
    if let Some(ref mut desktop) = *global {
        if let Some(ref mut wm) = desktop.window_manager_mut() {
            return wm.center_window(window_id);
        }
    }
    false
}

/// Show or hide a window without closing it.
pub fn set_window_visible(window_id: WindowId, visible: bool) -> bool {
    let mut global = GLOBAL_DESKTOP.lock();
    if let Some(ref mut desktop) = *global {
        if let Some(ref mut wm) = desktop.window_manager_mut() {
            return wm.set_window_visible(window_id, visible);
        }
    }
    false
}

/// Minimize a window.
pub fn minimize_window(window_id: WindowId) -> bool {
    let mut global = GLOBAL_DESKTOP.lock();
    if let Some(ref mut desktop) = *global {
        if let Some(ref mut wm) = desktop.window_manager_mut() {
            return wm.minimize_window(window_id);
        }
    }
    false
}

/// Restore a minimized or maximized window.
pub fn restore_window(window_id: WindowId) -> bool {
    let mut global = GLOBAL_DESKTOP.lock();
    if let Some(ref mut desktop) = *global {
        if let Some(ref mut wm) = desktop.window_manager_mut() {
            return wm.restore_window(window_id);
        }
    }
    false
}

/// Maximize a window to the usable desktop area.
pub fn maximize_window(window_id: WindowId) -> bool {
    let mut global = GLOBAL_DESKTOP.lock();
    if let Some(ref mut desktop) = *global {
        if let Some(ref mut wm) = desktop.window_manager_mut() {
            return wm.maximize_window(window_id);
        }
    }
    false
}

/// Clear all text content from a window.
pub fn clear_window_content(window_id: WindowId) -> bool {
    let mut global = GLOBAL_DESKTOP.lock();
    if let Some(ref mut desktop) = *global {
        if let Some(ref mut wm) = desktop.window_manager_mut() {
            return wm.clear_window_content(window_id);
        }
    }
    false
}

/// Append one text line to a window.
pub fn append_window_line(window_id: WindowId, line: &'static str) -> bool {
    let mut global = GLOBAL_DESKTOP.lock();
    if let Some(ref mut desktop) = *global {
        if let Some(ref mut wm) = desktop.window_manager_mut() {
            return wm.append_window_line(window_id, line);
        }
    }
    false
}

/// Create a global desktop button.
pub fn create_button(rect: Rect, text: &'static str) -> ButtonId {
    let mut global = GLOBAL_DESKTOP.lock();
    if let Some(ref mut desktop) = *global {
        if let Some(ref mut wm) = desktop.window_manager_mut() {
            return wm.create_button(rect, text);
        }
    }
    ButtonId(0)
}

/// Get the visible button under a point.
pub fn button_at_point(x: usize, y: usize) -> Option<ButtonId> {
    let global = GLOBAL_DESKTOP.lock();
    global
        .as_ref()
        .and_then(|desktop| desktop.window_manager())
        .and_then(|wm| wm.button_at_point(x, y))
}

/// Enable or disable a button.
pub fn set_button_enabled(button_id: ButtonId, enabled: bool) -> bool {
    let mut global = GLOBAL_DESKTOP.lock();
    if let Some(ref mut desktop) = *global {
        if let Some(ref mut wm) = desktop.window_manager_mut() {
            return wm.set_button_enabled(button_id, enabled);
        }
    }
    false
}

/// Show or hide a button.
pub fn set_button_visible(button_id: ButtonId, visible: bool) -> bool {
    let mut global = GLOBAL_DESKTOP.lock();
    if let Some(ref mut desktop) = *global {
        if let Some(ref mut wm) = desktop.window_manager_mut() {
            return wm.set_button_visible(button_id, visible);
        }
    }
    false
}

/// Number of open windows.
pub fn window_count() -> usize {
    let global = GLOBAL_DESKTOP.lock();
    global
        .as_ref()
        .and_then(|desktop| desktop.window_manager())
        .map_or(0, |wm| wm.get_window_count())
}

/// Number of desktop buttons.
pub fn button_count() -> usize {
    let global = GLOBAL_DESKTOP.lock();
    global
        .as_ref()
        .and_then(|desktop| desktop.window_manager())
        .map_or(0, |wm| wm.get_button_count())
}

/// Move the cursor.
pub fn set_cursor_position(x: usize, y: usize) -> bool {
    let mut global = GLOBAL_DESKTOP.lock();
    if let Some(ref mut desktop) = *global {
        if let Some(ref mut wm) = desktop.window_manager_mut() {
            wm.set_cursor_position(x, y);
            return true;
        }
    }
    false
}

/// Show or hide the cursor.
pub fn set_cursor_visible(visible: bool) -> bool {
    let mut global = GLOBAL_DESKTOP.lock();
    if let Some(ref mut desktop) = *global {
        if let Some(ref mut wm) = desktop.window_manager_mut() {
            wm.set_cursor_visible(visible);
            return true;
        }
    }
    false
}

/// Handle mouse move
pub fn handle_mouse_move(x: usize, y: usize) {
    let mut global = GLOBAL_DESKTOP.lock();
    if let Some(ref mut desktop) = *global {
        desktop.add_event(DesktopEvent::MouseMove { x, y });
    }
}

/// Handle mouse down
pub fn handle_mouse_down(x: usize, y: usize, button: MouseButton) {
    let mut global = GLOBAL_DESKTOP.lock();
    if let Some(ref mut desktop) = *global {
        desktop.add_event(DesktopEvent::MouseDown { x, y, button });
    }
}

/// Handle mouse up
pub fn handle_mouse_up(x: usize, y: usize, button: MouseButton) {
    let mut global = GLOBAL_DESKTOP.lock();
    if let Some(ref mut desktop) = *global {
        desktop.add_event(DesktopEvent::MouseUp { x, y, button });
    }
}

/// Handle key down
pub fn handle_key_down(key: u8) {
    let mut global = GLOBAL_DESKTOP.lock();
    if let Some(ref mut desktop) = *global {
        desktop.add_event(DesktopEvent::KeyDown { key });
    }
}

/// Process all pending desktop events
pub fn process_desktop_events() {
    let mut global = GLOBAL_DESKTOP.lock();
    if let Some(ref mut desktop) = *global {
        desktop.process_events();
    }
}

/// Render desktop
pub fn render_desktop() {
    let mut global = GLOBAL_DESKTOP.lock();
    if let Some(ref mut desktop) = *global {
        if let Some(ref mut wm) = desktop.window_manager_mut() {
            if wm.needs_redraw() {
                wm.render();
            }
        }
    }
}

/// Check if desktop needs redraw
pub fn desktop_needs_redraw() -> bool {
    let global = GLOBAL_DESKTOP.lock();
    if let Some(ref desktop) = *global {
        if let Some(ref wm) = desktop.window_manager() {
            return wm.needs_redraw();
        }
    }
    false
}

/// Invalidate desktop for redraw
pub fn invalidate_desktop() {
    let mut global = GLOBAL_DESKTOP.lock();
    if let Some(ref mut desktop) = *global {
        if let Some(ref mut wm) = desktop.window_manager_mut() {
            wm.force_redraw();
        }
    }
}

// =============================================================================
// Wrapper functions for legacy API compatibility
// =============================================================================

/// Handle scroll event.
pub fn handle_scroll(x: i32, y: i32, delta: i32) {
    if x < 0 || y < 0 || delta == 0 {
        return;
    }

    let mut global = GLOBAL_DESKTOP.lock();
    if let Some(ref mut desktop) = *global {
        desktop.add_event(DesktopEvent::Scroll {
            x: x as usize,
            y: y as usize,
            delta,
        });
    }
}

// Simplified test functions (without #[cfg(feature = "std-tests")] // Disabled: #[cfg(feature = "disabled-tests")] // #[cfg(feature = "disabled-tests")] // #[test_case] attributes to avoid no_std issues)
#[cfg(test)]
mod tests {
    use super::*;

    fn test_desktop_creation() {
        let config = DesktopConfig::default();
        let desktop = Desktop::new(config);
        assert_eq!(desktop.status(), DesktopStatus::Uninitialized);
    }

    fn test_desktop_initialization() {
        let config = DesktopConfig::default();
        let mut desktop = Desktop::new(config);
        assert!(desktop.init().is_ok());
        assert_eq!(desktop.status(), DesktopStatus::Running);
    }

    fn test_event_handling() {
        let config = DesktopConfig::default();
        let mut desktop = Desktop::new(config);
        let _ = desktop.init();

        desktop.add_event(DesktopEvent::MouseMove { x: 100, y: 200 });
        desktop.process_events();
    }

    fn test_window_creation() {
        let window_id = create_window("Test Window", 10, 10, 300, 200);
        assert_ne!(window_id.0, 0);
    }
}
