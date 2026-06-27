//! Simple Desktop Environment for RustOS
//!
//! A functional text-mode desktop with windows, taskbar, and interactive elements

use crate::print;
use crate::vga_buffer::{Color, VGA_WRITER};
use alloc::format;
use heapless::String;
use lazy_static::lazy_static;
use spin::Mutex;

const SCREEN_WIDTH: usize = 80;
const SCREEN_HEIGHT: usize = 25;
const TASKBAR_HEIGHT: usize = 1;
const DESKTOP_HEIGHT: usize = SCREEN_HEIGHT - TASKBAR_HEIGHT;

/// Desktop state
pub struct Desktop {
    current_time: usize,
    active_window: Option<usize>,
    windows: [Option<Window>; 5],
    menu_open: bool,
}

/// Window structure
#[derive(Clone)]
pub struct Window {
    id: usize,
    title: String<32>, // Fixed-size string for no_std
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    content: WindowContent,
    minimized: bool,
}

/// Window content types with interactive state
#[derive(Clone)]
pub enum WindowContent {
    Terminal(TerminalState),
    FileManager(FileManagerState),
    Calculator(CalculatorState),
    TextEditor(TextEditorState),
    SystemInfo(SystemInfoState),
}

/// Terminal state for interactive shell
#[derive(Clone)]
pub struct TerminalState {
    current_directory: String<64>,
    command_history: heapless::Vec<String<128>, 16>,
    current_command: String<128>,
    output_lines: heapless::Vec<String<128>, 10>,
    cursor_pos: usize,
}

/// File manager state for directory browsing  
#[derive(Clone)]
pub struct FileManagerState {
    current_path: String<128>,
    selected_file: usize,
    files: heapless::Vec<FileEntry, 16>,
    view_mode: FileViewMode,
}

/// Calculator state for mathematical operations
#[derive(Clone)]
pub struct CalculatorState {
    display: String<32>,
    current_operation: Option<char>,
    previous_value: f64,
    current_value: f64,
}

/// Text editor state for file editing
#[derive(Clone)]
pub struct TextEditorState {
    filename: String<64>,
    content: heapless::Vec<String<128>, 20>,
    cursor_line: usize,
    cursor_col: usize,
    modified: bool,
}

/// System info state for real-time monitoring
#[derive(Clone)]
pub struct SystemInfoState {
    refresh_counter: u32,
    cpu_usage: u8,
    memory_usage: u64,
    uptime: u64,
}

/// File entry for file manager
#[derive(Clone)]
pub struct FileEntry {
    name: String<64>,
    is_directory: bool,
    size: u64,
    permissions: String<16>,
}

/// File manager view modes
#[derive(Clone)]
pub enum FileViewMode {
    List,
    Icons,
    Details,
}

impl TerminalState {
    pub fn new() -> Self {
        let mut state = Self {
            current_directory: String::new(),
            command_history: heapless::Vec::new(),
            current_command: String::new(),
            output_lines: heapless::Vec::new(),
            cursor_pos: 0,
        };
        let _ = state.current_directory.push_str("/");
        let _ = state
            .output_lines
            .push("RustOS fallback console".try_into().unwrap_or_default());
        state
    }
}

impl FileManagerState {
    pub fn new() -> Self {
        let mut state = Self {
            current_path: String::new(),
            selected_file: 0,
            files: heapless::Vec::new(),
            view_mode: FileViewMode::List,
        };
        let _ = state.current_path.push_str("/");
        state.refresh();
        state
    }

    pub fn refresh(&mut self) {
        self.files.clear();
        match crate::vfs::vfs_readdir(self.current_path.as_str()) {
            Ok(entries) => {
                for entry in entries {
                    if self.files.len() >= 16 {
                        break;
                    }
                    let mut name = String::new();
                    let _ = name.push_str(entry.name.as_str());
                    let is_directory = entry.inode_type == crate::vfs::InodeType::Directory;
                    let size = 0u64;
                    let mut perms = String::new();
                    let _ = perms.push_str(if is_directory { "drwx" } else { "-rw-" });
                    let _ = self.files.push(FileEntry {
                        name,
                        is_directory,
                        size,
                        permissions: perms,
                    });
                }
            }
            Err(_) => {}
        }
        if self.selected_file >= self.files.len() {
            self.selected_file = self.files.len().saturating_sub(1);
        }
    }
}

impl CalculatorState {
    pub fn new() -> Self {
        Self {
            display: "0".try_into().unwrap_or_default(),
            current_operation: None,
            previous_value: 0.0,
            current_value: 0.0,
        }
    }
}

impl TextEditorState {
    pub fn new() -> Self {
        let mut state = Self {
            filename: "untitled.txt".try_into().unwrap_or_default(),
            content: heapless::Vec::new(),
            cursor_line: 0,
            cursor_col: 0,
            modified: false,
        };
        let _ = state.content.push("".try_into().unwrap_or_default()); // Start with one empty line
        state
    }
}

impl SystemInfoState {
    pub fn new() -> Self {
        Self {
            refresh_counter: 0,
            cpu_usage: 0,
            memory_usage: 0,
            uptime: 0,
        }
    }

    pub fn update(&mut self) {
        self.refresh_counter += 1;

        if let Ok(basic) = crate::memory_basic::get_memory_stats() {
            self.memory_usage = basic.usable_memory as u64;
        } else if let Some(stats) = crate::memory::get_memory_stats() {
            self.memory_usage = stats.total_memory as u64;
        }

        self.cpu_usage = crate::performance_monitor::cpu_utilization();
        self.uptime = crate::time::uptime_ms() / 1000;
    }
}

impl Desktop {
    pub fn new() -> Self {
        crate::serial_println!("Desktop::new: creating minimal struct");
        // Don't initialize windows array here - it's too large for stack
        // Use MaybeUninit pattern to avoid stack overflow
        Self {
            current_time: 0,
            active_window: None,
            windows: Default::default(), // Uses Default impl for array of Option
            menu_open: false,
        }
    }

