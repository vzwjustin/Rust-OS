//! MEI (Intel Management Engine Interface) subsystem
//!
//! Provides Intel MEI client bus for communication with the Management Engine.
//! Mirrors Linux's `drivers/misc/mei/`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// MEI device state (Linux `enum mei_dev_state`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeiDevState {
    Init,
    InitClients,
    Enabled,
    Resetting,
    Disabled,
    PowerDown,
    PowerUp,
}

/// MEI client (Linux `struct mei_cl`).
pub struct MeiClient {
    pub id: u32,
    pub dev_id: u32,
    pub name: String,
    pub uuid: [u8; 16],
    pub state: MeiClState,
    pub tx_ring: Vec<Vec<u8>>,
    pub rx_ring: Vec<Vec<u8>>,
    pub msg_sent: u32,
    pub msg_recv: u32,
    pub max_conn: u8,
}

/// MEI client state (Linux `enum mei_cl_state`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeiClState {
    Disconnected,
    Connecting,
    Connected,
    Disconnecting,
}

/// MEI device (Linux `struct mei_device`).
pub struct MeiDevice {
    pub id: u32,
    pub name: String,
    pub ops: MeiDevOps,
    pub state: MeiDevState,
    pub client_ids: Vec<u32>,
    pub fw_version: [u16; 3],
    pub hbm_version: u8,
    pub max_clients: u32,
}

/// MEI device operations (Linux `struct mei_hw_ops`).
pub struct MeiDevOps {
    pub init: fn(dev_id: u32) -> Result<(), &'static str>,
    pub reset: fn(dev_id: u32) -> Result<(), &'static str>,
    pub hbm_start: fn(dev_id: u32) -> Result<(), &'static str>,
    pub send_msg: fn(dev_id: u32, client_id: u32, data: &[u8]) -> Result<usize, &'static str>,
    pub recv_msg: fn(dev_id: u32, client_id: u32, buf: &mut [u8]) -> Result<usize, &'static str>,
    pub pg_set: fn(dev_id: u32, state: MeiPgState) -> Result<(), &'static str>,
}

/// MEI power gating state (Linux `enum mei_pg_state`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeiPgState {
    Off,
    On,
    Hibernation,
}

/// MEI client driver (Linux `struct mei_cl_driver`).
pub struct MeiClDriver {
    pub name: String,
    pub uuid: [u8; 16],
    pub probe: fn(client_id: u32) -> Result<(), &'static str>,
    pub remove: fn(client_id: u32) -> Result<(), &'static str>,
    pub recv: Option<fn(client_id: u32, data: &[u8])>,
}

// ── Registry ────────────────────────────────────────────────────────────

static DEV_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static CLIENT_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DRIVER_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static MEI_DEVS: RwLock<BTreeMap<u32, MeiDevice>> = RwLock::new(BTreeMap::new());
static MEI_CLIENTS: RwLock<BTreeMap<u32, MeiClient>> = RwLock::new(BTreeMap::new());
static MEI_DRIVERS: RwLock<BTreeMap<u32, MeiClDriver>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register an MEI device.
pub fn register_device(name: &str, ops: MeiDevOps, max_clients: u32) -> Result<u32, &'static str> {
    let id = DEV_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dev = MeiDevice {
        id,
        name: String::from(name),
        ops,
        state: MeiDevState::Init,
        client_ids: Vec::new(),
        fw_version: [0, 0, 0],
        hbm_version: 1,
        max_clients,
    };
    MEI_DEVS.write().insert(id, dev);
    Ok(id)
}

/// Initialize an MEI device (Linux `mei_start`).
pub fn init_device(dev_id: u32) -> Result<(), &'static str> {
    let (init_fn, hbm_fn) = {
        let devs = MEI_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("MEI device not found")?;
        (dev.ops.init, dev.ops.hbm_start)
    };
    (init_fn)(dev_id)?;

    {
        let mut devs = MEI_DEVS.write();
        if let Some(dev) = devs.get_mut(&dev_id) {
            dev.state = MeiDevState::InitClients;
        }
    }

    (hbm_fn)(dev_id)?;

    let mut devs = MEI_DEVS.write();
    if let Some(dev) = devs.get_mut(&dev_id) {
        dev.state = MeiDevState::Enabled;
        dev.fw_version = [11, 0, 0];
    }
    Ok(())
}

/// Register an MEI client.
pub fn register_client(
    dev_id: u32,
    name: &str,
    uuid: [u8; 16],
    max_conn: u8,
) -> Result<u32, &'static str> {
    let client_id = CLIENT_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let client = MeiClient {
        id: client_id,
        dev_id,
        name: String::from(name),
        uuid,
        state: MeiClState::Disconnected,
        tx_ring: Vec::new(),
        rx_ring: Vec::new(),
        msg_sent: 0,
        msg_recv: 0,
        max_conn,
    };
    MEI_CLIENTS.write().insert(client_id, client);

    let mut devs = MEI_DEVS.write();
    if let Some(dev) = devs.get_mut(&dev_id) {
        dev.client_ids.push(client_id);
    }

    try_match_driver(client_id)?;
    Ok(client_id)
}

/// Connect an MEI client (Linux `mei_cl_connect`).
pub fn connect_client(client_id: u32) -> Result<(), &'static str> {
    let mut clients = MEI_CLIENTS.write();
    let client = clients.get_mut(&client_id).ok_or("MEI client not found")?;
    client.state = MeiClState::Connected;
    Ok(())
}

/// Disconnect an MEI client (Linux `mei_cl_disconnect`).
pub fn disconnect_client(client_id: u32) -> Result<(), &'static str> {
    let mut clients = MEI_CLIENTS.write();
    let client = clients.get_mut(&client_id).ok_or("MEI client not found")?;
    client.state = MeiClState::Disconnected;
    Ok(())
}

