//! Auxiliary display subsystem
//!
//! Provides framework for auxiliary/character LCD displays and panels.
//! Mirrors Linux's `drivers/auxdisplay/`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// Auxiliary display device.
pub struct AuxDisplay {
    pub id: u32,
    pub name: String,
    pub display_type: AuxDisplayType,
    pub width: u32,
    pub height: u32,
    pub char_width: u32,
    pub char_height: u32,
    pub state: AuxDisplayState,
    pub ops: AuxDisplayOps,
    pub cursor_x: u32,
    pub cursor_y: u32,
    pub buffer: Vec<u8>,
    pub backlight: bool,
    pub contrast: u8,
}

/// Auxiliary display type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuxDisplayType {
    CharLcd,
    GraphicLcd,
    Vfd,
    LedMatrix,
    Oled,
    EInk,
}

/// Auxiliary display state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuxDisplayState {
    Unregistered,
    Registered,
    Initialized,
    On,
    Off,
    Suspended,
}

/// Auxiliary display operations.
pub struct AuxDisplayOps {
    pub init: fn(dev_id: u32) -> Result<(), &'static str>,
    pub clear: fn(dev_id: u32) -> Result<(), &'static str>,
    pub home: fn(dev_id: u32) -> Result<(), &'static str>,
    pub set_cursor: fn(dev_id: u32, x: u32, y: u32) -> Result<(), &'static str>,
    pub write_char: fn(dev_id: u32, ch: u8) -> Result<(), &'static str>,
    pub write_string: fn(dev_id: u32, s: &str) -> Result<usize, &'static str>,
    pub set_backlight: fn(dev_id: u32, on: bool) -> Result<(), &'static str>,
    pub set_contrast: fn(dev_id: u32, val: u8) -> Result<(), &'static str>,
    pub scroll: fn(dev_id: u32, dir: ScrollDir, count: u32) -> Result<(), &'static str>,
    pub flush: fn(dev_id: u32) -> Result<(), &'static str>,
    pub suspend: fn(dev_id: u32) -> Result<(), &'static str>,
    pub resume: fn(dev_id: u32) -> Result<(), &'static str>,
}

/// Scroll direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollDir {
    Left,
    Right,
    Up,
    Down,
}

// ── Registry ────────────────────────────────────────────────────────────

static DEV_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static AUX_DISPLAYS: RwLock<BTreeMap<u32, AuxDisplay>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register an auxiliary display.
pub fn register_device(
    name: &str,
    display_type: AuxDisplayType,
    width: u32,
    height: u32,
    ops: AuxDisplayOps,
) -> Result<u32, &'static str> {
    let id = DEV_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let buf_size = (width * height) as usize;
    let dev = AuxDisplay {
        id,
        name: String::from(name),
        display_type,
        width,
        height,
        char_width: if display_type == AuxDisplayType::CharLcd {
            8
        } else {
            6
        },
        char_height: if display_type == AuxDisplayType::CharLcd {
            8
        } else {
            8
        },
        state: AuxDisplayState::Registered,
        ops,
        cursor_x: 0,
        cursor_y: 0,
        buffer: alloc::vec![0u8; buf_size],
        backlight: true,
        contrast: 128,
    };
    AUX_DISPLAYS.write().insert(id, dev);
    Ok(id)
}

/// Initialize a display.
pub fn init_display(dev_id: u32) -> Result<(), &'static str> {
    let init_fn = {
        let displays = AUX_DISPLAYS.read();
        let dev = displays.get(&dev_id).ok_or("AuxDisplay not found")?;
        dev.ops.init
    };
    (init_fn)(dev_id)?;

    let mut displays = AUX_DISPLAYS.write();
    if let Some(dev) = displays.get_mut(&dev_id) {
        dev.state = AuxDisplayState::Initialized;
    }
    Ok(())
}

/// Clear the display.
pub fn clear(dev_id: u32) -> Result<(), &'static str> {
    let clear_fn = {
        let displays = AUX_DISPLAYS.read();
        let dev = displays.get(&dev_id).ok_or("AuxDisplay not found")?;
        dev.ops.clear
    };
    (clear_fn)(dev_id)?;

    let mut displays = AUX_DISPLAYS.write();
    if let Some(dev) = displays.get_mut(&dev_id) {
        dev.buffer.fill(0x20); // Space
        dev.cursor_x = 0;
        dev.cursor_y = 0;
    }
    Ok(())
}

/// Set cursor position.
pub fn set_cursor(dev_id: u32, x: u32, y: u32) -> Result<(), &'static str> {
    let set_fn = {
        let displays = AUX_DISPLAYS.read();
        let dev = displays.get(&dev_id).ok_or("AuxDisplay not found")?;
        if x >= dev.width || y >= dev.height {
            return Err("Cursor out of bounds");
        }
        dev.ops.set_cursor
    };
    (set_fn)(dev_id, x, y)?;

    let mut displays = AUX_DISPLAYS.write();
    if let Some(dev) = displays.get_mut(&dev_id) {
        dev.cursor_x = x;
        dev.cursor_y = y;
    }
    Ok(())
}

