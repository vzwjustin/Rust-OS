//! SIOX bus subsystem (mirrors Linux `drivers/siox/`)
//!
//! Models a SIOX master driving a shift-register chain of devices. Each device
//! contributes a fixed number of input/output bytes that are clocked around
//! the loop on every `poll` cycle.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

#[derive(Clone)]
struct SioxDevice {
    position: u32,
    inbytes: u8,
    outbytes: u8,
    /// Output shadow written by the host, shifted out to the device.
    out: Vec<u8>,
    /// Input latched from the device on the last cycle.
    in_: Vec<u8>,
}

struct SioxMaster {
    id: u32,
    name: String,
    devices: Vec<SioxDevice>,
    cycles: u64,
}

// ── Registry ──────────────────────────────────────────────────────────────

static MASTERS: RwLock<BTreeMap<u32, SioxMaster>> = RwLock::new(BTreeMap::new());
static NEXT_ID: AtomicU32 = AtomicU32::new(0);

// ── Public API ──────────────────────────────────────────────────────────

pub fn register_master(name: &str) -> u32 {
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    MASTERS.write().insert(
        id,
        SioxMaster {
            id,
            name: String::from(name),
            devices: Vec::new(),
            cycles: 0,
        },
    );
    id
}

pub fn add_device(master_id: u32, inbytes: u8, outbytes: u8) -> Result<u32, &'static str> {
    let mut masters = MASTERS.write();
    let m = masters
        .get_mut(&master_id)
        .ok_or("siox: master not found")?;
    let position = m.devices.len() as u32;
    m.devices.push(SioxDevice {
        position,
        inbytes,
        outbytes,
        out: alloc::vec![0u8; outbytes as usize],
        in_: alloc::vec![0u8; inbytes as usize],
    });
    Ok(position)
}

pub fn set_out(master_id: u32, position: u32, data: &[u8]) -> Result<(), &'static str> {
    let mut masters = MASTERS.write();
    let m = masters
        .get_mut(&master_id)
        .ok_or("siox: master not found")?;
    let dev = m
        .devices
        .iter_mut()
        .find(|d| d.position == position)
        .ok_or("siox: device not found")?;
    if data.len() != dev.out.len() {
        return Err("siox: output length mismatch");
    }
    dev.out.copy_from_slice(data);
    Ok(())
}

pub fn get_in(master_id: u32, position: u32) -> Result<Vec<u8>, &'static str> {
    let masters = MASTERS.read();
    let m = masters.get(&master_id).ok_or("siox: master not found")?;
    let dev = m
        .devices
        .iter()
        .find(|d| d.position == position)
        .ok_or("siox: device not found")?;
    Ok(dev.in_.clone())
}

/// Clock one shift cycle around the chain. The software model loops each
/// device's output back into its own input (a wired-back test harness).
pub fn poll(master_id: u32) -> Result<(), &'static str> {
    let mut masters = MASTERS.write();
    let m = masters
        .get_mut(&master_id)
        .ok_or("siox: master not found")?;
    for dev in m.devices.iter_mut() {
        let n = core::cmp::min(dev.in_.len(), dev.out.len());
        dev.in_[..n].copy_from_slice(&dev.out[..n]);
    }
    m.cycles += 1;
    Ok(())
}

pub fn device_count(master_id: u32) -> usize {
    MASTERS
        .read()
        .get(&master_id)
        .map(|m| m.devices.len())
        .unwrap_or(0)
}

pub fn master_count() -> usize {
    MASTERS.read().len()
}

/// Initialize SIOX with a software master and a single 1-in/1-out device.
pub fn init() -> Result<(), &'static str> {
    if !MASTERS.read().is_empty() {
        return Ok(());
    }
    let m = register_master("siox-0");
    add_device(m, 1, 1)?;
    crate::serial_println!("siox: master siox-0, {} device(s)", device_count(m));
    Ok(())
}