/// Send a message to an MEI client (Linux `mei_cl_write`).
pub fn send_msg(client_id: u32, data: &[u8]) -> Result<usize, &'static str> {
    let (dev_id, send_fn) = {
        let clients = MEI_CLIENTS.read();
        let client = clients.get(&client_id).ok_or("MEI client not found")?;
        if client.state != MeiClState::Connected {
            return Err("MEI client not connected");
        }
        let devs = MEI_DEVS.read();
        let dev = devs.get(&client.dev_id).ok_or("MEI device not found")?;
        if dev.state != MeiDevState::Enabled {
            return Err("MEI device not enabled");
        }
        (client.dev_id, dev.ops.send_msg)
    };
    let n = (send_fn)(dev_id, client_id, data)?;

    let mut clients = MEI_CLIENTS.write();
    if let Some(client) = clients.get_mut(&client_id) {
        client.msg_sent += 1;
        let mut tx = Vec::new();
        tx.extend_from_slice(data);
        client.tx_ring.push(tx);
    }
    Ok(n)
}

/// Receive a message from an MEI client (Linux `mei_cl_read`).
pub fn recv_msg(client_id: u32, buf: &mut [u8]) -> Result<usize, &'static str> {
    let (dev_id, recv_fn) = {
        let clients = MEI_CLIENTS.read();
        let client = clients.get(&client_id).ok_or("MEI client not found")?;
        if client.state != MeiClState::Connected {
            return Err("MEI client not connected");
        }
        let devs = MEI_DEVS.read();
        let dev = devs.get(&client.dev_id).ok_or("MEI device not found")?;
        (client.dev_id, dev.ops.recv_msg)
    };
    let n = (recv_fn)(dev_id, client_id, buf)?;

    let mut clients = MEI_CLIENTS.write();
    if let Some(client) = clients.get_mut(&client_id) {
        client.msg_recv += 1;
    }
    Ok(n)
}

/// Set power gating state (Linux `mei_pg_set`).
pub fn set_pg_state(dev_id: u32, state: MeiPgState) -> Result<(), &'static str> {
    let pg_fn = {
        let devs = MEI_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("MEI device not found")?;
        dev.ops.pg_set
    };
    (pg_fn)(dev_id, state)
}

/// Register an MEI client driver.
pub fn register_driver(driver: MeiClDriver) -> Result<u32, &'static str> {
    let id = DRIVER_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let uuid = driver.uuid;
    MEI_DRIVERS.write().insert(id, driver);

    let client_ids: Vec<u32> = {
        let clients = MEI_CLIENTS.read();
        clients
            .iter()
            .filter(|(_, c)| c.uuid == uuid)
            .map(|(id, _)| *id)
            .collect()
    };
    for cid in client_ids {
        try_match_driver(cid)?;
    }
    Ok(id)
}

/// Try to match a client with a driver.
fn try_match_driver(client_id: u32) -> Result<(), &'static str> {
    let matched = {
        let clients = MEI_CLIENTS.read();
        let client = match clients.get(&client_id) {
            Some(c) => c,
            None => return Ok(()),
        };
        let uuid = client.uuid;

        let drivers = MEI_DRIVERS.read();
        let mut found: Option<fn(u32) -> Result<(), &'static str>> = None;
        for (_, drv) in drivers.iter() {
            if drv.uuid == uuid {
                found = Some(drv.probe);
                break;
            }
        }
        found
    };

    if let Some(probe_fn) = matched {
        (probe_fn)(client_id)?;
    }
    Ok(())
}

/// List all MEI devices.
pub fn list_devices() -> Vec<(u32, String, MeiDevState)> {
    MEI_DEVS
        .read()
        .iter()
        .map(|(id, d)| (*id, d.name.clone(), d.state))
        .collect()
}

/// List clients on a device.
pub fn list_clients(dev_id: u32) -> Result<Vec<(u32, String, MeiClState)>, &'static str> {
    let devs = MEI_DEVS.read();
    let dev = devs.get(&dev_id).ok_or("MEI device not found")?;
    let clients = MEI_CLIENTS.read();
    let mut result = Vec::new();
    for &cid in &dev.client_ids {
        if let Some(client) = clients.get(&cid) {
            result.push((client.id, client.name.clone(), client.state));
        }
    }
    Ok(result)
}

/// Count registered devices.
pub fn device_count() -> usize {
    MEI_DEVS.read().len()
}

// ── Software MEI ────────────────────────────────────────────────────────

fn sw_init(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_reset(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_hbm_start(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_send_msg(_dev_id: u32, _client_id: u32, data: &[u8]) -> Result<usize, &'static str> {
    Ok(data.len())
}
fn sw_recv_msg(_dev_id: u32, _client_id: u32, buf: &mut [u8]) -> Result<usize, &'static str> {
    for b in buf.iter_mut() {
        *b = 0;
    }
    Ok(buf.len())
}
fn sw_pg_set(_dev_id: u32, _state: MeiPgState) -> Result<(), &'static str> {
    Ok(())
}

/// Software MEI device ops.
pub fn software_mei_ops() -> MeiDevOps {
    MeiDevOps {
        init: sw_init,
        reset: sw_reset,
        hbm_start: sw_hbm_start,
        send_msg: sw_send_msg,
        recv_msg: sw_recv_msg,
        pg_set: sw_pg_set,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

fn null_probe(_client_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn null_remove(_client_id: u32) -> Result<(), &'static str> {
    Ok(())
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("mei: subsystem ready");
    Ok(())
}