    /// Initialize and show the desktop
    pub fn init(&mut self) {
        crate::serial_println!("Desktop::init: clear_screen");
        self.clear_screen();
        crate::serial_println!("Desktop::init: draw_wallpaper");
        self.draw_wallpaper();
        crate::serial_println!("Desktop::init: draw_taskbar");
        self.draw_taskbar();
        crate::serial_println!("Desktop::init: create_default_windows");
        self.create_default_windows();
        crate::serial_println!("Desktop::init: refresh_display");
        self.refresh_display();
        crate::serial_println!("Desktop::init: done");
    }

    /// Clear the entire screen
    fn clear_screen(&self) {
        let mut writer = VGA_WRITER.lock();
        writer.clear_screen();
    }

    /// Draw desktop wallpaper/background
    fn draw_wallpaper(&self) {
        self.set_cursor(0, 0);
        self.set_color(Color::Blue, Color::Black);

        // Draw a simple pattern background using while loops (for loops crash in nightly)
        let mut y: usize = 0;
        while y < DESKTOP_HEIGHT {
            let mut x: usize = 0;
            while x < SCREEN_WIDTH {
                self.set_cursor(x, y);
                if (x + y) % 4 == 0 {
                    print!(".");
                } else {
                    print!(" ");
                }
                x = x.wrapping_add(1);
            }
            y = y.wrapping_add(1);
        }
    }

    /// Draw the taskbar at the bottom
    fn draw_taskbar(&self) {
        let y = SCREEN_HEIGHT - 1;

        // Taskbar background
        self.set_color(Color::White, Color::DarkGray);
        self.set_cursor(0, y);
        let mut i: usize = 0;
        while i < SCREEN_WIDTH {
            print!(" ");
            i = i.wrapping_add(1);
        }

        // Start menu button
        self.set_cursor(0, y);
        self.set_color(Color::Black, Color::LightGray);
        print!(" RustOS ");

        // Window buttons
        let mut x = 8;
        let mut wi: usize = 0;
        while wi < self.windows.len() {
            if let Some(ref win) = self.windows[wi] {
                if !win.minimized {
                    self.set_cursor(x, y);
                    if Some(wi) == self.active_window {
                        self.set_color(Color::Yellow, Color::Blue);
                    } else {
                        self.set_color(Color::Black, Color::LightGray);
                    }
                    print!(" {} ", win.title);
                    x += win.title.len() + 2;
                }
            }
            wi = wi.wrapping_add(1);
        }

        // Clock
        self.set_cursor(SCREEN_WIDTH - 8, y);
        self.set_color(Color::Black, Color::LightGray);
        print!(
            " {:02}:{:02} ",
            (self.current_time / 60) % 24,
            self.current_time % 60
        );
    }

    /// Create some default windows with interactive content
    fn create_default_windows(&mut self) {
        // Terminal window with interactive shell
        self.windows[0] = Some(Window {
            id: 0,
            title: String::from("Terminal"),
            x: 2,
            y: 2,
            width: 35,
            height: 12,
            content: WindowContent::Terminal(TerminalState::new()),
            minimized: false,
        });

        // File Manager with filesystem integration
        self.windows[1] = Some(Window {
            id: 1,
            title: String::from("Files"),
            x: 40,
            y: 3,
            width: 35,
            height: 15,
            content: WindowContent::FileManager(FileManagerState::new()),
            minimized: false,
        });

        // System Info with real-time monitoring (minimized by default)
        self.windows[2] = Some(Window {
            id: 2,
            title: String::from("System Monitor"),
            x: 10,
            y: 8,
            width: 30,
            height: 10,
            content: WindowContent::SystemInfo(SystemInfoState::new()),
            minimized: true,
        });

        // Calculator application
        self.windows[3] = Some(Window {
            id: 3,
            title: String::from("Calculator"),
            x: 45,
            y: 19,
            width: 25,
            height: 8,
            content: WindowContent::Calculator(CalculatorState::new()),
            minimized: true,
        });

        // Text Editor
        self.windows[4] = Some(Window {
            id: 4,
            title: String::from("Text Editor"),
            x: 5,
            y: 15,
            width: 30,
            height: 8,
            content: WindowContent::TextEditor(TextEditorState::new()),
            minimized: true,
        });

        self.active_window = Some(0);
    }

    /// Draw all windows
    fn draw_windows(&self) {
        let mut i: usize = 0;
        while i < self.windows.len() {
            if let Some(ref win) = self.windows[i] {
                if !win.minimized {
                    self.draw_window(win, Some(i) == self.active_window);
                }
            }
            i = i.wrapping_add(1);
        }
    }

    /// Draw a single window
    fn draw_window(&self, window: &Window, is_active: bool) {
        let title_color = if is_active {
            Color::Yellow
        } else {
            Color::White
        };
        let border_color = if is_active {
            Color::LightBlue
        } else {
            Color::LightGray
        };

        // Draw window border
        self.set_color(Color::Black, border_color);

        // Top border with title
        self.set_cursor(window.x, window.y);
        print!("+");
        let mut j: usize = 0;
        while j < window.width.saturating_sub(2) {
            print!("-");
            j = j.wrapping_add(1);
        }
        print!("+");

        // Title bar
        self.set_cursor(window.x + 1, window.y);
        self.set_color(title_color, border_color);
        print!(" {} ", window.title);

        // Close button
        self.set_cursor(window.x + window.width - 3, window.y);
        self.set_color(Color::Red, border_color);
        print!("x");

        // Side borders and content area
        let mut row: usize = 1;
        while row < window.height.saturating_sub(1) {
            self.set_cursor(window.x, window.y + row);
            self.set_color(Color::Black, border_color);
            print!("|");

            // Content area
            self.set_cursor(window.x + 1, window.y + row);
            self.set_color(Color::White, Color::Black);
            let mut k: usize = 0;
            while k < window.width.saturating_sub(2) {
                print!(" ");
                k = k.wrapping_add(1);
            }

            self.set_cursor(window.x + window.width - 1, window.y + row);
            self.set_color(Color::Black, border_color);
            print!("|");
            row = row.wrapping_add(1);
        }

        // Bottom border
        self.set_cursor(window.x, window.y + window.height - 1);
        self.set_color(Color::Black, border_color);
        print!("+");
        let mut m: usize = 0;
        while m < window.width.saturating_sub(2) {
            print!("-");
            m = m.wrapping_add(1);
        }
        print!("+");

        // Draw window content
        self.draw_window_content(window);
    }

