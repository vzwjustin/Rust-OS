//! VGA Mode 13h Graphics Driver
//! 320x200 resolution with 256 colors - classic 32-bit era graphics
//!
//! Framebuffer is at physical address 0xA0000
//! Virtual address = physical_memory_offset + 0xA0000

use core::sync::atomic::{AtomicU64, Ordering};
use lazy_static::lazy_static;
use spin::Mutex;
use x86_64::instructions::port::Port;

/// Screen dimensions for Mode 13h
pub const SCREEN_WIDTH: usize = 320;
pub const SCREEN_HEIGHT: usize = 200;
pub const FRAMEBUFFER_PHYS_ADDR: usize = 0xA0000;

/// Physical memory offset from bootloader (set during init)
static PHYS_MEM_OFFSET: AtomicU64 = AtomicU64::new(0);

/// Get the virtual address of the VGA framebuffer
#[inline]
fn get_framebuffer_addr() -> usize {
    let offset = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    if offset == 0 {
        // Fall back to identity mapping (for testing)
        FRAMEBUFFER_PHYS_ADDR
    } else {
        offset as usize + FRAMEBUFFER_PHYS_ADDR
    }
}

/// VGA port addresses
const VGA_MISC_WRITE: u16 = 0x3C2;
const VGA_SEQ_INDEX: u16 = 0x3C4;
const VGA_SEQ_DATA: u16 = 0x3C5;
const VGA_CRTC_INDEX: u16 = 0x3D4;
const VGA_CRTC_DATA: u16 = 0x3D5;
const VGA_GC_INDEX: u16 = 0x3CE;
const VGA_GC_DATA: u16 = 0x3CF;
const VGA_AC_INDEX: u16 = 0x3C0;
const VGA_AC_WRITE: u16 = 0x3C0;
const VGA_AC_READ: u16 = 0x3C1;
const VGA_INSTAT_READ: u16 = 0x3DA;
const VGA_DAC_WRITE_INDEX: u16 = 0x3C8;
const VGA_DAC_DATA: u16 = 0x3C9;

/// Classic Windows 95/98 color palette (256 colors)
pub mod colors {
    pub const BLACK: u8 = 0;
    pub const DARK_BLUE: u8 = 1;
    pub const DARK_GREEN: u8 = 2;
    pub const DARK_CYAN: u8 = 3;
    pub const DARK_RED: u8 = 4;
    pub const DARK_MAGENTA: u8 = 5;
    pub const BROWN: u8 = 6;
    pub const LIGHT_GRAY: u8 = 7;
    pub const DARK_GRAY: u8 = 8;
    pub const BLUE: u8 = 9;
    pub const GREEN: u8 = 10;
    pub const CYAN: u8 = 11;
    pub const RED: u8 = 12;
    pub const MAGENTA: u8 = 13;
    pub const YELLOW: u8 = 14;
    pub const WHITE: u8 = 15;

    // Windows 95 specific colors
    pub const DESKTOP_TEAL: u8 = 16;
    pub const TITLE_BAR_BLUE: u8 = 17;
    pub const TITLE_BAR_INACTIVE: u8 = 18;
    pub const BUTTON_FACE: u8 = 19;
    pub const BUTTON_HIGHLIGHT: u8 = 20;
    pub const BUTTON_SHADOW: u8 = 21;
    pub const WINDOW_BACKGROUND: u8 = 22;
    pub const MENU_BAR: u8 = 23;
}

lazy_static! {
    pub static ref VGA_MODE13H: Mutex<VgaMode13h> = Mutex::new(VgaMode13h::new());
}

pub struct VgaMode13h {
    initialized: bool,
}

impl VgaMode13h {
    pub fn new() -> Self {
        Self { initialized: false }
    }

