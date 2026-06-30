//! # RustOS Desktop Window Manager
//!
//! A comprehensive desktop environment with window management, UI components,
//! and event handling for the RustOS kernel.

use crate::desktop::app_grid;
use crate::graphics::framebuffer::{Color, Rect};
use crate::vfs::{vfs_close, vfs_open, vfs_read, vfs_readdir, InodeType, OpenFlags};
use alloc::format;
use core::cmp::{max, min};
use core::fmt::Write as _;
use heapless::{String as HString, Vec};

/// Maximum lines of text content per window
pub const MAX_CONTENT_LINES: usize = 32;
pub const MAX_SHELL_LINES: usize = 12;
pub const MAX_FM_ENTRIES: usize = 24;
pub const MAX_MONITOR_LINES: usize = 16;
/// Size of the resize handle in the bottom-right corner
pub const RESIZE_HANDLE_SIZE: usize = 12;

/// Maximum number of windows that can be managed simultaneously
pub const MAX_WINDOWS: usize = 64;

/// Default window title bar height
pub const TITLE_BAR_HEIGHT: usize = 28;

/// Default window border width
pub const BORDER_WIDTH: usize = 1;

/// Linux-style shell chrome dimensions (GNOME-like top panel + left launcher).
pub const MENU_BAR_HEIGHT: usize = 30;
pub const DOCK_HEIGHT: usize = 64;
pub const DOCK_ICON_SIZE: usize = 42;
pub const DOCK_ICON_GAP: usize = 10;
pub const DOCK_ICON_COUNT: usize = 6;
pub const WINDOW_SHADOW_MARGIN: usize = 6;
pub const FM_ROW_HEIGHT: usize = 18;

/// Minimum window size
pub const MIN_WINDOW_WIDTH: usize = 200;
pub const MIN_WINDOW_HEIGHT: usize = 150;

/// Context menu constants
pub const MENU_ITEM_HEIGHT: usize = 22;
pub const MENU_ITEM_PADDING: usize = 12;
pub const MENU_MIN_WIDTH: usize = 140;
pub const MENU_BORDER_WIDTH: usize = 1;

/// Snap zone size — how close to an edge before snapping engages
pub const SNAP_ZONE: usize = 8;

/// Number of virtual workspaces
pub const WORKSPACE_COUNT: usize = 4;

/// Desktop colors
pub mod colors {
    use crate::graphics::framebuffer::Color;

    // Ubuntu / Yaru desktop palette (Jammy-style aubergine wallpaper, dark panel,
    // left dock, Ubuntu orange accents).
    pub const DESKTOP_BACKGROUND_TOP: Color = Color::rgb(44, 0, 30); // #2C001E
    pub const DESKTOP_BACKGROUND_BOTTOM: Color = Color::rgb(94, 39, 80); // #5E2750
    pub const DESKTOP_GLOW: Color = Color::rgb(233, 84, 32); // #E95420 Ubuntu orange
    pub const MENU_BAR_BACKGROUND: Color = Color::rgb(26, 26, 26);
    pub const MENU_BAR_TOP: Color = Color::rgb(30, 30, 30);
    pub const MENU_BAR_BOTTOM: Color = Color::rgb(19, 19, 19);
    pub const DOCK_TOP: Color = Color::rgb(48, 48, 48);
    pub const DOCK_BOTTOM: Color = Color::rgb(36, 36, 36);
    pub const TITLE_ACTIVE_TOP: Color = Color::rgb(119, 41, 83); // aubergine accent
    pub const TITLE_ACTIVE_BOTTOM: Color = Color::rgb(94, 39, 80);
    pub const MENU_BAR_HIGHLIGHT: Color = Color::rgb(55, 55, 55);
    pub const MENU_BAR_ACCENT: Color = Color::rgb(12, 12, 12);
    pub const MENU_BAR_ICON: Color = Color::rgb(255, 255, 255);
    pub const DESKTOP_BACKGROUND: Color = DESKTOP_BACKGROUND_TOP;
    pub const WINDOW_BACKGROUND: Color = Color::rgb(250, 250, 250);
    pub const WINDOW_SURFACE_TOP: Color = Color::rgb(255, 255, 255);
    pub const WINDOW_SURFACE_BOTTOM: Color = Color::rgb(245, 245, 245);
    pub const WINDOW_SURFACE_ALT: Color = Color::rgb(248, 248, 248);
    pub const WINDOW_SHADOW: Color = Color::rgb(8, 4, 12);
    pub const TITLE_BAR_ACTIVE: Color = Color::rgb(119, 41, 83);
    pub const TITLE_BAR_INACTIVE: Color = Color::rgb(90, 90, 90);
    pub const TITLE_INACTIVE_TOP: Color = Color::rgb(100, 100, 100);
    pub const TITLE_INACTIVE_BOTTOM: Color = Color::rgb(70, 70, 70);
    pub const BORDER_ACTIVE: Color = Color::rgb(233, 84, 32);
    pub const BORDER_INACTIVE: Color = Color::rgb(120, 120, 120);
    pub const TEXT_COLOR: Color = Color::rgb(28, 28, 28);
    pub const TEXT_COLOR_WHITE: Color = Color::rgb(255, 255, 255);
    pub const TEXT_COLOR_MUTED: Color = Color::rgb(140, 140, 140);
    pub const SHELL_BACKGROUND_TOP: Color = Color::rgb(44, 0, 30);
    pub const SHELL_BACKGROUND_BOTTOM: Color = Color::rgb(26, 26, 26);
    pub const SHELL_TEXT: Color = Color::rgb(255, 255, 255);
    pub const SHELL_PROMPT: Color = Color::rgb(233, 84, 32);
    pub const BUTTON_BACKGROUND: Color = Color::rgb(245, 245, 245);
    pub const BUTTON_HOVER: Color = Color::rgb(235, 235, 235);
    pub const BUTTON_PRESSED: Color = Color::rgb(220, 220, 220);
    pub const DOCK_BACKGROUND: Color = Color::rgb(44, 44, 44);
    pub const DOCK_GLASS: Color = Color::rgb(58, 58, 58);
    pub const DOCK_HIGHLIGHT: Color = Color::rgb(72, 72, 72);
    pub const DOCK_ICON_ACCENT: Color = Color::rgb(233, 84, 32);
    pub const DOCK_INDICATOR: Color = Color::rgb(255, 255, 255);
    pub const WINDOW_BTN_MIN: Color = Color::rgb(255, 189, 68);
    pub const WINDOW_BTN_MAX: Color = Color::rgb(40, 200, 64);
    pub const WINDOW_BTN_CLOSE: Color = Color::rgb(255, 95, 87);
    pub const FM_SELECTION: Color = Color::rgb(233, 84, 32);
}

/// Window state enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowState {
    Normal,
    Maximized,
    Minimized,
    Closed,
    Snapped,
}

/// Which side a window is snapped to
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapSide {
    Left,
    Right,
    Top,
    Bottom,
}

/// Context menu item
#[derive(Debug, Clone)]
pub struct ContextMenuItem {
    pub label: &'static str,
    pub action: MenuAction,
    pub enabled: bool,
    pub separator: bool,
}

/// Action triggered by a context menu item
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    None,
    CloseWindow,
    MinimizeWindow,
    MaximizeWindow,
    RestoreWindow,
    BringToFront,
    Refresh,
    OpenShell,
    OpenFileManager,
    OpenSystemMonitor,
    OpenTextEditor,
    OpenNetworkStatus,
    NewFolder,
    Delete,
    Rename,
    Properties,
    NextWorkspace,
    PrevWorkspace,
    GoToWorkspace(u8),
}

/// Context menu state
#[derive(Debug, Clone)]
pub struct ContextMenu {
    pub items: Vec<ContextMenuItem, 16>,
    pub rect: Rect,
    pub visible: bool,
    pub selected: usize,
    pub target_window: Option<WindowId>,
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
    pub workspace: u8,
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
            workspace: 0,
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

/// Directory entry shown in the file manager window.
#[derive(Debug, Clone)]
struct FileManagerEntry {
    name: HString<64>,
    is_directory: bool,
    size: u64,
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
    pub(super) needs_redraw: bool,
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
    file_manager_window: Option<WindowId>,
    fm_path: HString<128>,
    fm_selected: usize,
    fm_scroll: usize,
    fm_entries: Vec<FileManagerEntry, MAX_FM_ENTRIES>,
    system_monitor_window: Option<WindowId>,
    monitor_lines: Vec<HString<96>, MAX_MONITOR_LINES>,
    last_tick_second: u64,
    /// Context menu state
    context_menu: ContextMenu,
    /// Active workspace (0-indexed)
    current_workspace: u8,
    /// Text editor window
    text_editor_window: Option<WindowId>,
    /// Text editor content
    text_editor_lines: Vec<HString<96>, MAX_CONTENT_LINES>,
    /// Text editor cursor row
    te_cursor_row: usize,
    /// Text editor cursor col
    te_cursor_col: usize,
    /// Network status window
    network_status_window: Option<WindowId>,
    /// Network status lines
    net_status_lines: Vec<HString<96>, MAX_MONITOR_LINES>,
    /// Snapped side for the currently snapped window
    snapped_side: Option<SnapSide>,
    /// App-grid overlay open?
    app_grid_open: bool,
    /// Live search query in the app grid
    app_grid_query: HString<32>,
    /// Current app-grid page
    app_grid_page: usize,
    /// GNOME-style OSD text.
    gnome_osd_text: HString<64>,
    /// Uptime second when OSD should disappear.
    gnome_osd_until: u64,
    /// Uptime second when monitor labels should disappear.
    gnome_monitor_labels_until: u64,
    /// Notification system state
    pub(super) notifications: super::widgets::NotificationSystem,
    /// Alt-tab switcher state
    pub(super) alt_tab: super::widgets::AltTabSwitcher,
    /// Power dialog state
    pub(super) power_dialog: super::widgets::PowerDialog,
    /// Calendar / notification center dropdown open?
    pub(super) calendar_open: bool,
    /// Quick-settings toggle grid
    pub(super) quick_toggles: heapless::Vec<super::widgets::QuickToggle, 6>,
    /// Battery state
    pub(super) battery: super::widgets::BatteryState,
    /// Brightness level (0-100)
    pub(super) brightness: u8,
    /// Volume level (0-100)
    pub(super) volume: u8,
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
            file_manager_window: None,
            fm_path: HString::new(),
            fm_selected: 0,
            fm_scroll: 0,
            fm_entries: Vec::new(),
            system_monitor_window: None,
            monitor_lines: Vec::new(),
            last_tick_second: 0,
            context_menu: ContextMenu {
                items: Vec::new(),
                rect: Rect::new(0, 0, 0, 0),
                visible: false,
                selected: 0,
                target_window: None,
            },
            current_workspace: 0,
            text_editor_window: None,
            text_editor_lines: Vec::new(),
            te_cursor_row: 0,
            te_cursor_col: 0,
            network_status_window: None,
            net_status_lines: Vec::new(),
            snapped_side: None,
            app_grid_open: false,
            app_grid_query: HString::new(),
            app_grid_page: 0,
            gnome_osd_text: HString::new(),
            gnome_osd_until: 0,
            gnome_monitor_labels_until: 0,
            notifications: super::widgets::NotificationSystem::new(),
            alt_tab: super::widgets::AltTabSwitcher::new(),
            power_dialog: super::widgets::PowerDialog::new(),
            calendar_open: false,
            quick_toggles: super::widgets::default_toggles(true, false, true, false),
            battery: super::widgets::BatteryState::none(),
            brightness: 80,
            volume: 60,
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

