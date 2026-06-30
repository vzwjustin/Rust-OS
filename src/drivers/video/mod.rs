//! Framebuffer console / fbdev-style consumer.
//!
//! Software-backed consumer of the DRM/KMS layer in `src/drivers/gpu/`. It
//! attaches to a DRM framebuffer (by its backing dumb buffer-object handle),
//! and renders pixels and an 8x8-font text console directly into that buffer's
//! allocation. Mirrors Linux's `drivers/video/fbdev` + `fbcon` relationship to
//! DRM (`drm_fb_helper`): the DRM layer owns the buffer, fbdev draws into it.
//!
//! This module is declared from `src/drivers/gpu/mod.rs` (via `#[path]`) because
//! `src/drivers/mod.rs` does not carry a `pub mod video;` declaration.

use core::sync::atomic::{AtomicBool, Ordering};
use spin::RwLock;

use crate::drivers::gpu::{with_buffer_object, DrmFourCc};

/// Packed 0x00RRGGBB color used by the XRGB8888/ARGB8888 console.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color(pub u32);

impl Color {
    pub const BLACK: Self = Color(0x0000_0000);
    pub const WHITE: Self = Color(0x00FF_FFFF);
    pub const RED: Self = Color(0x00FF_0000);
    pub const GREEN: Self = Color(0x0000_FF00);
    pub const BLUE: Self = Color(0x0000_00FF);
    pub const GRAY: Self = Color(0x0020_2024);
}

const FONT_W: u32 = 8;
const FONT_H: u32 = 8;

/// An fbdev console bound to a DRM framebuffer's backing buffer object.
pub struct FbConsole {
    pub device_id: u32,
    pub crtc_id: u32,
    pub fb_id: u32,
    pub bo_handle: u32,
    pub width: u32,
    pub height: u32,
    pub pitch: u32,
    pub format: DrmFourCc,
    /// Text cursor in character cells.
    pub col: u32,
    pub row: u32,
    pub fg: Color,
    pub bg: Color,
}

impl FbConsole {
    fn cols(&self) -> u32 {
        self.width / FONT_W
    }
    fn rows(&self) -> u32 {
        self.height / FONT_H
    }
}

static FBCON: RwLock<Option<FbConsole>> = RwLock::new(None);
static ATTACHED: AtomicBool = AtomicBool::new(false);

/// Write a single pixel into the bound framebuffer's backing store.
fn write_pixel(buf: &mut [u8], pitch: u32, x: u32, y: u32, color: Color) {
    let off = (y as usize) * (pitch as usize) + (x as usize) * 4;
    if off + 4 <= buf.len() {
        // Little-endian XRGB8888: byte order B, G, R, X.
        buf[off] = (color.0 & 0xFF) as u8;
        buf[off + 1] = ((color.0 >> 8) & 0xFF) as u8;
        buf[off + 2] = ((color.0 >> 16) & 0xFF) as u8;
        buf[off + 3] = 0xFF;
    }
}

/// Fill the whole framebuffer with one color.
pub fn clear(color: Color) -> Result<(), &'static str> {
    let guard = FBCON.read();
    let con = guard.as_ref().ok_or("fbcon: not attached")?;
    let (handle, pitch, w, h) = (con.bo_handle, con.pitch, con.width, con.height);
    with_buffer_object(handle, |buf| {
        for y in 0..h {
            for x in 0..w {
                write_pixel(buf, pitch, x, y, color);
            }
        }
    })?;
    Ok(())
}

/// Fill an axis-aligned rectangle (clipped to the framebuffer).
pub fn fill_rect(x: u32, y: u32, w: u32, h: u32, color: Color) -> Result<(), &'static str> {
    let guard = FBCON.read();
    let con = guard.as_ref().ok_or("fbcon: not attached")?;
    let (handle, pitch, fw, fh) = (con.bo_handle, con.pitch, con.width, con.height);
    let x1 = core::cmp::min(x + w, fw);
    let y1 = core::cmp::min(y + h, fh);
    with_buffer_object(handle, |buf| {
        for yy in y..y1 {
            for xx in x..x1 {
                write_pixel(buf, pitch, xx, yy, color);
            }
        }
    })?;
    Ok(())
}

/// Draw a diagnostic test pattern (four color quadrants + border).
pub fn draw_test_pattern() -> Result<(), &'static str> {
    let (w, h) = dimensions().ok_or("fbcon: not attached")?;
    let (hw, hh) = (w / 2, h / 2);
    fill_rect(0, 0, hw, hh, Color::RED)?;
    fill_rect(hw, 0, w - hw, hh, Color::GREEN)?;
    fill_rect(0, hh, hw, h - hh, Color::BLUE)?;
    fill_rect(hw, hh, w - hw, h - hh, Color::WHITE)?;
    Ok(())
}

