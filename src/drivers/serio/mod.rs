//! SERIO subsystem
//!
//! Provides serio bus for serial I/O devices (keyboards, mice, gameports).
//! Mirrors Linux's `drivers/input/serio/serio.c`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// Serio port (Linux `struct serio`).
pub struct SerioPort {
    pub id: u32,
    pub name: String,
    pub phys: String,
    pub port_type: SerioType,
    pub child_ids: Vec<u32>,
    pub parent_id: Option<u32>,
    pub ops: SerioOps,
    pub state: SerioState,
    pub dev_id: Option<u32>,
}

/// Serio type (Linux `enum serio_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SerioType {
    Pci,
    Pnp,
    Rs232,
    Amiga,
    X86,
    H8042,
    Ps2,
    Gameport,
    Userspace,
}

/// Serio state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SerioState {
    Unregistered,
    Registered,
    Open,
    Closed,
}

/// Serio operations (Linux `struct serio_driver` + `struct serio`).
pub struct SerioOps {
    pub open: fn(port_id: u32) -> Result<(), &'static str>,
    pub close: fn(port_id: u32) -> Result<(), &'static str>,
    pub write: fn(port_id: u32, data: &[u8]) -> Result<usize, &'static str>,
    pub start: fn(port_id: u32) -> Result<(), &'static str>,
    pub stop: fn(port_id: u32) -> Result<(), &'static str>,
}

/// Serio device/driver (Linux `struct serio_driver`).
pub struct SerioDriver {
    pub id: u32,
    pub name: String,
    pub id_table: Vec<SerioDeviceId>,
    pub probe: fn(port_id: u32, drv_id: u32) -> Result<u32, &'static str>,
    pub disconnect: fn(dev_id: u32) -> Result<(), &'static str>,
    pub reconnect: Option<fn(dev_id: u32) -> Result<(), &'static str>>,
    pub interrupt: fn(dev_id: u32, data: &[u8]) -> Result<(), &'static str>,
}

/// Serio device ID (Linux `struct serio_device_id`).
#[derive(Debug, Clone)]
pub struct SerioDeviceId {
    pub port_type: SerioType,
    pub extra: u8,
    pub proto: u8,
    pub id: u8,
}

/// Serio device (bound port+driver).
pub struct SerioBoundDev {
    pub id: u32,
    pub port_id: u32,
    pub drv_id: u32,
    pub name: String,
    pub bound: bool,
}

// ── Registry ────────────────────────────────────────────────────────────

static PORT_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DRV_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DEV_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static SERIO_PORTS: RwLock<BTreeMap<u32, SerioPort>> = RwLock::new(BTreeMap::new());
static SERIO_DRVS: RwLock<BTreeMap<u32, SerioDriver>> = RwLock::new(BTreeMap::new());
static SERIO_DEVS: RwLock<BTreeMap<u32, SerioBoundDev>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register a serio port (Linux `serio_register_port`).
pub fn register_port(
    name: &str,
    port_type: SerioType,
    parent_id: Option<u32>,
    ops: SerioOps,
) -> Result<u32, &'static str> {
    let id = PORT_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let port = SerioPort {
        id,
        name: String::from(name),
        phys: alloc::format!("serio{}", id),
        port_type,
        child_ids: Vec::new(),
        parent_id,
        ops,
        state: SerioState::Registered,
        dev_id: None,
    };
    SERIO_PORTS.write().insert(id, port);

    if let Some(pid) = parent_id {
        let mut ports = SERIO_PORTS.write();
        if let Some(parent) = ports.get_mut(&pid) {
            parent.child_ids.push(id);
        }
    }

    // Try to match a driver
    try_match_driver(id)?;
    Ok(id)
}

/// Open a serio port (Linux `serio_open`).
pub fn open_port(port_id: u32) -> Result<(), &'static str> {
    let (open_fn, start_fn) = {
        let ports = SERIO_PORTS.read();
        let port = ports.get(&port_id).ok_or("Serio port not found")?;
        (port.ops.open, port.ops.start)
    };
    (open_fn)(port_id)?;
    (start_fn)(port_id)?;

    let mut ports = SERIO_PORTS.write();
    if let Some(port) = ports.get_mut(&port_id) {
        port.state = SerioState::Open;
    }
    Ok(())
}