    /// Create a file manager window backed by the kernel VFS.
    pub fn create_file_manager_window(
        &mut self,
        x: usize,
        y: usize,
        width: usize,
        height: usize,
    ) -> WindowId {
        let window_id = self.create_window("Files", x, y, width, height);
        self.file_manager_window = Some(window_id);
        self.fm_path.clear();
        let _ = self.fm_path.push_str("/");
        self.fm_selected = 0;
        self.fm_scroll = 0;
        self.refresh_file_manager();
        self.needs_redraw = true;
        window_id
    }

    /// Create a system monitor window with live kernel statistics.
    pub fn create_system_monitor_window(
        &mut self,
        x: usize,
        y: usize,
        width: usize,
        height: usize,
    ) -> WindowId {
        let window_id = self.create_window("System Monitor", x, y, width, height);
        self.system_monitor_window = Some(window_id);
        self.refresh_system_monitor();
        self.needs_redraw = true;
        window_id
    }

    /// Refresh dynamic desktop state (clock, stats, file listings).
    pub fn tick(&mut self) {
        let second = crate::time::uptime_ms() / 1000;
        if second == self.last_tick_second {
            return;
        }
        self.last_tick_second = second;
        self.refresh_system_monitor();
        if self.file_manager_window.is_some() {
            self.refresh_file_manager();
        }
        if self.network_status_window.is_some() {
            self.refresh_network_status();
        }
        if self.gnome_osd_until != 0 && second >= self.gnome_osd_until {
            self.gnome_osd_until = 0;
            self.gnome_osd_text.clear();
        }
        if self.gnome_monitor_labels_until != 0 && second >= self.gnome_monitor_labels_until {
            self.gnome_monitor_labels_until = 0;
        }
        self.notifications.tick();
        if self.notifications.banner_id.is_some() {
            self.needs_redraw = true;
        }
        self.needs_redraw = true;
    }

    fn refresh_file_manager(&mut self) {
        self.fm_entries.clear();
        let base = self.fm_path.as_str();
        match vfs_readdir(base) {
            Ok(entries) => {
                for entry in entries {
                    if self.fm_entries.len() >= MAX_FM_ENTRIES {
                        break;
                    }
                    let mut name = HString::new();
                    let _ = name.push_str(entry.name.as_str());
                    let is_directory = entry.inode_type == InodeType::Directory;
                    let full_path = if base == "/" {
                        format!("/{}", entry.name)
                    } else {
                        format!("{}/{}", base, entry.name)
                    };
                    let size = if is_directory {
                        0
                    } else {
                        crate::vfs::vfs_stat(&full_path)
                            .map(|stat| stat.size)
                            .unwrap_or(0)
                    };
                    let _ = self.fm_entries.push(FileManagerEntry {
                        name,
                        is_directory,
                        size,
                    });
                }
            }
            Err(_) => {
                let mut name = HString::new();
                let _ = name.push_str("(unreadable)");
                let _ = self.fm_entries.push(FileManagerEntry {
                    name,
                    is_directory: false,
                    size: 0,
                });
            }
        }

        if self.fm_selected >= self.fm_entries.len() {
            self.fm_selected = self.fm_entries.len().saturating_sub(1);
        }
    }

    fn refresh_system_monitor(&mut self) {
        unsafe {
            crate::early_serial_write_str("MON:enter\n");
        }
        self.monitor_lines.clear();

        let uptime_s = crate::time::uptime_ms() / 1000;
        let clock = Self::format_clock();

        let _ = self.push_monitor_line(&format!("RustOS x86_64 kernel"));
        let _ = self.push_monitor_line("");
        let _ = self.push_monitor_line(&format!("Clock: {}", clock));
        let _ = self.push_monitor_line(&format!("Uptime: {}s", uptime_s));

        unsafe {
            crate::early_serial_write_str("MON:mem\n");
        }
        if let Some(stats) = crate::memory::get_memory_stats() {
            let _ = self.push_monitor_line(&format!(
                "Memory: {} / {} MiB",
                stats.allocated_memory_mb(),
                stats.total_memory_mb()
            ));
            let _ = self.push_monitor_line(&format!(
                "Free: {} MiB ({}%)",
                stats.free_memory_mb(),
                stats.memory_usage_percent() as u32
            ));
        } else if let Ok(basic) = crate::memory_basic::get_memory_stats() {
            let total_mib = basic.usable_memory / (1024 * 1024);
            let _ = self.push_monitor_line(&format!("Usable RAM: {} MiB", total_mib));
        }

        unsafe {
            crate::early_serial_write_str("MON:cpu\n");
        }
        let cpu = crate::performance_monitor::cpu_utilization();
        let _ = self.push_monitor_line(&format!("CPU load est: {}%", cpu));

        unsafe {
            crate::early_serial_write_str("MON:procs\n");
        }
        let procs = crate::process::get_process_manager().list_processes().len();
        let _ = self.push_monitor_line(&format!("Processes: {}", procs));

        unsafe {
            crate::early_serial_write_str("MON:ifaces\n");
        }
        let ifaces = crate::net::network_stack().interface_count();
        let _ = self.push_monitor_line(&format!("Network: {} interface(s)", ifaces));

        unsafe {
            crate::early_serial_write_str("MON:mounts\n");
        }
        let mounts = crate::fs::vfs().list_mounts().len();
        let _ = self.push_monitor_line(&format!("Mount points: {}", mounts));
        unsafe {
            crate::early_serial_write_str("MON:done\n");
        }
    }

    fn push_monitor_line(&mut self, text: &str) -> bool {
        if self.monitor_lines.len() >= MAX_MONITOR_LINES {
            return false;
        }
        let mut line = HString::new();
        let _ = line.push_str(text);
        self.monitor_lines.push(line).is_ok()
    }

    fn format_clock() -> HString<16> {
        let ts = crate::time::system_time();
        let secs = ts % 86400;
        let hours = (secs / 3600) % 24;
        let minutes = (secs / 60) % 60;
        let mut out = HString::new();
        let _ = write!(out, "{:02}:{:02}", hours, minutes);
        out
    }

    fn network_tray_label() -> HString<16> {
        let count = crate::net::network_stack().interface_count();
        let mut out = HString::new();
        let _ = write!(out, "NET {}", count);
        out
    }

    fn fm_enter_selected(&mut self) {
        if self.fm_entries.is_empty() {
            return;
        }
        let entry = &self.fm_entries[self.fm_selected];
        if !entry.is_directory {
            return;
        }

        let name = entry.name.as_str();
        if name == ".." {
            self.fm_go_up();
            return;
        }

        if self.fm_path.as_str() == "/" {
            let _ = self.fm_path.push_str(name);
        } else {
            let _ = self.fm_path.push_str("/");
            let _ = self.fm_path.push_str(name);
        }
        self.fm_selected = 0;
        self.fm_scroll = 0;
        self.refresh_file_manager();
        self.needs_redraw = true;
    }

    fn fm_go_up(&mut self) {
        let path = self.fm_path.as_str();
        if path == "/" {
            return;
        }
        if let Some(pos) = path.rfind('/') {
            self.fm_path.truncate(pos);
            if self.fm_path.is_empty() {
                let _ = self.fm_path.push_str("/");
            }
        } else {
            self.fm_path.clear();
            let _ = self.fm_path.push_str("/");
        }
        self.fm_selected = 0;
        self.fm_scroll = 0;
        self.refresh_file_manager();
        self.needs_redraw = true;
    }

