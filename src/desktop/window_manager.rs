//! # RustOS Desktop Window Manager
//!
//! A comprehensive desktop environment with window management, UI components,
//! and event handling for the RustOS kernel.

use crate::graphics::framebuffer::{Color, Rect};
use alloc::format;
use core::cmp::{max, min};
use heapless::{String as HString, Vec};

/// Maximum lines of text content per window
pub const MAX_CONTENT_LINES: usize = 32;
pub const MAX_SHELL_LINES: usize = 12;
/// Size of the resize handle in the bottom-right corner
pub const RESIZE_HANDLE_SIZE: usize = 12;

/// Maximum number of windows that can be managed simultaneously
pub const MAX_WINDOWS: usize = 64;

/// Default window title bar height
pub const TITLE_BAR_HEIGHT: usize = 24;

/// Default window border width
pub const BORDER_WIDTH: usize = 2;

/// Linux-style shell chrome dimensions.
pub const MENU_BAR_HEIGHT: usize = 30;
pub const DOCK_HEIGHT: usize = 64;
pub const DOCK_ICON_SIZE: usize = 42;
pub const DOCK_ICON_GAP: usize = 10;
pub const DOCK_ICON_COUNT: usize = 6;
pub const WINDOW_SHADOW_MARGIN: usize = 6;
pub const TRAFFIC_LIGHT_RADIUS: usize = 6;
pub const TRAFFIC_LIGHT_SPACING: usize = 14;

/// Minimum window size
pub const MIN_WINDOW_WIDTH: usize = 200;
pub const MIN_WINDOW_HEIGHT: usize = 150;

/// Desktop colors
pub mod colors {
    use crate::graphics::framebuffer::Color;

    pub const DESKTOP_BACKGROUND_TOP: Color = Color::rgb(46, 29, 48);
    pub const DESKTOP_BACKGROUND_BOTTOM: Color = Color::rgb(82, 42, 36);
    pub const DESKTOP_GLOW: Color = Color::rgb(184, 85, 44);
    pub const MENU_BAR_BACKGROUND: Color = Color::rgb(31, 31, 36);
    pub const MENU_BAR_HIGHLIGHT: Color = Color::rgb(68, 68, 76);
    pub const MENU_BAR_ACCENT: Color = Color::rgb(18, 18, 22);
    pub const MENU_BAR_ICON: Color = Color::rgb(238, 238, 238);
    pub const DESKTOP_BACKGROUND: Color = DESKTOP_BACKGROUND_TOP;
    pub const WINDOW_BACKGROUND: Color = Color::rgb(248, 250, 255);
    pub const WINDOW_SHADOW: Color = Color::rgb(12, 15, 24);
    pub const TITLE_BAR_ACTIVE: Color = Color::rgb(48, 48, 54);
    pub const TITLE_BAR_INACTIVE: Color = Color::rgb(62, 62, 68);
    pub const BORDER_ACTIVE: Color = Color::rgb(230, 99, 41);
    pub const BORDER_INACTIVE: Color = Color::rgb(94, 94, 102);
    pub const TEXT_COLOR: Color = Color::rgb(0, 0, 0);
    pub const TEXT_COLOR_WHITE: Color = Color::rgb(255, 255, 255);
    pub const BUTTON_BACKGROUND: Color = Color::rgb(235, 236, 240);
    pub const BUTTON_HOVER: Color = Color::rgb(210, 212, 218);
    pub const BUTTON_PRESSED: Color = Color::rgb(188, 190, 198);
    pub const DOCK_BACKGROUND: Color = Color::rgb(34, 34, 40);
    pub const DOCK_GLASS: Color = Color::rgb(42, 42, 48);
    pub const DOCK_HIGHLIGHT: Color = Color::rgb(76, 76, 84);
    pub const DOCK_ICON_ACCENT: Color = Color::rgb(230, 99, 41);
    pub const DOCK_INDICATOR: Color = Color::rgb(245, 245, 245);
    pub const TRAFFIC_LIGHT_RED: Color = Color::rgb(218, 76, 50);
    pub const TRAFFIC_LIGHT_YELLOW: Color = Color::rgb(120, 120, 128);
    pub const TRAFFIC_LIGHT_GREEN: Color = Color::rgb(120, 120, 128);
}

/// Window state enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowState {
    Normal,
    Maximized,
    Minimized,
    Closed,
}

/// Mouse button enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// Event types for the desktop environment
#[derive(Debug, Clone)]
pub enum DesktopEvent {
    MouseMove {
        x: usize,
        y: usize,
    },
    MouseDown {
        x: usize,
        y: usize,
        button: MouseButton,
    },
    MouseUp {
        x: usize,
        y: usize,
        button: MouseButton,
    },
    KeyDown {
        key: u8,
    },
    KeyUp {
        key: u8,
    },
    Scroll {
        x: usize,
        y: usize,
        delta: i32,
    },
    WindowClose {
        window_id: WindowId,
    },
    WindowFocus {
        window_id: WindowId,
    },
    WindowResize {
        window_id: WindowId,
        width: usize,
        height: usize,
    },
    WindowMove {
        window_id: WindowId,
        x: usize,
        y: usize,
    },
}

/// Unique identifier for windows
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WindowId(pub usize);

/// Unique identifier for buttons
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ButtonId(pub usize);

/// Window structure
#[derive(Debug, Clone)]
pub struct Window {
    pub id: WindowId,
    pub title: &'static str,
    pub rect: Rect,
    pub client_area: Rect,
    pub state: WindowState,
    pub focused: bool,
    pub resizable: bool,
    pub movable: bool,
    pub visible: bool,
    pub has_title_bar: bool,
    pub has_border: bool,
    pub background_color: Color,
    pub border_color: Color,
    pub title_bar_color: Color,
    pub z_order: usize,
    pub content_lines: Vec<&'static str, MAX_CONTENT_LINES>,
    pub scroll_offset: usize,
}

/// Button structure
#[derive(Debug, Clone)]
pub struct Button {
    pub id: ButtonId,
    pub rect: Rect,
    pub text: &'static str,
    pub background_color: Color,
    pub text_color: Color,
    pub pressed: bool,
    pub hovered: bool,
    pub enabled: bool,
    pub visible: bool,
}

/// Cursor structure
#[derive(Debug, Clone)]
pub struct Cursor {
    pub x: usize,
    pub y: usize,
    pub visible: bool,
    pub color: Color,
}

impl WindowId {
    pub const INVALID: WindowId = WindowId(usize::MAX);
}

impl ButtonId {
    pub const INVALID: ButtonId = ButtonId(usize::MAX);
}

impl Window {
    /// Create a new window
    pub fn new(
        id: WindowId,
        title: &'static str,
        x: usize,
        y: usize,
        width: usize,
        height: usize,
    ) -> Self {
        let rect = Rect::new(x, y, width, height);
        let client_area = Rect::new(
            x + BORDER_WIDTH,
            y + TITLE_BAR_HEIGHT + BORDER_WIDTH,
            width.saturating_sub(2 * BORDER_WIDTH),
            height.saturating_sub(TITLE_BAR_HEIGHT + 2 * BORDER_WIDTH),
        );

        Self {
            id,
            title,
            rect,
            client_area,
            state: WindowState::Normal,
            focused: false,
            resizable: true,
            movable: true,
            visible: true,
            has_title_bar: true,
            has_border: true,
            background_color: colors::WINDOW_BACKGROUND,
            border_color: colors::BORDER_INACTIVE,
            title_bar_color: colors::TITLE_BAR_INACTIVE,
            z_order: 0,
            content_lines: Vec::new(),
            scroll_offset: 0,
        }
    }
}

