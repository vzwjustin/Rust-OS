//! GPIB (IEEE-488) instrument bus subsystem (mirrors Linux `drivers/gpib/`)
//!
//! Registers GPIB interface boards and the instruments addressed on them,
//! routing addressed read/write traffic through the board controller ops.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

pub const GPIB_MAX_ADDR: u8 = 30;

// ── Types ───────────────────────────────────────────────────────────────

pub struct GpibBoardOps {
    /// Address `pad`, then write the command/data bytes.
    pub write: fn(pad: u8, data: &[u8]) -> Result<(), &'static str>,
    /// Address `pad`, then read up to `len` bytes.
    pub read: fn(pad: u8, len: usize) -> Result<Vec<u8>, &'static str>,
    pub get_name: fn() -> &'static str,
}

#[derive(Clone)]
struct Instrument {
    pad: u8,
    name: String,
}

struct GpibBoard {
    id: u32,
    name: String,
    /// Primary address of the controller-in-charge (usually 0).
    controller_pad: u8,
    instruments: Vec<Instrument>,
    ops: GpibBoardOps,
}

// ── Software loopback board ───────────────────────────────────────────────

fn loopback_write(_pad: u8, _data: &[u8]) -> Result<(), &'static str> {
    Ok(())
}

fn loopback_read(_pad: u8, len: usize) -> Result<Vec<u8>, &'static str> {
    // Emulate a "*IDN?" style response.
    let resp = b"RUSTOS,VIRT-INSTR,0,1.0\n";
    Ok(resp.iter().take(len).copied().collect())
}

const LOOPBACK_OPS: GpibBoardOps = GpibBoardOps {
    write: loopback_write,
    read: loopback_read,
    get_name: || "gpib-loopback",
};

// ── Registry ──────────────────────────────────────────────────────────────

static BOARDS: RwLock<BTreeMap<u32, GpibBoard>> = RwLock::new(BTreeMap::new());
static NEXT_ID: AtomicU32 = AtomicU32::new(0);

// ── Public API ──────────────────────────────────────────────────────────

pub fn register_board(name: &str, controller_pad: u8, ops: GpibBoardOps) -> u32 {
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    BOARDS.write().insert(
        id,
        GpibBoard {
            id,
            name: String::from(name),
            controller_pad,
            instruments: Vec::new(),
            ops,
        },
    );
    id
}

pub fn add_instrument(board_id: u32, pad: u8, name: &str) -> Result<(), &'static str> {
    if pad > GPIB_MAX_ADDR {
        return Err("gpib: address out of range");
    }
    let mut boards = BOARDS.write();
    let b = boards.get_mut(&board_id).ok_or("gpib: board not found")?;
    if b.instruments.iter().any(|i| i.pad == pad) {
        return Err("gpib: address already in use");
    }
    b.instruments.push(Instrument {
        pad,
        name: String::from(name),
    });
    Ok(())
}

pub fn write(board_id: u32, pad: u8, data: &[u8]) -> Result<(), &'static str> {
    let boards = BOARDS.read();
    let b = boards.get(&board_id).ok_or("gpib: board not found")?;
    if !b.instruments.iter().any(|i| i.pad == pad) {
        return Err("gpib: no instrument at address");
    }
    (b.ops.write)(pad, data)
}

pub fn read(board_id: u32, pad: u8, len: usize) -> Result<Vec<u8>, &'static str> {
    let boards = BOARDS.read();
    let b = boards.get(&board_id).ok_or("gpib: board not found")?;
    if !b.instruments.iter().any(|i| i.pad == pad) {
        return Err("gpib: no instrument at address");
    }
    (b.ops.read)(pad, len)
}

pub fn controller_pad(board_id: u32) -> Option<u8> {
    BOARDS.read().get(&board_id).map(|b| b.controller_pad)
}

pub fn instrument_count(board_id: u32) -> usize {
    BOARDS
        .read()
        .get(&board_id)
        .map(|b| b.instruments.len())
        .unwrap_or(0)
}

pub fn board_count() -> usize {
    BOARDS.read().len()
}

/// Initialize GPIB with a software loopback board and one virtual instrument.
pub fn init() -> Result<(), &'static str> {
    if !BOARDS.read().is_empty() {
        return Ok(());
    }
    let b = register_board("gpib0", 0, LOOPBACK_OPS);
    add_instrument(b, 1, "virt-instr")?;
    crate::serial_println!("gpib: board gpib0, {} instrument(s)", instrument_count(b));
    Ok(())
}