    /// Draw content inside a window with interactive state
    fn draw_window_content(&self, window: &Window) {
        match &window.content {
            WindowContent::Terminal(state) => {
                self.draw_terminal_content(window, state);
            }
            WindowContent::FileManager(state) => {
                self.draw_file_manager_content(window, state);
            }
            WindowContent::Calculator(state) => {
                self.draw_calculator_content(window, state);
            }
            WindowContent::TextEditor(state) => {
                self.draw_text_editor_content(window, state);
            }
            WindowContent::SystemInfo(state) => {
                self.draw_system_info_content(window, state);
            }
        }
    }

    /// Draw interactive terminal content
    fn draw_terminal_content(&self, window: &Window, state: &TerminalState) {
        // Terminal header
        self.set_cursor(window.x + 2, window.y + 2);
        self.set_color(Color::LightGreen, Color::Black);
        print!("RustOS Terminal v1.0");

        // Current directory
        self.set_cursor(window.x + 2, window.y + 3);
        self.set_color(Color::LightBlue, Color::Black);
        print!("{}$", state.current_directory.as_str());

        // Command output
        let mut line = 4;
        for output in &state.output_lines {
            if line >= window.y + window.height - 2 {
                break;
            }
            self.set_cursor(window.x + 2, line);
            self.set_color(Color::White, Color::Black);
            print!("{}", output.as_str());
            line += 1;
        }

        // Current command input
        if line < window.y + window.height - 1 {
            self.set_cursor(window.x + 2, line);
            self.set_color(Color::LightGreen, Color::Black);
            print!("$ ");
            self.set_color(Color::White, Color::Black);
            print!("{}_", state.current_command.as_str());
        }
    }

    /// Draw file manager content with real filesystem integration
    fn draw_file_manager_content(&self, window: &Window, state: &FileManagerState) {
        // File manager header
        self.set_cursor(window.x + 2, window.y + 2);
        self.set_color(Color::LightCyan, Color::Black);
        print!("📁 {}", state.current_path.as_str());

        // Column headers
        self.set_cursor(window.x + 2, window.y + 3);
        self.set_color(Color::Yellow, Color::Black);
        print!("Name          Size   Perms");

        // File list
        let mut line = 4;
        let mut fi: usize = 0;
        while fi < state.files.len() {
            if line >= window.y + window.height - 1 {
                break;
            }
            let file = &state.files[fi];

            self.set_cursor(window.x + 2, line);

            // Highlight selected file
            if fi == state.selected_file {
                self.set_color(Color::Black, Color::White);
            } else {
                self.set_color(Color::White, Color::Black);
            }

            // File icon and name
            let icon = if file.is_directory { "[D]" } else { "[F]" };
            print!("{} {:<8}", icon, file.name.as_str());

            // File size
            if file.is_directory {
                print!(" <DIR> ");
            } else if file.size < 1024 {
                print!("    {}B", file.size);
            } else if file.size < 1024 * 1024 {
                print!("   {}K", file.size / 1024);
            } else {
                print!("   {}M", file.size / (1024 * 1024));
            }

            // Permissions (shortened)
            let perms = file.permissions.as_str();
            if perms.len() >= 6 {
                print!(" {}", &perms[..6]);
            }

            line += 1;
            fi = fi.wrapping_add(1);
        }
    }

    /// Draw calculator content
    fn draw_calculator_content(&self, window: &Window, state: &CalculatorState) {
        // Calculator display
        self.set_cursor(window.x + 2, window.y + 2);
        self.set_color(Color::Black, Color::LightGray);
        print!("{:>20}", state.display.as_str());

        // Calculator buttons layout
        let buttons = [
            ["C", "+/-", "%", "÷"],
            ["7", "8", "9", "×"],
            ["4", "5", "6", "-"],
            ["1", "2", "3", "+"],
            ["0", ".", "=", "="],
        ];

        let mut row: usize = 0;
        while row < buttons.len() {
            let mut col: usize = 0;
            while col < buttons[row].len() {
                self.set_cursor(window.x + 2 + col * 5, window.y + 4 + row);
                self.set_color(Color::Black, Color::LightGray);
                print!("[{}]", buttons[row][col]);
                col = col.wrapping_add(1);
            }
            row = row.wrapping_add(1);
        }
    }

    /// Draw text editor content
    fn draw_text_editor_content(&self, window: &Window, state: &TextEditorState) {
        // Editor header with filename
        self.set_cursor(window.x + 2, window.y + 2);
        self.set_color(Color::LightCyan, Color::Black);
        let modified_marker = if state.modified { "*" } else { "" };
        print!("📝 {}{}", state.filename.as_str(), modified_marker);

        // Line numbers and content
        let mut line = 3;
        let mut i: usize = 0;
        while i < state.content.len() {
            if line >= window.y + window.height - 1 {
                break;
            }
            let content_line = &state.content[i];

            self.set_cursor(window.x + 2, line);

            // Line number
            self.set_color(Color::DarkGray, Color::Black);
            print!("{}: ", i + 1);

            // Content with cursor
            self.set_color(Color::White, Color::Black);
            if i == state.cursor_line {
                // Show cursor position
                let content = content_line.as_str();
                if state.cursor_col < content.len() {
                    print!("{}", &content[..state.cursor_col]);
                    self.set_color(Color::Black, Color::White);
                    print!("{}", content.chars().nth(state.cursor_col).unwrap_or(' '));
                    self.set_color(Color::White, Color::Black);
                    if state.cursor_col + 1 < content.len() {
                        print!("{}", &content[state.cursor_col + 1..]);
                    }
                } else {
                    print!("{}_", content);
                }
            } else {
                print!("{}", content_line.as_str());
            }

            line += 1;
            i = i.wrapping_add(1);
        }
    }