    fn handle_file_manager_key(&mut self, key: u8) -> bool {
        match key {
            38 => {
                if self.fm_selected > 0 {
                    self.fm_selected -= 1;
                    if self.fm_selected < self.fm_scroll {
                        self.fm_scroll = self.fm_selected;
                    }
                    self.needs_redraw = true;
                }
                true
            }
            40 => {
                if self.fm_selected + 1 < self.fm_entries.len() {
                    self.fm_selected += 1;
                    self.needs_redraw = true;
                }
                true
            }
            13 => {
                self.fm_enter_selected();
                true
            }
            8 => {
                self.fm_go_up();
                true
            }
            _ => false,
        }
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
                self.push_shell_line(" ls cat mem date ps mounts");
                self.push_shell_line(" ifaces storage net clear");
            }
            "uptime" => {
                let line = format!("uptime: {}s", crate::time::uptime_ms() / 1000);
                self.push_shell_line(&line);
            }
            "date" => {
                let line = format!("time: {}", Self::format_clock());
                self.push_shell_line(&line);
            }
            "mem" => {
                if let Some(stats) = crate::memory::get_memory_stats() {
                    let line = format!(
                        "mem: {} / {} MiB ({}% used)",
                        stats.allocated_memory_mb(),
                        stats.total_memory_mb(),
                        stats.memory_usage_percent() as u32
                    );
                    self.push_shell_line(&line);
                } else if let Ok(basic) = crate::memory_basic::get_memory_stats() {
                    let mib = basic.usable_memory / (1024 * 1024);
                    let line = format!("usable ram: {} MiB", mib);
                    self.push_shell_line(&line);
                } else {
                    self.push_shell_line("memory stats unavailable");
                }
            }
            "ps" => {
                let processes = crate::process::get_process_manager().list_processes();
                let line = format!("processes: {}", processes.len());
                self.push_shell_line(&line);
                for (pid, name, state, _) in processes.iter().take(6) {
                    let line = format!("  pid {} {:?} {}", pid, state, name.as_str());
                    self.push_shell_line(&line);
                }
            }
            cmd if cmd.starts_with("ls") => {
                let path = cmd.strip_prefix("ls").unwrap_or("").trim();
                let path = if path.is_empty() { "/" } else { path };
                match vfs_readdir(path) {
                    Ok(entries) => {
                        let line = format!("{}:", path);
                        self.push_shell_line(&line);
                        for entry in entries.iter().take(12) {
                            let kind = if entry.inode_type == InodeType::Directory {
                                "d"
                            } else {
                                "f"
                            };
                            let line = format!("  [{}] {}", kind, entry.name.as_str());
                            self.push_shell_line(&line);
                        }
                    }
                    Err(_) => self.push_shell_line("ls: cannot read directory"),
                }
            }
            cmd if cmd.starts_with("cat ") => {
                let path = cmd.strip_prefix("cat ").unwrap_or("").trim();
                if path.is_empty() {
                    self.push_shell_line("cat: missing path");
                    return;
                }
                match vfs_open(path, OpenFlags::RDONLY, 0) {
                    Ok(fd) => {
                        let mut buf = [0u8; 128];
                        match vfs_read(fd, &mut buf) {
                            Ok(n) if n > 0 => {
                                if let Ok(text) = core::str::from_utf8(&buf[..n]) {
                                    for line in text.lines().take(8) {
                                        self.push_shell_line(line);
                                    }
                                } else {
                                    let line = format!("{} bytes (binary)", n);
                                    self.push_shell_line(&line);
                                }
                            }
                            Ok(_) => self.push_shell_line("(empty file)"),
                            Err(_) => self.push_shell_line("cat: read error"),
                        }
                        let _ = vfs_close(fd);
                    }
                    Err(_) => self.push_shell_line("cat: not found"),
                }
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
                let interfaces = crate::net::network_stack().list_interfaces();
                let line = format!("network interfaces: {}", interfaces.len());
                self.push_shell_line(&line);
                for iface in interfaces.iter().take(3) {
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

    // ==================================================================
    // Context Menu
    // ==================================================================

    /// Show a context menu at the given position with the given items.
    fn show_context_menu(
        &mut self,
        x: usize,
        y: usize,
        items: Vec<ContextMenuItem, 16>,
        target: Option<WindowId>,
    ) {
        let font = crate::graphics::get_default_font();
        let mut menu_width = MENU_MIN_WIDTH;
        for item in &items {
            if !item.separator {
                let w = item.label.len() * font.char_width + MENU_ITEM_PADDING * 2;
                if w > menu_width {
                    menu_width = w;
                }
            }
        }
        let menu_height = items.len() * MENU_ITEM_HEIGHT;
        let mx = x.min(self.desktop_rect.width.saturating_sub(menu_width));
        let my = y.min(self.desktop_rect.height.saturating_sub(menu_height));

        self.context_menu = ContextMenu {
            items,
            rect: Rect::new(mx, my, menu_width, menu_height),
            visible: true,
            selected: 0,
            target_window: target,
        };
        self.needs_redraw = true;
    }

    /// Hide the context menu.
    fn hide_context_menu(&mut self) {
        if self.context_menu.visible {
            self.context_menu.visible = false;
            self.needs_redraw = true;
        }
    }

    /// Execute the selected context menu action.
    fn execute_menu_action(&mut self, action: MenuAction) {
        let target = self.context_menu.target_window;
        self.hide_context_menu();
        match action {
            MenuAction::None => {}
            MenuAction::CloseWindow => {
                if let Some(id) = target {
                    self.close_window(id);
                }
            }
            MenuAction::MinimizeWindow => {
                if let Some(id) = target {
                    self.minimize_window(id);
                }
            }
            MenuAction::MaximizeWindow => {
                if let Some(id) = target {
                    self.maximize_window(id);
                }
            }
            MenuAction::RestoreWindow => {
                if let Some(id) = target {
                    self.restore_window(id);
                }
            }
            MenuAction::BringToFront => {
                if let Some(id) = target {
                    self.bring_to_front(id);
                }
            }
            MenuAction::Refresh => {
                self.refresh_file_manager();
                self.refresh_system_monitor();
                self.refresh_network_status();
                self.needs_redraw = true;
            }
            MenuAction::OpenShell => {
                if let Some(id) = self.shell_window {
                    self.bring_to_front(id);
                } else {
                    self.create_shell_window(120, 80, 440, 260);
                }
            }
            MenuAction::OpenFileManager => {
                if let Some(id) = self.file_manager_window {
                    self.bring_to_front(id);
                } else {
                    self.create_file_manager_window(160, 96, 420, 300);
                }
            }
            MenuAction::OpenSystemMonitor => {
                if let Some(id) = self.system_monitor_window {
                    self.bring_to_front(id);
                } else {
                    self.create_system_monitor_window(200, 120, 360, 240);
                }
            }
            MenuAction::OpenTextEditor => {
                if let Some(id) = self.text_editor_window {
                    self.bring_to_front(id);
                } else {
                    self.create_text_editor_window(180, 100, 420, 300);
                }
            }
            MenuAction::OpenNetworkStatus => {
                if let Some(id) = self.network_status_window {
                    self.bring_to_front(id);
                } else {
                    self.create_network_status_window(200, 120, 360, 240);
                }
            }
            MenuAction::NewFolder
            | MenuAction::Delete
            | MenuAction::Rename
            | MenuAction::Properties => {
                // File operations — would need VFS write support
            }
            MenuAction::NextWorkspace => {
                self.switch_workspace((self.current_workspace + 1) % WORKSPACE_COUNT as u8);
            }
            MenuAction::PrevWorkspace => {
                self.switch_workspace(if self.current_workspace == 0 {
                    WORKSPACE_COUNT as u8 - 1
                } else {
                    self.current_workspace - 1
                });
            }
            MenuAction::GoToWorkspace(ws) => {
                self.switch_workspace(ws);
            }
        }
    }

    /// Handle a click inside the context menu. Returns true if consumed.
    fn handle_context_menu_click(&mut self, x: usize, y: usize) -> bool {
        if !self.context_menu.visible {
            return false;
        }
        if !self.context_menu.rect.contains(x, y) {
            self.hide_context_menu();
            return true;
        }
        let rel_y = y.saturating_sub(self.context_menu.rect.y);
        let item_index = rel_y / MENU_ITEM_HEIGHT;
        if item_index < self.context_menu.items.len() {
            let item = &self.context_menu.items[item_index];
            if item.enabled && !item.separator {
                let action = item.action;
                self.execute_menu_action(action);
                return true;
            }
        }
        true
    }

    /// Build the desktop right-click context menu items.
    fn desktop_context_menu_items(&self) -> Vec<ContextMenuItem, 16> {
        let mut items = Vec::new();
        let _ = items.push(ContextMenuItem {
            label: "Open Shell",
            action: MenuAction::OpenShell,
            enabled: true,
            separator: false,
        });
        let _ = items.push(ContextMenuItem {
            label: "Open Files",
            action: MenuAction::OpenFileManager,
            enabled: true,
            separator: false,
        });
        let _ = items.push(ContextMenuItem {
            label: "System Monitor",
            action: MenuAction::OpenSystemMonitor,
            enabled: true,
            separator: false,
        });
        let _ = items.push(ContextMenuItem {
            label: "Text Editor",
            action: MenuAction::OpenTextEditor,
            enabled: true,
            separator: false,
        });
        let _ = items.push(ContextMenuItem {
            label: "Network Status",
            action: MenuAction::OpenNetworkStatus,
            enabled: true,
            separator: false,
        });
        let _ = items.push(ContextMenuItem {
            label: "",
            action: MenuAction::None,
            enabled: false,
            separator: true,
        });
        let _ = items.push(ContextMenuItem {
            label: "Refresh",
            action: MenuAction::Refresh,
            enabled: true,
            separator: false,
        });
        items
    }

    /// Build the window title bar right-click context menu items.
    fn window_context_menu_items(&self) -> Vec<ContextMenuItem, 16> {
        let mut items = Vec::new();
        let _ = items.push(ContextMenuItem {
            label: "Minimize",
            action: MenuAction::MinimizeWindow,
            enabled: true,
            separator: false,
        });
        let _ = items.push(ContextMenuItem {
            label: "Maximize",
            action: MenuAction::MaximizeWindow,
            enabled: true,
            separator: false,
        });
        let _ = items.push(ContextMenuItem {
            label: "Restore",
            action: MenuAction::RestoreWindow,
            enabled: true,
            separator: false,
        });
        let _ = items.push(ContextMenuItem {
            label: "",
            action: MenuAction::None,
            enabled: false,
            separator: true,
        });
        let _ = items.push(ContextMenuItem {
            label: "Close",
            action: MenuAction::CloseWindow,
            enabled: true,
            separator: false,
        });
        items
    }

    // ==================================================================
    // Text Editor
    // ==================================================================

    /// Create a text editor window.
    pub fn create_text_editor_window(
        &mut self,
        x: usize,
        y: usize,
        width: usize,
        height: usize,
    ) -> WindowId {
        let window_id = self.create_window("Text Editor", x, y, width, height);
        self.text_editor_window = Some(window_id);
        self.text_editor_lines.clear();
        let mut line = HString::new();
        let _ = line.push_str("# RustOS Text Editor");
        let _ = self.text_editor_lines.push(line);
        let mut line = HString::new();
        let _ = line.push_str("# Type to edit, Enter for new line");
        let _ = self.text_editor_lines.push(line);
        self.te_cursor_row = 2;
        self.te_cursor_col = 0;
        self.needs_redraw = true;
        window_id
    }

    /// Handle text editor keyboard input.
    fn handle_text_editor_key(&mut self, key: u8) -> bool {
        match key {
            13 => {
                // Enter — start a new line
                if self.text_editor_lines.len() < MAX_CONTENT_LINES {
                    let mut line = HString::new();
                    let _ = line.push_str("");
                    let _ = self.text_editor_lines.push(line);
                    self.te_cursor_row = self.text_editor_lines.len() - 1;
                    self.te_cursor_col = 0;
                    self.needs_redraw = true;
                }
                true
            }
            8 => {
                // Backspace
                if self.te_cursor_col > 0 {
                    self.te_cursor_col -= 1;
                    if self.te_cursor_row < self.text_editor_lines.len() {
                        let line = &mut self.text_editor_lines[self.te_cursor_row];
                        let col = self.te_cursor_col;
                        if col < line.len() {
                            let before = &line.as_str()[..col];
                            let after = &line.as_str()[col + 1..];
                            let mut new_line = HString::new();
                            let _ = new_line.push_str(before);
                            let _ = new_line.push_str(after);
                            *line = new_line;
                        }
                    }
                } else if self.te_cursor_row > 0 {
                    // Merge with previous line
                    self.te_cursor_row -= 1;
                    if self.te_cursor_row < self.text_editor_lines.len() {
                        self.te_cursor_col = self.text_editor_lines[self.te_cursor_row].len();
                        // Remove current line and append to previous
                        // (simplified: just move cursor)
                    }
                }
                self.needs_redraw = true;
                true
            }
            38 => {
                // Up arrow
                if self.te_cursor_row > 0 {
                    self.te_cursor_row -= 1;
                    if self.te_cursor_row < self.text_editor_lines.len() {
                        self.te_cursor_col = self
                            .te_cursor_col
                            .min(self.text_editor_lines[self.te_cursor_row].len());
                    }
                    self.needs_redraw = true;
                }
                true
            }
            40 => {
                // Down arrow
                if self.te_cursor_row + 1 < self.text_editor_lines.len() {
                    self.te_cursor_row += 1;
                    self.te_cursor_col = self
                        .te_cursor_col
                        .min(self.text_editor_lines[self.te_cursor_row].len());
                    self.needs_redraw = true;
                }
                true
            }
            37 => {
                // Left arrow
                if self.te_cursor_col > 0 {
                    self.te_cursor_col -= 1;
                } else if self.te_cursor_row > 0 {
                    self.te_cursor_row -= 1;
                    if self.te_cursor_row < self.text_editor_lines.len() {
                        self.te_cursor_col = self.text_editor_lines[self.te_cursor_row].len();
                    }
                }
                self.needs_redraw = true;
                true
            }
            39 => {
                // Right arrow
                if self.te_cursor_row < self.text_editor_lines.len() {
                    if self.te_cursor_col < self.text_editor_lines[self.te_cursor_row].len() {
                        self.te_cursor_col += 1;
                    } else if self.te_cursor_row + 1 < self.text_editor_lines.len() {
                        self.te_cursor_row += 1;
                        self.te_cursor_col = 0;
                    }
                }
                self.needs_redraw = true;
                true
            }
            32..=126 => {
                // Printable character
                if self.te_cursor_row < self.text_editor_lines.len() {
                    let line = &mut self.text_editor_lines[self.te_cursor_row];
                    if line.len() < 95 {
                        let col = self.te_cursor_col.min(line.len());
                        let before = &line.as_str()[..col];
                        let after = &line.as_str()[col..];
                        let mut new_line = HString::new();
                        let _ = new_line.push_str(before);
                        let _ = new_line.push(key as char);
                        let _ = new_line.push_str(after);
                        *line = new_line;
                        self.te_cursor_col += 1;
                    }
                } else {
                    // Create new line
                    let mut new_line = HString::new();
                    let _ = new_line.push(key as char);
                    let _ = self.text_editor_lines.push(new_line);
                    self.te_cursor_row = self.text_editor_lines.len() - 1;
                    self.te_cursor_col = 1;
                }
                self.needs_redraw = true;
                true
            }
            _ => false,
        }
    }

    // ==================================================================
    // Network Status
    // ==================================================================

    /// Create a network status window.
    pub fn create_network_status_window(
        &mut self,
        x: usize,
        y: usize,
        width: usize,
        height: usize,
    ) -> WindowId {
        let window_id = self.create_window("Network Status", x, y, width, height);
        self.network_status_window = Some(window_id);
        self.refresh_network_status();
        self.needs_redraw = true;
        window_id
    }

    /// Refresh network status display data.
    fn refresh_network_status(&mut self) {
        self.net_status_lines.clear();
        let ifaces = crate::net::network_stack().list_interfaces();
        let _ = self.push_net_status_line(&format!("Network Interfaces: {}", ifaces.len()));
        let _ = self.push_net_status_line("");

        for iface in ifaces.iter().take(6) {
            let state = if iface.flags.up { "UP" } else { "DOWN" };
            let _ = self.push_net_status_line(&format!("{}: {}", iface.name, state));
            if let Some(addr) = iface.ip_addresses.first() {
                let _ = self.push_net_status_line(&format!("  addr: {}", addr));
            }
            if let crate::net::NetworkAddress::Mac(mac) = iface.mac_address {
                let _ = self.push_net_status_line(&format!(
                    "  mac: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                    mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
                ));
            }
            let _ = self.push_net_status_line("");
        }
    }

    fn push_net_status_line(&mut self, text: &str) -> bool {
        if self.net_status_lines.len() >= MAX_MONITOR_LINES {
            return false;
        }
        let mut line = HString::new();
        let _ = line.push_str(text);
        self.net_status_lines.push(line).is_ok()
    }

    // ==================================================================
    // Window Snapping
    // ==================================================================

    /// Snap a window to a screen edge.
    pub fn snap_window(&mut self, window_id: WindowId, side: SnapSide) -> bool {
        let avail_x = DOCK_HEIGHT;
        let avail_y = MENU_BAR_HEIGHT;
        let avail_w = self.desktop_rect.width.saturating_sub(DOCK_HEIGHT);
        let avail_h = self.desktop_rect.height.saturating_sub(MENU_BAR_HEIGHT);

        let (x, y, w, h) = match side {
            SnapSide::Left => (avail_x, avail_y, avail_w / 2, avail_h),
            SnapSide::Right => (avail_x + avail_w / 2, avail_y, avail_w / 2, avail_h),
            SnapSide::Top => (avail_x, avail_y, avail_w, avail_h / 2),
            SnapSide::Bottom => (avail_x, avail_y + avail_h / 2, avail_w, avail_h / 2),
        };

        if let Some(window) = self.get_window_mut(window_id) {
            window.state = WindowState::Snapped;
            window.visible = true;
            window.rect = Rect::new(x, y, max(w, MIN_WINDOW_WIDTH), max(h, MIN_WINDOW_HEIGHT));
            window.client_area = Rect::new(
                x + BORDER_WIDTH,
                y + TITLE_BAR_HEIGHT + BORDER_WIDTH,
                w.saturating_sub(2 * BORDER_WIDTH),
                h.saturating_sub(TITLE_BAR_HEIGHT + 2 * BORDER_WIDTH),
            );
            self.snapped_side = Some(side);
            self.needs_redraw = true;
            true
        } else {
            false
        }
    }

    /// Check if a dragged window should snap to an edge.
    fn check_snap_drag(&mut self, x: usize, y: usize) {
        if self.dragging_window.is_none() {
            return;
        }
        let avail_x = DOCK_HEIGHT;
        let avail_y = MENU_BAR_HEIGHT;

        if x <= avail_x + SNAP_ZONE {
            if let Some(id) = self.dragging_window {
                self.snap_window(id, SnapSide::Left);
                self.dragging_window = None;
            }
        } else if x >= self.desktop_rect.width.saturating_sub(SNAP_ZONE) {
            if let Some(id) = self.dragging_window {
                self.snap_window(id, SnapSide::Right);
                self.dragging_window = None;
            }
        } else if y <= avail_y + SNAP_ZONE {
            if let Some(id) = self.dragging_window {
                self.maximize_window(id);
                self.dragging_window = None;
            }
        }
    }

    // ==================================================================
    // Workspaces
    // ==================================================================

    /// Switch to a different workspace.
    pub fn switch_workspace(&mut self, workspace: u8) {
        if workspace == self.current_workspace {
            return;
        }
        self.current_workspace = workspace % WORKSPACE_COUNT as u8;
        for window in &mut self.windows {
            if window.state != WindowState::Closed {
                window.visible = window.workspace == self.current_workspace;
            }
        }
        self.needs_redraw = true;
    }

    /// Get the current workspace index.
    pub fn current_workspace(&self) -> u8 {
        self.current_workspace
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
        for slot in 0..DOCK_ICON_COUNT {
            let icon_rect = Rect::new(icon_x, icon_y, DOCK_ICON_SIZE, DOCK_ICON_SIZE);
            if icon_rect.contains(x, y) {
                return match slot {
                    0 => self.shell_window,
                    1 => self.file_manager_window,
                    2 => self.system_monitor_window,
                    3 => None, // Activities
                    4 => self.text_editor_window,
                    5 => self.network_status_window,
                    _ => None,
                };
            }
            icon_y += DOCK_ICON_SIZE + DOCK_ICON_GAP;
        }
        None
    }

    fn launcher_slot_at_point(&self, x: usize, y: usize) -> Option<usize> {
        if !self.dock_rect.contains(x, y) {
            return None;
        }

        let icon_x = self.dock_rect.x + (self.dock_rect.width.saturating_sub(DOCK_ICON_SIZE)) / 2;
        let mut icon_y = self.dock_rect.y + 16;
        for slot in 0..DOCK_ICON_COUNT {
            let icon_rect = Rect::new(icon_x, icon_y, DOCK_ICON_SIZE, DOCK_ICON_SIZE);
            if icon_rect.contains(x, y) {
                return Some(slot);
            }
            icon_y += DOCK_ICON_SIZE + DOCK_ICON_GAP;
        }
        None
    }

    fn launch_app_slot(&mut self, slot: usize) -> bool {
        match slot {
            0 => {
                if let Some(id) = self.shell_window {
                    self.bring_to_front(id)
                } else {
                    self.create_shell_window(120, 80, 440, 260);
                    true
                }
            }
            1 => {
                if let Some(id) = self.file_manager_window {
                    self.bring_to_front(id)
                } else {
                    self.create_file_manager_window(160, 96, 420, 300);
                    true
                }
            }
            2 => {
                if let Some(id) = self.system_monitor_window {
                    self.bring_to_front(id)
                } else {
                    self.create_system_monitor_window(200, 120, 360, 240);
                    true
                }
            }
            3 => {
                self.activities_open = !self.activities_open;
                self.quick_settings_open = false;
                self.needs_redraw = true;
                true
            }
            4 => {
                if let Some(id) = self.text_editor_window {
                    self.bring_to_front(id)
                } else {
                    self.create_text_editor_window(180, 100, 420, 300);
                    true
                }
            }
            5 => {
                if let Some(id) = self.network_status_window {
                    self.bring_to_front(id)
                } else {
                    self.create_network_status_window(200, 120, 360, 240);
                    true
                }
            }
            _ => false,
        }
    }

    fn file_manager_row_at_point(&self, x: usize, y: usize) -> Option<usize> {
        let window_id = self.file_manager_window?;
        let window = self.get_window(window_id)?;
        if !window.client_area.contains(x, y) {
            return None;
        }

        let _header_rows = 2;
        let first_row_y = window.client_area.y + 8 + FM_ROW_HEIGHT + 4 + FM_ROW_HEIGHT;
        if y < first_row_y {
            return None;
        }

        let row = (y - first_row_y) / FM_ROW_HEIGHT + self.fm_scroll;
        if row < self.fm_entries.len() {
            Some(row)
        } else {
            None
        }
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
            if self.shell_window == Some(window_id) {
                self.shell_window = None;
            }
            if self.file_manager_window == Some(window_id) {
                self.file_manager_window = None;
            }
            if self.system_monitor_window == Some(window_id) {
                self.system_monitor_window = None;
            }
            if self.text_editor_window == Some(window_id) {
                self.text_editor_window = None;
            }
            if self.network_status_window == Some(window_id) {
                self.network_status_window = None;
            }

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
        // Close context menu on any key
        if self.context_menu.visible {
            self.hide_context_menu();
        }

        // App grid is modal: it captures all keys while open.
        if self.app_grid_open {
            return self.handle_app_grid_key(key);
        }

        if self.focused_window == self.file_manager_window {
            if self.handle_file_manager_key(key) {
                return true;
            }
        }

        if self.focused_window == self.shell_window && key != 27 && key != b'\t' {
            return self.handle_shell_key(key);
        }

        if self.focused_window == self.text_editor_window && key != 27 {
            return self.handle_text_editor_key(key);
        }

        match key {
            27 => {
                if self.alt_tab.open {
                    self.alt_tab.close();
                    self.needs_redraw = true;
                    true
                } else if self.power_dialog.open {
                    self.power_dialog.close();
                    self.needs_redraw = true;
                    true
                } else if self.calendar_open {
                    self.calendar_open = false;
                    self.needs_redraw = true;
                    true
                } else if self.activities_open || self.quick_settings_open {
                    self.activities_open = false;
                    self.quick_settings_open = false;
                    self.needs_redraw = true;
                    true
                } else {
                    false
                }
            }
            b'\t' => {
                if self.alt_tab.open {
                    self.alt_tab.next();
                    self.needs_redraw = true;
                    return true;
                }

                let mut open_wins: Vec<WindowId, 64> = Vec::new();
                for w in &self.windows {
                    if w.visible && w.state != WindowState::Closed {
                        let _ = open_wins.push(w.id);
                    }
                }
                if open_wins.is_empty() {
                    return false;
                }
                self.alt_tab.open(&open_wins);
                self.needs_redraw = true;
                true
            }
            13 => {
                if self.alt_tab.open {
                    if let Some(wid) = self.alt_tab.current() {
                        self.alt_tab.close();
                        self.needs_redraw = true;
                        return self.bring_to_front(wid);
                    }
                    self.alt_tab.close();
                    self.needs_redraw = true;
                    return true;
                }
                false
            }
            b'p' | b'P' => {
                if !self.power_dialog.open {
                    self.power_dialog.open();
                    self.needs_redraw = true;
                }
                true
            }
            b'n' | b'N' => {
                self.notifications.push(
                    "System",
                    "Welcome to RustOS",
                    "Desktop environment is ready.",
                );
                self.needs_redraw = true;
                true
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
            b'g' | b'G' => {
                self.open_app_grid();
                true
            }
            b'm' | b'M' => self
                .focused_window
                .map_or(false, |window_id| self.minimize_window(window_id)),
            b'1' => {
                self.switch_workspace(0);
                true
            }
            b'2' => {
                self.switch_workspace(1);
                true
            }
            b'3' => {
                self.switch_workspace(2);
                true
            }
            b'4' => {
                self.switch_workspace(3);
                true
            }
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
    pub fn handle_mouse_down(&mut self, x: usize, y: usize, button: MouseButton) -> bool {
        // Close alt-tab on any click
        if self.alt_tab.open {
            if let Some(wid) = self.alt_tab.current() {
                self.alt_tab.close();
                self.bring_to_front(wid);
            } else {
                self.alt_tab.close();
            }
            self.needs_redraw = true;
        }

        // Handle context menu clicks first
        if self.handle_context_menu_click(x, y) {
            return true;
        }

        // App grid is modal: it absorbs all clicks while open.
        if self.app_grid_open {
            self.handle_app_grid_click(x, y);
            return true;
        }

        // Power dialog is modal
        if self.power_dialog.open {
            if let Some(action) = super::widgets::power_dialog_action_at(
                &self.power_dialog,
                self.desktop_rect.width,
                self.desktop_rect.height,
                x, y,
            ) {
                self.power_dialog.close();
                match action {
                    super::widgets::PowerAction::Shutdown => {
                        crate::serial_println!("Power: shutdown requested");
                    }
                    super::widgets::PowerAction::Restart => {
                        crate::serial_println!("Power: restart requested");
                    }
                    super::widgets::PowerAction::Logoff => {
                        crate::serial_println!("Power: logoff requested");
                    }
                    super::widgets::PowerAction::Cancel => {}
                }
                self.needs_redraw = true;
            }
            return true;
        }

        // Calendar dropdown is modal
        if self.calendar_open {
            let pw = 360;
            let px = self.desktop_rect.width.saturating_sub(pw + 8);
            let py = MENU_BAR_HEIGHT + 4;
            let panel = Rect::new(px, py, pw, 420);
            if panel.contains(x, y) {
                // Check clear button
                let clear_x = px + pw - 60;
                if Rect::new(clear_x, py + 40, 44, 18).contains(x, y) {
                    self.notifications.clear_all();
                    self.needs_redraw = true;
                }
            } else {
                self.calendar_open = false;
                self.needs_redraw = true;
            }
            return true;
        }

        // Quick settings toggle clicks
        if self.quick_settings_open {
            let panel_w = 280;
            let panel_x = self.desktop_rect.width.saturating_sub(panel_w + 8);
            let panel_y = self.menu_bar_rect.height + 8;
            if let Some(tid) = super::widgets::toggle_at_point(
                &self.quick_toggles, panel_x, panel_y, panel_w, x, y,
            ) {
                for tog in &mut self.quick_toggles {
                    if tog.id == tid {
                        tog.active = !tog.active;
                        match tid {
                            super::widgets::ToggleId::DoNotDisturb => {
                                self.notifications.do_not_disturb = tog.active;
                            }
                            _ => {}
                        }
                        break;
                    }
                }
                self.needs_redraw = true;
                return true;
            }
            // Power button in quick settings
            let pw_btn = Rect::new(panel_x + panel_w - 44, panel_y + 340, 32, 32);
            if pw_btn.contains(x, y) {
                self.power_dialog.open();
                self.quick_settings_open = false;
                self.needs_redraw = true;
                return true;
            }
        }

        // Clock click → calendar dropdown
        let font = crate::graphics::get_default_font();
        let clock = Self::format_clock();
        let clock_w = clock.len() * font.char_width;
        let clock_rect = Rect::new(
            self.menu_bar_rect.x + self.menu_bar_rect.width.saturating_sub(clock_w + 12),
            self.menu_bar_rect.y + 5,
            clock_w + 8,
            20,
        );
        if clock_rect.contains(x, y) {
            self.calendar_open = !self.calendar_open;
            self.quick_settings_open = false;
            self.activities_open = false;
            self.needs_redraw = true;
            return true;
        }

        // Right-click: show context menu
        if button == MouseButton::Right {
            // Check if right-clicking on a window title bar
            if let Some(window_id) = self.window_at_point(x, y) {
                if let Some(window) = self.get_window(window_id) {
                    let title_rect = Rect::new(
                        window.rect.x,
                        window.rect.y,
                        window.rect.width,
                        TITLE_BAR_HEIGHT,
                    );
                    if title_rect.contains(x, y) {
                        let items = self.window_context_menu_items();
                        self.show_context_menu(x, y, items, Some(window_id));
                        return true;
                    }
                }
            }

            // Desktop right-click
            if !self.menu_bar_rect.contains(x, y) && !self.dock_rect.contains(x, y) {
                let items = self.desktop_context_menu_items();
                self.show_context_menu(x, y, items, None);
                return true;
            }
            return false;
        }

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

        // Workspace switcher clicks
        let switcher_x = self.menu_bar_rect.width.saturating_sub(260);
        let switcher_y = self.menu_bar_rect.y + 7;
        for i in 0..WORKSPACE_COUNT {
            let ws_rect = Rect::new(switcher_x + i * 22, switcher_y, 16, 16);
            if ws_rect.contains(x, y) {
                self.switch_workspace(i as u8);
                return true;
            }
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

        if let Some(slot) = self.launcher_slot_at_point(x, y) {
            return self.launch_app_slot(slot);
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

                if self.file_manager_window == Some(window_id) {
                    if let Some(row) = self.file_manager_row_at_point(x, y) {
                        self.fm_selected = row;
                        self.needs_redraw = true;
                        return true;
                    }
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
    pub fn handle_mouse_up(&mut self, x: usize, y: usize, _button: MouseButton) {
        // Check if window should snap to edge
        self.check_snap_drag(x, y);
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

        if self.app_grid_open {
            self.render_app_grid();
        }

        if self.gnome_monitor_labels_until != 0 {
            self.render_monitor_labels();
        }

        if self.gnome_osd_until != 0 {
            self.render_gnome_osd();
        }

        // Widget overlays
        super::widgets::render_banner(&self.notifications, self.desktop_rect.width);

        if self.calendar_open {
            super::widgets::render_calendar_dropdown(
                &self.notifications,
                self.desktop_rect.width,
                self.calendar_open,
            );
        }

        if self.alt_tab.open {
            let mut titles: Vec<(WindowId, &str), 64> = Vec::new();
            for w in &self.windows {
                if w.visible && w.state != WindowState::Closed {
                    let _ = titles.push((w.id, w.title));
                }
            }
            super::widgets::render_alt_tab(
                &self.alt_tab,
                &titles,
                self.desktop_rect.width,
                self.desktop_rect.height,
            );
        }

        if self.power_dialog.open {
            super::widgets::render_power_dialog(
                &self.power_dialog,
                self.desktop_rect.width,
                self.desktop_rect.height,
            );
        }

        // Render context menu on top
        if self.context_menu.visible {
            self.render_context_menu();
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
            // Vertical gloss: lighter top, darker bottom — reads as a real
            // shaded title bar rather than a flat VGA-style block.
            let (title_top, title_bottom) = if window.focused {
                (colors::TITLE_ACTIVE_TOP, colors::TITLE_ACTIVE_BOTTOM)
            } else {
                (
                    Self::shade_color(window.title_bar_color, 12),
                    Self::shade_color(window.title_bar_color, -22),
                )
            };
            self.fill_vertical_gradient(title_rect, title_top, title_bottom);
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
        self.fill_vertical_gradient(
            window.client_area,
            colors::WINDOW_SURFACE_TOP,
            colors::WINDOW_SURFACE_BOTTOM,
        );
        let client_highlight = Rect::new(
            window.client_area.x,
            window.client_area.y,
            window.client_area.width,
            1,
        );
        crate::graphics::framebuffer::fill_rect(client_highlight, colors::WINDOW_BACKGROUND);

        if self.shell_window == Some(window.id) {
            self.render_shell_content(window);
            return;
        }

        if self.file_manager_window == Some(window.id) {
            self.render_file_manager_content(window);
            return;
        }

        if self.system_monitor_window == Some(window.id) {
            self.render_monitor_content(window);
            return;
        }

        if self.text_editor_window == Some(window.id) {
            self.render_text_editor_content(window);
            return;
        }

        if self.network_status_window == Some(window.id) {
            self.render_network_status_content(window);
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

        // Render scroll bar if content overflows
        let total_lines = window.content_lines.len();
        let visible_lines = window.client_area.height / line_height;
        if total_lines > visible_lines {
            let bar_x = window.client_area.x + window.client_area.width.saturating_sub(8);
            let bar_height = window.client_area.height;
            let bar_bg = Rect::new(bar_x, window.client_area.y, 4, bar_height);
            crate::graphics::framebuffer::fill_rect(bar_bg, colors::TITLE_BAR_INACTIVE);

            let thumb_height = (bar_height * visible_lines / total_lines).max(12);
            let max_scroll = total_lines.saturating_sub(visible_lines);
            let thumb_y = if max_scroll > 0 {
                window.client_area.y
                    + (bar_height.saturating_sub(thumb_height) * window.scroll_offset / max_scroll)
            } else {
                window.client_area.y
            };
            let thumb = Rect::new(bar_x, thumb_y, 4, thumb_height);
            crate::graphics::framebuffer::fill_rect(thumb, colors::BORDER_INACTIVE);
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
        self.fill_vertical_gradient(
            window.client_area,
            colors::SHELL_BACKGROUND_TOP,
            colors::SHELL_BACKGROUND_BOTTOM,
        );
        let prompt_bar = Rect::new(
            window.client_area.x,
            window.client_area.y,
            window.client_area.width,
            1,
        );
        crate::graphics::framebuffer::fill_rect(prompt_bar, colors::DOCK_HIGHLIGHT);

        let mut text_y = window.client_area.y + 8;

        for line in &self.shell_lines {
            crate::graphics::draw_text(
                line.as_str(),
                window.client_area.x + 8,
                text_y,
                colors::SHELL_TEXT,
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
            colors::SHELL_PROMPT,
            font,
        );
    }

    fn render_file_manager_content(&self, window: &Window) {
        let font = crate::graphics::get_default_font();
        let line_height = FM_ROW_HEIGHT;
        let mut text_y = window.client_area.y + 8;
        let header = Rect::new(
            window.client_area.x,
            window.client_area.y,
            window.client_area.width,
            line_height + 8,
        );
        self.fill_vertical_gradient(
            header,
            colors::WINDOW_SURFACE_TOP,
            colors::WINDOW_SURFACE_ALT,
        );

        let path_line = format!("Path: {}", self.fm_path.as_str());
        crate::graphics::draw_text(
            &path_line,
            window.client_area.x + 8,
            text_y,
            colors::TEXT_COLOR,
            font,
        );
        text_y += line_height + 4;

        crate::graphics::draw_text(
            "Name              Type    Size",
            window.client_area.x + 8,
            text_y,
            colors::TEXT_COLOR_MUTED,
            font,
        );
        text_y += line_height;

        for (index, entry) in self.fm_entries.iter().enumerate().skip(self.fm_scroll) {
            if text_y + font.char_height > window.client_area.y + window.client_area.height {
                break;
            }

            let kind = if entry.is_directory { "dir" } else { "file" };
            let size_str = if entry.is_directory {
                format!("<DIR>")
            } else if entry.size < 1024 {
                format!("{} B", entry.size)
            } else {
                format!("{} KiB", entry.size / 1024)
            };
            let line = format!("{:<16}  {:<4}  {}", entry.name.as_str(), kind, size_str);

            let fg = if index == self.fm_selected {
                colors::TEXT_COLOR_WHITE
            } else {
                colors::TEXT_COLOR
            };
            let bg = if index == self.fm_selected {
                colors::FM_SELECTION
            } else if index % 2 == 0 {
                colors::WINDOW_SURFACE_TOP
            } else {
                colors::WINDOW_SURFACE_ALT
            };

            let row_rect = Rect::new(
                window.client_area.x + 4,
                text_y.saturating_sub(2),
                window.client_area.width.saturating_sub(8),
                line_height,
            );
            crate::graphics::framebuffer::fill_rect(row_rect, bg);
            crate::graphics::draw_text(&line, window.client_area.x + 8, text_y, fg, font);
            text_y += line_height;
        }
    }

    fn render_monitor_content(&self, window: &Window) {
        let font = crate::graphics::get_default_font();
        let line_height = font.char_height + 2;
        let mut text_y = window.client_area.y + 8;

        for line in &self.monitor_lines {
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
    }

    fn render_text_editor_content(&self, window: &Window) {
        let font = crate::graphics::get_default_font();
        let line_height = font.char_height + 2;
        let mut text_y = window.client_area.y + 8;

        for (i, line) in self.text_editor_lines.iter().enumerate() {
            let is_cursor_line = i == self.te_cursor_row;
            let fg = if is_cursor_line {
                colors::TEXT_COLOR
            } else {
                colors::TEXT_COLOR_MUTED
            };
            crate::graphics::draw_text(line.as_str(), window.client_area.x + 8, text_y, fg, font);

            // Draw cursor indicator
            if is_cursor_line {
                let cursor_x = window.client_area.x + 8 + self.te_cursor_col * font.char_width;
                let cursor_rect = Rect::new(cursor_x, text_y, 2, font.char_height);
                crate::graphics::framebuffer::fill_rect(cursor_rect, colors::BORDER_ACTIVE);
            }

            text_y += line_height;
            if text_y + font.char_height > window.client_area.y + window.client_area.height {
                return;
            }
        }
    }

    fn render_network_status_content(&self, window: &Window) {
        let font = crate::graphics::get_default_font();
        let line_height = font.char_height + 2;
        let mut text_y = window.client_area.y + 8;

        for line in &self.net_status_lines {
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

        self.fill_vertical_gradient(
            button.rect,
            Self::shade_color(bg_color, 18),
            Self::shade_color(bg_color, -10),
        );
        crate::graphics::framebuffer::draw_rect(button.rect, colors::BORDER_INACTIVE, 1);
    }

    /// Render the context menu
    fn render_context_menu(&self) {
        let menu = &self.context_menu;
        if !menu.visible || menu.items.is_empty() {
            return;
        }

        // Shadow
        let shadow = Rect::new(
            menu.rect.x + 2,
            menu.rect.y + 2,
            menu.rect.width,
            menu.rect.height,
        );
        crate::graphics::framebuffer::fill_rect(shadow, colors::WINDOW_SHADOW);

        // Background
        crate::graphics::framebuffer::fill_rect(menu.rect, colors::WINDOW_BACKGROUND);
        crate::graphics::framebuffer::draw_rect(menu.rect, colors::BORDER_ACTIVE, 1);

        let font = crate::graphics::get_default_font();
        let text_y_offset = (MENU_ITEM_HEIGHT.saturating_sub(font.char_height)) / 2;

        for (i, item) in menu.items.iter().enumerate() {
            let item_rect = Rect::new(
                menu.rect.x,
                menu.rect.y + i * MENU_ITEM_HEIGHT,
                menu.rect.width,
                MENU_ITEM_HEIGHT,
            );

            if item.separator {
                let sep = Rect::new(
                    item_rect.x + 4,
                    item_rect.y + MENU_ITEM_HEIGHT / 2,
                    item_rect.width.saturating_sub(8),
                    1,
                );
                crate::graphics::framebuffer::fill_rect(sep, colors::BORDER_INACTIVE);
                continue;
            }

            // Hover/selected highlight
            if i == menu.selected {
                crate::graphics::framebuffer::fill_rect(item_rect, colors::FM_SELECTION);
            }

            let text_color = if item.enabled {
                colors::TEXT_COLOR
            } else {
                colors::TEXT_COLOR_MUTED
            };

            crate::graphics::draw_text(
                item.label,
                item_rect.x + MENU_ITEM_PADDING,
                item_rect.y + text_y_offset,
                text_color,
                font,
            );
        }
    }

    /// Render cursor — arrow shape with outline
    fn render_cursor(&self) {
        let cx = self.cursor.x;
        let cy = self.cursor.y;
        let w = self.desktop_rect.width;
        let h = self.desktop_rect.height;

        // Arrow cursor shape (11x16):
        //  X
        //  XX
        //  X X
        //  X  X
        //  X   X
        //  X    X
        //  X     X
        //  X      X
        //  X       X
        //  X    XXXXX
        //  X    X
        //  X   X
        //  X  X
        //  X X
        //  XX
        //  X
        let outline = Color::rgb(0, 0, 0);
        let fill = Color::rgb(255, 255, 255);

        // Arrow outline (draw slightly larger for border effect)
        let arrow_outline: [(isize, isize); 18] = [
            (0, 0),
            (0, 1),
            (0, 2),
            (0, 3),
            (0, 4),
            (0, 5),
            (0, 6),
            (0, 7),
            (0, 8),
            (0, 9),
            (0, 10),
            (0, 11),
            (0, 12),
            (0, 13),
            (0, 14),
            (1, 14),
            (2, 13),
            (3, 12),
        ];

        // Draw outline first (offset by 1 in each direction)
        for &(dx, dy) in &arrow_outline {
            for &(ox, oy) in &[(0isize, 0isize), (-1, 0), (1, 0), (0, -1), (0, 1)] {
                let px = cx as isize + dx + ox;
                let py = cy as isize + dy + oy;
                if px >= 0 && py >= 0 && (px as usize) < w && (py as usize) < h {
                    crate::graphics::framebuffer::set_pixel(px as usize, py as usize, outline);
                }
            }
        }

        // Fill the arrow body
        for dy in 0..15isize {
            for dx in 0..=dy.min(8) {
                let px = cx as isize + dx;
                let py = cy as isize + dy;
                if px >= 0 && py >= 0 && (px as usize) < w && (py as usize) < h {
                    // Skip the tail area (below the arrowhead)
                    if dy <= 8 || dx <= 3 {
                        crate::graphics::framebuffer::set_pixel(px as usize, py as usize, fill);
                    }
                }
            }
        }

        // Draw the arrowhead base line (horizontal bar at row 9)
        for dx in 4..=9 {
            let px = cx as isize + dx;
            let py = cy as isize + 9;
            if px >= 0 && py >= 0 && (px as usize) < w && (py as usize) < h {
                crate::graphics::framebuffer::set_pixel(px as usize, py as usize, fill);
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
        let width = max(self.desktop_rect.width, 1);
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

        // Ubuntu Jammy-style wallpaper accents: soft orange orb + aubergine glow.
        let orb_cx = self.desktop_rect.x + width * 3 / 4;
        let orb_cy = self.desktop_rect.y + height * 4 / 5;
        for (radius, color) in [
            (140usize, Color::rgb(180, 50, 20)),
            (110, Color::rgb(210, 70, 28)),
            (82, Color::rgb(233, 84, 32)),
            (58, Color::rgb(245, 120, 60)),
        ] {
            crate::graphics::primitives::fill_circle(orb_cx, orb_cy, radius, color);
        }
        let glow_cx = self.desktop_rect.x + width / 5;
        let glow_cy = self.desktop_rect.y + height / 3;
        for (radius, color) in [
            (120usize, Color::rgb(60, 10, 45)),
            (90, Color::rgb(94, 39, 80)),
            (62, Color::rgb(119, 41, 83)),
        ] {
            crate::graphics::primitives::fill_circle(glow_cx, glow_cy, radius, color);
        }
    }

    fn render_desktop_icons(&self) {
        // Ubuntu keeps the desktop clean — wallpaper only, no center watermark.
    }

    /// Draw text scaled by an integer factor (1x, 2x, etc.)
    fn draw_text_scaled(
        text: &str,
        x: usize,
        y: usize,
        color: Color,
        font: &crate::graphics::BitmapFont,
        scale: usize,
    ) {
        let mut cx = x;
        for ch in text.chars() {
            let char_code = ch as usize;
            if char_code >= 256 {
                cx += font.char_width * scale;
                continue;
            }
            let offset = char_code.saturating_mul(font.char_height);
            if offset >= font.data.len() {
                cx += font.char_width * scale;
                continue;
            }
            for row in 0..font.char_height {
                let byte = font.data[offset + row];
                for col in 0..font.char_width {
                    if (byte >> (7 - col)) & 1 == 1 {
                        let px = cx + col * scale;
                        let py = y + row * scale;
                        let pixel = Rect::new(px, py, scale, scale);
                        crate::graphics::framebuffer::fill_rect(pixel, color);
                    }
                }
            }
            cx += font.char_width * scale;
        }
    }

    /// Render a GNOME subsystem readiness panel in the bottom-right corner.
    fn render_gnome_status_panel(&self) {
        let font = crate::graphics::get_default_font();
        let panel_w = 220;
        let panel_h = 148;
        let panel_x = self.desktop_rect.width.saturating_sub(panel_w + 16);
        let panel_y = self
            .desktop_rect
            .height
            .saturating_sub(panel_h + DOCK_HEIGHT + 16);
        let panel = Rect::new(panel_x, panel_y, panel_w, panel_h);

        // Semi-transparent dark background
        self.fill_vertical_gradient(panel, Color::rgb(20, 24, 32), Color::rgb(12, 16, 24));
        crate::graphics::framebuffer::draw_rect(panel, Color::rgb(54, 132, 245), 1);

        let mut ty = panel_y + 10;
        crate::graphics::draw_text(
            "GNOME Status",
            panel_x + 12,
            ty,
            Color::rgb(244, 247, 252),
            font,
        );
        ty += font.char_height + 6;

        // Separator line
        let sep = Rect::new(panel_x + 12, ty, panel_w - 24, 1);
        crate::graphics::framebuffer::fill_rect(sep, Color::rgb(54, 132, 245));
        ty += 6;

        // Subsystem status rows
        let subsystems: [(&str, bool); 5] = [
            ("D-Bus", crate::dbus::is_ready()),
            ("Wayland", crate::wayland::is_ready()),
            ("Mutter", crate::mutter::is_ready()),
            ("GNOME Overlay", crate::gnome_overlay::is_ready()),
            ("DRM/KMS", crate::vfs::drmfs::smoke_check().is_ok()),
        ];

        for (name, ready) in subsystems {
            // Status dot
            let dot_color = if ready {
                Color::rgb(46, 204, 113)
            } else {
                Color::rgb(231, 76, 60)
            };
            crate::graphics::primitives::fill_circle(panel_x + 18, ty + 5, 4, dot_color);

            // Label
            crate::graphics::draw_text(
                name,
                panel_x + 30,
                ty,
                if ready {
                    Color::rgb(200, 220, 240)
                } else {
                    Color::rgb(140, 150, 170)
                },
                font,
            );

            // Status text
            let status = if ready { "Ready" } else { "Blocked" };
            let st_x = panel_x + panel_w.saturating_sub((status.len() + 2) * font.char_width);
            crate::graphics::draw_text(
                status,
                st_x,
                ty,
                if ready {
                    Color::rgb(46, 204, 113)
                } else {
                    Color::rgb(231, 76, 60)
                },
                font,
            );

            ty += font.char_height + 4;
        }
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

        // Workspace thumbnails strip (GNOME-style, right side)
        let ws_thumb_w = 120;
        let ws_thumb_h = 68;
        let ws_strip_x = overlay.x + overlay.width.saturating_sub(ws_thumb_w + 18);
        let ws_strip_y = overlay.y + 42;
        crate::graphics::draw_text(
            "Workspaces",
            ws_strip_x,
            ws_strip_y,
            colors::MENU_BAR_ICON,
            font,
        );
        for i in 0..WORKSPACE_COUNT {
            let ty = ws_strip_y + 20 + i * (ws_thumb_h + 8);
            if ty + ws_thumb_h > overlay.y + overlay.height {
                break;
            }
            let thumb = Rect::new(ws_strip_x, ty, ws_thumb_w, ws_thumb_h);
            let is_current = i == self.current_workspace as usize;
            let bg = if is_current {
                Color::rgb(50, 50, 60)
            } else {
                Color::rgb(36, 36, 42)
            };
            crate::graphics::framebuffer::fill_rect(thumb, bg);
            let border = if is_current {
                colors::DOCK_ICON_ACCENT
            } else {
                Color::rgb(60, 60, 68)
            };
            crate::graphics::framebuffer::draw_rect(thumb, border, if is_current { 2 } else { 1 });

            // Mini window indicators inside thumbnail
            let win_count = self
                .windows
                .iter()
                .filter(|w| w.state != WindowState::Closed && w.workspace == i as u8)
                .count();
            if win_count > 0 {
                let mini_w = 20;
                let mini_h = 12;
                let mini_x = thumb.x + 6;
                let mini_y = thumb.y + 6;
                for j in 0..win_count.min(4) {
                    let mx = mini_x + j * (mini_w + 2);
                    crate::graphics::framebuffer::fill_rect(
                        Rect::new(mx, mini_y, mini_w, mini_h),
                        Color::rgb(80, 80, 100),
                    );
                    crate::graphics::framebuffer::draw_rect(
                        Rect::new(mx, mini_y, mini_w, mini_h),
                        Color::rgb(100, 100, 120),
                        1,
                    );
                }
            }

            let label = format!("WS {}", i + 1);
            crate::graphics::draw_text(
                &label,
                thumb.x + 6,
                thumb.y + ws_thumb_h.saturating_sub(font.char_height + 4),
                if is_current { colors::TEXT_COLOR_WHITE } else { colors::MENU_BAR_ICON },
                font,
            );
        }

        // Window previews (left area, with mini title bars)
        let win_area_w = overlay.width.saturating_sub(ws_thumb_w + 48);
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
            self.fill_vertical_gradient(
                tile,
                Self::shade_color(tile_color, 18),
                Self::shade_color(tile_color, -20),
            );
            crate::graphics::framebuffer::draw_rect(tile, colors::BORDER_INACTIVE, 1);

            // Mini title bar inside preview
            let mini_title = Rect::new(tile.x, tile.y, tile.width, 16);
            self.fill_vertical_gradient(
                mini_title,
                if window.focused { colors::TITLE_ACTIVE_TOP } else { colors::TITLE_INACTIVE_TOP },
                if window.focused { colors::TITLE_ACTIVE_BOTTOM } else { colors::TITLE_INACTIVE_BOTTOM },
            );
            crate::graphics::draw_text(
                window.title,
                tile.x + 6,
                tile.y + 3,
                colors::TEXT_COLOR_WHITE,
                font,
            );

            // Mini window content placeholder
            let content = Rect::new(tile.x + 2, tile.y + 18, tile.width - 4, tile.height - 20);
            crate::graphics::framebuffer::fill_rect(content, Color::rgb(50, 50, 56));

            // Draw a few lines to simulate content
            for line_i in 0..3 {
                let line_y = content.y + 6 + line_i * 14;
                if line_y + 8 > content.y + content.height {
                    break;
                }
                let line_w = (content.width - 12) * (3 - line_i) / 3;
                crate::graphics::framebuffer::fill_rect(
                    Rect::new(content.x + 6, line_y, line_w, 6),
                    Color::rgb(80, 80, 90),
                );
            }

            x += 196;
            if x + 180 > overlay.x + win_area_w {
                x = overlay.x + 18;
                y += 112;
            }
            if y + 96 > overlay.y + overlay.height {
                break;
            }
        }

        let app_y = overlay.y + overlay.height.saturating_sub(56);
        let open_count = self
            .windows
            .iter()
            .filter(|window| window.state != WindowState::Closed)
            .count();
        let apps_line = format!("{} open windows  |  WS {} of {}", open_count, self.current_workspace + 1, WORKSPACE_COUNT);
        crate::graphics::draw_text(
            &apps_line,
            overlay.x + 18,
            app_y,
            colors::MENU_BAR_ICON,
            font,
        );
    }

    pub fn gnome_focus_search(&mut self) -> bool {
        self.open_app_grid();
        true
    }

    pub fn gnome_show_applications(&mut self) -> bool {
        self.open_app_grid();
        true
    }

    pub fn gnome_hide_overview(&mut self) -> bool {
        self.activities_open = false;
        self.app_grid_open = false;
        self.needs_redraw = true;
        true
    }

    pub fn gnome_toggle_overview(&mut self) -> bool {
        self.activities_open = !self.activities_open;
        self.quick_settings_open = false;
        if self.activities_open {
            self.app_grid_open = false;
        }
        self.needs_redraw = true;
        true
    }

    pub fn gnome_show_osd(&mut self, text: &str) -> bool {
        self.gnome_osd_text.clear();
        let label = if text.is_empty() { "OSD" } else { text };
        let _ = self.gnome_osd_text.push_str(label);
        self.gnome_osd_until = crate::time::uptime_ms() / 1000 + 3;
        self.needs_redraw = true;
        true
    }

    pub fn gnome_show_monitor_labels(&mut self) -> bool {
        self.gnome_monitor_labels_until = crate::time::uptime_ms() / 1000 + 3;
        self.needs_redraw = true;
        true
    }

    pub fn gnome_hide_monitor_labels(&mut self) -> bool {
        self.gnome_monitor_labels_until = 0;
        self.needs_redraw = true;
        true
    }

    fn open_app_grid(&mut self) {
        self.app_grid_open = true;
        self.app_grid_query.clear();
        self.app_grid_page = 0;
        self.needs_redraw = true;
    }

    fn handle_app_grid_key(&mut self, key: u8) -> bool {
        match key {
            27 => {
                self.app_grid_open = false;
                self.needs_redraw = true;
                true
            }
            13 => {
                let apps = app_grid::filter(self.app_grid_query.as_str());
                let page_items = app_grid::page_slice(&apps, self.app_grid_page);
                if let Some(app) = page_items.first() {
                    let slot = app.slot;
                    self.app_grid_open = false;
                    self.needs_redraw = true;
                    self.launch_app_slot(slot);
                }
                true
            }
            8 => {
                self.app_grid_query.pop();
                self.app_grid_page = 0;
                self.needs_redraw = true;
                true
            }
            b'\t' => {
                let apps = app_grid::filter(self.app_grid_query.as_str());
                let pages = app_grid::page_count(apps.len());
                self.app_grid_page = (self.app_grid_page + 1) % pages;
                self.needs_redraw = true;
                true
            }
            c if c.is_ascii_graphic() => {
                let _ = self.app_grid_query.push(c as char);
                self.app_grid_page = 0;
                self.needs_redraw = true;
                true
            }
            _ => false,
        }
    }

    fn handle_app_grid_click(&mut self, x: usize, y: usize) {
        let overlay = self.app_grid_rect();
        if !overlay.contains(x, y) {
            self.app_grid_open = false;
            self.needs_redraw = true;
            return;
        }

        let apps = app_grid::filter(self.app_grid_query.as_str());
        let page_items = app_grid::page_slice(&apps, self.app_grid_page);
        let font = crate::graphics::get_default_font();
        let row_height = font.char_height + 12;
        let list_y = overlay.y + 48;

        if y < list_y {
            return;
        }

        let row = (y - list_y) / row_height;
        if row < page_items.len() {
            let slot = page_items[row].slot;
            self.app_grid_open = false;
            self.needs_redraw = true;
            self.launch_app_slot(slot);
        }
    }

    fn render_app_grid(&self) {
        let overlay = self.app_grid_rect();
        crate::graphics::framebuffer::fill_rect(overlay, colors::WINDOW_BACKGROUND);
        crate::graphics::framebuffer::draw_rect(overlay, colors::BORDER_ACTIVE, 2);

        let font = crate::graphics::get_default_font();
        let title = "Applications";
        crate::graphics::draw_text(
            title,
            overlay.x + 12,
            overlay.y + 8,
            colors::TEXT_COLOR_WHITE,
            font,
        );

        let query_label = format!("Search: {}", self.app_grid_query.as_str());
        crate::graphics::draw_text(
            &query_label,
            overlay.x + 12,
            overlay.y + 28,
            colors::TEXT_COLOR_WHITE,
            font,
        );

        let apps = app_grid::filter(self.app_grid_query.as_str());
        let page_items = app_grid::page_slice(&apps, self.app_grid_page);
        let row_height = font.char_height + 12;
        let mut y = overlay.y + 48;
        for app in page_items {
            let label = format!("{} {}", app.icon, app.name);
            crate::graphics::draw_text(&label, overlay.x + 16, y, colors::TEXT_COLOR_WHITE, font);
            y += row_height;
        }

        let pages = app_grid::page_count(apps.len());
        if pages > 1 {
            let footer = format!("Page {}/{} (Tab to cycle)", self.app_grid_page + 1, pages);
            crate::graphics::draw_text(
                &footer,
                overlay.x + 12,
                overlay.y + overlay.height.saturating_sub(20),
                colors::TEXT_COLOR_WHITE,
                font,
            );
        }
    }

    fn app_grid_rect(&self) -> Rect {
        let w = 320.min(self.desktop_rect.width - 40);
        let h = 280.min(self.desktop_rect.height - 40);
        let x = (self.desktop_rect.width - w) / 2;
        let y = (self.desktop_rect.height - h) / 2;
        Rect::new(x, y, w, h)
    }

    fn render_gnome_osd(&self) {
        let font = crate::graphics::get_default_font();
        let text = self.gnome_osd_text.as_str();
        let text_w = text.len().saturating_mul(font.char_width);
        let w = (text_w + 44).clamp(180, self.desktop_rect.width.saturating_sub(40));
        let h = 58;
        let x = (self.desktop_rect.width.saturating_sub(w)) / 2;
        let y = self
            .desktop_rect
            .height
            .saturating_sub(h + DOCK_ICON_GAP + 24);
        let panel = Rect::new(x, y, w, h);
        self.fill_vertical_gradient(panel, colors::DOCK_GLASS, colors::MENU_BAR_BACKGROUND);
        crate::graphics::framebuffer::draw_rect(panel, colors::DOCK_ICON_ACCENT, 2);
        crate::graphics::draw_text(
            text,
            panel.x + 22,
            panel.y + (panel.height.saturating_sub(font.char_height)) / 2,
            colors::TEXT_COLOR_WHITE,
            font,
        );
    }

    fn render_monitor_labels(&self) {
        let font = crate::graphics::get_default_font();
        let w = 180;
        let h = 74;
        let x = (self.desktop_rect.width.saturating_sub(w)) / 2;
        let y = self.menu_bar_rect.height + 36;
        let panel = Rect::new(x, y, w, h);
        let dims = format!("{}x{}", self.desktop_rect.width, self.desktop_rect.height);
        self.fill_vertical_gradient(panel, colors::TITLE_ACTIVE_TOP, colors::TITLE_ACTIVE_BOTTOM);
        crate::graphics::framebuffer::draw_rect(panel, colors::TEXT_COLOR_WHITE, 2);
        crate::graphics::draw_text(
            "Monitor 1",
            panel.x + 26,
            panel.y + 18,
            colors::TEXT_COLOR_WHITE,
            font,
        );
        crate::graphics::draw_text(
            &dims,
            panel.x + 26,
            panel.y + 40,
            colors::MENU_BAR_ICON,
            font,
        );
    }

    fn render_quick_settings(&self) {
        let font = crate::graphics::get_default_font();
        let panel_w = 280;
        let panel_x = self.desktop_rect.width.saturating_sub(panel_w + 8);
        let panel_y = self.menu_bar_rect.height + 8;
        let panel = Rect::new(panel_x, panel_y, panel_w, 380);

        // Shadow
        crate::graphics::framebuffer::fill_rect(
            Rect::new(panel_x + 4, panel_y + 4, panel_w, 380),
            Color::new(0, 0, 0, 100),
        );

        // Background
        self.fill_vertical_gradient(panel, Color::rgb(36, 36, 40), Color::rgb(28, 28, 32));
        crate::graphics::framebuffer::draw_rect(panel, Color::rgb(55, 55, 62), 1);

        // Toggle grid (GNOME 43+ style)
        let toggles_h = super::widgets::render_quick_toggles(
            &self.quick_toggles,
            panel_x, panel_y, panel_w,
        );

        // Sliders section
        let slider_y = panel_y + toggles_h + 16;
        super::widgets::render_slider(
            panel_x + 16, slider_y, panel_w - 32,
            "Brightness", self.brightness, 100,
        );
        super::widgets::render_slider(
            panel_x + 16, slider_y + 36, panel_w - 32,
            "Volume", self.volume, 100,
        );

        // Battery indicator
        let bat_y = slider_y + 80;
        super::widgets::render_battery_indicator(&self.battery, panel_x + 16, bat_y);

        // System stats section
        let stats_y = bat_y + 28;
        let uptime = format!("Uptime: {}s", crate::time::uptime_ms() / 1000);
        crate::graphics::draw_text(
            &uptime,
            panel_x + 16, stats_y,
            colors::MENU_BAR_ICON, font,
        );
        let windows = format!(
            "Windows: {}",
            self.windows.iter().filter(|w| w.state != WindowState::Closed).count()
        );
        crate::graphics::draw_text(
            &windows,
            panel_x + 16, stats_y + 18,
            colors::MENU_BAR_ICON, font,
        );
        let net_line = format!(
            "Net: {} ifaces",
            crate::net::network_stack().interface_count()
        );
        crate::graphics::draw_text(
            &net_line,
            panel_x + 16, stats_y + 36,
            colors::MENU_BAR_ICON, font,
        );
        if let Some(stats) = crate::memory::get_memory_stats() {
            let mem_line = format!(
                "Mem: {}/{} MiB",
                stats.allocated_memory_mb(),
                stats.total_memory_mb()
            );
            crate::graphics::draw_text(
                &mem_line,
                panel_x + 16, stats_y + 54,
                colors::MENU_BAR_ICON, font,
            );
        }

        // Power button at bottom
        let pw_btn = Rect::new(panel_x + panel_w - 44, panel_y + 340, 32, 32);
        self.draw_circle(pw_btn.x + 16, pw_btn.y + 16, 14, Color::rgb(60, 60, 68));
        self.draw_circle(pw_btn.x + 16, pw_btn.y + 16, 12, Color::rgb(200, 60, 50));
        crate::graphics::draw_text("P", pw_btn.x + 10, pw_btn.y + 9, colors::TEXT_COLOR_WHITE, font);
    }

    fn render_menu_bar(&self) {
        self.fill_vertical_gradient(
            self.menu_bar_rect,
            colors::MENU_BAR_TOP,
            colors::MENU_BAR_BOTTOM,
        );

        let bottom = Rect::new(
            self.menu_bar_rect.x,
            self.menu_bar_rect.y + self.menu_bar_rect.height.saturating_sub(1),
            self.menu_bar_rect.width,
            1,
        );
        crate::graphics::framebuffer::fill_rect(bottom, colors::MENU_BAR_ACCENT);

        let app_button = Rect::new(self.menu_bar_rect.x + 10, self.menu_bar_rect.y + 5, 72, 20);
        self.fill_vertical_gradient(
            app_button,
            Self::shade_color(colors::MENU_BAR_HIGHLIGHT, 18),
            colors::MENU_BAR_HIGHLIGHT,
        );
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
        let mut task_x = self.menu_bar_rect.x + 120;
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
            let (task_top, task_bottom) = if window.focused {
                (colors::TITLE_ACTIVE_TOP, colors::TITLE_ACTIVE_BOTTOM)
            } else {
                (
                    Self::shade_color(colors::MENU_BAR_HIGHLIGHT, 12),
                    colors::MENU_BAR_HIGHLIGHT,
                )
            };
            self.fill_vertical_gradient(task_rect, task_top, task_bottom);
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

        // Notification badge (between system tray and clock)
        let unread = self.notifications.unread_count();
        if unread > 0 {
            let badge_str = if unread < 10 {
                format!("({})", unread)
            } else {
                format!("(9+)")
            };
            let badge_w = badge_str.len() * font.char_width;
            let clock_w = Self::format_clock().len() * font.char_width;
            let badge_x = self.menu_bar_rect.x
                + self.menu_bar_rect.width.saturating_sub(clock_w + badge_w + 20);
            crate::graphics::draw_text(
                &badge_str,
                badge_x,
                text_y,
                colors::DOCK_ICON_ACCENT,
                font,
            );
        }

        let clock = Self::format_clock();
        let right_text = format!("{}", clock.as_str());
        let right_width = right_text.len() * font.char_width;
        let right_x =
            self.menu_bar_rect.x + self.menu_bar_rect.width.saturating_sub(right_width + 12);
        crate::graphics::draw_text(&right_text, right_x, text_y, colors::MENU_BAR_ICON, font);
    }

    fn render_dock(&self) {
        self.fill_vertical_gradient(self.dock_rect, colors::DOCK_TOP, colors::DOCK_BOTTOM);

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

        for i in 0..WORKSPACE_COUNT {
            let rect = Rect::new(switcher_x + i * 22, switcher_y, 16, 16);
            let color = if i == self.current_workspace as usize {
                colors::DOCK_ICON_ACCENT
            } else {
                colors::MENU_BAR_HIGHLIGHT
            };
            self.fill_vertical_gradient(rect, Self::shade_color(color, 12), color);
            crate::graphics::framebuffer::draw_rect(rect, colors::BORDER_INACTIVE, 1);
        }
    }

    fn render_system_tray(&self, text_y: usize) {
        let font = crate::graphics::get_default_font();
        let label = Self::network_tray_label();
        let tray_w = label.len() * font.char_width + 16;
        let tray_x = self.menu_bar_rect.width.saturating_sub(tray_w + 90);
        let tray_rect = Rect::new(
            tray_x.saturating_sub(8),
            self.menu_bar_rect.y + 5,
            tray_w,
            20,
        );
        self.fill_vertical_gradient(
            tray_rect,
            Self::shade_color(colors::MENU_BAR_ACCENT, 22),
            colors::MENU_BAR_ACCENT,
        );
        crate::graphics::framebuffer::draw_rect(tray_rect, colors::BORDER_INACTIVE, 1);
        crate::graphics::draw_text(
            label.as_str(),
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
            Color::rgb(119, 41, 83),
            Color::rgb(53, 132, 228),
            Color::rgb(46, 204, 113),
            Color::rgb(241, 196, 15),
            Color::rgb(155, 89, 182),
        ];
        let icon_labels = ["Tm", "Fi", "Mo", "St", "Tx", "Nw"];

        for i in 0..DOCK_ICON_COUNT {
            if icon_y + DOCK_ICON_SIZE > launcher_rect.y + launcher_rect.height {
                break;
            }

            let window_for_slot = match i {
                0 => self.shell_window,
                1 => self.file_manager_window,
                2 => self.system_monitor_window,
                4 => self.text_editor_window,
                5 => self.network_status_window,
                _ => None,
            };
            let icon_rect = Rect::new(icon_x, icon_y, DOCK_ICON_SIZE, DOCK_ICON_SIZE);
            let border = if self
                .focused_window
                .is_some_and(|id| window_for_slot == Some(id))
            {
                colors::DOCK_ICON_ACCENT
            } else {
                colors::DOCK_HIGHLIGHT
            };
            self.fill_vertical_gradient(
                icon_rect,
                Self::shade_color(colors::DOCK_GLASS, 22),
                colors::DOCK_GLASS,
            );
            crate::graphics::framebuffer::draw_rect(icon_rect, border, 1);

            let inner = Rect::new(
                icon_x + 7,
                icon_y + 7,
                DOCK_ICON_SIZE - 14,
                DOCK_ICON_SIZE - 14,
            );
            let icon_color = icon_colors[i % icon_colors.len()];
            self.fill_vertical_gradient(
                inner,
                Self::shade_color(icon_color, 24),
                Self::shade_color(icon_color, -14),
            );

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
                colors::WINDOW_BTN_MIN,
            ),
            (
                Rect::new(max_x, button_y, 22, button_h),
                colors::WINDOW_BTN_MAX,
            ),
            (
                Rect::new(close_x, button_y, 22, button_h),
                colors::WINDOW_BTN_CLOSE,
            ),
        ];

        for (rect, color) in buttons.iter() {
            let center_x = rect.x + rect.width / 2;
            let center_y = rect.y + rect.height / 2;
            self.draw_circle(center_x, center_y, 7, Self::shade_color(*color, -20));
            self.draw_circle(center_x, center_y, 6, *color);
            self.draw_circle(
                center_x.saturating_sub(2),
                center_y.saturating_sub(2),
                2,
                Self::shade_color(*color, 34),
            );
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
        let soft_shadow = Rect::new(
            window.rect.x.saturating_sub(WINDOW_SHADOW_MARGIN / 2),
            window.rect.y.saturating_sub(WINDOW_SHADOW_MARGIN / 2),
            window.rect.width + WINDOW_SHADOW_MARGIN,
            window.rect.height + WINDOW_SHADOW_MARGIN,
        );
        crate::graphics::framebuffer::fill_rect(
            soft_shadow,
            Self::shade_color(colors::WINDOW_SHADOW, 16),
        );
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

    /// Top-to-bottom gradient — gives flat chrome (panels, title bars, dock)
    /// the shaded depth of a real true-color desktop instead of a VGA flat fill.
    fn fill_vertical_gradient(&self, rect: Rect, top: Color, bottom: Color) {
        if rect.height == 0 {
            return;
        }
        for row in 0..rect.height {
            let color = Self::lerp_color(top, bottom, row, rect.height - 1);
            let line = Rect::new(rect.x, rect.y + row, rect.width, 1);
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
