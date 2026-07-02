//! Production VGA text mode buffer for RustOS
//!
//! Provides real VGA text mode output at 0xB8000

use core::fmt;
use lazy_static::lazy_static;
use spin::Mutex;
use volatile::Volatile;

/// VGA text mode buffer dimensions
pub const BUFFER_HEIGHT: usize = 25;
pub const BUFFER_WIDTH: usize = 80;

/// VGA color codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Color {
    Black = 0,
    Blue = 1,
    Green = 2,
    Cyan = 3,
    Red = 4,
    Magenta = 5,
    Brown = 6,
    LightGray = 7,
    DarkGray = 8,
    LightBlue = 9,
    LightGreen = 10,
    LightCyan = 11,
    LightRed = 12,
    Pink = 13,
    Yellow = 14,
    White = 15,
}

/// Color combination for foreground and background
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
struct ColorCode(u8);

impl ColorCode {
    const fn new(foreground: Color, background: Color) -> Self {
        ColorCode((background as u8) << 4 | (foreground as u8))
    }
}

/// VGA text buffer character
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
struct ScreenChar {
    ascii_character: u8,
    color_code: ColorCode,
}

/// VGA text buffer (memory-mapped at 0xB8000)
#[repr(transparent)]
struct Buffer {
    chars: [[Volatile<ScreenChar>; BUFFER_WIDTH]; BUFFER_HEIGHT],
}

/// VGA text mode writer
pub struct Writer {
    column_position: usize,
    row_position: usize,
    color_code: ColorCode,
    buffer: &'static mut Buffer,
}

impl Writer {
    /// Write a single byte
    pub fn write_byte(&mut self, byte: u8) {
        match byte {
            b'\n' => self.new_line(),
            b'\r' => self.column_position = 0,
            b'\t' => {
                // Tab to next 4-column boundary
                let tab_stop = (self.column_position + 4) & !3;
                while self.column_position < tab_stop && self.column_position < BUFFER_WIDTH {
                    self.write_byte(b' ');
                }
            }
            0x08 => {
                // Backspace
                if self.column_position > 0 {
                    self.column_position -= 1;
                    self.write_byte(b' ');
                    self.column_position -= 1;
                }
            }
            byte => {
                if self.column_position >= BUFFER_WIDTH {
                    self.new_line();
                }

                let row = self.row_position;
                let col = self.column_position;

                self.buffer.chars[row][col].write(ScreenChar {
                    ascii_character: byte,
                    color_code: self.color_code,
                });

                self.column_position += 1;
            }
        }
    }

    /// Write a string
    pub fn write_string(&mut self, s: &str) {
        for byte in s.bytes() {
            match byte {
                // Printable ASCII or newline
                0x20..=0x7e | b'\n' | b'\r' | b'\t' | 0x08 => self.write_byte(byte),
                // Non-printable, show a ■
                _ => self.write_byte(0xfe),
            }
        }
    }

    /// Create a new line
    fn new_line(&mut self) {
        if self.row_position >= BUFFER_HEIGHT - 1 {
            // Scroll up
            for row in 1..BUFFER_HEIGHT {
                for col in 0..BUFFER_WIDTH {
                    let character = self.buffer.chars[row][col].read();
                    self.buffer.chars[row - 1][col].write(character);
                }
            }

            // Clear last line
            self.clear_row(BUFFER_HEIGHT - 1);
        } else {
            self.row_position += 1;
        }
        self.column_position = 0;
    }

    /// Clear a row
    fn clear_row(&mut self, row: usize) {
        let blank = ScreenChar {
            ascii_character: b' ',
            color_code: self.color_code,
        };
        for col in 0..BUFFER_WIDTH {
            self.buffer.chars[row][col].write(blank);
        }
    }

    /// Clear the entire screen
    pub fn clear_screen(&mut self) {
        for row in 0..BUFFER_HEIGHT {
            self.clear_row(row);
        }
        self.row_position = 0;
        self.column_position = 0;
        self.update_cursor();
    }

    /// Set color for future writes
    pub fn set_color(&mut self, foreground: Color, background: Color) {
        self.color_code = ColorCode::new(foreground, background);
    }

    /// Set cursor position
    pub fn set_cursor_position(&mut self, row: usize, col: usize) {
        self.row_position = row.min(BUFFER_HEIGHT - 1);
        self.column_position = col.min(BUFFER_WIDTH - 1);
        self.update_cursor();
    }