    /// Draw system information with real-time data
    fn draw_system_info_content(&self, window: &Window, state: &SystemInfoState) {
        // System info header
        self.set_cursor(window.x + 2, window.y + 2);
        self.set_color(Color::LightCyan, Color::Black);
        print!("System Monitor");

        // System information
        self.set_cursor(window.x + 2, window.y + 4);
        self.set_color(Color::White, Color::Black);
        print!("OS: RustOS v1.0");

        self.set_cursor(window.x + 2, window.y + 5);
        print!("Arch: x86_64");

        self.set_cursor(window.x + 2, window.y + 6);
        print!("Arch: x86_64");

        self.set_cursor(window.x + 2, window.y + 7);
        if state.memory_usage > 0 {
            let mib = state.memory_usage / (1024 * 1024);
            print!("RAM: {} MiB usable", mib);
        } else {
            print!("RAM: unavailable");
        }

        self.set_cursor(window.x + 2, window.y + 8);
        print!("CPU est: {}%", state.cpu_usage);

        self.set_cursor(window.x + 2, window.y + 9);
        print!("Uptime: {}s", state.uptime);
    }

    /// Show start menu
    fn draw_start_menu(&self) {
        let menu_x = 1;
        let menu_y = DESKTOP_HEIGHT - 8;
        let menu_width: usize = 20;
        let menu_height: usize = 7;

        // Menu background
        self.set_color(Color::Black, Color::LightGray);
        let mut mr: usize = 0;
        while mr < menu_height {
            self.set_cursor(menu_x, menu_y + mr);
            let mut mc: usize = 0;
            while mc < menu_width {
                print!(" ");
                mc = mc.wrapping_add(1);
            }
            mr = mr.wrapping_add(1);
        }

        // Menu border
        self.set_color(Color::Black, Color::White);
        self.set_cursor(menu_x, menu_y);
        print!("+");
        let mut tb: usize = 0;
        while tb < menu_width.saturating_sub(2) {
            print!("-");
            tb = tb.wrapping_add(1);
        }
        print!("+");

        // Menu items
        self.set_cursor(menu_x + 1, menu_y + 1);
        self.set_color(Color::Black, Color::LightGray);
        print!(" [S] System Info");

        self.set_cursor(menu_x + 1, menu_y + 2);
        print!(" [T] Terminal");

        self.set_cursor(menu_x + 1, menu_y + 3);
        print!(" [F] File Manager");

        self.set_cursor(menu_x + 1, menu_y + 4);
        print!(" [C] Calculator");

        self.set_cursor(menu_x + 1, menu_y + 5);
        print!(" [Q] Shutdown");

        // Bottom border
        self.set_cursor(menu_x, menu_y + menu_height - 1);
        self.set_color(Color::Black, Color::White);
        print!("+");
        let mut bb: usize = 0;
        while bb < menu_width.saturating_sub(2) {
            print!("-");
            bb = bb.wrapping_add(1);
        }
        print!("+");
    }

    /// Handle keyboard input
    pub fn handle_key(&mut self, key: u8) {
        match key {
            b'm' => {
                // Toggle start menu
                self.menu_open = !self.menu_open;
                self.refresh_display();
            }
            b'1'..=b'5' => {
                // Switch to window
                let window_id = (key - b'1') as usize;
                if self.windows[window_id].is_some() {
                    self.active_window = Some(window_id);
                    if let Some(ref mut window) = self.windows[window_id] {
                        window.minimized = false;
                    }
                    self.refresh_display();
                }
            }
            b'h' => {
                // Hide/minimize active window
                if let Some(active_id) = self.active_window {
                    if let Some(ref mut window) = self.windows[active_id] {
                        window.minimized = true;
                    }
                    self.active_window = None;
                    self.refresh_display();
                }
            }
            b'n' => {
                // Create new terminal window
                let mut ni: usize = 0;
                while ni < self.windows.len() {
                    if self.windows[ni].is_none() {
                        // Create a simple title without format macro
                        let mut title = String::new();
                        let _ = title.push_str("Terminal ");
                        // Simple number conversion for terminal numbering
                        let num_char = match ni + 1 {
                            1 => '1',
                            2 => '2',
                            3 => '3',
                            4 => '4',
                            5 => '5',
                            _ => '?',
                        };
                        let _ = title.push(num_char);

                        self.windows[ni] = Some(Window {
                            id: ni,
                            title,
                            x: 5 + ni * 3,
                            y: 3 + ni * 2,
                            width: 30,
                            height: 10,
                            content: WindowContent::Terminal(TerminalState::new()),
                            minimized: false,
                        });
                        self.active_window = Some(ni);
                        self.refresh_display();
                        break;
                    }
                    ni = ni.wrapping_add(1);
                }
            }
            _ => {}
        }
    }

    /// Update desktop (called periodically)
    /// Update desktop state and applications
    pub fn update(&mut self) {
        let uptime_s = crate::time::uptime_ms() / 1000;
        let secs = crate::time::system_time() % 86400;
        self.current_time = ((secs / 3600) % 24) as usize * 60 + ((secs / 60) % 60) as usize;

        if uptime_s % 2 == 0 {
            self.update_system_info();
        }

        if uptime_s % 5 == 0 {
            self.draw_taskbar();
        }

        if uptime_s % 10 == 0 {
            self.refresh_display();
        }
    }

    /// Update system information in all system info windows
    fn update_system_info(&mut self) {
        for window in &mut self.windows {
            if let Some(ref mut win) = window {
                if let WindowContent::SystemInfo(ref mut state) = win.content {
                    state.update();
                }
                if let WindowContent::FileManager(ref mut state) = win.content {
                    state.refresh();
                }
            }
        }
    }

