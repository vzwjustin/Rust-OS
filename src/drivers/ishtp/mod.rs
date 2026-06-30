//! ISHTP (Intel Integrated Sensor Hub Transport Protocol) subsystem
//!
//! Provides ISHTP client bus for Intel ISH sensor communication.
//! Mirrors Linux's `drivers/hid/intel-ish-hid/ishtp/`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// ISHTP device (Linux `struct ishtp_device`).
pub struct IshtpDevice {
    pub id: u32,
    pub name: String,
    pub ops: IshtpDevOps,
    pub state: IshtpDevState,
    pub client_ids: Vec<u32>,
    pub max_clients: u32,
    pub fw_loaded: bool,
}

/// ISHTP device state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IshtpDevState {
    Uninitialized,
    Init,
    Start,
    Disconnect,
    Recover,
    Disabled,
}

/// ISHTP client (Linux `struct ishtp_cl`).
pub struct IshtpClient {
    pub id: u32,
    pub dev_id: u32,
    pub name: String,
    pub guid: [u8; 16],
    pub state: IshtpClState,
    pub tx_ring_size: u32,
    pub rx_ring_size: u32,
    pub fc_off: bool, // Flow control off
    pub msg_sent: u32,
    pub msg_recv: u32,
}

/// ISHTP client state (Linux `enum ishtp_cl_state`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IshtpClState {
    Disconnected,
    Connecting,
    Connected,
    Disconnecting,
}

/// ISHTP device operations.
pub struct IshtpDevOps {
    pub init: fn(dev_id: u32) -> Result<(), &'static str>,
    pub reset: fn(dev_id: u32) -> Result<(), &'static str>,
    pub hbm_start: fn(dev_id: u32) -> Result<(), &'static str>,
    pub send_msg: fn(dev_id: u32, client_id: u32, data: &[u8]) -> Result<usize, &'static str>,
    pub recv_msg: fn(dev_id: u32, client_id: u32, buf: &mut [u8]) -> Result<usize, &'static str>,
}

/// ISHTP client driver (Linux `struct ishtp_cl_driver`).
pub struct IshtpClDriver {
    pub name: String,
    pub guid: [u8; 16],
    pub probe: fn(client_id: u32) -> Result<(), &'static str>,
    pub remove: fn(client_id: u32) -> Result<(), &'static str>,
    pub recv: Option<fn(client_id: u32, data: &[u8])>,
}

// ── Registry ────────────────────────────────────────────────────────────

static DEV_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static CLIENT_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DRIVER_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static ISHTP_DEVS: RwLock<BTreeMap<u32, IshtpDevice>> = RwLock::new(BTreeMap::new());
static ISHTP_CLIENTS: RwLock<BTreeMap<u32, IshtpClient>> = RwLock::new(BTreeMap::new());
static ISHTP_DRIVERS: RwLock<BTreeMap<u32, IshtpClDriver>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register an ISHTP device.
pub fn register_device(
    name: &str,
    ops: IshtpDevOps,
    max_clients: u32,
) -> Result<u32, &'static str> {
    let id = DEV_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dev = IshtpDevice {
        id,
        name: String::from(name),
        ops,
        state: IshtpDevState::Init,
        client_ids: Vec::new(),
        max_clients,
        fw_loaded: false,
    };
    ISHTP_DEVS.write().insert(id, dev);
    Ok(id)
}

/// Initialize an ISHTP device (Linux `ishtp_dev_init` + `ishtp_start`).
pub fn init_device(dev_id: u32) -> Result<(), &'static str> {
    let (init_fn, hbm_fn) = {
        let devs = ISHTP_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("ISHTP device not found")?;
        (dev.ops.init, dev.ops.hbm_start)
    };
    (init_fn)(dev_id)?;

    {
        let mut devs = ISHTP_DEVS.write();
        if let Some(dev) = devs.get_mut(&dev_id) {
            dev.state = IshtpDevState::Start;
            dev.fw_loaded = true;
        }
    }

    (hbm_fn)(dev_id)?;
    Ok(())
}

/// Register an ISHTP client on a device.
pub fn register_client(dev_id: u32, name: &str, guid: [u8; 16]) -> Result<u32, &'static str> {
    let client_id = CLIENT_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let client = IshtpClient {
        id: client_id,
        dev_id,
        name: String::from(name),
        guid,
        state: IshtpClState::Disconnected,
        tx_ring_size: 16,
        rx_ring_size: 16,
        fc_off: false,
        msg_sent: 0,
        msg_recv: 0,
    };
    ISHTP_CLIENTS.write().insert(client_id, client);

    let mut devs = ISHTP_DEVS.write();
    if let Some(dev) = devs.get_mut(&dev_id) {
        dev.client_ids.push(client_id);
    }

    try_match_driver(client_id)?;
    Ok(client_id)
}