/// Write a string to the display.
pub fn write_string(dev_id: u32, s: &str) -> Result<usize, &'static str> {
    let (write_fn, width, height, cx, cy) = {
        let displays = AUX_DISPLAYS.read();
        let dev = displays.get(&dev_id).ok_or("AuxDisplay not found")?;
        (
            dev.ops.write_string,
            dev.width,
            dev.height,
            dev.cursor_x,
            dev.cursor_y,
        )
    };
    let n = (write_fn)(dev_id, s)?;

    // Update buffer and cursor
    let mut displays = AUX_DISPLAYS.write();
    if let Some(dev) = displays.get_mut(&dev_id) {
        let mut x = cx;
        let mut y = cy;
        for &b in s.as_bytes() {
            if x < width && y < height {
                let idx = (y * width + x) as usize;
                if idx < dev.buffer.len() {
                    dev.buffer[idx] = b;
                }
                x += 1;
            }
            if x >= width {
                x = 0;
                y += 1;
                if y >= height {
                    y = 0;
                }
            }
        }
        dev.cursor_x = x;
        dev.cursor_y = y;
    }
    Ok(n)
}

/// Set backlight on/off.
pub fn set_backlight(dev_id: u32, on: bool) -> Result<(), &'static str> {
    let set_fn = {
        let displays = AUX_DISPLAYS.read();
        let dev = displays.get(&dev_id).ok_or("AuxDisplay not found")?;
        dev.ops.set_backlight
    };
    (set_fn)(dev_id, on)?;

    let mut displays = AUX_DISPLAYS.write();
    if let Some(dev) = displays.get_mut(&dev_id) {
        dev.backlight = on;
    }
    Ok(())
}

/// Set contrast.
pub fn set_contrast(dev_id: u32, val: u8) -> Result<(), &'static str> {
    let set_fn = {
        let displays = AUX_DISPLAYS.read();
        let dev = displays.get(&dev_id).ok_or("AuxDisplay not found")?;
        dev.ops.set_contrast
    };
    (set_fn)(dev_id, val)?;

    let mut displays = AUX_DISPLAYS.write();
    if let Some(dev) = displays.get_mut(&dev_id) {
        dev.contrast = val;
    }
    Ok(())
}

/// Scroll the display.
pub fn scroll(dev_id: u32, dir: ScrollDir, count: u32) -> Result<(), &'static str> {
    let scroll_fn = {
        let displays = AUX_DISPLAYS.read();
        let dev = displays.get(&dev_id).ok_or("AuxDisplay not found")?;
        dev.ops.scroll
    };
    (scroll_fn)(dev_id, dir, count)
}

/// Suspend the display.
pub fn suspend(dev_id: u32) -> Result<(), &'static str> {
    let suspend_fn = {
        let displays = AUX_DISPLAYS.read();
        let dev = displays.get(&dev_id).ok_or("AuxDisplay not found")?;
        dev.ops.suspend
    };
    (suspend_fn)(dev_id)?;

    let mut displays = AUX_DISPLAYS.write();
    if let Some(dev) = displays.get_mut(&dev_id) {
        dev.state = AuxDisplayState::Suspended;
    }
    Ok(())
}

/// Resume the display.
pub fn resume(dev_id: u32) -> Result<(), &'static str> {
    let resume_fn = {
        let displays = AUX_DISPLAYS.read();
        let dev = displays.get(&dev_id).ok_or("AuxDisplay not found")?;
        dev.ops.resume
    };
    (resume_fn)(dev_id)?;

    let mut displays = AUX_DISPLAYS.write();
    if let Some(dev) = displays.get_mut(&dev_id) {
        dev.state = AuxDisplayState::On;
    }
    Ok(())
}

/// List all displays.
pub fn list_displays() -> Vec<(u32, String, AuxDisplayType, u32, u32, AuxDisplayState)> {
    AUX_DISPLAYS
        .read()
        .iter()
        .map(|(id, d)| {
            (
                *id,
                d.name.clone(),
                d.display_type,
                d.width,
                d.height,
                d.state,
            )
        })
        .collect()
}

/// Count registered displays.
pub fn display_count() -> usize {
    AUX_DISPLAYS.read().len()
}

// ── Software auxdisplay ─────────────────────────────────────────────────

fn sw_init(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_clear(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_home(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_set_cursor(_dev_id: u32, _x: u32, _y: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_write_char(_dev_id: u32, _ch: u8) -> Result<(), &'static str> {
    Ok(())
}
fn sw_write_string(_dev_id: u32, s: &str) -> Result<usize, &'static str> {
    Ok(s.len())
}
fn sw_set_backlight(_dev_id: u32, _on: bool) -> Result<(), &'static str> {
    Ok(())
}
fn sw_set_contrast(_dev_id: u32, _val: u8) -> Result<(), &'static str> {
    Ok(())
}
fn sw_scroll(_dev_id: u32, _dir: ScrollDir, _count: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_flush(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_suspend(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_resume(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}

/// Software auxdisplay ops.
pub fn software_auxdisplay_ops() -> AuxDisplayOps {
    AuxDisplayOps {
        init: sw_init,
        clear: sw_clear,
        home: sw_home,
        set_cursor: sw_set_cursor,
        write_char: sw_write_char,
        write_string: sw_write_string,
        set_backlight: sw_set_backlight,
        set_contrast: sw_set_contrast,
        scroll: sw_scroll,
        flush: sw_flush,
        suspend: sw_suspend,
        resume: sw_resume,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    if !AUX_DISPLAYS.read().is_empty() {
        return Ok(());
    }

    let ops = software_auxdisplay_ops();
    let dev_id = register_device("sw-charlcd", AuxDisplayType::CharLcd, 16, 2, ops)?;
    crate::serial_println!("auxdisplay: 16x2 char LCD registered (id={})", dev_id);
    Ok(())
}