    /// Refresh the entire display
    pub fn refresh_display(&self) {
        self.clear_screen();
        self.draw_wallpaper();
        self.draw_windows();
        self.draw_taskbar();

        if self.menu_open {
            self.draw_start_menu();
        }

        // Show help
        self.show_help();
    }

    /// Show help information
    fn show_help(&self) {
        self.set_cursor(2, 0);
        self.set_color(Color::Yellow, Color::Blue);
        print!(" RustOS Desktop - Keys: M=Menu, 1-5=Windows, H=Hide, N=New Terminal ");
    }

    /// Helper functions
    fn set_cursor(&self, x: usize, y: usize) {
        let mut writer = VGA_WRITER.lock();
        writer.set_cursor_position(y, x);
    }

    fn set_color(&self, foreground: Color, background: Color) {
        let mut writer = VGA_WRITER.lock();
        writer.set_color(foreground, background);
    }
}

// Global desktop instance
lazy_static! {
    static ref DESKTOP: Mutex<Option<Desktop>> = Mutex::new(None);
}

/// Initialize the desktop
pub fn init_desktop() {
    crate::serial_println!("simple_desktop: init_desktop start");

    let mut desktop_lock = DESKTOP.lock();
    if desktop_lock.is_none() {
        let mut desktop = Desktop::new();
        desktop.init();
        *desktop_lock = Some(desktop);
    }

    crate::serial_println!("simple_desktop: init_desktop done");
}

/// Draw a classic 32-bit style desktop UI
fn draw_simple_desktop() {
    crate::serial_println!("simple_desktop: draw_simple_desktop start");

    // Clear screen with classic teal/cyan desktop color
    {
        let mut writer = VGA_WRITER.lock();
        writer.set_color(Color::White, Color::Cyan);
        let mut y: usize = 0;
        while y < DESKTOP_HEIGHT {
            let mut x: usize = 0;
            while x < SCREEN_WIDTH {
                writer.set_cursor_position(y, x);
                writer.write_byte(b' ');
                x = x.wrapping_add(1);
            }
            y = y.wrapping_add(1);
        }
    }

    // Draw desktop icons
    draw_desktop_icons();

    crate::serial_println!("simple_desktop: desktop drawn");

    // Draw classic 3D taskbar
    draw_32bit_taskbar();

    crate::serial_println!("simple_desktop: taskbar drawn");

    // Draw windows
    draw_32bit_window(5, 2, 35, 10, "My Computer", true);
    draw_32bit_window(42, 4, 32, 12, "Welcome", false);
    draw_program_manager(2, 13, 28, 8);

    crate::serial_println!("simple_desktop: draw_simple_desktop done");
}

/// Draw desktop icons in classic style
fn draw_desktop_icons() {
    let mut writer = VGA_WRITER.lock();

    // Console icon
    writer.set_cursor_position(1, 2);
    writer.set_color(Color::Black, Color::Cyan);
    writer.write_string("[SH]");
    writer.set_cursor_position(2, 1);
    writer.write_string("Shell");

    // System status
    writer.set_cursor_position(4, 2);
    writer.write_string("[OK]");
    writer.set_cursor_position(5, 1);
    writer.write_string("Status");

    // Network
    writer.set_cursor_position(7, 2);
    writer.write_string("[NT]");
    writer.set_cursor_position(8, 1);
    writer.write_string("Net n/a");
}

/// Draw classic 32-bit 3D taskbar
fn draw_32bit_taskbar() {
    let mut writer = VGA_WRITER.lock();
    let taskbar_y = SCREEN_HEIGHT - 1;

    // Taskbar background - raised 3D effect
    writer.set_color(Color::Black, Color::LightGray);
    let mut x: usize = 0;
    while x < SCREEN_WIDTH {
        writer.set_cursor_position(taskbar_y, x);
        writer.write_byte(b' ');
        x = x.wrapping_add(1);
    }

    // Start button with 3D raised look
    writer.set_cursor_position(taskbar_y, 0);
    writer.set_color(Color::White, Color::LightGray);
    writer.write_byte(b'[');
    writer.set_color(Color::Black, Color::LightGray);
    writer.write_string("Start");
    writer.set_color(Color::DarkGray, Color::LightGray);
    writer.write_byte(b']');

    // Quick launch separator
    writer.set_cursor_position(taskbar_y, 8);
    writer.set_color(Color::DarkGray, Color::LightGray);
    writer.write_byte(b'|');

    // Running programs in taskbar
    writer.set_cursor_position(taskbar_y, 10);
    writer.set_color(Color::Black, Color::White);
    writer.write_string(" My Computer ");

    writer.set_cursor_position(taskbar_y, 24);
    writer.set_color(Color::Black, Color::LightGray);
    writer.write_string(" Welcome ");

    // System tray area (sunken)
    writer.set_cursor_position(taskbar_y, SCREEN_WIDTH - 18);
    writer.set_color(Color::DarkGray, Color::LightGray);
    writer.write_byte(b'[');
    writer.set_color(Color::Black, Color::LightGray);
    writer.write_string(" Vol  Net 12:00");
    writer.set_color(Color::DarkGray, Color::LightGray);
    writer.write_byte(b']');
}