    /// Initialize Mode 13h (320x200, 256 colors)
    /// Uses the physical memory offset already set via set_phys_mem_offset()
    pub fn init(&mut self) {
        if self.initialized {
            return;
        }

        let current_offset = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
        crate::serial_println!(
            "VGA Mode 13h: Initializing with phys_mem_offset = 0x{:x}",
            current_offset
        );

        unsafe {
            self.set_mode_13h();
            self.setup_palette();
        }

        self.initialized = true;
        crate::serial_println!("VGA Mode 13h: Ready (320x200, 256 colors)");

        // Test pattern to verify graphics are working
        crate::serial_println!("VGA Mode 13h: Drawing test pattern...");
    }

    /// Initialize Mode 13h with physical memory offset from bootloader
    pub fn init_with_offset(&mut self, phys_mem_offset: u64) {
        // Only store the offset if a non-zero value is provided
        if phys_mem_offset != 0 {
            PHYS_MEM_OFFSET.store(phys_mem_offset, Ordering::Relaxed);
        }
        self.init();
    }

    /// Set VGA to Mode 13h by programming registers directly
    unsafe fn set_mode_13h(&self) {
        // Sequencer registers for Mode 13h
        let seq_regs: [(u8, u8); 5] = [
            (0x00, 0x03), // Reset
            (0x01, 0x01), // Clocking mode
            (0x02, 0x0F), // Map mask - enable all planes
            (0x03, 0x00), // Character map select
            (0x04, 0x0E), // Memory mode - chain-4, extended memory
        ];

        // CRTC registers for Mode 13h (320x200)
        let crtc_regs: [(u8, u8); 25] = [
            (0x00, 0x5F), // Horizontal total
            (0x01, 0x4F), // Horizontal display end
            (0x02, 0x50), // Start horizontal blanking
            (0x03, 0x82), // End horizontal blanking
            (0x04, 0x54), // Start horizontal retrace
            (0x05, 0x80), // End horizontal retrace
            (0x06, 0xBF), // Vertical total
            (0x07, 0x1F), // Overflow
            (0x08, 0x00), // Preset row scan
            (0x09, 0x41), // Maximum scan line
            (0x0A, 0x00), // Cursor start
            (0x0B, 0x00), // Cursor end
            (0x0C, 0x00), // Start address high
            (0x0D, 0x00), // Start address low
            (0x0E, 0x00), // Cursor location high
            (0x0F, 0x00), // Cursor location low
            (0x10, 0x9C), // Vertical retrace start
            (0x11, 0x0E), // Vertical retrace end
            (0x12, 0x8F), // Vertical display end
            (0x13, 0x28), // Offset (logical width / 8)
            (0x14, 0x40), // Underline location
            (0x15, 0x96), // Start vertical blanking
            (0x16, 0xB9), // End vertical blanking
            (0x17, 0xA3), // CRTC mode control
            (0x18, 0xFF), // Line compare
        ];

        // Graphics controller registers for Mode 13h
        let gc_regs: [(u8, u8); 9] = [
            (0x00, 0x00), // Set/reset
            (0x01, 0x00), // Enable set/reset
            (0x02, 0x00), // Color compare
            (0x03, 0x00), // Data rotate
            (0x04, 0x00), // Read map select
            (0x05, 0x40), // Graphics mode - 256 color mode
            (0x06, 0x05), // Miscellaneous - graphics mode, A0000
            (0x07, 0x0F), // Color don't care
            (0x08, 0xFF), // Bit mask
        ];

        // Attribute controller registers for Mode 13h
        let ac_regs: [(u8, u8); 21] = [
            (0x00, 0x00),
            (0x01, 0x01),
            (0x02, 0x02),
            (0x03, 0x03),
            (0x04, 0x04),
            (0x05, 0x05),
            (0x06, 0x06),
            (0x07, 0x07),
            (0x08, 0x08),
            (0x09, 0x09),
            (0x0A, 0x0A),
            (0x0B, 0x0B),
            (0x0C, 0x0C),
            (0x0D, 0x0D),
            (0x0E, 0x0E),
            (0x0F, 0x0F),
            (0x10, 0x41), // Mode control
            (0x11, 0x00), // Overscan color
            (0x12, 0x0F), // Color plane enable
            (0x13, 0x00), // Horizontal panning
            (0x14, 0x00), // Color select
        ];

        // Write miscellaneous output register
        let mut misc_port: Port<u8> = Port::new(VGA_MISC_WRITE);
        misc_port.write(0x63);

        // Program sequencer
        let mut seq_index: Port<u8> = Port::new(VGA_SEQ_INDEX);
        let mut seq_data: Port<u8> = Port::new(VGA_SEQ_DATA);

        // Unlock sequencer
        seq_index.write(0x00);
        seq_data.write(0x01);

        let mut i = 0;
        while i < seq_regs.len() {
            seq_index.write(seq_regs[i].0);
            seq_data.write(seq_regs[i].1);
            i += 1;
        }

        // Unlock CRTC registers
        let mut crtc_index: Port<u8> = Port::new(VGA_CRTC_INDEX);
        let mut crtc_data: Port<u8> = Port::new(VGA_CRTC_DATA);

        crtc_index.write(0x11);
        let val = crtc_data.read();
        crtc_data.write(val & 0x7F);

        // Program CRTC
        let mut j = 0;
        while j < crtc_regs.len() {
            crtc_index.write(crtc_regs[j].0);
            crtc_data.write(crtc_regs[j].1);
            j += 1;
        }

        // Program graphics controller
        let mut gc_index: Port<u8> = Port::new(VGA_GC_INDEX);
        let mut gc_data: Port<u8> = Port::new(VGA_GC_DATA);

        let mut k = 0;
        while k < gc_regs.len() {
            gc_index.write(gc_regs[k].0);
            gc_data.write(gc_regs[k].1);
            k += 1;
        }

        // Reset attribute controller flip-flop
        let mut instat: Port<u8> = Port::new(VGA_INSTAT_READ);
        let _ = instat.read();

        // Program attribute controller
        let mut ac_index: Port<u8> = Port::new(VGA_AC_INDEX);
        let mut ac_write: Port<u8> = Port::new(VGA_AC_WRITE);

        let mut m = 0;
        while m < ac_regs.len() {
            let _ = instat.read(); // Reset flip-flop
            ac_index.write(ac_regs[m].0);
            ac_write.write(ac_regs[m].1);
            m += 1;
        }

        // Enable video output
        let _ = instat.read();
        ac_index.write(0x20);
    }

