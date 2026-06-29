//! Parallel port subsystem
//!
//! Provides parallel port framework for printer, scanner, and other
//! IEEE 1284 parallel port devices.
//! Mirrors Linux's `drivers/parport/`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// Parallel port (Linux `struct parport`).
pub struct Parport {
    pub id: u32,
    pub name: String,
    pub base: u64,
    pub size: u64,
    pub irq: u32,
    pub dma: u32,
    pub modes: ParportMode,
    pub ops: ParportOps,
    pub state: ParportState,
    pub device_ids: Vec<u32>,
    pub cad_id: Option<u32>,
}

/// Parallel port mode (Linux `enum parport_mode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParportMode {
    pub spp: bool,
    pub ps2: bool,
    pub epp: bool,
    pub ecp: bool,
    pub dma: bool,
    pub pcps2: bool,
}

impl Default for ParportMode {
    fn default() -> Self {
        Self {
            spp: true,
            ps2: true,
            epp: false,
            ecp: false,
            dma: false,
            pcps2: false,
        }
    }
}

/// Parallel port state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParportState {
    Unregistered,
    Registered,
    Claimed,
    Released,
}

/// Parallel port operations (Linux `struct parport_operations`).
pub struct ParportOps {
    pub write_data: fn(port_id: u32, data: u8) -> Result<(), &'static str>,
    pub read_data: fn(port_id: u32) -> Result<u8, &'static str>,
    pub write_control: fn(port_id: u32, ctrl: u8) -> Result<(), &'static str>,
    pub read_control: fn(port_id: u32) -> Result<u8, &'static str>,
    pub read_status: fn(port_id: u32) -> Result<u8, &'static str>,
    pub enable_irq: fn(port_id: u32) -> Result<(), &'static str>,
    pub disable_irq: fn(port_id: u32) -> Result<(), &'static str>,
    pub data_forward: fn(port_id: u32) -> Result<(), &'static str>,
    pub data_reverse: fn(port_id: u32) -> Result<(), &'static str>,
    pub epp_write_data: fn(port_id: u32, data: &[u8]) -> Result<usize, &'static str>,
    pub epp_read_data: fn(port_id: u32, buf: &mut [u8]) -> Result<usize, &'static str>,
    pub ecp_write: fn(port_id: u32, data: &[u8]) -> Result<usize, &'static str>,
    pub ecp_read: fn(port_id: u32, buf: &mut [u8]) -> Result<usize, &'static str>,
}

/// Parallel port device/driver (Linux `struct parport_driver` + `struct pardevice`).
pub struct ParportDriver {
    pub id: u32,
    pub name: String,
    pub attach: fn(port_id: u32) -> Result<u32, &'static str>,
    pub detach: fn(dev_id: u32) -> Result<(), &'static str>,
}

/// Parallel port device (Linux `struct pardevice`).
pub struct Pardevice {
    pub id: u32,
    pub port_id: u32,
    pub drv_id: u32,
    pub name: String,
    pub state: PardevState,
    pub preempt: bool,
    pub exclusive: bool,
    pub mode: ParportMode,
}

/// Parallel device state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PardevState {
    Unregistered,
    Registered,
    Claimed,
    Waiting,
    Released,
}

// ── Registry ────────────────────────────────────────────────────────────

static PORT_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DRV_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DEV_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static PARPORTS: RwLock<BTreeMap<u32, Parport>> = RwLock::new(BTreeMap::new());
static PARPORT_DRVS: RwLock<BTreeMap<u32, ParportDriver>> = RwLock::new(BTreeMap::new());
static PARDEVS: RwLock<BTreeMap<u32, Pardevice>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register a parallel port (Linux `parport_register_port`).
pub fn register_port(
    name: &str,
    base: u64,
    size: u64,
    irq: u32,
    dma: u32,
    modes: ParportMode,
    ops: ParportOps,
) -> Result<u32, &'static str> {
    let id = PORT_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let port = Parport {
        id,
        name: String::from(name),
        base,
        size,
        irq,
        dma,
        modes,
        ops,
        state: ParportState::Registered,
        device_ids: Vec::new(),
        cad_id: None,
    };
    PARPORTS.write().insert(id, port);

    // Try to attach existing drivers
    let drv_entries: Vec<(u32, fn(u32) -> Result<u32, &'static str>)> = {
        let drvs = PARPORT_DRVS.read();
        drvs.iter().map(|(did, d)| (*did, d.attach)).collect()
    };
    for (drv_id, attach_fn) in drv_entries {
        let dev_id = (attach_fn)(id)?;
        let mut devs = PARDEVS.write();
        devs.insert(
            dev_id,
            Pardevice {
                id: dev_id,
                port_id: id,
                drv_id,
                name: String::from(name),
                state: PardevState::Registered,
                preempt: false,
                exclusive: false,
                mode: modes,
            },
        );
        let mut ports = PARPORTS.write();
        if let Some(port) = ports.get_mut(&id) {
            port.device_ids.push(dev_id);
        }
    }
    Ok(id)
}