/// Draw a classic 32-bit style window with 3D borders
fn draw_32bit_window(x: usize, y: usize, w: usize, h: usize, title: &str, active: bool) {
    let mut writer = VGA_WRITER.lock();

    // Window background (gray)
    writer.set_color(Color::Black, Color::LightGray);
    let mut wy: usize = 0;
    while wy < h {
        let mut wx: usize = 0;
        while wx < w {
            writer.set_cursor_position(y + wy, x + wx);
            writer.write_byte(b' ');
            wx = wx.wrapping_add(1);
        }
        wy = wy.wrapping_add(1);
    }

    // 3D Border - outer highlight (top-left = white, bottom-right = dark)
    // Top edge (white/highlight)
    writer.set_color(Color::White, Color::LightGray);
    writer.set_cursor_position(y, x);
    let mut i: usize = 0;
    while i < w {
        writer.write_byte(b'_');
        i = i.wrapping_add(1);
    }

    // Left edge (white/highlight)
    let mut ly: usize = 1;
    while ly < h {
        writer.set_cursor_position(y + ly, x);
        writer.write_byte(b'|');
        ly = ly.wrapping_add(1);
    }

    // Bottom edge (dark/shadow)
    writer.set_color(Color::DarkGray, Color::LightGray);
    writer.set_cursor_position(y + h - 1, x);
    let mut bi: usize = 0;
    while bi < w {
        writer.write_byte(b'_');
        bi = bi.wrapping_add(1);
    }

    // Right edge (dark/shadow)
    let mut ry: usize = 1;
    while ry < h {
        writer.set_cursor_position(y + ry, x + w - 1);
        writer.write_byte(b'|');
        ry = ry.wrapping_add(1);
    }

    // Title bar
    let title_bg = if active { Color::Blue } else { Color::DarkGray };
    writer.set_color(Color::White, title_bg);
    writer.set_cursor_position(y + 1, x + 1);
    let mut ti: usize = 0;
    while ti < w - 2 {
        writer.write_byte(b' ');
        ti = ti.wrapping_add(1);
    }

    // Window title
    writer.set_cursor_position(y + 1, x + 2);
    writer.write_string(title);

    // Window control buttons (minimize, maximize, close)
    let btn_x = x + w - 10;
    writer.set_cursor_position(y + 1, btn_x);
    writer.set_color(Color::Black, Color::LightGray);
    writer.write_string("[_]");
    writer.set_cursor_position(y + 1, btn_x + 3);
    writer.write_string("[O]");
    writer.set_cursor_position(y + 1, btn_x + 6);
    writer.set_color(Color::White, Color::Red);
    writer.write_string("[X]");

    // Menu bar
    writer.set_color(Color::Black, Color::LightGray);
    writer.set_cursor_position(y + 2, x + 1);
    writer.write_string(" File  Edit  View  Help ");
}

/// Draw Program Manager window (classic 32-bit style)
fn draw_program_manager(x: usize, y: usize, w: usize, h: usize) {
    let mut writer = VGA_WRITER.lock();

    // Window background
    writer.set_color(Color::Black, Color::LightGray);
    let mut wy: usize = 0;
    while wy < h {
        let mut wx: usize = 0;
        while wx < w {
            writer.set_cursor_position(y + wy, x + wx);
            writer.write_byte(b' ');
            wx = wx.wrapping_add(1);
        }
        wy = wy.wrapping_add(1);
    }

    // 3D borders
    writer.set_color(Color::White, Color::LightGray);
    writer.set_cursor_position(y, x);
    let mut i: usize = 0;
    while i < w {
        writer.write_byte(b'_');
        i = i.wrapping_add(1);
    }

    writer.set_color(Color::DarkGray, Color::LightGray);
    writer.set_cursor_position(y + h - 1, x);
    let mut bi: usize = 0;
    while bi < w {
        writer.write_byte(b'_');
        bi = bi.wrapping_add(1);
    }

    // Title bar (inactive - gray)
    writer.set_color(Color::White, Color::DarkGray);
    writer.set_cursor_position(y + 1, x + 1);
    let mut ti: usize = 0;
    while ti < w - 2 {
        writer.write_byte(b' ');
        ti = ti.wrapping_add(1);
    }
    writer.set_cursor_position(y + 1, x + 2);
    writer.write_string("Programs");

    // Close button
    writer.set_cursor_position(y + 1, x + w - 4);
    writer.set_color(Color::Black, Color::LightGray);
    writer.write_string("[X]");

    // Program icons inside
    writer.set_color(Color::Black, Color::LightGray);
    writer.set_cursor_position(y + 3, x + 2);
    writer.write_string("[A] Accessories");
    writer.set_cursor_position(y + 4, x + 2);
    writer.write_string("[G] Games");
    writer.set_cursor_position(y + 5, x + 2);
    writer.write_string("[S] System");
}

/// Draw a simple welcome window
fn draw_welcome_window() {
    crate::serial_println!("draw_welcome_window: start");
    let win_x: usize = 20;
    let win_y: usize = 5;
    let win_w: usize = 40;
    let win_h: usize = 12;

    crate::serial_println!("draw_welcome_window: getting VGA lock");
    let mut writer = VGA_WRITER.lock();
    crate::serial_println!("draw_welcome_window: got VGA lock");

    // Window background
    writer.set_color(Color::White, Color::Black);
    let mut wy: usize = 0;
    while wy < win_h {
        let mut wx: usize = 0;
        while wx < win_w {
            writer.set_cursor_position(win_y + wy, win_x + wx);
            writer.write_byte(b' ');
            wx = wx.wrapping_add(1);
        }
        wy = wy.wrapping_add(1);
    }

    // Window border - top
    writer.set_color(Color::Black, Color::LightGray);
    writer.set_cursor_position(win_y, win_x);
    writer.write_byte(b'+');
    let mut i: usize = 0;
    while i < win_w - 2 {
        writer.write_byte(b'-');
        i = i.wrapping_add(1);
    }
    writer.write_byte(b'+');

    // Title
    writer.set_cursor_position(win_y, win_x + 2);
    writer.set_color(Color::Yellow, Color::LightGray);
    writer.write_string(" Welcome to RustOS ");

    // Close button
    writer.set_cursor_position(win_y, win_x + win_w - 3);
    writer.set_color(Color::Red, Color::LightGray);
    writer.write_byte(b'x');

    // Side borders and bottom
    let mut row: usize = 1;
    while row < win_h - 1 {
        writer.set_cursor_position(win_y + row, win_x);
        writer.set_color(Color::Black, Color::LightGray);
        writer.write_byte(b'|');
        writer.set_cursor_position(win_y + row, win_x + win_w - 1);
        writer.write_byte(b'|');
        row = row.wrapping_add(1);
    }

    // Bottom border
    writer.set_cursor_position(win_y + win_h - 1, win_x);
    writer.write_byte(b'+');
    let mut j: usize = 0;
    while j < win_w - 2 {
        writer.write_byte(b'-');
        j = j.wrapping_add(1);
    }
    writer.write_byte(b'+');

    // Window content
    writer.set_color(Color::White, Color::Black);
    writer.set_cursor_position(win_y + 3, win_x + 3);
    writer.write_string("RustOS Desktop Environment");

    writer.set_cursor_position(win_y + 5, win_x + 3);
    writer.write_string("The kernel has booted successfully!");

    writer.set_cursor_position(win_y + 7, win_x + 3);
    writer.set_color(Color::LightGreen, Color::Black);
    writer.write_string("Press keys to interact:");

    writer.set_cursor_position(win_y + 8, win_x + 3);
    writer.set_color(Color::Cyan, Color::Black);
    writer.write_string("  M - Menu  |  1-5 - Windows");

    writer.set_cursor_position(win_y + 9, win_x + 3);
    writer.write_string("  Q - Quit  |  H - Help");
}