/// Close a serio port (Linux `serio_close`).
pub fn close_port(port_id: u32) -> Result<(), &'static str> {
    let (close_fn, stop_fn) = {
        let ports = SERIO_PORTS.read();
        let port = ports.get(&port_id).ok_or("Serio port not found")?;
        (port.ops.close, port.ops.stop)
    };
    (close_fn)(port_id)?;
    (stop_fn)(port_id)?;

    let mut ports = SERIO_PORTS.write();
    if let Some(port) = ports.get_mut(&port_id) {
        port.state = SerioState::Closed;
    }
    Ok(())
}

/// Write data to a serio port (Linux `serio_write`).
pub fn write_port(port_id: u32, data: &[u8]) -> Result<usize, &'static str> {
    let write_fn = {
        let ports = SERIO_PORTS.read();
        let port = ports.get(&port_id).ok_or("Serio port not found")?;
        if port.state != SerioState::Open {
            return Err("Serio port not open");
        }
        port.ops.write
    };
    (write_fn)(port_id, data)
}

/// Register a serio driver (Linux `serio_register_driver`).
pub fn register_driver(driver: SerioDriver) -> Result<u32, &'static str> {
    let id = DRV_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let id_table = driver.id_table.clone();
    SERIO_DRVS.write().insert(id, driver);

    // Try to match with existing ports
    let port_ids: Vec<u32> = SERIO_PORTS
        .read()
        .iter()
        .filter(|(_, p)| p.dev_id.is_none())
        .map(|(id, _)| *id)
        .collect();

    for port_id in port_ids {
        let _ = try_match_with_driver(port_id, id, &id_table);
    }
    Ok(id)
}

/// Try to match a port with any driver.
fn try_match_driver(port_id: u32) -> Result<(), &'static str> {
    let (port_type, port_extra, port_proto, port_id_val) = {
        let ports = SERIO_PORTS.read();
        let port = ports.get(&port_id).ok_or("Serio port not found")?;
        if port.dev_id.is_some() {
            return Ok(());
        }
        (port.port_type, 0u8, 0u8, 0u8)
    };

    let entries: Vec<(u32, fn(u32, u32) -> Result<u32, &'static str>, String)> = {
        let drivers = SERIO_DRVS.read();
        drivers
            .iter()
            .flat_map(|(drv_id, drv)| {
                drv.id_table.iter().filter_map(move |id| {
                    if id.port_type == port_type || id.port_type == SerioType::Userspace {
                        Some((*drv_id, drv.probe, drv.name.clone()))
                    } else {
                        None
                    }
                })
            })
            .collect()
    };
    for (drv_id, probe_fn, drv_name) in entries {
        let dev_id = (probe_fn)(port_id, drv_id)?;
        let bound = SerioBoundDev {
            id: dev_id,
            port_id,
            drv_id,
            name: drv_name,
            bound: true,
        };
        SERIO_DEVS.write().insert(dev_id, bound);

        let mut ports = SERIO_PORTS.write();
        if let Some(port) = ports.get_mut(&port_id) {
            port.dev_id = Some(dev_id);
        }
        return Ok(());
    }
    let _ = (port_extra, port_proto, port_id_val);
    Ok(())
}

/// Try to match a specific port with a specific driver.
fn try_match_with_driver(
    port_id: u32,
    drv_id: u32,
    id_table: &[SerioDeviceId],
) -> Result<(), &'static str> {
    let port_type = {
        let ports = SERIO_PORTS.read();
        let port = ports.get(&port_id).ok_or("Serio port not found")?;
        if port.dev_id.is_some() {
            return Ok(());
        }
        port.port_type
    };

    for id in id_table {
        if id.port_type == port_type || id.port_type == SerioType::Userspace {
            let (probe_fn, drv_name) = {
                let drivers = SERIO_DRVS.read();
                let drv = drivers.get(&drv_id).ok_or("Serio driver not found")?;
                (drv.probe, drv.name.clone())
            };

            let dev_id = (probe_fn)(port_id, drv_id)?;
            let bound = SerioBoundDev {
                id: dev_id,
                port_id,
                drv_id,
                name: drv_name,
                bound: true,
            };
            SERIO_DEVS.write().insert(dev_id, bound);

            let mut ports = SERIO_PORTS.write();
            if let Some(port) = ports.get_mut(&port_id) {
                port.dev_id = Some(dev_id);
            }
            return Ok(());
        }
    }
    Ok(())
}