/// Register a parallel port driver (Linux `parport_register_driver`).
pub fn register_driver(driver: ParportDriver) -> Result<u32, &'static str> {
    let id = DRV_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let attach_fn = driver.attach;
    let drv_name = driver.name.clone();
    PARPORT_DRVS.write().insert(id, driver);

    // Attach to existing ports
    let port_ids: Vec<u32> = PARPORTS.read().keys().copied().collect();
    for port_id in port_ids {
        let dev_id = (attach_fn)(port_id)?;
        let mut devs = PARDEVS.write();
        devs.insert(
            dev_id,
            Pardevice {
                id: dev_id,
                port_id,
                drv_id: id,
                name: drv_name.clone(),
                state: PardevState::Registered,
                preempt: false,
                exclusive: false,
                mode: ParportMode::default(),
            },
        );
        let mut ports = PARPORTS.write();
        if let Some(port) = ports.get_mut(&port_id) {
            port.device_ids.push(dev_id);
        }
    }
    Ok(id)
}

/// Claim a parallel port device (Linux `parport_claim`).
pub fn claim_device(dev_id: u32) -> Result<(), &'static str> {
    let port_id = {
        let mut devs = PARDEVS.write();
        let dev = devs.get_mut(&dev_id).ok_or("Parport device not found")?;
        if dev.state == PardevState::Claimed {
            return Err("Parport device already claimed");
        }
        dev.state = PardevState::Claimed;
        dev.port_id
    };

    let mut ports = PARPORTS.write();
    if let Some(port) = ports.get_mut(&port_id) {
        if port.cad_id.is_some() && port.cad_id != Some(dev_id) {
            // Port already claimed by another device
            let mut devs = PARDEVS.write();
            if let Some(d) = devs.get_mut(&dev_id) {
                d.state = PardevState::Waiting;
            }
            return Err("Port claimed by another device");
        }
        port.cad_id = Some(dev_id);
        port.state = ParportState::Claimed;
    }
    Ok(())
}

/// Release a parallel port device (Linux `parport_release`).
pub fn release_device(dev_id: u32) -> Result<(), &'static str> {
    let port_id = {
        let mut devs = PARDEVS.write();
        let dev = devs.get_mut(&dev_id).ok_or("Parport device not found")?;
        dev.state = PardevState::Released;
        dev.port_id
    };

    let mut ports = PARPORTS.write();
    if let Some(port) = ports.get_mut(&port_id) {
        if port.cad_id == Some(dev_id) {
            port.cad_id = None;
            port.state = ParportState::Released;
        }
    }
    Ok(())
}

/// Write data to a parallel port (Linux `parport_write_data`).
pub fn write_data(port_id: u32, data: u8) -> Result<(), &'static str> {
    let write_fn = {
        let ports = PARPORTS.read();
        let port = ports.get(&port_id).ok_or("Parport not found")?;
        if port.cad_id.is_none() {
            return Err("Port not claimed");
        }
        port.ops.write_data
    };
    (write_fn)(port_id, data)
}

/// Read data from a parallel port (Linux `parport_read_data`).
pub fn read_data(port_id: u32) -> Result<u8, &'static str> {
    let read_fn = {
        let ports = PARPORTS.read();
        let port = ports.get(&port_id).ok_or("Parport not found")?;
        port.ops.read_data
    };
    (read_fn)(port_id)
}

/// Write control register (Linux `parport_write_control`).
pub fn write_control(port_id: u32, ctrl: u8) -> Result<(), &'static str> {
    let write_fn = {
        let ports = PARPORTS.read();
        let port = ports.get(&port_id).ok_or("Parport not found")?;
        port.ops.write_control
    };
    (write_fn)(port_id, ctrl)
}

/// Read status register (Linux `parport_read_status`).
pub fn read_status(port_id: u32) -> Result<u8, &'static str> {
    let read_fn = {
        let ports = PARPORTS.read();
        let port = ports.get(&port_id).ok_or("Parport not found")?;
        port.ops.read_status
    };
    (read_fn)(port_id)
}