/// Render one glyph at character cell (col,row).
fn draw_glyph(buf: &mut [u8], con: &FbConsole, col: u32, row: u32, ch: char) {
    let glyph = font8x8(ch);
    let ox = col * FONT_W;
    let oy = row * FONT_H;
    for (gy, bits) in glyph.iter().enumerate() {
        for gx in 0..FONT_W {
            let on = (bits >> (7 - gx)) & 1 != 0;
            let color = if on { con.fg } else { con.bg };
            write_pixel(buf, con.pitch, ox + gx, oy + gy as u32, color);
        }
    }
}

/// Print a string to the console with newline + wrap + scroll-by-clear.
pub fn print(s: &str) -> Result<(), &'static str> {
    let mut guard = FBCON.write();
    let con = guard.as_mut().ok_or("fbcon: not attached")?;
    let (cols, rows) = (con.cols(), con.rows());
    let handle = con.bo_handle;
    // Snapshot drawing parameters to avoid borrow conflicts inside the closure.
    let mut col = con.col;
    let mut row = con.row;
    let con_copy = FbConsole {
        device_id: con.device_id,
        crtc_id: con.crtc_id,
        fb_id: con.fb_id,
        bo_handle: con.bo_handle,
        width: con.width,
        height: con.height,
        pitch: con.pitch,
        format: con.format,
        col: con.col,
        row: con.row,
        fg: con.fg,
        bg: con.bg,
    };
    with_buffer_object(handle, |buf| {
        for ch in s.chars() {
            match ch {
                '\n' => {
                    col = 0;
                    row += 1;
                }
                '\r' => col = 0,
                _ => {
                    if col >= cols {
                        col = 0;
                        row += 1;
                    }
                    if row >= rows {
                        // Simple scroll: wrap to top (software console).
                        row = 0;
                    }
                    draw_glyph(buf, &con_copy, col, row, ch);
                    col += 1;
                }
            }
        }
    })?;
    con.col = col;
    con.row = row;
    Ok(())
}

/// Dimensions of the attached framebuffer in pixels.
pub fn dimensions() -> Option<(u32, u32)> {
    FBCON.read().as_ref().map(|c| (c.width, c.height))
}

/// True once an fbdev console is bound to a DRM framebuffer.
pub fn is_attached() -> bool {
    ATTACHED.load(Ordering::SeqCst)
}

/// Attach the fbdev console to a DRM framebuffer + its backing buffer object.
///
/// Called by `gpu::init()` after a modeset commit. Idempotent.
pub fn init_with_framebuffer(
    device_id: u32,
    crtc_id: u32,
    fb_id: u32,
    bo_handle: u32,
    width: u32,
    height: u32,
    pitch: u32,
) -> Result<(), &'static str> {
    if ATTACHED.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    let con = FbConsole {
        device_id,
        crtc_id,
        fb_id,
        bo_handle,
        width,
        height,
        pitch,
        format: DrmFourCc::XRGB8888,
        col: 0,
        row: 0,
        fg: Color::WHITE,
        bg: Color::GRAY,
    };
    let (cols, rows) = (con.cols(), con.rows());
    *FBCON.write() = Some(con);

    clear(Color::GRAY)?;
    print("RustOS fbdev console\n")?;
    crate::serial_println!(
        "video: fbcon attached fb={} bo={} {}x{} ({}x{} cells)",
        fb_id,
        bo_handle,
        width,
        height,
        cols,
        rows
    );
    Ok(())
}

/// Idempotent standalone init. If `gpu::init()` already attached a console this
/// is a no-op; otherwise it simply reports that no scanout target is bound yet.
pub fn init() -> Result<(), &'static str> {
    if is_attached() {
        return Ok(());
    }
    crate::serial_println!("video: fbdev ready (awaiting DRM framebuffer)");
    Ok(())
}

// ── Minimal 8x8 bitmap font ──────────────────────────────────────────────
//
// Each glyph is 8 rows; the high bit of each byte is the leftmost pixel.
// Only the characters used by the boot console are defined; everything else
// renders blank. Lowercase maps to uppercase.