/// Deliver interrupt data to the bound driver (Linux `serio_interrupt`).
pub fn interrupt(port_id: u32, data: &[u8]) -> Result<(), &'static str> {
    let dev_id = {
        let ports = SERIO_PORTS.read();
        let port = ports.get(&port_id).ok_or("Serio port not found")?;
        port.dev_id.ok_or("No driver bound to serio port")?
    };

    let interrupt_fn = {
        let devs = SERIO_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("Serio device not found")?;
        let drivers = SERIO_DRVS.read();
        let drv = drivers.get(&dev.drv_id).ok_or("Serio driver not found")?;
        drv.interrupt
    };
    (interrupt_fn)(dev_id, data)
}

/// Unregister a serio port (Linux `serio_unregister_port`).
pub fn unregister_port(port_id: u32) -> Result<(), &'static str> {
    // Disconnect bound device
    if let Some(dev_id) = {
        let ports = SERIO_PORTS.read();
        ports.get(&port_id).and_then(|p| p.dev_id)
    } {
        let disconnect_fn = {
            let devs = SERIO_DEVS.read();
            let dev = devs.get(&dev_id).ok_or("Serio device not found")?;
            let drivers = SERIO_DRVS.read();
            let drv = drivers.get(&dev.drv_id).ok_or("Serio driver not found")?;
            drv.disconnect
        };
        (disconnect_fn)(dev_id)?;
        SERIO_DEVS.write().remove(&dev_id);
    }

    SERIO_PORTS.write().remove(&port_id);
    Ok(())
}

/// List all serio ports.
pub fn list_ports() -> Vec<(u32, String, SerioType, SerioState, Option<u32>)> {
    SERIO_PORTS
        .read()
        .iter()
        .map(|(id, p)| (*id, p.name.clone(), p.port_type, p.state, p.dev_id))
        .collect()
}

/// Count registered ports.
pub fn port_count() -> usize {
    SERIO_PORTS.read().len()
}

// ── Software serio ──────────────────────────────────────────────────────

fn sw_open(_port_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_close(_port_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_write(_port_id: u32, data: &[u8]) -> Result<usize, &'static str> {
    Ok(data.len())
}
fn sw_start(_port_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_stop(_port_id: u32) -> Result<(), &'static str> {
    Ok(())
}

/// Software serio port ops.
pub fn software_serio_ops() -> SerioOps {
    SerioOps {
        open: sw_open,
        close: sw_close,
        write: sw_write,
        start: sw_start,
        stop: sw_stop,
    }
}

fn sw_probe(port_id: u32, drv_id: u32) -> Result<u32, &'static str> {
    Ok(DEV_ID_COUNTER.fetch_add(1, Ordering::SeqCst)
        | (port_id << 16) as u32
        | (drv_id << 24) as u32)
}
fn sw_disconnect(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_interrupt(_dev_id: u32, _data: &[u8]) -> Result<(), &'static str> {
    Ok(())
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    // Register a PS/2 keyboard port
    let kb_port = register_port("sw-ps2-kb", SerioType::H8042, None, software_serio_ops())?;

    // Register a PS/2 mouse port
    let ms_port = register_port("sw-ps2-ms", SerioType::H8042, None, software_serio_ops())?;

    // Register a driver
    let mut id_table = Vec::new();
    id_table.push(SerioDeviceId {
        port_type: SerioType::H8042,
        extra: 0,
        proto: 0,
        id: 0,
    });
    let driver = SerioDriver {
        id: 0,
        name: String::from("sw-serio-drv"),
        id_table,
        probe: sw_probe,
        disconnect: sw_disconnect,
        reconnect: None,
        interrupt: sw_interrupt,
    };
    register_driver(driver)?;

    // Open ports
    open_port(kb_port)?;
    open_port(ms_port)?;

    // Write some data
    write_port(kb_port, &[0xED, 0x07])?; // Set scroll lock LED

    // Simulate interrupt (keyboard scancode)
    interrupt(kb_port, &[0x1C])?; // Enter key make code

    // Close ports
    close_port(kb_port)?;
    close_port(ms_port)?;

    Ok(())
}