    /// Set up the Windows 95 style color palette
    unsafe fn setup_palette(&self) {
        let mut dac_index: Port<u8> = Port::new(VGA_DAC_WRITE_INDEX);
        let mut dac_data: Port<u8> = Port::new(VGA_DAC_DATA);

        // Standard 16 colors (VGA palette, values are 0-63)
        let standard_colors: [(u8, u8, u8); 16] = [
            (0, 0, 0),    // 0: Black
            (0, 0, 42),   // 1: Dark Blue
            (0, 42, 0),   // 2: Dark Green
            (0, 42, 42),  // 3: Dark Cyan
            (42, 0, 0),   // 4: Dark Red
            (42, 0, 42),  // 5: Dark Magenta
            (42, 21, 0),  // 6: Brown
            (42, 42, 42), // 7: Light Gray
            (21, 21, 21), // 8: Dark Gray
            (21, 21, 63), // 9: Blue
            (21, 63, 21), // 10: Green
            (21, 63, 63), // 11: Cyan
            (63, 21, 21), // 12: Red
            (63, 21, 63), // 13: Magenta
            (63, 63, 21), // 14: Yellow
            (63, 63, 63), // 15: White
        ];

        // Windows 95 special colors (indices 16-31)
        let win95_colors: [(u8, u8, u8); 16] = [
            (0, 32, 32),  // 16: Desktop Teal
            (0, 0, 50),   // 17: Title Bar Blue (active)
            (32, 32, 32), // 18: Title Bar Gray (inactive)
            (48, 48, 48), // 19: Button Face (3D gray)
            (63, 63, 63), // 20: Button Highlight (white)
            (21, 21, 21), // 21: Button Shadow (dark gray)
            (63, 63, 63), // 22: Window Background (white)
            (48, 48, 48), // 23: Menu Bar
            (0, 42, 0),   // 24: Selection Green
            (50, 50, 0),  // 25: Selection Yellow
            (42, 42, 63), // 26: Light Blue
            (63, 48, 48), // 27: Light Red
            (48, 63, 48), // 28: Light Green
            (32, 32, 48), // 29: Slate Blue
            (48, 32, 32), // 30: Maroon
            (32, 48, 32), // 31: Forest Green
        ];

        // Set standard colors
        let mut idx = 0;
        while idx < 16 {
            dac_index.write(idx as u8);
            dac_data.write(standard_colors[idx].0);
            dac_data.write(standard_colors[idx].1);
            dac_data.write(standard_colors[idx].2);
            idx += 1;
        }

        // Set Windows 95 colors
        let mut idx2 = 0;
        while idx2 < 16 {
            dac_index.write((16 + idx2) as u8);
            dac_data.write(win95_colors[idx2].0);
            dac_data.write(win95_colors[idx2].1);
            dac_data.write(win95_colors[idx2].2);
            idx2 += 1;
        }

        // Fill remaining palette with gradient colors
        let mut idx3: u8 = 32;
        while idx3 < 255 {
            dac_index.write(idx3);
            // Create a gradient
            let r = ((idx3 as u16 - 32) * 63 / 223) as u8;
            let g = ((idx3 as u16 - 32) * 63 / 223) as u8;
            let b = ((idx3 as u16 - 32) * 63 / 223) as u8;
            dac_data.write(r);
            dac_data.write(g);
            dac_data.write(b);
            idx3 = idx3.wrapping_add(1);
        }
    }