fn font8x8(ch: char) -> [u8; 8] {
    let c = if ch.is_ascii_lowercase() {
        ch.to_ascii_uppercase()
    } else {
        ch
    };
    match c {
        ' ' => [0; 8],
        'A' => [0x18, 0x24, 0x42, 0x42, 0x7E, 0x42, 0x42, 0x00],
        'B' => [0x7C, 0x42, 0x42, 0x7C, 0x42, 0x42, 0x7C, 0x00],
        'C' => [0x3C, 0x42, 0x40, 0x40, 0x40, 0x42, 0x3C, 0x00],
        'D' => [0x78, 0x44, 0x42, 0x42, 0x42, 0x44, 0x78, 0x00],
        'E' => [0x7E, 0x40, 0x40, 0x7C, 0x40, 0x40, 0x7E, 0x00],
        'F' => [0x7E, 0x40, 0x40, 0x7C, 0x40, 0x40, 0x40, 0x00],
        'G' => [0x3C, 0x42, 0x40, 0x4E, 0x42, 0x42, 0x3C, 0x00],
        'H' => [0x42, 0x42, 0x42, 0x7E, 0x42, 0x42, 0x42, 0x00],
        'I' => [0x3E, 0x08, 0x08, 0x08, 0x08, 0x08, 0x3E, 0x00],
        'J' => [0x1E, 0x04, 0x04, 0x04, 0x44, 0x44, 0x38, 0x00],
        'K' => [0x42, 0x44, 0x48, 0x70, 0x48, 0x44, 0x42, 0x00],
        'L' => [0x40, 0x40, 0x40, 0x40, 0x40, 0x40, 0x7E, 0x00],
        'M' => [0x42, 0x66, 0x5A, 0x5A, 0x42, 0x42, 0x42, 0x00],
        'N' => [0x42, 0x62, 0x52, 0x4A, 0x46, 0x42, 0x42, 0x00],
        'O' => [0x3C, 0x42, 0x42, 0x42, 0x42, 0x42, 0x3C, 0x00],
        'P' => [0x7C, 0x42, 0x42, 0x7C, 0x40, 0x40, 0x40, 0x00],
        'Q' => [0x3C, 0x42, 0x42, 0x42, 0x4A, 0x44, 0x3A, 0x00],
        'R' => [0x7C, 0x42, 0x42, 0x7C, 0x48, 0x44, 0x42, 0x00],
        'S' => [0x3C, 0x42, 0x40, 0x3C, 0x02, 0x42, 0x3C, 0x00],
        'T' => [0x7F, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x00],
        'U' => [0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x3C, 0x00],
        'V' => [0x42, 0x42, 0x42, 0x42, 0x42, 0x24, 0x18, 0x00],
        'W' => [0x42, 0x42, 0x42, 0x5A, 0x5A, 0x66, 0x42, 0x00],
        'X' => [0x42, 0x24, 0x18, 0x18, 0x18, 0x24, 0x42, 0x00],
        'Y' => [0x41, 0x22, 0x14, 0x08, 0x08, 0x08, 0x08, 0x00],
        'Z' => [0x7E, 0x04, 0x08, 0x10, 0x20, 0x40, 0x7E, 0x00],
        '0' => [0x3C, 0x46, 0x4A, 0x52, 0x62, 0x42, 0x3C, 0x00],
        '1' => [0x08, 0x18, 0x28, 0x08, 0x08, 0x08, 0x3E, 0x00],
        '2' => [0x3C, 0x42, 0x02, 0x0C, 0x30, 0x40, 0x7E, 0x00],
        '3' => [0x3C, 0x42, 0x02, 0x1C, 0x02, 0x42, 0x3C, 0x00],
        '4' => [0x04, 0x0C, 0x14, 0x24, 0x7E, 0x04, 0x04, 0x00],
        '5' => [0x7E, 0x40, 0x7C, 0x02, 0x02, 0x42, 0x3C, 0x00],
        '6' => [0x1C, 0x20, 0x40, 0x7C, 0x42, 0x42, 0x3C, 0x00],
        '7' => [0x7E, 0x02, 0x04, 0x08, 0x10, 0x20, 0x20, 0x00],
        '8' => [0x3C, 0x42, 0x42, 0x3C, 0x42, 0x42, 0x3C, 0x00],
        '9' => [0x3C, 0x42, 0x42, 0x3E, 0x02, 0x04, 0x38, 0x00],
        '.' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x00],
        ',' => [0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x08, 0x10],
        ':' => [0x00, 0x18, 0x18, 0x00, 0x18, 0x18, 0x00, 0x00],
        '-' => [0x00, 0x00, 0x00, 0x7E, 0x00, 0x00, 0x00, 0x00],
        '/' => [0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x80, 0x00],
        '@' => [0x3C, 0x42, 0x5A, 0x56, 0x5E, 0x40, 0x3C, 0x00],
        '(' => [0x0C, 0x10, 0x20, 0x20, 0x20, 0x10, 0x0C, 0x00],
        ')' => [0x30, 0x08, 0x04, 0x04, 0x04, 0x08, 0x30, 0x00],
        '!' => [0x18, 0x18, 0x18, 0x18, 0x00, 0x18, 0x18, 0x00],
        '#' => [0x24, 0x24, 0x7E, 0x24, 0x7E, 0x24, 0x24, 0x00],
        '+' => [0x00, 0x08, 0x08, 0x3E, 0x08, 0x08, 0x00, 0x00],
        '=' => [0x00, 0x00, 0x7E, 0x00, 0x7E, 0x00, 0x00, 0x00],
        _ => [0x00, 0x7E, 0x42, 0x42, 0x42, 0x7E, 0x00, 0x00], // unknown: hollow box
    }
}