/// Get desktop reference safely
pub fn with_desktop<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&mut Desktop) -> R,
{
    let mut desktop_lock = DESKTOP.lock();
    if let Some(ref mut desktop) = *desktop_lock {
        Some(f(desktop))
    } else {
        None
    }
}

/// Legacy desktop loop - DEPRECATED
/// The real desktop loop with keyboard integration is now in main.rs::desktop_main_loop()
/// This function is kept for compatibility but should not be used
pub fn run_desktop() -> ! {
    crate::println!(
        "Warning: run_desktop() is deprecated. Use main.rs::desktop_main_loop() instead."
    );
    init_desktop();

    loop {
        with_desktop(|desktop| {
            desktop.update();
        });

        // Real keyboard input is handled in main.rs, not here
        // This loop now only handles periodic updates

        // Sleep between updates to prevent excessive CPU usage
        let mut si: u32 = 0;
        while si < 100_000 {
            unsafe {
                core::arch::asm!("nop");
            }
            si = si.wrapping_add(1);
        }

        // Halt CPU until next interrupt
        unsafe {
            core::arch::asm!("hlt");
        }
    }
}

// =============================================================================
// VGA MODE 13H DIAGNOSTIC FALLBACK (320x200x8)
// =============================================================================
// Limited graphics fallback used only when the 32-bit framebuffer UI is unavailable.

/// Initialize limited graphical fallback.
pub fn init_pixel_desktop() {
    crate::serial_println!("pixel_desktop: Initializing VGA Mode 13h...");

    // Initialize VGA Mode 13h (320x200x8).
    crate::vga_mode13h::init();

    if crate::vga_mode13h::is_ready() {
        crate::serial_println!("pixel_desktop: VGA Mode 13h ready, drawing desktop...");
        draw_pixel_desktop();
        crate::serial_println!("pixel_desktop: Desktop rendered!");
    } else {
        crate::serial_println!(
            "pixel_desktop: VGA Mode 13h init failed, falling back to text mode"
        );
        init_desktop();
    }
}

/// Draw limited diagnostic desktop.
fn draw_pixel_desktop() {
    use crate::vga_mode13h::{clear_screen, colors};

    // Clear screen with diagnostic fallback color.
    clear_screen(colors::DESKTOP_TEAL);

    // Draw desktop icons
    draw_pixel_desktop_icons();

    // Draw taskbar at bottom
    draw_pixel_taskbar();

    // Draw some classic windows
    draw_pixel_window(20, 20, 140, 100, "Shell", true);
    draw_pixel_window(170, 40, 130, 90, "Welcome", false);
}

/// Draw desktop icons.
fn draw_pixel_desktop_icons() {
    use crate::vga_mode13h::{colors, draw_rect, draw_string, fill_rect};

    // My Computer icon at top-left
    // Icon background (32x32)
    fill_rect(10, 10, 32, 32, colors::WINDOW_BACKGROUND);
    draw_rect(10, 10, 32, 32, colors::BLACK);
    // Simple monitor shape
    fill_rect(14, 14, 24, 18, colors::DARK_BLUE);
    fill_rect(16, 16, 20, 14, colors::CYAN);
    fill_rect(22, 32, 8, 4, colors::DARK_GRAY);
    fill_rect(18, 36, 16, 2, colors::DARK_GRAY);
    // Label
    draw_string(4, 46, "Shell", colors::WHITE, colors::DESKTOP_TEAL);

    // System status icon
    fill_rect(10, 60, 32, 32, colors::WINDOW_BACKGROUND);
    draw_rect(10, 60, 32, 32, colors::BLACK);
    fill_rect(18, 64, 16, 4, colors::DARK_GRAY);
    fill_rect(16, 68, 20, 18, colors::LIGHT_GRAY);
    fill_rect(18, 70, 16, 14, colors::DARK_GRAY);
    // Label
    draw_string(4, 96, "Status", colors::WHITE, colors::DESKTOP_TEAL);

    // Network icon
    fill_rect(10, 110, 32, 32, colors::WINDOW_BACKGROUND);
    draw_rect(10, 110, 32, 32, colors::BLACK);
    // Simple network/globe shape
    fill_rect(18, 114, 16, 16, colors::BLUE);
    fill_rect(22, 118, 8, 8, colors::CYAN);
    fill_rect(14, 130, 24, 6, colors::DARK_GRAY);
    // Label
    draw_string(0, 146, "Net n/a", colors::WHITE, colors::DESKTOP_TEAL);
}