    /// Check if mode 13h is initialized
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }
}

/// Set a pixel at (x, y) with the given color index
#[inline]
pub fn set_pixel(x: usize, y: usize, color: u8) {
    if x >= SCREEN_WIDTH || y >= SCREEN_HEIGHT {
        return;
    }
    let pixel_offset = y * SCREEN_WIDTH + x;
    let fb_addr = get_framebuffer_addr();
    // Use inline assembly to write to framebuffer, bypassing Rust's UB checks
    // This is necessary for memory-mapped I/O which has different semantics
    unsafe {
        let addr = fb_addr.wrapping_add(pixel_offset);
        core::arch::asm!(
            "mov byte ptr [{addr}], {val}",
            addr = in(reg) addr,
            val = in(reg_byte) color,
            options(nostack, preserves_flags)
        );
    }
}

/// Get the pixel color at (x, y)
#[inline]
pub fn get_pixel(x: usize, y: usize) -> u8 {
    if x >= SCREEN_WIDTH || y >= SCREEN_HEIGHT {
        return 0;
    }
    let pixel_offset = y * SCREEN_WIDTH + x;
    let fb_addr = get_framebuffer_addr();
    unsafe {
        let addr = fb_addr.wrapping_add(pixel_offset);
        let result: u8;
        core::arch::asm!(
            "mov {val}, byte ptr [{addr}]",
            addr = in(reg) addr,
            val = out(reg_byte) result,
            options(nostack, preserves_flags, readonly)
        );
        result
    }
}

/// Fill the entire screen with a color
pub fn clear_screen(color: u8) {
    let fb_addr = get_framebuffer_addr();
    // Use rep stosb for fast memory fill
    let count = SCREEN_WIDTH * SCREEN_HEIGHT;
    unsafe {
        core::arch::asm!(
            "rep stosb",
            inout("rdi") fb_addr => _,
            inout("rcx") count => _,
            in("al") color,
            options(nostack, preserves_flags)
        );
    }
}

/// Draw a filled rectangle
pub fn fill_rect(x: usize, y: usize, w: usize, h: usize, color: u8) {
    let mut py = y;
    while py < y + h && py < SCREEN_HEIGHT {
        let mut px = x;
        while px < x + w && px < SCREEN_WIDTH {
            set_pixel(px, py, color);
            px = px.wrapping_add(1);
        }
        py = py.wrapping_add(1);
    }
}