impl Button {
    /// Create a new button
    pub fn new(id: ButtonId, rect: Rect, text: &'static str) -> Self {
        Self {
            id,
            rect,
            text,
            background_color: colors::BUTTON_BACKGROUND,
            text_color: colors::TEXT_COLOR,
            pressed: false,
            hovered: false,
            enabled: true,
            visible: true,
        }
    }
}

impl Cursor {
    /// Create a new cursor
    pub fn new() -> Self {
        Self {
            x: 0,
            y: 0,
            visible: true,
            color: Color::rgb(255, 255, 255),
        }
    }
}

/// Main desktop window manager
pub struct WindowManager {
    windows: Vec<Window, MAX_WINDOWS>,
    buttons: Vec<Button, 32>,
    window_count: usize,
    next_window_id: usize,
    next_button_id: usize,
    focused_window: Option<WindowId>,
    desktop_rect: Rect,
    needs_redraw: bool,
    cursor: Cursor,
    dragging_window: Option<WindowId>,
    drag_offset: (usize, usize),
    resizing_window: Option<WindowId>,
    resize_start_size: (usize, usize),
    menu_bar_rect: Rect,
    dock_rect: Rect,
    activities_open: bool,
    quick_settings_open: bool,
    shell_window: Option<WindowId>,
    shell_input: HString<64>,
    shell_lines: Vec<HString<96>, MAX_SHELL_LINES>,
}

impl WindowManager {
    /// Create a new window manager
    pub fn new(screen_width: usize, screen_height: usize) -> Self {
        let menu_bar_rect = Rect::new(0, 0, screen_width, MENU_BAR_HEIGHT);
        let dock_width = DOCK_HEIGHT.min(screen_width);
        let dock_height = screen_height.saturating_sub(MENU_BAR_HEIGHT);
        let dock_rect = Rect::new(0, MENU_BAR_HEIGHT, dock_width, dock_height);

        Self {
            windows: Vec::new(),
            buttons: Vec::new(),
            window_count: 0,
            next_window_id: 1,
            next_button_id: 1,
            focused_window: None,
            desktop_rect: Rect::new(0, 0, screen_width, screen_height),
            needs_redraw: true,
            cursor: Cursor::new(),
            dragging_window: None,
            drag_offset: (0, 0),
            resizing_window: None,
            resize_start_size: (0, 0),
            menu_bar_rect,
            dock_rect,
            activities_open: false,
            quick_settings_open: false,
            shell_window: None,
            shell_input: HString::new(),
            shell_lines: Vec::new(),
        }
    }

    /// Create a new window
    pub fn create_window(
        &mut self,
        title: &'static str,
        x: usize,
        y: usize,
        width: usize,
        height: usize,
    ) -> WindowId {
        let window_id = WindowId(self.next_window_id);
        self.next_window_id += 1;

        let mut window = Window::new(window_id, title, x, y, width, height);
        window.focused = true;
        window.z_order = self.window_count;
        window.border_color = colors::BORDER_ACTIVE;
        window.title_bar_color = colors::TITLE_BAR_ACTIVE;

        // Unfocus other windows
        for w in &mut self.windows {
            w.focused = false;
            w.border_color = colors::BORDER_INACTIVE;
            w.title_bar_color = colors::TITLE_BAR_INACTIVE;
        }

        let _ = self.windows.push(window);
        self.window_count += 1;
        self.focused_window = Some(window_id);
        self.needs_redraw = true;
        window_id
    }

    /// Create and focus the kernel shell window.
    pub fn create_shell_window(
        &mut self,
        x: usize,
        y: usize,
        width: usize,
        height: usize,
    ) -> WindowId {
        let window_id = self.create_window("Kernel Shell", x, y, width, height);
        self.shell_window = Some(window_id);
        self.shell_input.clear();
        self.shell_lines.clear();
        self.push_shell_line("RustOS kernel shell");
        self.push_shell_line("type help");
        self.needs_redraw = true;
        window_id
    }

    fn push_shell_line(&mut self, text: &str) {
        if self.shell_lines.len() == MAX_SHELL_LINES {
            let _ = self.shell_lines.remove(0);
        }

        let mut line = HString::new();
        let _ = line.push_str(text);
        let _ = self.shell_lines.push(line);
    }

    fn handle_shell_key(&mut self, key: u8) -> bool {
        match key {
            8 => {
                let _ = self.shell_input.pop();
            }
            13 => {
                let mut command: HString<256> = HString::new();
                let _ = command.push_str(self.shell_input.trim());
                let prompt = format!("> {}", self.shell_input.as_str());
                self.push_shell_line(&prompt);
                self.shell_input.clear();
                self.execute_shell_command(command.as_str());
            }
            32..=126 => {
                let _ = self.shell_input.push(key as char);
            }
            _ => return false,
        }

        self.needs_redraw = true;
        true
    }

    fn execute_shell_command(&mut self, command: &str) {
        match command {
            "" => {}
            "help" => {
                self.push_shell_line("commands: help uptime windows");
                self.push_shell_line(" mounts ifaces storage clear");
            }
            "uptime" => {
                let line = format!("uptime: {}s", crate::time::uptime_ms() / 1000);
                self.push_shell_line(&line);
            }
            "windows" => {
                let open = self
                    .windows
                    .iter()
                    .filter(|window| window.state != WindowState::Closed)
                    .count();
                let line = format!("windows: {}", open);
                self.push_shell_line(&line);
            }
            "net" | "ifaces" => {
                let interfaces = crate::net::network_stack().list_interfaces().len();
                let line = format!("network interfaces: {}", interfaces);
                self.push_shell_line(&line);
                for iface in crate::net::network_stack().list_interfaces().iter().take(3) {
                    let state = if iface.flags.up { "up" } else { "down" };
                    let line = format!(
                        "{}: {} {} addr",
                        iface.name,
                        state,
                        iface.ip_addresses.len()
                    );
                    self.push_shell_line(&line);
                    if let Some(address) = iface.ip_addresses.first() {
                        let line = format!("  {}", address);
                        self.push_shell_line(&line);
                    }
                }
            }
            "fs" | "mounts" => {
                let mounts = crate::fs::vfs().list_mounts();
                let line = format!("mounts: {}", mounts.len());
                self.push_shell_line(&line);
                for (path, fs_type) in mounts.iter().take(4) {
                    let line = format!("{} {}", path, fs_type);
                    self.push_shell_line(&line);
                }
            }
            "storage" => {
                let status = crate::drivers::storage::get_subsystem_status();
                let line = format!("storage devices: {}", status.device_count);
                self.push_shell_line(&line);
                let mib = status.total_capacity_bytes / (1024 * 1024);
                let line = format!("capacity: {} MiB", mib);
                self.push_shell_line(&line);
                for device in crate::drivers::storage::get_storage_device_list()
                    .iter()
                    .take(3)
                {
                    let line = format!("{}: {}", device.id, device.device_type);
                    self.push_shell_line(&line);
                }
            }
            "clear" => {
                self.shell_lines.clear();
            }
            _ => {
                self.push_shell_line("unknown command");
            }
        }
    }