/// Draw fallback taskbar.
fn draw_pixel_taskbar() {
    use crate::vga_mode13h::{
        colors, draw_3d_rect, draw_string, fill_rect, hline, SCREEN_HEIGHT, SCREEN_WIDTH,
    };

    let taskbar_height: usize = 28;
    let taskbar_y: usize = SCREEN_HEIGHT - taskbar_height;

    // Taskbar background
    fill_rect(
        0,
        taskbar_y,
        SCREEN_WIDTH,
        taskbar_height,
        colors::BUTTON_FACE,
    );

    // Top edge highlight
    hline(0, taskbar_y, SCREEN_WIDTH, colors::BUTTON_HIGHLIGHT);

    // Start button with 3D effect
    let start_x: usize = 2;
    let start_y: usize = taskbar_y + 3;
    let start_w: usize = 54;
    let start_h: usize = 22;

    fill_rect(start_x, start_y, start_w, start_h, colors::BUTTON_FACE);
    draw_3d_rect(start_x, start_y, start_w, start_h, true);

    // Start button text
    draw_string(
        start_x + 4,
        start_y + 7,
        "Start",
        colors::BLACK,
        colors::BUTTON_FACE,
    );

    // Quick launch separator
    fill_rect(60, taskbar_y + 4, 2, 20, colors::BUTTON_SHADOW);
    fill_rect(62, taskbar_y + 4, 1, 20, colors::BUTTON_HIGHLIGHT);

    // System tray area
    let tray_x: usize = SCREEN_WIDTH - 60;
    fill_rect(tray_x, taskbar_y + 2, 58, 24, colors::BUTTON_SHADOW);
    fill_rect(tray_x + 1, taskbar_y + 3, 56, 22, colors::BUTTON_FACE);

    // Clock in system tray
    let ts = crate::time::system_time() % 86400;
    let hours = (ts / 3600) % 24;
    let minutes = (ts / 60) % 60;
    let mut clock = heapless::String::<8>::new();
    let _ = clock.push_str(&format!("{:02}:{:02}", hours, minutes));
    draw_string(
        tray_x + 8,
        taskbar_y + 10,
        clock.as_str(),
        colors::BLACK,
        colors::BUTTON_FACE,
    );
}

/// Draw a fallback diagnostic window.
fn draw_pixel_window(x: usize, y: usize, w: usize, h: usize, title: &str, active: bool) {
    use crate::vga_mode13h::{colors, draw_3d_rect, draw_string, fill_rect};

    // Window background
    fill_rect(x, y, w, h, colors::BUTTON_FACE);

    // Outer 3D border (raised)
    draw_3d_rect(x, y, w, h, true);

    // Inner border
    fill_rect(x + 2, y + 2, w - 4, h - 4, colors::BUTTON_FACE);

    // Title bar
    let title_bar_color = if active {
        colors::TITLE_BAR_BLUE
    } else {
        colors::TITLE_BAR_INACTIVE
    };
    let title_bar_height: usize = 18;
    fill_rect(x + 3, y + 3, w - 6, title_bar_height, title_bar_color);

    // Title text
    draw_string(x + 6, y + 7, title, colors::WHITE, title_bar_color);

    // Window control buttons (minimize, maximize, close)
    let btn_y: usize = y + 5;
    let btn_size: usize = 14;

    // Close button (red X)
    let close_x: usize = x + w - 20;
    fill_rect(close_x, btn_y, btn_size, btn_size, colors::BUTTON_FACE);
    draw_3d_rect(close_x, btn_y, btn_size, btn_size, true);
    draw_string(
        close_x + 4,
        btn_y + 3,
        "X",
        colors::BLACK,
        colors::BUTTON_FACE,
    );

    // Maximize button
    let max_x: usize = close_x - 16;
    fill_rect(max_x, btn_y, btn_size, btn_size, colors::BUTTON_FACE);
    draw_3d_rect(max_x, btn_y, btn_size, btn_size, true);

    // Minimize button
    let min_x: usize = max_x - 16;
    fill_rect(min_x, btn_y, btn_size, btn_size, colors::BUTTON_FACE);
    draw_3d_rect(min_x, btn_y, btn_size, btn_size, true);
    draw_string(
        min_x + 4,
        btn_y + 8,
        "_",
        colors::BLACK,
        colors::BUTTON_FACE,
    );

    // Menu bar
    let menu_y: usize = y + 3 + title_bar_height;
    fill_rect(x + 3, menu_y, w - 6, 16, colors::BUTTON_FACE);
    draw_string(
        x + 6,
        menu_y + 4,
        "File",
        colors::BLACK,
        colors::BUTTON_FACE,
    );
    draw_string(
        x + 38,
        menu_y + 4,
        "Edit",
        colors::BLACK,
        colors::BUTTON_FACE,
    );
    draw_string(
        x + 70,
        menu_y + 4,
        "View",
        colors::BLACK,
        colors::BUTTON_FACE,
    );
    draw_string(
        x + 102,
        menu_y + 4,
        "Help",
        colors::BLACK,
        colors::BUTTON_FACE,
    );

    // Client area (white background)
    let client_y: usize = menu_y + 16;
    let client_h: usize = h - (client_y - y) - 4;
    fill_rect(x + 3, client_y, w - 6, client_h, colors::WINDOW_BACKGROUND);

    // Some content in the window
    if title == "Shell" {
        draw_string(
            x + 8,
            client_y + 8,
            "Kernel shell (text mode)",
            colors::BLACK,
            colors::WINDOW_BACKGROUND,
        );
        draw_string(
            x + 8,
            client_y + 20,
            "Use Terminal window keys",
            colors::BLACK,
            colors::WINDOW_BACKGROUND,
        );
    } else if title == "Welcome" {
        draw_string(
            x + 8,
            client_y + 8,
            "Welcome to",
            colors::BLACK,
            colors::WINDOW_BACKGROUND,
        );
        draw_string(
            x + 8,
            client_y + 20,
            "RustOS!",
            colors::BLUE,
            colors::WINDOW_BACKGROUND,
        );
        draw_string(
            x + 8,
            client_y + 36,
            "Version 1.0",
            colors::DARK_GRAY,
            colors::WINDOW_BACKGROUND,
        );
    }
}