/// Connect an ISHTP client (Linux `ishtp_cl_connect`).
pub fn connect_client(client_id: u32) -> Result<(), &'static str> {
    let (dev_id, send_fn) = {
        let clients = ISHTP_CLIENTS.read();
        let client = clients.get(&client_id).ok_or("ISHTP client not found")?;
        if client.state == IshtpClState::Connected {
            return Ok(());
        }
        (client.dev_id, {
            let devs = ISHTP_DEVS.read();
            let dev = devs.get(&client.dev_id).ok_or("ISHTP device not found")?;
            dev.ops.send_msg
        })
    };

    {
        let mut clients = ISHTP_CLIENTS.write();
        if let Some(client) = clients.get_mut(&client_id) {
            client.state = IshtpClState::Connecting;
        }
    }

    let hbm_connect_msg: [u8; 4] = [0x01, 0x00, 0x00, 0x00];
    (send_fn)(dev_id, client_id, &hbm_connect_msg)?;

    let mut clients = ISHTP_CLIENTS.write();
    if let Some(client) = clients.get_mut(&client_id) {
        client.state = IshtpClState::Connected;
        client.fc_off = false;
    }
    Ok(())
}

/// Disconnect an ISHTP client (Linux `ishtp_cl_disconnect`).
pub fn disconnect_client(client_id: u32) -> Result<(), &'static str> {
    let mut clients = ISHTP_CLIENTS.write();
    let client = clients
        .get_mut(&client_id)
        .ok_or("ISHTP client not found")?;
    client.state = IshtpClState::Disconnecting;
    client.state = IshtpClState::Disconnected;
    Ok(())
}

/// Send a message from a client (Linux `ishtp_cl_send`).
pub fn send_msg(client_id: u32, data: &[u8]) -> Result<usize, &'static str> {
    let (dev_id, send_fn) = {
        let clients = ISHTP_CLIENTS.read();
        let client = clients.get(&client_id).ok_or("ISHTP client not found")?;
        if client.state != IshtpClState::Connected {
            return Err("ISHTP client not connected");
        }
        let devs = ISHTP_DEVS.read();
        let dev = devs.get(&client.dev_id).ok_or("ISHTP device not found")?;
        (client.dev_id, dev.ops.send_msg)
    };
    let n = (send_fn)(dev_id, client_id, data)?;

    let mut clients = ISHTP_CLIENTS.write();
    if let Some(client) = clients.get_mut(&client_id) {
        client.msg_sent += 1;
    }
    Ok(n)
}

/// Receive a message for a client (Linux `ishtp_cl_recv`).
pub fn recv_msg(client_id: u32, buf: &mut [u8]) -> Result<usize, &'static str> {
    let (dev_id, recv_fn) = {
        let clients = ISHTP_CLIENTS.read();
        let client = clients.get(&client_id).ok_or("ISHTP client not found")?;
        if client.state != IshtpClState::Connected {
            return Err("ISHTP client not connected");
        }
        let devs = ISHTP_DEVS.read();
        let dev = devs.get(&client.dev_id).ok_or("ISHTP device not found")?;
        (client.dev_id, dev.ops.recv_msg)
    };
    let n = (recv_fn)(dev_id, client_id, buf)?;

    let mut clients = ISHTP_CLIENTS.write();
    if let Some(client) = clients.get_mut(&client_id) {
        client.msg_recv += 1;
    }
    Ok(n)
}

/// Register an ISHTP client driver.
pub fn register_driver(driver: IshtpClDriver) -> Result<u32, &'static str> {
    let id = DRIVER_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let guid = driver.guid;
    ISHTP_DRIVERS.write().insert(id, driver);

    let client_ids: Vec<u32> = {
        let clients = ISHTP_CLIENTS.read();
        clients
            .iter()
            .filter(|(_, c)| c.guid == guid)
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
        let clients = ISHTP_CLIENTS.read();
        let client = match clients.get(&client_id) {
            Some(c) => c,
            None => return Ok(()),
        };
        let guid = client.guid;

        let drivers = ISHTP_DRIVERS.read();
        let mut found: Option<fn(u32) -> Result<(), &'static str>> = None;
        for (_, drv) in drivers.iter() {
            if drv.guid == guid {
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

/// List all ISHTP devices.
pub fn list_devices() -> Vec<(u32, String, IshtpDevState)> {
    ISHTP_DEVS
        .read()
        .iter()
        .map(|(id, d)| (*id, d.name.clone(), d.state))
        .collect()
}

/// List clients on a device.
pub fn list_clients(dev_id: u32) -> Result<Vec<(u32, String, IshtpClState)>, &'static str> {
    let devs = ISHTP_DEVS.read();
    let dev = devs.get(&dev_id).ok_or("ISHTP device not found")?;
    let clients = ISHTP_CLIENTS.read();
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
    ISHTP_DEVS.read().len()
}

// ── Software ISHTP ──────────────────────────────────────────────────────

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

/// Software ISHTP device ops.
pub fn software_ishtp_ops() -> IshtpDevOps {
    IshtpDevOps {
        init: sw_init,
        reset: sw_reset,
        hbm_start: sw_hbm_start,
        send_msg: sw_send_msg,
        recv_msg: sw_recv_msg,
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
    if !ISHTP_DEVS.read().is_empty() {
        return Ok(());
    }

    let ops = software_ishtp_ops();
    let dev_id = register_device("sw-ishtp", ops, 8)?;
    init_device(dev_id)?;
    crate::serial_println!(
        "ishtp: software device registered and initialized (id={})",
        dev_id
    );
    Ok(())
}