    /// Set text content lines for a window
    pub fn set_window_content(&mut self, window_id: WindowId, lines: &[&'static str]) -> bool {
        if let Some(window) = self.get_window_mut(window_id) {
            window.content_lines.clear();
            for line in lines {
                let _ = window.content_lines.push(*line);
            }
            window.scroll_offset = 0;
            self.needs_redraw = true;
            true
        } else {
            false
        }
    }

    /// Get window by ID
    pub fn get_window(&self, window_id: WindowId) -> Option<&Window> {
        self.windows.iter().find(|w| w.id == window_id)
    }

    /// Get mutable window by ID
    pub fn get_window_mut(&mut self, window_id: WindowId) -> Option<&mut Window> {
        self.windows.iter_mut().find(|w| w.id == window_id)
    }

    /// Focus a window
    pub fn focus_window(&mut self, window_id: WindowId) -> bool {
        let mut found = false;
        for window in &mut self.windows {
            if window.id == window_id {
                window.focused = true;
                window.border_color = colors::BORDER_ACTIVE;
                window.title_bar_color = colors::TITLE_BAR_ACTIVE;
                found = true;
            } else {
                window.focused = false;
                window.border_color = colors::BORDER_INACTIVE;
                window.title_bar_color = colors::TITLE_BAR_INACTIVE;
            }
        }
        if found {
            self.focused_window = Some(window_id);
            self.needs_redraw = true;
        }
        found
    }

    /// Bring a window to the front and focus it.
    pub fn bring_to_front(&mut self, window_id: WindowId) -> bool {
        if self.get_window(window_id).is_none() {
            return false;
        }

        let top_z = self
            .windows
            .iter()
            .map(|window| window.z_order)
            .max()
            .unwrap_or(0);

        if let Some(window) = self.get_window_mut(window_id) {
            window.z_order = top_z.saturating_add(1);
            window.state = WindowState::Normal;
            window.visible = true;
        }

        self.focus_window(window_id)
    }

    /// Get window at point
    pub fn window_at_point(&self, x: usize, y: usize) -> Option<WindowId> {
        self.windows
            .iter()
            .filter(|w| w.visible && w.rect.contains(x, y))
            .max_by_key(|w| w.z_order)
            .map(|w| w.id)
    }

    fn panel_window_at_point(&self, x: usize, y: usize) -> Option<WindowId> {
        if !self.menu_bar_rect.contains(x, y) {
            return None;
        }

        let font = crate::graphics::get_default_font();
        let mut task_x = self.menu_bar_rect.x + 220;
        for window in self
            .windows
            .iter()
            .filter(|window| window.state != WindowState::Closed)
        {
            let task_width = (window.title.len() * font.char_width + 22).clamp(72, 180);
            let task_rect = Rect::new(task_x, self.menu_bar_rect.y + 5, task_width, 20);
            if task_rect.contains(x, y) {
                return Some(window.id);
            }
            task_x += task_width + 8;
            if task_x >= self.menu_bar_rect.width.saturating_sub(160) {
                break;
            }
        }
        None
    }

    fn launcher_window_at_point(&self, x: usize, y: usize) -> Option<WindowId> {
        if !self.dock_rect.contains(x, y) {
            return None;
        }

        let icon_x = self.dock_rect.x + (self.dock_rect.width.saturating_sub(DOCK_ICON_SIZE)) / 2;
        let mut icon_y = self.dock_rect.y + 16;
        for window in self
            .windows
            .iter()
            .filter(|window| window.state != WindowState::Closed)
        {
            let icon_rect = Rect::new(icon_x, icon_y, DOCK_ICON_SIZE, DOCK_ICON_SIZE);
            if icon_rect.contains(x, y) {
                return Some(window.id);
            }
            icon_y += DOCK_ICON_SIZE + DOCK_ICON_GAP;
            if icon_y + DOCK_ICON_SIZE > self.dock_rect.y + self.dock_rect.height {
                break;
            }
        }
        None
    }

    fn activities_window_at_point(&self, px: usize, py: usize) -> Option<WindowId> {
        let overlay = Rect::new(
            self.dock_rect.width + 24,
            self.menu_bar_rect.height + 24,
            self.desktop_rect
                .width
                .saturating_sub(self.dock_rect.width + 48),
            self.desktop_rect
                .height
                .saturating_sub(self.menu_bar_rect.height + 48),
        );

        let mut x = overlay.x + 18;
        let mut y = overlay.y + 66;
        for window in self
            .windows
            .iter()
            .filter(|window| window.state != WindowState::Closed)
        {
            let tile = Rect::new(x, y, 180, 96);
            if tile.contains(px, py) {
                return Some(window.id);
            }

            x += 196;
            if x + 180 > overlay.x + overlay.width {
                x = overlay.x + 18;
                y += 112;
            }
            if y + 96 > overlay.y + overlay.height {
                break;
            }
        }

        None
    }

    /// Close a window
    pub fn close_window(&mut self, window_id: WindowId) -> bool {
        if let Some(pos) = self.windows.iter().position(|w| w.id == window_id) {
            self.windows.swap_remove(pos);
            self.window_count = self.window_count.saturating_sub(1);

            if self.focused_window == Some(window_id) {
                self.focused_window = self.windows.last().map(|w| w.id);
            }

            self.needs_redraw = true;
            true
        } else {
            false
        }
    }

    /// Move a window to an absolute desktop position.
    pub fn move_window(&mut self, window_id: WindowId, x: usize, y: usize) -> bool {
        if let Some(window) = self.get_window_mut(window_id) {
            window.rect.x = x;
            window.rect.y = y;
            window.client_area.x = x + BORDER_WIDTH;
            window.client_area.y = y + TITLE_BAR_HEIGHT + BORDER_WIDTH;
            self.needs_redraw = true;
            true
        } else {
            false
        }
    }

    /// Resize a window, clamped to the desktop minimum size.
    pub fn resize_window(&mut self, window_id: WindowId, width: usize, height: usize) -> bool {
        let width = max(width, MIN_WINDOW_WIDTH);
        let height = max(height, MIN_WINDOW_HEIGHT);

        if let Some(window) = self.get_window_mut(window_id) {
            window.rect.width = width;
            window.rect.height = height;
            window.client_area.width = width.saturating_sub(2 * BORDER_WIDTH);
            window.client_area.height = height.saturating_sub(TITLE_BAR_HEIGHT + 2 * BORDER_WIDTH);
            self.needs_redraw = true;
            true
        } else {
            false
        }
    }

    /// Center a window on the desktop.
    pub fn center_window(&mut self, window_id: WindowId) -> bool {
        let (x, y) = if let Some(window) = self.get_window(window_id) {
            (
                self.desktop_rect.width.saturating_sub(window.rect.width) / 2,
                self.desktop_rect.height.saturating_sub(window.rect.height) / 2,
            )
        } else {
            return false;
        };

        self.move_window(window_id, x, y)
    }

    /// Set window visibility without destroying it.
    pub fn set_window_visible(&mut self, window_id: WindowId, visible: bool) -> bool {
        if let Some(window) = self.get_window_mut(window_id) {
            window.visible = visible;
            if visible && window.state == WindowState::Minimized {
                window.state = WindowState::Normal;
            } else if !visible && self.focused_window == Some(window_id) {
                self.focused_window = None;
            }
            self.needs_redraw = true;
            true
        } else {
            false
        }
    }

    /// Minimize a window.
    pub fn minimize_window(&mut self, window_id: WindowId) -> bool {
        if let Some(window) = self.get_window_mut(window_id) {
            window.state = WindowState::Minimized;
            window.visible = false;
            if self.focused_window == Some(window_id) {
                self.focused_window = None;
            }
            self.needs_redraw = true;
            true
        } else {
            false
        }
    }

    /// Restore a minimized or maximized window.
    pub fn restore_window(&mut self, window_id: WindowId) -> bool {
        if let Some(window) = self.get_window_mut(window_id) {
            window.state = WindowState::Normal;
            window.visible = true;
            self.needs_redraw = true;
            true
        } else {
            false
        }
    }

    /// Maximize a window to the usable desktop area.
    pub fn maximize_window(&mut self, window_id: WindowId) -> bool {
        let width = self.desktop_rect.width.saturating_sub(DOCK_HEIGHT);
        let height = self.desktop_rect.height.saturating_sub(MENU_BAR_HEIGHT);

        if let Some(window) = self.get_window_mut(window_id) {
            window.state = WindowState::Maximized;
            window.visible = true;
            window.rect.x = DOCK_HEIGHT;
            window.rect.y = MENU_BAR_HEIGHT;
            window.rect.width = max(width, MIN_WINDOW_WIDTH);
            window.rect.height = max(height, MIN_WINDOW_HEIGHT);
            window.client_area.x = DOCK_HEIGHT + BORDER_WIDTH;
            window.client_area.y = MENU_BAR_HEIGHT + TITLE_BAR_HEIGHT + BORDER_WIDTH;
            window.client_area.width = window.rect.width.saturating_sub(2 * BORDER_WIDTH);
            window.client_area.height = window
                .rect
                .height
                .saturating_sub(TITLE_BAR_HEIGHT + 2 * BORDER_WIDTH);
            self.needs_redraw = true;
            true
        } else {
            false
        }
    }

    /// Remove all text content from a window.
    pub fn clear_window_content(&mut self, window_id: WindowId) -> bool {
        if let Some(window) = self.get_window_mut(window_id) {
            window.content_lines.clear();
            window.scroll_offset = 0;
            self.needs_redraw = true;
            true
        } else {
            false
        }
    }

    /// Append one text line to a window.
    pub fn append_window_line(&mut self, window_id: WindowId, line: &'static str) -> bool {
        if let Some(window) = self.get_window_mut(window_id) {
            if window.content_lines.push(line).is_ok() {
                self.needs_redraw = true;
                true
            } else {
                false
            }
        } else {
            false
        }
    }

    /// Create a button
    pub fn create_button(&mut self, rect: Rect, text: &'static str) -> ButtonId {
        let button_id = ButtonId(self.next_button_id);
        self.next_button_id += 1;

        let button = Button::new(button_id, rect, text);
        let _ = self.buttons.push(button);
        self.needs_redraw = true;
        button_id
    }

    /// Get button under a point.
    pub fn button_at_point(&self, x: usize, y: usize) -> Option<ButtonId> {
        self.buttons
            .iter()
            .find(|button| button.visible && button.rect.contains(x, y))
            .map(|button| button.id)
    }

    /// Enable or disable a button.
    pub fn set_button_enabled(&mut self, button_id: ButtonId, enabled: bool) -> bool {
        if let Some(button) = self
            .buttons
            .iter_mut()
            .find(|button| button.id == button_id)
        {
            button.enabled = enabled;
            self.needs_redraw = true;
            true
        } else {
            false
        }
    }

    /// Show or hide a button.
    pub fn set_button_visible(&mut self, button_id: ButtonId, visible: bool) -> bool {
        if let Some(button) = self
            .buttons
            .iter_mut()
            .find(|button| button.id == button_id)
        {
            button.visible = visible;
            self.needs_redraw = true;
            true
        } else {
            false
        }
    }

    /// Handle desktop keyboard shortcuts.
    pub fn handle_key_down(&mut self, key: u8) -> bool {
        if self.focused_window == self.shell_window && key != 27 && key != b'\t' {
            return self.handle_shell_key(key);
        }

        match key {
            27 => {
                if self.activities_open || self.quick_settings_open {
                    self.activities_open = false;
                    self.quick_settings_open = false;
                    self.needs_redraw = true;
                    true
                } else {
                    false
                }
            }
            b'\t' => {
                let window_count = self.windows.len();
                if window_count == 0 {
                    return false;
                }

                let start = self
                    .focused_window
                    .and_then(|id| self.windows.iter().position(|window| window.id == id))
                    .unwrap_or(window_count);

                for offset in 1..=window_count {
                    let index = (start + window_count - offset) % window_count;
                    let window = &self.windows[index];
                    if window.visible && window.state != WindowState::Closed {
                        return self.bring_to_front(window.id);
                    }
                }

                false
            }
            b'a' | b'A' => {
                self.activities_open = !self.activities_open;
                self.quick_settings_open = false;
                self.needs_redraw = true;
                true
            }
            b'q' | b'Q' => {
                self.quick_settings_open = !self.quick_settings_open;
                self.activities_open = false;
                self.needs_redraw = true;
                true
            }
            b'm' | b'M' => self
                .focused_window
                .map_or(false, |window_id| self.minimize_window(window_id)),
            _ => false,
        }
    }

    /// Scroll text content in the window under the cursor.
    pub fn handle_scroll(&mut self, x: usize, y: usize, delta: i32) -> bool {
        let Some(window_id) = self.window_at_point(x, y) else {
            return false;
        };

        let line_height = crate::graphics::get_default_font().char_height + 2;
        let Some(window) = self.get_window_mut(window_id) else {
            return false;
        };

        let visible_lines = window.client_area.height.saturating_sub(16) / line_height.max(1);
        let max_offset = window
            .content_lines
            .len()
            .saturating_sub(visible_lines.max(1));
        let old_offset = window.scroll_offset;

        if delta < 0 {
            window.scroll_offset = window.scroll_offset.saturating_add(1).min(max_offset);
        } else if delta > 0 {
            window.scroll_offset = window.scroll_offset.saturating_sub(1);
        }

        if window.scroll_offset != old_offset {
            self.needs_redraw = true;
            true
        } else {
            false
        }
    }

    /// Handle mouse move
    pub fn handle_mouse_move(&mut self, x: usize, y: usize) {
        self.cursor.x = x;
        self.cursor.y = y;

        if let Some(window_id) = self.dragging_window {
            let drag_offset = self.drag_offset;
            if let Some(window) = self.get_window_mut(window_id) {
                let new_x = x.saturating_sub(drag_offset.0);
                let new_y = y.saturating_sub(drag_offset.1);
                window.rect.x = new_x;
                window.rect.y = new_y;
                window.client_area.x = new_x + BORDER_WIDTH;
                window.client_area.y = new_y + TITLE_BAR_HEIGHT + BORDER_WIDTH;
            }
            self.needs_redraw = true;
        }

        if let Some(window_id) = self.resizing_window {
            let drag_offset = self.drag_offset;
            if let Some(window) = self.get_window_mut(window_id) {
                let new_w = max(
                    x.saturating_sub(window.rect.x)
                        .saturating_sub(drag_offset.0),
                    MIN_WINDOW_WIDTH,
                );
                let new_h = max(
                    y.saturating_sub(window.rect.y)
                        .saturating_sub(drag_offset.1),
                    MIN_WINDOW_HEIGHT,
                );
                window.rect.width = new_w;
                window.rect.height = new_h;
                window.client_area.width = new_w.saturating_sub(2 * BORDER_WIDTH);
                window.client_area.height =
                    new_h.saturating_sub(TITLE_BAR_HEIGHT + 2 * BORDER_WIDTH);
            }
            self.needs_redraw = true;
        }

        // Update button hover states
        for button in &mut self.buttons {
            button.hovered = button.rect.contains(x, y);
        }
    }

    /// Handle mouse down
    pub fn handle_mouse_down(&mut self, x: usize, y: usize, _button: MouseButton) -> bool {
        let activities_button =
            Rect::new(self.menu_bar_rect.x + 10, self.menu_bar_rect.y + 5, 72, 20);
        if activities_button.contains(x, y) {
            self.activities_open = !self.activities_open;
            self.quick_settings_open = false;
            self.needs_redraw = true;
            return true;
        }

        let quick_settings_button = Rect::new(
            self.menu_bar_rect.width.saturating_sub(160),
            self.menu_bar_rect.y + 4,
            150,
            22,
        );
        if quick_settings_button.contains(x, y) {
            self.quick_settings_open = !self.quick_settings_open;
            self.activities_open = false;
            self.needs_redraw = true;
            return true;
        }

        if self.activities_open {
            if let Some(window_id) = self.activities_window_at_point(x, y) {
                self.activities_open = false;
                return self.bring_to_front(window_id);
            }
        }

        if let Some(window_id) = self.panel_window_at_point(x, y) {
            return self.bring_to_front(window_id);
        }

        if let Some(window_id) = self.launcher_window_at_point(x, y) {
            return self.bring_to_front(window_id);
        }

        if let Some(window_id) = self.window_at_point(x, y) {
            self.focus_window(window_id);

            let window_info = if let Some(window) = self.get_window(window_id) {
                Some((
                    window.rect.x,
                    window.rect.y,
                    window.rect.width,
                    window.rect.height,
                    window.resizable,
                ))
            } else {
                None
            };

            if let Some((win_x, win_y, win_width, win_height, resizable)) = window_info {
                // Linux-style close button on the right side of the title bar.
                let close_rect = Rect::new(
                    win_x + win_width.saturating_sub(30),
                    win_y + BORDER_WIDTH + 4,
                    22,
                    TITLE_BAR_HEIGHT.saturating_sub(8),
                );
                let max_rect = Rect::new(
                    win_x + win_width.saturating_sub(56),
                    win_y + BORDER_WIDTH + 4,
                    22,
                    TITLE_BAR_HEIGHT.saturating_sub(8),
                );
                let min_rect = Rect::new(
                    win_x + win_width.saturating_sub(82),
                    win_y + BORDER_WIDTH + 4,
                    22,
                    TITLE_BAR_HEIGHT.saturating_sub(8),
                );
                if min_rect.contains(x, y) {
                    self.minimize_window(window_id);
                    return true;
                }
                if max_rect.contains(x, y) {
                    if self
                        .get_window(window_id)
                        .map_or(false, |w| w.state == WindowState::Maximized)
                    {
                        self.restore_window(window_id);
                    } else {
                        self.maximize_window(window_id);
                    }
                    return true;
                }
                if close_rect.contains(x, y) {
                    self.close_window(window_id);
                    return true;
                }

                // Check resize handle (bottom-right corner)
                if resizable {
                    let handle_x = win_x + win_width.saturating_sub(RESIZE_HANDLE_SIZE);
                    let handle_y = win_y + win_height.saturating_sub(RESIZE_HANDLE_SIZE);
                    if x >= handle_x && y >= handle_y {
                        self.resizing_window = Some(window_id);
                        self.drag_offset = (
                            x.saturating_sub(win_x + win_width),
                            y.saturating_sub(win_y + win_height),
                        );
                        return true;
                    }
                }

                // Check title bar for dragging
                let title_rect = Rect::new(win_x, win_y, win_width, TITLE_BAR_HEIGHT);
                if title_rect.contains(x, y) {
                    self.dragging_window = Some(window_id);
                    self.drag_offset = (x.saturating_sub(win_x), y.saturating_sub(win_y));
                    return true;
                }
            }
        }

        // Check buttons
        for button in &mut self.buttons {
            if button.rect.contains(x, y) && button.enabled && button.visible {
                button.pressed = true;
                self.needs_redraw = true;
                return true;
            }
        }

        false
    }

    /// Handle mouse up
    pub fn handle_mouse_up(&mut self, _x: usize, _y: usize, _button: MouseButton) {
        self.dragging_window = None;
        self.resizing_window = None;

        for button in &mut self.buttons {
            if button.pressed {
                button.pressed = false;
                self.needs_redraw = true;
            }
        }
    }

    /// Render the desktop
    pub fn render(&mut self) {
        if !self.needs_redraw {
            return;
        }

        // Clear desktop background
        self.render_background();
        self.render_desktop_icons();
        self.render_menu_bar();
        self.render_dock();

        // Render windows (back to front)
        let mut sorted_windows: Vec<&Window, MAX_WINDOWS> = Vec::new();
        for window in &self.windows {
            if window.visible {
                let _ = sorted_windows.push(window);
            }
        }

        // Simple sort by z_order
        for i in 0..sorted_windows.len() {
            for j in i + 1..sorted_windows.len() {
                if sorted_windows[i].z_order > sorted_windows[j].z_order {
                    sorted_windows.swap(i, j);
                }
            }
        }

        for window in &sorted_windows {
            self.render_window(window);
        }

        // Render buttons
        for button in &self.buttons {
            if button.visible {
                self.render_button(button);
            }
        }

        if self.activities_open {
            self.render_activities_overview();
        }

        if self.quick_settings_open {
            self.render_quick_settings();
        }

        // Render cursor
        if self.cursor.visible {
            self.render_cursor();
        }

        self.needs_redraw = false;
    }

    /// Render a single window
    fn render_window(&self, window: &Window) {
        self.render_window_shadow(window);

        // Render border
        if window.has_border {
            crate::graphics::framebuffer::draw_rect(window.rect, window.border_color, BORDER_WIDTH);
        }

        // Render title bar
        if window.has_title_bar {
            let title_rect = Rect::new(
                window.rect.x + BORDER_WIDTH,
                window.rect.y + BORDER_WIDTH,
                window.rect.width.saturating_sub(2 * BORDER_WIDTH),
                TITLE_BAR_HEIGHT,
            );
            let title_end_color = if window.focused {
                Self::shade_color(window.title_bar_color, -40)
            } else {
                Self::shade_color(window.title_bar_color, -20)
            };
            self.fill_horizontal_gradient(title_rect, window.title_bar_color, title_end_color);
            self.render_window_controls(window);

            // Render title text
            let title_text_color = colors::TEXT_COLOR_WHITE;
            let title_x = window.rect.x + BORDER_WIDTH + 12;
            let title_y = window.rect.y + BORDER_WIDTH + (TITLE_BAR_HEIGHT.saturating_sub(8)) / 2;
            crate::graphics::draw_text(
                window.title,
                title_x,
                title_y,
                title_text_color,
                crate::graphics::get_default_font(),
            );
        }

        // Render window content area
        crate::graphics::framebuffer::fill_rect(window.client_area, window.background_color);

        if self.shell_window == Some(window.id) {
            self.render_shell_content(window);
            return;
        }

        // Render window content text
        let font = crate::graphics::get_default_font();
        let line_height = font.char_height + 2;
        let mut text_y = window.client_area.y + 8;
        for line in window.content_lines.iter().skip(window.scroll_offset) {
            crate::graphics::draw_text(
                line,
                window.client_area.x + 8,
                text_y,
                colors::TEXT_COLOR,
                font,
            );
            text_y += line_height;
            if text_y + font.char_height > window.client_area.y + window.client_area.height {
                break;
            }
        }

        // Render resize handle indicator (bottom-right corner)
        if window.resizable {
            let handle_x = window.rect.x + window.rect.width.saturating_sub(RESIZE_HANDLE_SIZE);
            let handle_y = window.rect.y + window.rect.height.saturating_sub(RESIZE_HANDLE_SIZE);
            for i in 0..3 {
                let offset = (2 - i) as usize;
                let px = handle_x + offset + 2;
                let py = handle_y + 4 + i * 3;
                if px < window.rect.x + window.rect.width && py < window.rect.y + window.rect.height
                {
                    crate::graphics::framebuffer::set_pixel(px, py, colors::BORDER_INACTIVE);
                    crate::graphics::framebuffer::set_pixel(px + 1, py, colors::BORDER_INACTIVE);
                }
            }
        }
    }

    fn render_shell_content(&self, window: &Window) {
        let font = crate::graphics::get_default_font();
        let line_height = font.char_height + 2;
        let mut text_y = window.client_area.y + 8;

        for line in &self.shell_lines {
            crate::graphics::draw_text(
                line.as_str(),
                window.client_area.x + 8,
                text_y,
                colors::TEXT_COLOR,
                font,
            );
            text_y += line_height;
            if text_y + font.char_height > window.client_area.y + window.client_area.height {
                return;
            }
        }

        let prompt = format!("> {}_", self.shell_input.as_str());
        crate::graphics::draw_text(
            &prompt,
            window.client_area.x + 8,
            text_y,
            colors::TEXT_COLOR,
            font,
        );
    }

    /// Render a button
    fn render_button(&self, button: &Button) {
        let bg_color = if button.pressed {
            colors::BUTTON_PRESSED
        } else if button.hovered {
            colors::BUTTON_HOVER
        } else {
            button.background_color
        };

        crate::graphics::framebuffer::fill_rect(button.rect, bg_color);
        crate::graphics::framebuffer::draw_rect(button.rect, colors::BORDER_INACTIVE, 1);
    }

    /// Render cursor
    fn render_cursor(&self) {
        // Simple cursor - just a few pixels
        for dy in 0..10 {
            for dx in 0..2 {
                if self.cursor.x + dx < self.desktop_rect.width
                    && self.cursor.y + dy < self.desktop_rect.height
                {
                    crate::graphics::framebuffer::set_pixel(
                        self.cursor.x + dx,
                        self.cursor.y + dy,
                        self.cursor.color,
                    );
                }
            }
        }
    }

    /// Get focused window
    pub fn get_focused_window(&self) -> Option<WindowId> {
        self.focused_window
    }

    /// Get window count
    pub fn get_window_count(&self) -> usize {
        self.windows.len()
    }

    /// Get button count.
    pub fn get_button_count(&self) -> usize {
        self.buttons.len()
    }

    /// Set cursor position
    pub fn set_cursor_position(&mut self, x: usize, y: usize) {
        self.cursor.x = x.min(self.desktop_rect.width.saturating_sub(1));
        self.cursor.y = y.min(self.desktop_rect.height.saturating_sub(1));
    }

    /// Show/hide cursor
    pub fn set_cursor_visible(&mut self, visible: bool) {
        self.cursor.visible = visible;
        self.needs_redraw = true;
    }

    /// Get desktop rect
    pub fn get_desktop_rect(&self) -> Rect {
        self.desktop_rect
    }

    /// Check if redraw is needed
    pub fn needs_redraw(&self) -> bool {
        self.needs_redraw
    }

    /// Force redraw
    pub fn force_redraw(&mut self) {
        self.needs_redraw = true;
    }

    fn render_background(&self) {
        let height = max(self.desktop_rect.height, 1);
        for row in 0..height {
            let color = Self::lerp_color(
                colors::DESKTOP_BACKGROUND_TOP,
                colors::DESKTOP_BACKGROUND_BOTTOM,
                row,
                height - 1,
            );
            let stripe = Rect::new(
                self.desktop_rect.x,
                self.desktop_rect.y + row,
                self.desktop_rect.width,
                1,
            );
            crate::graphics::framebuffer::fill_rect(stripe, color);
        }
    }

    fn render_desktop_icons(&self) {
        let font = crate::graphics::get_default_font();
        let x = self.dock_rect.width + 24;
        let y = self.menu_bar_rect.height + 28;
        let uptime = crate::time::uptime_ms() / 1000;
        let windows = self
            .windows
            .iter()
            .filter(|window| window.state != WindowState::Closed)
            .count();
        let line1 = format!("RustOS Desktop");
        let line2 = format!("Open windows: {}", windows);
        let line3 = format!("Uptime: {}s", uptime);
        crate::graphics::draw_text(&line1, x, y, colors::TEXT_COLOR_WHITE, font);
        crate::graphics::draw_text(&line2, x, y + 18, colors::TEXT_COLOR_WHITE, font);
        crate::graphics::draw_text(&line3, x, y + 36, colors::TEXT_COLOR_WHITE, font);
    }

    fn render_activities_overview(&self) {
        let font = crate::graphics::get_default_font();
        let overlay = Rect::new(
            self.dock_rect.width + 24,
            self.menu_bar_rect.height + 24,
            self.desktop_rect
                .width
                .saturating_sub(self.dock_rect.width + 48),
            self.desktop_rect
                .height
                .saturating_sub(self.menu_bar_rect.height + 48),
        );

        crate::graphics::framebuffer::fill_rect(overlay, Color::rgb(28, 28, 34));
        crate::graphics::framebuffer::draw_rect(overlay, colors::DOCK_ICON_ACCENT, 2);
        crate::graphics::draw_text(
            "Activities Overview",
            overlay.x + 18,
            overlay.y + 16,
            colors::TEXT_COLOR_WHITE,
            font,
        );
        crate::graphics::draw_text(
            "Windows",
            overlay.x + 18,
            overlay.y + 42,
            colors::MENU_BAR_ICON,
            font,
        );

        let mut x = overlay.x + 18;
        let mut y = overlay.y + 66;
        for window in self
            .windows
            .iter()
            .filter(|window| window.state != WindowState::Closed)
        {
            let tile = Rect::new(x, y, 180, 96);
            let tile_color = if window.focused {
                colors::DOCK_ICON_ACCENT
            } else {
                colors::MENU_BAR_HIGHLIGHT
            };
            crate::graphics::framebuffer::fill_rect(tile, tile_color);
            crate::graphics::framebuffer::draw_rect(tile, colors::BORDER_INACTIVE, 1);
            crate::graphics::draw_text(
                window.title,
                tile.x + 10,
                tile.y + 10,
                colors::TEXT_COLOR_WHITE,
                font,
            );

            x += 196;
            if x + 180 > overlay.x + overlay.width {
                x = overlay.x + 18;
                y += 112;
            }
            if y + 96 > overlay.y + overlay.height {
                break;
            }
        }

        let app_y = overlay.y + overlay.height.saturating_sub(56);
        crate::graphics::draw_text(
            "Application registry unavailable",
            overlay.x + 18,
            app_y,
            colors::MENU_BAR_ICON,
            font,
        );
    }

    fn render_quick_settings(&self) {
        let font = crate::graphics::get_default_font();
        let panel = Rect::new(
            self.desktop_rect.width.saturating_sub(260),
            self.menu_bar_rect.height + 8,
            244,
            150,
        );
        let uptime = format!("Uptime: {}s", crate::time::uptime_ms() / 1000);
        let windows = format!(
            "Open windows: {}",
            self.windows
                .iter()
                .filter(|window| window.state != WindowState::Closed)
                .count()
        );
        let focused = self
            .focused_window
            .and_then(|id| self.get_window(id))
            .map_or("none", |window| window.title);
        let focused_line = format!("Focused: {}", focused);
        let network_line = format!(
            "Network interfaces: {}",
            crate::net::network_stack().list_interfaces().len()
        );

        crate::graphics::framebuffer::fill_rect(panel, Color::rgb(32, 32, 38));
        crate::graphics::framebuffer::draw_rect(panel, colors::DOCK_ICON_ACCENT, 2);
        crate::graphics::draw_text(
            "System",
            panel.x + 14,
            panel.y + 12,
            colors::TEXT_COLOR_WHITE,
            font,
        );
        crate::graphics::draw_text(
            &uptime,
            panel.x + 14,
            panel.y + 36,
            colors::MENU_BAR_ICON,
            font,
        );
        crate::graphics::draw_text(
            &windows,
            panel.x + 14,
            panel.y + 54,
            colors::MENU_BAR_ICON,
            font,
        );
        crate::graphics::draw_text(
            &focused_line,
            panel.x + 14,
            panel.y + 72,
            colors::MENU_BAR_ICON,
            font,
        );
        crate::graphics::draw_text(
            &network_line,
            panel.x + 14,
            panel.y + 98,
            colors::MENU_BAR_ICON,
            font,
        );
        crate::graphics::draw_text(
            "Audio: unavailable",
            panel.x + 14,
            panel.y + 116,
            colors::MENU_BAR_ICON,
            font,
        );
    }

    fn render_menu_bar(&self) {
        crate::graphics::framebuffer::fill_rect(self.menu_bar_rect, colors::MENU_BAR_BACKGROUND);

        let bottom = Rect::new(
            self.menu_bar_rect.x,
            self.menu_bar_rect.y + self.menu_bar_rect.height.saturating_sub(1),
            self.menu_bar_rect.width,
            1,
        );
        crate::graphics::framebuffer::fill_rect(bottom, colors::MENU_BAR_ACCENT);

        let app_button = Rect::new(self.menu_bar_rect.x + 10, self.menu_bar_rect.y + 5, 72, 20);
        crate::graphics::framebuffer::fill_rect(app_button, colors::MENU_BAR_HIGHLIGHT);
        crate::graphics::framebuffer::draw_rect(app_button, colors::BORDER_INACTIVE, 1);

        let font = crate::graphics::get_default_font();
        let text_y = self.menu_bar_rect.y + (MENU_BAR_HEIGHT.saturating_sub(font.char_height)) / 2;
        crate::graphics::draw_text(
            "Activities",
            app_button.x + 8,
            text_y,
            colors::MENU_BAR_ICON,
            font,
        );
        crate::graphics::draw_text(
            "RustOS Desktop",
            self.menu_bar_rect.x + 104,
            text_y,
            colors::MENU_BAR_ICON,
            font,
        );

        let mut task_x = self.menu_bar_rect.x + 220;
        let task_limit = self.menu_bar_rect.width.saturating_sub(160);
        for window in self
            .windows
            .iter()
            .filter(|window| window.state != WindowState::Closed)
        {
            let task_width = (window.title.len() * font.char_width + 22).clamp(72, 180);
            if task_x + task_width >= task_limit {
                break;
            }
            let task_rect = Rect::new(task_x, self.menu_bar_rect.y + 5, task_width, 20);
            let task_color = if window.focused {
                colors::DOCK_ICON_ACCENT
            } else {
                colors::MENU_BAR_HIGHLIGHT
            };
            crate::graphics::framebuffer::fill_rect(task_rect, task_color);
            crate::graphics::framebuffer::draw_rect(task_rect, colors::BORDER_INACTIVE, 1);
            crate::graphics::draw_text(
                window.title,
                task_rect.x + 8,
                text_y,
                colors::TEXT_COLOR_WHITE,
                font,
            );
            task_x += task_width + 8;
        }

        self.render_workspace_switcher();
        self.render_system_tray(text_y);

        let right_text = format!("UP {}s", crate::time::uptime_ms() / 1000);
        let right_width = right_text.len() * font.char_width;
        let right_x =
            self.menu_bar_rect.x + self.menu_bar_rect.width.saturating_sub(right_width + 12);
        crate::graphics::draw_text(&right_text, right_x, text_y, colors::MENU_BAR_ICON, font);
    }

    fn render_dock(&self) {
        crate::graphics::framebuffer::fill_rect(self.dock_rect, colors::DOCK_BACKGROUND);

        let edge = Rect::new(
            self.dock_rect.x + self.dock_rect.width.saturating_sub(1),
            self.dock_rect.y,
            1,
            self.dock_rect.height,
        );
        crate::graphics::framebuffer::fill_rect(edge, colors::MENU_BAR_ACCENT);

        self.render_dock_icons(self.dock_rect);
    }

    fn render_workspace_switcher(&self) {
        let switcher_x = self.menu_bar_rect.width.saturating_sub(260);
        let switcher_y = self.menu_bar_rect.y + 7;

        for i in 0..4 {
            let rect = Rect::new(switcher_x + i * 22, switcher_y, 16, 16);
            let color = if i == 0 {
                colors::DOCK_ICON_ACCENT
            } else {
                colors::MENU_BAR_HIGHLIGHT
            };
            crate::graphics::framebuffer::fill_rect(rect, color);
            crate::graphics::framebuffer::draw_rect(rect, colors::BORDER_INACTIVE, 1);
        }
    }

    fn render_system_tray(&self, text_y: usize) {
        let font = crate::graphics::get_default_font();
        let tray_x = self.menu_bar_rect.width.saturating_sub(152);
        let tray_rect = Rect::new(tray_x.saturating_sub(8), self.menu_bar_rect.y + 5, 86, 20);
        crate::graphics::framebuffer::fill_rect(tray_rect, colors::MENU_BAR_ACCENT);
        crate::graphics::framebuffer::draw_rect(tray_rect, colors::BORDER_INACTIVE, 1);
        crate::graphics::draw_text(
            "NET n/a",
            tray_rect.x + 8,
            text_y,
            colors::MENU_BAR_ICON,
            font,
        );
    }

    fn render_dock_icons(&self, launcher_rect: Rect) {
        let font = crate::graphics::get_default_font();
        let icon_x = launcher_rect.x + (launcher_rect.width.saturating_sub(DOCK_ICON_SIZE)) / 2;
        let mut icon_y = launcher_rect.y + 16;
        let icon_colors = [
            colors::DOCK_ICON_ACCENT,
            Color::rgb(84, 160, 255),
            Color::rgb(78, 188, 116),
            Color::rgb(212, 152, 48),
            Color::rgb(160, 100, 220),
            Color::rgb(218, 76, 50),
        ];
        let icon_labels = ["H", "F", "T", "W", "S", "A"];

        for i in 0..DOCK_ICON_COUNT {
            if icon_y + DOCK_ICON_SIZE > launcher_rect.y + launcher_rect.height {
                break;
            }

            let window_for_slot = self
                .windows
                .iter()
                .filter(|window| window.state != WindowState::Closed)
                .nth(i);
            let icon_rect = Rect::new(icon_x, icon_y, DOCK_ICON_SIZE, DOCK_ICON_SIZE);
            let border = if window_for_slot.map_or(false, |window| window.focused) {
                colors::DOCK_ICON_ACCENT
            } else {
                colors::DOCK_HIGHLIGHT
            };
            crate::graphics::framebuffer::fill_rect(icon_rect, colors::DOCK_GLASS);
            crate::graphics::framebuffer::draw_rect(icon_rect, border, 1);

            let inner = Rect::new(
                icon_x + 7,
                icon_y + 7,
                DOCK_ICON_SIZE - 14,
                DOCK_ICON_SIZE - 14,
            );
            crate::graphics::framebuffer::fill_rect(inner, icon_colors[i % icon_colors.len()]);

            let label = icon_labels[i % icon_labels.len()];
            let label_x =
                icon_x + (DOCK_ICON_SIZE.saturating_sub(label.len() * font.char_width)) / 2;
            let label_y = icon_y + (DOCK_ICON_SIZE.saturating_sub(font.char_height)) / 2;
            crate::graphics::draw_text(label, label_x, label_y, colors::DOCK_INDICATOR, font);

            if window_for_slot.is_some() {
                let indicator = Rect::new(
                    launcher_rect.x + 3,
                    icon_y + 10,
                    3,
                    DOCK_ICON_SIZE.saturating_sub(20),
                );
                crate::graphics::framebuffer::fill_rect(indicator, colors::DOCK_INDICATOR);
            }
            icon_y += DOCK_ICON_SIZE + DOCK_ICON_GAP;
        }
    }

    fn render_window_controls(&self, window: &Window) {
        let button_y = window.rect.y + BORDER_WIDTH + 4;
        let close_x = window.rect.x + window.rect.width.saturating_sub(30);
        let max_x = close_x.saturating_sub(26);
        let min_x = max_x.saturating_sub(26);
        let button_h = TITLE_BAR_HEIGHT.saturating_sub(8);

        let buttons = [
            (
                Rect::new(min_x, button_y, 22, button_h),
                colors::TRAFFIC_LIGHT_YELLOW,
            ),
            (
                Rect::new(max_x, button_y, 22, button_h),
                colors::TRAFFIC_LIGHT_GREEN,
            ),
            (
                Rect::new(close_x, button_y, 22, button_h),
                colors::TRAFFIC_LIGHT_RED,
            ),
        ];

        for (rect, color) in buttons.iter() {
            crate::graphics::framebuffer::fill_rect(*rect, *color);
            crate::graphics::framebuffer::draw_rect(*rect, colors::BORDER_INACTIVE, 1);
        }
    }

    fn render_window_shadow(&self, window: &Window) {
        if WINDOW_SHADOW_MARGIN == 0 {
            return;
        }

        let shadow_rect = Rect::new(
            window.rect.x.saturating_sub(WINDOW_SHADOW_MARGIN),
            window.rect.y.saturating_sub(WINDOW_SHADOW_MARGIN),
            window.rect.width + WINDOW_SHADOW_MARGIN * 2,
            window.rect.height + WINDOW_SHADOW_MARGIN * 2,
        );

        crate::graphics::framebuffer::fill_rect(shadow_rect, colors::WINDOW_SHADOW);
    }

    fn render_glow(&self, rect: Rect) {
        let steps = min(rect.height / 2, 12).max(1);
        for i in 0..steps {
            let inset = i * 4;
            if rect.width <= inset * 2 || rect.height <= inset * 2 {
                break;
            }
            let glow_rect = Rect::new(
                rect.x + inset,
                rect.y + inset,
                rect.width.saturating_sub(inset * 2),
                rect.height.saturating_sub(inset * 2),
            );
            let delta = ((steps - i) * 4).min(48) as i16;
            let shade = Self::shade_color(colors::DESKTOP_GLOW, delta);
            crate::graphics::framebuffer::fill_rect(glow_rect, shade);
        }
    }

    fn draw_circle(&self, center_x: usize, center_y: usize, radius: usize, color: Color) {
        let radius = radius as isize;
        let radius_sq = radius * radius;
        let center_x = center_x as isize;
        let center_y = center_y as isize;
        let width = self.desktop_rect.width as isize;
        let height = self.desktop_rect.height as isize;

        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx * dx + dy * dy <= radius_sq {
                    let x = center_x + dx;
                    let y = center_y + dy;
                    if x >= 0 && y >= 0 && x < width && y < height {
                        crate::graphics::framebuffer::set_pixel(x as usize, y as usize, color);
                    }
                }
            }
        }
    }

    fn fill_horizontal_gradient(&self, rect: Rect, start: Color, end: Color) {
        if rect.width == 0 {
            return;
        }

        for column in 0..rect.width {
            let color = Self::lerp_color(start, end, column, rect.width - 1);
            let line = Rect::new(rect.x + column, rect.y, 1, rect.height);
            crate::graphics::framebuffer::fill_rect(line, color);
        }
    }

    fn lerp_color(start: Color, end: Color, numerator: usize, denominator: usize) -> Color {
        if denominator == 0 {
            return start;
        }

        let r = Self::lerp_channel(start.r, end.r, numerator, denominator);
        let g = Self::lerp_channel(start.g, end.g, numerator, denominator);
        let b = Self::lerp_channel(start.b, end.b, numerator, denominator);
        Color::rgb(r, g, b)
    }

    fn lerp_channel(start: u8, end: u8, numerator: usize, denominator: usize) -> u8 {
        if denominator == 0 {
            return start;
        }
        let start = start as i32;
        let end = end as i32;
        let diff = end - start;
        let value = start + diff * numerator as i32 / denominator as i32;
        value.clamp(0, 255) as u8
    }

    fn shade_color(color: Color, delta: i16) -> Color {
        let adjust = |channel: u8| -> u8 {
            let value = channel as i32 + delta as i32;
            value.clamp(0, 255) as u8
        };

        Color::rgb(adjust(color.r), adjust(color.g), adjust(color.b))
    }
}