/// Draw a horizontal line
pub fn hline(x: usize, y: usize, len: usize, color: u8) {
    if y >= SCREEN_HEIGHT {
        return;
    }
    let mut px = x;
    while px < x + len && px < SCREEN_WIDTH {
        set_pixel(px, y, color);
        px = px.wrapping_add(1);
    }
}

/// Draw a vertical line
pub fn vline(x: usize, y: usize, len: usize, color: u8) {
    if x >= SCREEN_WIDTH {
        return;
    }
    let mut py = y;
    while py < y + len && py < SCREEN_HEIGHT {
        set_pixel(x, py, color);
        py = py.wrapping_add(1);
    }
}

/// Draw a rectangle outline
pub fn draw_rect(x: usize, y: usize, w: usize, h: usize, color: u8) {
    hline(x, y, w, color); // Top
    hline(x, y + h - 1, w, color); // Bottom
    vline(x, y, h, color); // Left
    vline(x + w - 1, y, h, color); // Right
}

/// Draw a 3D raised button/border effect
pub fn draw_3d_rect(x: usize, y: usize, w: usize, h: usize, raised: bool) {
    let (highlight, shadow) = if raised {
        (colors::BUTTON_HIGHLIGHT, colors::BUTTON_SHADOW)
    } else {
        (colors::BUTTON_SHADOW, colors::BUTTON_HIGHLIGHT)
    };

    // Top and left edges (highlight)
    hline(x, y, w, highlight);
    vline(x, y, h, highlight);

    // Bottom and right edges (shadow)
    hline(x, y + h - 1, w, shadow);
    vline(x + w - 1, y, h, shadow);
}

/// Draw a simple 8x8 character (basic font)
pub fn draw_char(x: usize, y: usize, ch: char, fg: u8, bg: u8) {
    // Simple 8x8 bitmap font - just a few characters for demo
    let bitmap = get_char_bitmap(ch);

    let mut row = 0;
    while row < 8 {
        let mut col = 0;
        while col < 8 {
            let px = x + col;
            let py = y + row;
            if px < SCREEN_WIDTH && py < SCREEN_HEIGHT {
                let bit = (bitmap[row] >> (7 - col)) & 1;
                set_pixel(px, py, if bit == 1 { fg } else { bg });
            }
            col = col.wrapping_add(1);
        }
        row = row.wrapping_add(1);
    }
}

/// Draw a string of characters
pub fn draw_string(x: usize, y: usize, s: &str, fg: u8, bg: u8) {
    let mut cx = x;
    for ch in s.chars() {
        if cx + 8 > SCREEN_WIDTH {
            break;
        }
        draw_char(cx, y, ch, fg, bg);
        cx = cx.wrapping_add(8);
    }
}

