//! ATA (Advanced Technology Attachment) driver subsystem
//!
//! Provides a framework for ATA/ATAPI/SATA controllers and devices.
//! Mirrors Linux's `drivers/ata/`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// ATA device class (Linux `enum ata_device_class`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AtaDeviceClass {
    Pata,
    Sata,
    Atapi,
    Satapi,
}

/// ATA device state (Linux `enum ata_device_state`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AtaDeviceState {
    Idle,
    Busy,
    Ready,
    Error,
    Offline,
}

/// ATA transfer mode (Linux `enum ata_xfer_cls`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AtaXferMode {
    Pio(u32),
    Mwdma(u32),
    Udma(u32),
}

/// ATA device (Linux `struct ata_device`).
pub struct AtaDevice {
    pub id: u32,
    pub port_id: u32,
    pub class: AtaDeviceClass,
    pub model: String,
    pub serial: String,
    pub firmware: String,
    pub state: AtaDeviceState,
    pub sectors: u64,
    pub sector_size: u32,
    pub xfer_mode: AtaXferMode,
    pub lba48: bool,
    pub ncq: bool,
}

/// ATA port (Linux `struct ata_port`).
pub struct AtaPort {
    pub id: u32,
    pub name: String,
    pub ops: AtaPortOps,
    pub device_ids: Vec<u32>,
    pub flags: u32,
}

/// ATA port operations (Linux `struct ata_port_ops`).
pub struct AtaPortOps {
    pub init: fn(port_id: u32) -> Result<(), &'static str>,
    pub softreset: fn(port_id: u32) -> Result<(), &'static str>,
    pub hardreset: fn(port_id: u32) -> Result<(), &'static str>,
    pub read:
        fn(port_id: u32, dev_id: u32, lba: u64, buf: &mut [u8]) -> Result<usize, &'static str>,
    pub write: fn(port_id: u32, dev_id: u32, lba: u64, buf: &[u8]) -> Result<usize, &'static str>,
    pub set_mode: fn(port_id: u32, dev_id: u32, mode: AtaXferMode) -> Result<(), &'static str>,
}

// ── Registry ────────────────────────────────────────────────────────────

static PORT_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DEV_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static ATA_PORTS: RwLock<BTreeMap<u32, AtaPort>> = RwLock::new(BTreeMap::new());
static ATA_DEVS: RwLock<BTreeMap<u32, AtaDevice>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register an ATA port (controller).
pub fn register_port(name: &str, ops: AtaPortOps, flags: u32) -> Result<u32, &'static str> {
    let id = PORT_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let port = AtaPort {
        id,
        name: String::from(name),
        ops,
        device_ids: Vec::new(),
        flags,
    };
    ATA_PORTS.write().insert(id, port);
    Ok(id)
}

/// Register an ATA device on a port.
pub fn register_device(
    port_id: u32,
    class: AtaDeviceClass,
    model: &str,
    serial: &str,
    firmware: &str,
    sectors: u64,
    sector_size: u32,
    lba48: bool,
    ncq: bool,
) -> Result<u32, &'static str> {
    if !ATA_PORTS.read().contains_key(&port_id) {
        return Err("ATA port not found");
    }
    let id = DEV_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dev = AtaDevice {
        id,
        port_id,
        class,
        model: String::from(model),
        serial: String::from(serial),
        firmware: String::from(firmware),
        state: AtaDeviceState::Idle,
        sectors,
        sector_size,
        xfer_mode: AtaXferMode::Pio(0),
        lba48,
        ncq,
    };
    ATA_DEVS.write().insert(id, dev);
    let mut ports = ATA_PORTS.write();
    if let Some(port) = ports.get_mut(&port_id) {
        port.device_ids.push(id);
    }
    Ok(id)
}

/// Read sectors from an ATA device.
pub fn read_sectors(dev_id: u32, lba: u64, buf: &mut [u8]) -> Result<usize, &'static str> {
    let (port_id, read_fn) = {
        let devs = ATA_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("ATA device not found")?;
        let ports = ATA_PORTS.read();
        let port = ports.get(&dev.port_id).ok_or("ATA port not found")?;
        (dev.port_id, port.ops.read)
    };
    (read_fn)(port_id, dev_id, lba, buf)
}

/// Write sectors to an ATA device.
pub fn write_sectors(dev_id: u32, lba: u64, buf: &[u8]) -> Result<usize, &'static str> {
    let (port_id, write_fn) = {
        let devs = ATA_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("ATA device not found")?;
        let ports = ATA_PORTS.read();
        let port = ports.get(&dev.port_id).ok_or("ATA port not found")?;
        (dev.port_id, port.ops.write)
    };
    (write_fn)(port_id, dev_id, lba, buf)
}

/// Set transfer mode on an ATA device.
pub fn set_xfer_mode(dev_id: u32, mode: AtaXferMode) -> Result<(), &'static str> {
    let (port_id, set_mode_fn) = {
        let devs = ATA_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("ATA device not found")?;
        let ports = ATA_PORTS.read();
        let port = ports.get(&dev.port_id).ok_or("ATA port not found")?;
        (dev.port_id, port.ops.set_mode)
    };
    (set_mode_fn)(port_id, dev_id, mode)?;
    let mut devs = ATA_DEVS.write();
    if let Some(dev) = devs.get_mut(&dev_id) {
        dev.xfer_mode = mode;
    }
    Ok(())
}

/// List all ATA ports.
pub fn list_ports() -> Vec<(u32, String, usize)> {
    ATA_PORTS
        .read()
        .iter()
        .map(|(id, p)| (*id, p.name.clone(), p.device_ids.len()))
        .collect()
}

/// List all ATA devices.
pub fn list_devices() -> Vec<(u32, u32, AtaDeviceClass, String, u64)> {
    ATA_DEVS
        .read()
        .iter()
        .map(|(id, d)| (*id, d.port_id, d.class, d.model.clone(), d.sectors))
        .collect()
}

/// Count registered devices.
pub fn device_count() -> usize {
    ATA_DEVS.read().len()
}

// ── Software ATA helpers ────────────────────────────────────────────────
// These operations are kept for unit-level wiring tests only.  The ATA boot
// path must not publish a synthetic disk as if it were real hardware.

fn sw_init(_port_id: u32) -> Result<(), &'static str> {
    Ok(())
}

fn sw_softreset(_port_id: u32) -> Result<(), &'static str> {
    Ok(())
}

fn sw_hardreset(_port_id: u32) -> Result<(), &'static str> {
    Ok(())
}

fn sw_read(_port_id: u32, _dev_id: u32, _lba: u64, buf: &mut [u8]) -> Result<usize, &'static str> {
    for b in buf.iter_mut() {
        *b = 0;
    }
    Ok(buf.len())
}

fn sw_write(_port_id: u32, _dev_id: u32, _lba: u64, _buf: &[u8]) -> Result<usize, &'static str> {
    Ok(_buf.len())
}

fn sw_set_mode(_port_id: u32, _dev_id: u32, _mode: AtaXferMode) -> Result<(), &'static str> {
    Ok(())
}

/// Software ATA port ops.
pub fn software_ata_ops() -> AtaPortOps {
    AtaPortOps {
        init: sw_init,
        softreset: sw_softreset,
        hardreset: sw_hardreset,
        read: sw_read,
        write: sw_write,
        set_mode: sw_set_mode,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("ata: subsystem ready ({} hardware port(s))", ATA_PORTS.read().len());
    Ok(())
}