/// Desktop event handling result
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventResult {
    Handled,
    NotHandled,
    WindowClosed(WindowId),
}

// Test functions (without test attributes to avoid no_std issues)
#[cfg(test)]
mod tests {
    use super::*;

    fn test_window_creation() {
        let mut wm = WindowManager::new(1920, 1080);
        let window_id = wm.create_window("Test Window", 100, 100, 400, 300);

        assert_ne!(window_id, WindowId::INVALID);
        assert_eq!(wm.window_count, 1);
    }

    fn test_window_focus() {
        let mut wm = WindowManager::new(1920, 1080);
        let window1 = wm.create_window("Window 1", 100, 100, 400, 300);
        let window2 = wm.create_window("Window 2", 200, 200, 400, 300);

        assert_eq!(wm.get_focused_window(), Some(window2));

        assert!(wm.focus_window(window1));
        assert_eq!(wm.get_focused_window(), Some(window1));
    }

    fn test_window_close() {
        let mut wm = WindowManager::new(1920, 1080);
        let window1 = wm.create_window("Window 1", 100, 100, 400, 300);
        let window2 = wm.create_window("Window 2", 200, 200, 400, 300);

        assert_eq!(wm.window_count, 2);
        assert!(wm.close_window(window1));
        assert_eq!(wm.window_count, 1);
        assert!(wm.get_window(window2).is_some());
        assert!(wm.get_window(window1).is_none());
    }

    fn test_window_at_point() {
        let mut wm = WindowManager::new(1920, 1080);
        let window1 = wm.create_window("Window 1", 100, 100, 200, 200);
        let window2 = wm.create_window("Window 2", 150, 150, 200, 200);

        assert_eq!(wm.window_at_point(120, 120), Some(window1));
        assert_eq!(wm.window_at_point(175, 175), Some(window2));
        assert_eq!(wm.window_at_point(50, 50), None);
    }

    fn test_button_creation() {
        let mut wm = WindowManager::new(1920, 1080);
        let button_rect = Rect::new(10, 10, 100, 30);
        let button_id = wm.create_button(button_rect, "Test Button");

        assert_ne!(button_id, ButtonId::INVALID);
        assert_eq!(wm.buttons.len(), 1);
    }
}