    /// Update hardware cursor position
    pub fn update_cursor(&self) {
        let pos = self.row_position * BUFFER_WIDTH + self.column_position;

        unsafe {
            // Cursor location low byte
            x86_64::instructions::port::Port::<u8>::new(0x3D4).write(0x0F);
            x86_64::instructions::port::Port::<u8>::new(0x3D5).write((pos & 0xFF) as u8);

            // Cursor location high byte
            x86_64::instructions::port::Port::<u8>::new(0x3D4).write(0x0E);
            x86_64::instructions::port::Port::<u8>::new(0x3D5).write((pos >> 8) as u8);
        }
    }
}

impl fmt::Write for Writer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_string(s);
        Ok(())
    }
}

lazy_static! {
    /// Global VGA writer instance
    pub static ref VGA_WRITER: Mutex<Writer> = Mutex::new(Writer {
        column_position: 0,
        row_position: 0,
        color_code: ColorCode::new(Color::White, Color::Black),
        // SAFETY: 0xb8000 is the fixed VGA text buffer MMIO address, always mapped in the kernel. Buffer is repr(transparent) over a 2D array.
        buffer: unsafe { &mut *(0xb8000 as *mut Buffer) },
    });
}

/// Initialize VGA buffer
pub fn init() {
    let mut writer = VGA_WRITER.lock();
    writer.clear_screen();
}

/// Print to VGA buffer (internal macro use)
#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    use x86_64::instructions::interrupts;

    // Disable interrupts to prevent deadlock
    interrupts::without_interrupts(|| {
        VGA_WRITER.lock().write_fmt(args).unwrap();
    });
}

/// Print macro for VGA output
#[macro_export]
macro_rules! vga_print {
    ($($arg:tt)*) => ($crate::vga_buffer::_print(format_args!($($arg)*)));
}

/// Print with newline macro for VGA output
#[macro_export]
macro_rules! vga_println {
    () => ($crate::vga_print!("\n"));
    ($($arg:tt)*) => ($crate::vga_print!("{}\n", format_args!($($arg)*)));
}

/// Clear the VGA screen
pub fn clear() {
    VGA_WRITER.lock().clear_screen();
}

/// Set VGA color
pub fn set_color(foreground: Color, background: Color) {
    VGA_WRITER.lock().set_color(foreground, background);
}

/// Get current cursor position
pub fn cursor_position() -> (usize, usize) {
    let writer = VGA_WRITER.lock();
    (writer.column_position, writer.row_position)
}

/// Set cursor position
pub fn set_cursor_position(row: usize, col: usize) {
    let mut writer = VGA_WRITER.lock();
    writer.row_position = row.min(BUFFER_HEIGHT - 1);
    writer.column_position = col.min(BUFFER_WIDTH - 1);
}

/// Write string to VGA buffer
pub fn write_string(s: &str) {
    use core::fmt::Write;
    VGA_WRITER.lock().write_str(s).unwrap();
}

/// Write bytes to VGA buffer
pub fn write_bytes(bytes: &[u8]) {
    let mut writer = VGA_WRITER.lock();
    for &byte in bytes {
        match byte {
            0x20..=0x7e | b'\n' => writer.write_byte(byte),
            _ => writer.write_byte(0xfe), // Display replacement character
        }
    }
}

/// Print bytes to VGA buffer (for syscall use)
pub fn print_bytes(bytes: &[u8]) {
    write_bytes(bytes);
}

/// Clear the entire screen
pub fn clear_screen() {
    let mut writer = VGA_WRITER.lock();
    writer.clear_screen();
}

/// Print string at specific position with color
pub fn print_at(x: usize, y: usize, text: &str, color: u8) {
    if y >= BUFFER_HEIGHT || x >= BUFFER_WIDTH {
        return;
    }

    let vga_buffer = 0xb8000 as *mut u8;
    let offset = (y * BUFFER_WIDTH + x) * 2;

    for (i, byte) in text.bytes().enumerate() {
        if x + i >= BUFFER_WIDTH {
            break;
        }
        unsafe {
            *vga_buffer.add(offset + i * 2) = byte;
            *vga_buffer.add(offset + i * 2 + 1) = color;
        }
    }
}