/// EPP write data (Linux `parport_epp_write_data`).
pub fn epp_write_data(port_id: u32, data: &[u8]) -> Result<usize, &'static str> {
    let write_fn = {
        let ports = PARPORTS.read();
        let port = ports.get(&port_id).ok_or("Parport not found")?;
        if !port.modes.epp {
            return Err("EPP mode not supported");
        }
        port.ops.epp_write_data
    };
    (write_fn)(port_id, data)
}

/// EPP read data (Linux `parport_epp_read_data`).
pub fn epp_read_data(port_id: u32, buf: &mut [u8]) -> Result<usize, &'static str> {
    let read_fn = {
        let ports = PARPORTS.read();
        let port = ports.get(&port_id).ok_or("Parport not found")?;
        if !port.modes.epp {
            return Err("EPP mode not supported");
        }
        port.ops.epp_read_data
    };
    (read_fn)(port_id, buf)
}

/// List all parallel ports.
pub fn list_ports() -> Vec<(u32, String, ParportState, ParportMode, usize)> {
    PARPORTS
        .read()
        .iter()
        .map(|(id, p)| (*id, p.name.clone(), p.state, p.modes, p.device_ids.len()))
        .collect()
}

/// Count registered ports.
pub fn port_count() -> usize {
    PARPORTS.read().len()
}

// ── Software parallel port ──────────────────────────────────────────────

fn sw_write_data(_port_id: u32, _data: u8) -> Result<(), &'static str> {
    Ok(())
}
fn sw_read_data(_port_id: u32) -> Result<u8, &'static str> {
    Ok(0)
}
fn sw_write_control(_port_id: u32, _ctrl: u8) -> Result<(), &'static str> {
    Ok(())
}
fn sw_read_control(_port_id: u32) -> Result<u8, &'static str> {
    Ok(0)
}
fn sw_read_status(_port_id: u32) -> Result<u8, &'static str> {
    Ok(0xDF)
}
fn sw_enable_irq(_port_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_disable_irq(_port_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_data_forward(_port_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_data_reverse(_port_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_epp_write(_port_id: u32, data: &[u8]) -> Result<usize, &'static str> {
    Ok(data.len())
}
fn sw_epp_read(_port_id: u32, buf: &mut [u8]) -> Result<usize, &'static str> {
    for b in buf.iter_mut() {
        *b = 0;
    }
    Ok(buf.len())
}
fn sw_ecp_write(_port_id: u32, data: &[u8]) -> Result<usize, &'static str> {
    Ok(data.len())
}
fn sw_ecp_read(_port_id: u32, buf: &mut [u8]) -> Result<usize, &'static str> {
    for b in buf.iter_mut() {
        *b = 0;
    }
    Ok(buf.len())
}

/// Software parallel port ops.
pub fn software_parport_ops() -> ParportOps {
    ParportOps {
        write_data: sw_write_data,
        read_data: sw_read_data,
        write_control: sw_write_control,
        read_control: sw_read_control,
        read_status: sw_read_status,
        enable_irq: sw_enable_irq,
        disable_irq: sw_disable_irq,
        data_forward: sw_data_forward,
        data_reverse: sw_data_reverse,
        epp_write_data: sw_epp_write,
        epp_read_data: sw_epp_read,
        ecp_write: sw_ecp_write,
        ecp_read: sw_ecp_read,
    }
}

fn sw_attach(port_id: u32) -> Result<u32, &'static str> {
    Ok(DEV_ID_COUNTER.fetch_add(1, Ordering::SeqCst) | (port_id << 16) as u32)
}
fn sw_detach(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    let ops = software_parport_ops();
    let modes = ParportMode {
        spp: true,
        ps2: true,
        epp: true,
        ecp: true,
        dma: false,
        pcps2: true,
    };
    let port_id = register_port("sw-parport0", 0x378, 8, 7, 0, modes, ops)?;

    // Register a printer driver
    let driver = ParportDriver {
        id: 0,
        name: String::from("sw-lp"),
        attach: sw_attach,
        detach: sw_detach,
    };
    let drv_id = register_driver(driver)?;

    // Find the device
    let dev_id = {
        let ports = PARPORTS.read();
        let port = ports.get(&port_id).ok_or("Port not found")?;
        port.device_ids
            .first()
            .copied()
            .ok_or("No device attached")?
    };

    // Claim the port
    claim_device(dev_id)?;

    // Write some data
    write_data(port_id, 0x41)?; // 'A'
    write_control(port_id, 0x0C)?; // Strobe
    let _status = read_status(port_id)?;

    // EPP write
    let test_data = [0x48, 0x65, 0x6C, 0x6C, 0x6F]; // "Hello"
    epp_write_data(port_id, &test_data)?;

    // Release
    release_device(dev_id)?;

    let _ = drv_id;
    Ok(())
}
