//! FSI (FRU Support Interface) bus subsystem (mirrors Linux `drivers/fsi/`)
//!
//! Models FSI masters driving a daisy chain of slaves, each exposing engines
//! at CFAM addresses. Supports 8/16/32-bit reads and writes into a slave's
//! flat address space.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

struct FsiSlave {
    link: u8,
    id: u8,
    /// Flat CFAM address space (sparse-backed for the model).
    mem: BTreeMap<u32, u8>,
}

struct FsiMaster {
    id: u32,
    name: String,
    slaves: Vec<FsiSlave>,
}

// ── Registry ──────────────────────────────────────────────────────────────

static MASTERS: RwLock<BTreeMap<u32, FsiMaster>> = RwLock::new(BTreeMap::new());
static NEXT_ID: AtomicU32 = AtomicU32::new(0);

// ── Public API ──────────────────────────────────────────────────────────

pub fn register_master(name: &str) -> u32 {
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    MASTERS.write().insert(
        id,
        FsiMaster {
            id,
            name: String::from(name),
            slaves: Vec::new(),
        },
    );
    id
}

pub fn add_slave(master_id: u32, link: u8, slave_id: u8) -> Result<(), &'static str> {
    let mut masters = MASTERS.write();
    let m = masters.get_mut(&master_id).ok_or("fsi: master not found")?;
    if m.slaves.iter().any(|s| s.link == link && s.id == slave_id) {
        return Err("fsi: slave already present");
    }
    m.slaves.push(FsiSlave {
        link,
        id: slave_id,
        mem: BTreeMap::new(),
    });
    Ok(())
}

fn slave_mut<'a>(
    m: &'a mut FsiMaster,
    link: u8,
    slave_id: u8,
) -> Result<&'a mut FsiSlave, &'static str> {
    m.slaves
        .iter_mut()
        .find(|s| s.link == link && s.id == slave_id)
        .ok_or("fsi: slave not found")
}

pub fn write(
    master_id: u32,
    link: u8,
    slave_id: u8,
    addr: u32,
    data: &[u8],
) -> Result<(), &'static str> {
    let mut masters = MASTERS.write();
    let m = masters.get_mut(&master_id).ok_or("fsi: master not found")?;
    let s = slave_mut(m, link, slave_id)?;
    for (i, &b) in data.iter().enumerate() {
        s.mem.insert(addr + i as u32, b);
    }
    Ok(())
}

pub fn read(
    master_id: u32,
    link: u8,
    slave_id: u8,
    addr: u32,
    len: usize,
) -> Result<Vec<u8>, &'static str> {
    let masters = MASTERS.read();
    let m = masters.get(&master_id).ok_or("fsi: master not found")?;
    let s = m
        .slaves
        .iter()
        .find(|s| s.link == link && s.id == slave_id)
        .ok_or("fsi: slave not found")?;
    Ok((0..len)
        .map(|i| *s.mem.get(&(addr + i as u32)).unwrap_or(&0))
        .collect())
}

pub fn slave_count(master_id: u32) -> usize {
    MASTERS
        .read()
        .get(&master_id)
        .map(|m| m.slaves.len())
        .unwrap_or(0)
}

pub fn master_count() -> usize {
    MASTERS.read().len()
}

/// Initialize FSI with a software master and one slave on link 0.
pub fn init() -> Result<(), &'static str> {
    if !MASTERS.read().is_empty() {
        return Ok(());
    }
    let m = register_master("fsi-master0");
    add_slave(m, 0, 0)?;
    write(m, 0, 0, 0x0, &[0xc0, 0x02, 0x00, 0x00])?; // CFAM config word
    crate::serial_println!("fsi: master0 with {} slave(s)", slave_count(m));
    Ok(())
}