/// Get 8x8 bitmap for a character (simple built-in font)
fn get_char_bitmap(ch: char) -> [u8; 8] {
    match ch {
        'A' => [0x18, 0x3C, 0x66, 0x7E, 0x66, 0x66, 0x66, 0x00],
        'B' => [0x7C, 0x66, 0x66, 0x7C, 0x66, 0x66, 0x7C, 0x00],
        'C' => [0x3C, 0x66, 0x60, 0x60, 0x60, 0x66, 0x3C, 0x00],
        'D' => [0x78, 0x6C, 0x66, 0x66, 0x66, 0x6C, 0x78, 0x00],
        'E' => [0x7E, 0x60, 0x60, 0x7C, 0x60, 0x60, 0x7E, 0x00],
        'F' => [0x7E, 0x60, 0x60, 0x7C, 0x60, 0x60, 0x60, 0x00],
        'G' => [0x3C, 0x66, 0x60, 0x6E, 0x66, 0x66, 0x3C, 0x00],
        'H' => [0x66, 0x66, 0x66, 0x7E, 0x66, 0x66, 0x66, 0x00],
        'I' => [0x3C, 0x18, 0x18, 0x18, 0x18, 0x18, 0x3C, 0x00],
        'J' => [0x1E, 0x0C, 0x0C, 0x0C, 0x6C, 0x6C, 0x38, 0x00],
        'K' => [0x66, 0x6C, 0x78, 0x70, 0x78, 0x6C, 0x66, 0x00],
        'L' => [0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x7E, 0x00],
        'M' => [0x63, 0x77, 0x7F, 0x6B, 0x63, 0x63, 0x63, 0x00],
        'N' => [0x66, 0x76, 0x7E, 0x7E, 0x6E, 0x66, 0x66, 0x00],
        'O' => [0x3C, 0x66, 0x66, 0x66, 0x66, 0x66, 0x3C, 0x00],
        'P' => [0x7C, 0x66, 0x66, 0x7C, 0x60, 0x60, 0x60, 0x00],
        'Q' => [0x3C, 0x66, 0x66, 0x66, 0x66, 0x3C, 0x0E, 0x00],
        'R' => [0x7C, 0x66, 0x66, 0x7C, 0x78, 0x6C, 0x66, 0x00],
        'S' => [0x3C, 0x66, 0x60, 0x3C, 0x06, 0x66, 0x3C, 0x00],
        'T' => [0x7E, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x00],
        'U' => [0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x3C, 0x00],
        'V' => [0x66, 0x66, 0x66, 0x66, 0x66, 0x3C, 0x18, 0x00],
        'W' => [0x63, 0x63, 0x63, 0x6B, 0x7F, 0x77, 0x63, 0x00],
        'X' => [0x66, 0x66, 0x3C, 0x18, 0x3C, 0x66, 0x66, 0x00],
        'Y' => [0x66, 0x66, 0x66, 0x3C, 0x18, 0x18, 0x18, 0x00],
        'Z' => [0x7E, 0x06, 0x0C, 0x18, 0x30, 0x60, 0x7E, 0x00],
        'a' => [0x00, 0x00, 0x3C, 0x06, 0x3E, 0x66, 0x3E, 0x00],
        'b' => [0x60, 0x60, 0x7C, 0x66, 0x66, 0x66, 0x7C, 0x00],
        'c' => [0x00, 0x00, 0x3C, 0x60, 0x60, 0x60, 0x3C, 0x00],
        'd' => [0x06, 0x06, 0x3E, 0x66, 0x66, 0x66, 0x3E, 0x00],
        'e' => [0x00, 0x00, 0x3C, 0x66, 0x7E, 0x60, 0x3C, 0x00],
        'f' => [0x1C, 0x30, 0x7C, 0x30, 0x30, 0x30, 0x30, 0x00],
        'g' => [0x00, 0x00, 0x3E, 0x66, 0x66, 0x3E, 0x06, 0x3C],
        'h' => [0x60, 0x60, 0x7C, 0x66, 0x66, 0x66, 0x66, 0x00],
        'i' => [0x18, 0x00, 0x38, 0x18, 0x18, 0x18, 0x3C, 0x00],
        'j' => [0x0C, 0x00, 0x0C, 0x0C, 0x0C, 0x0C, 0x6C, 0x38],
        'k' => [0x60, 0x60, 0x66, 0x6C, 0x78, 0x6C, 0x66, 0x00],
        'l' => [0x38, 0x18, 0x18, 0x18, 0x18, 0x18, 0x3C, 0x00],
        'm' => [0x00, 0x00, 0x66, 0x7F, 0x7F, 0x6B, 0x63, 0x00],
        'n' => [0x00, 0x00, 0x7C, 0x66, 0x66, 0x66, 0x66, 0x00],
        'o' => [0x00, 0x00, 0x3C, 0x66, 0x66, 0x66, 0x3C, 0x00],
        'p' => [0x00, 0x00, 0x7C, 0x66, 0x66, 0x7C, 0x60, 0x60],
        'q' => [0x00, 0x00, 0x3E, 0x66, 0x66, 0x3E, 0x06, 0x06],
        'r' => [0x00, 0x00, 0x7C, 0x66, 0x60, 0x60, 0x60, 0x00],
        's' => [0x00, 0x00, 0x3E, 0x60, 0x3C, 0x06, 0x7C, 0x00],
        't' => [0x30, 0x30, 0x7C, 0x30, 0x30, 0x30, 0x1C, 0x00],
        'u' => [0x00, 0x00, 0x66, 0x66, 0x66, 0x66, 0x3E, 0x00],
        'v' => [0x00, 0x00, 0x66, 0x66, 0x66, 0x3C, 0x18, 0x00],
        'w' => [0x00, 0x00, 0x63, 0x6B, 0x7F, 0x7F, 0x36, 0x00],
        'x' => [0x00, 0x00, 0x66, 0x3C, 0x18, 0x3C, 0x66, 0x00],
        'y' => [0x00, 0x00, 0x66, 0x66, 0x66, 0x3E, 0x06, 0x3C],
        'z' => [0x00, 0x00, 0x7E, 0x0C, 0x18, 0x30, 0x7E, 0x00],
        '0' => [0x3C, 0x66, 0x6E, 0x76, 0x66, 0x66, 0x3C, 0x00],
        '1' => [0x18, 0x38, 0x18, 0x18, 0x18, 0x18, 0x7E, 0x00],
        '2' => [0x3C, 0x66, 0x06, 0x0C, 0x18, 0x30, 0x7E, 0x00],
        '3' => [0x3C, 0x66, 0x06, 0x1C, 0x06, 0x66, 0x3C, 0x00],
        '4' => [0x0C, 0x1C, 0x3C, 0x6C, 0x7E, 0x0C, 0x0C, 0x00],
        '5' => [0x7E, 0x60, 0x7C, 0x06, 0x06, 0x66, 0x3C, 0x00],
        '6' => [0x1C, 0x30, 0x60, 0x7C, 0x66, 0x66, 0x3C, 0x00],
        '7' => [0x7E, 0x06, 0x0C, 0x18, 0x30, 0x30, 0x30, 0x00],
        '8' => [0x3C, 0x66, 0x66, 0x3C, 0x66, 0x66, 0x3C, 0x00],
        '9' => [0x3C, 0x66, 0x66, 0x3E, 0x06, 0x0C, 0x38, 0x00],
        ' ' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        '!' => [0x18, 0x18, 0x18, 0x18, 0x00, 0x00, 0x18, 0x00],
        ':' => [0x00, 0x18, 0x18, 0x00, 0x00, 0x18, 0x18, 0x00],
        '.' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x00],
        ',' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x30],
        '-' => [0x00, 0x00, 0x00, 0x7E, 0x00, 0x00, 0x00, 0x00],
        '_' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF],
        '/' => [0x02, 0x06, 0x0C, 0x18, 0x30, 0x60, 0x40, 0x00],
        '|' => [0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18],
        '[' => [0x3C, 0x30, 0x30, 0x30, 0x30, 0x30, 0x3C, 0x00],
        ']' => [0x3C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x3C, 0x00],
        '(' => [0x0C, 0x18, 0x30, 0x30, 0x30, 0x18, 0x0C, 0x00],
        ')' => [0x30, 0x18, 0x0C, 0x0C, 0x0C, 0x18, 0x30, 0x00],
        _ => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
    }
}

/// Initialize VGA Mode 13h (uses default/identity mapping)
pub fn init() {
    VGA_MODE13H.lock().init();
}

/// Initialize VGA Mode 13h with specific physical memory offset
pub fn init_with_phys_offset(phys_mem_offset: u64) {
    VGA_MODE13H.lock().init_with_offset(phys_mem_offset);
}

/// Set the physical memory offset (call before init if needed)
pub fn set_phys_mem_offset(offset: u64) {
    PHYS_MEM_OFFSET.store(offset, Ordering::Relaxed);
}

/// Check if Mode 13h is ready
pub fn is_ready() -> bool {
    VGA_MODE13H.lock().is_initialized()
}
