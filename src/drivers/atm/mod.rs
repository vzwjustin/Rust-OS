//! ATM (Asynchronous Transfer Mode) adapter subsystem
//! (mirrors Linux `drivers/atm/`)
//!
//! Registers ATM network adapters, manages virtual channel connections
//! (VPI/VCI), and transmits 53-byte cells through the adapter driver ops.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

pub const ATM_CELL_SIZE: usize = 53;
pub const ATM_CELL_PAYLOAD: usize = 48;

// ── Types ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AalType {
    Aal0,
    Aal5,
}

pub struct AtmDevOps {
    pub send: fn(vpi: u16, vci: u16, payload: &[u8]) -> Result<(), &'static str>,
    pub get_name: fn() -> &'static str,
}

#[derive(Clone)]
struct AtmVcc {
    vpi: u16,
    vci: u16,
    aal: AalType,
    tx_cells: u64,
}

struct AtmDevice {
    id: u32,
    name: String,
    link_rate_bps: u64,
    vccs: Vec<AtmVcc>,
    ops: AtmDevOps,
}

// ── Software loopback adapter ─────────────────────────────────────────────

fn loopback_send(_vpi: u16, _vci: u16, payload: &[u8]) -> Result<(), &'static str> {
    if payload.len() > ATM_CELL_PAYLOAD {
        return Err("atm: payload exceeds AAL0 cell");
    }
    Ok(())
}

const LOOPBACK_OPS: AtmDevOps = AtmDevOps {
    send: loopback_send,
    get_name: || "atm-loopback",
};

// ── Registry ──────────────────────────────────────────────────────────────

static ATM_DEVS: RwLock<BTreeMap<u32, AtmDevice>> = RwLock::new(BTreeMap::new());
static NEXT_ID: AtomicU32 = AtomicU32::new(0);

// ── Public API ──────────────────────────────────────────────────────────

pub fn register_device(
    name: &str,
    link_rate_bps: u64,
    ops: AtmDevOps,
) -> Result<u32, &'static str> {
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    ATM_DEVS.write().insert(
        id,
        AtmDevice {
            id,
            name: String::from(name),
            link_rate_bps,
            vccs: Vec::new(),
            ops,
        },
    );
    Ok(id)
}

pub fn open_vcc(dev_id: u32, vpi: u16, vci: u16, aal: AalType) -> Result<(), &'static str> {
    let mut devs = ATM_DEVS.write();
    let dev = devs.get_mut(&dev_id).ok_or("atm: device not found")?;
    if dev.vccs.iter().any(|v| v.vpi == vpi && v.vci == vci) {
        return Err("atm: VCC already open");
    }
    dev.vccs.push(AtmVcc {
        vpi,
        vci,
        aal,
        tx_cells: 0,
    });
    Ok(())
}

pub fn send(dev_id: u32, vpi: u16, vci: u16, payload: &[u8]) -> Result<(), &'static str> {
    let mut devs = ATM_DEVS.write();
    let dev = devs.get_mut(&dev_id).ok_or("atm: device not found")?;
    let vcc = dev
        .vccs
        .iter_mut()
        .find(|v| v.vpi == vpi && v.vci == vci)
        .ok_or("atm: VCC not open")?;
    (dev.ops.send)(vpi, vci, payload)?;
    vcc.tx_cells += 1;
    Ok(())
}

pub fn link_rate(dev_id: u32) -> Option<u64> {
    ATM_DEVS.read().get(&dev_id).map(|d| d.link_rate_bps)
}

pub fn device_count() -> usize {
    ATM_DEVS.read().len()
}

/// Initialize the ATM subsystem with a software loopback adapter.
pub fn init() -> Result<(), &'static str> {
    if !ATM_DEVS.read().is_empty() {
        return Ok(());
    }
    let dev = register_device("atm0", 155_520_000, LOOPBACK_OPS)?;
    open_vcc(dev, 0, 5, AalType::Aal5)?;
    crate::serial_println!("atm: {} adapter(s), signalling VCC 0/5 up", device_count());
    Ok(())
}
