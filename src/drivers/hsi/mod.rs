//! HSI (High-Speed Synchronous Serial Interface) subsystem
//!
//! Provides HSI bus framework for high-speed serial communication between
//! application processors and modems. Mirrors Linux's `drivers/hsi/hsi.c`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// HSI port (Linux `struct hsi_port`).
pub struct HsiPort {
    pub id: u32,
    pub controller_id: u32,
    pub name: String,
    pub ops: HsiPortOps,
    pub client_ids: Vec<u32>,
    pub claim_count: u32,
    pub claimed_by: Option<u32>,
}

/// HSI port operations (Linux `struct hsi_port_ops`).
pub struct HsiPortOps {
    pub start_tx: fn(port_id: u32, client_id: u32) -> Result<(), &'static str>,
    pub stop_tx: fn(port_id: u32, client_id: u32) -> Result<(), &'static str>,
    pub release: fn(port_id: u32, client_id: u32) -> Result<(), &'static str>,
    pub flush: fn(port_id: u32, client_id: u32) -> Result<(), &'static str>,
    pub async_read: fn(port_id: u32, client_id: u32, buf: &mut [u8]) -> Result<usize, &'static str>,
    pub async_write: fn(port_id: u32, client_id: u32, data: &[u8]) -> Result<usize, &'static str>,
    pub setup: fn(port_id: u32, client_id: u32, config: &HsiConfig) -> Result<(), &'static str>,
}

/// HSI configuration (Linux `struct hsi_config`).
#[derive(Debug, Clone)]
pub struct HsiConfig {
    pub mode: HsiMode,
    pub channels: u32,
    pub tx_cfg: HsiTxConfig,
    pub rx_cfg: HsiRxConfig,
}

/// HSI mode (Linux `enum hsi_mode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HsiMode {
    Frame,
    Stream,
}

/// HSI TX configuration.
#[derive(Debug, Clone)]
pub struct HsiTxConfig {
    pub speed: u32,
    pub flow: HsiFlowType,
}

/// HSI RX configuration.
#[derive(Debug, Clone)]
pub struct HsiRxConfig {
    pub speed: u32,
    pub flow: HsiFlowType,
    pub mode: HsiRxMode,
}

/// HSI flow type (Linux `enum hsi_flow_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HsiFlowType {
    NoFlow,
    Flow,
}

/// HSI RX mode (Linux `enum hsi_rx_mode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HsiRxMode {
    Continuous,
    Burst,
}

/// HSI client device (Linux `struct hsi_client`).
pub struct HsiClient {
    pub id: u32,
    pub port_id: u32,
    pub name: String,
    pub config: Option<HsiConfig>,
    pub driver_name: Option<String>,
    pub bound: bool,
}

/// HSI controller (Linux `struct hsi_controller`).
pub struct HsiController {
    pub id: u32,
    pub name: String,
    pub port_ids: Vec<u32>,
    pub num_ports: u32,
}

/// HSI driver (Linux `struct hsi_driver`).
pub struct HsiDriver {
    pub name: String,
    pub probe: fn(client_id: u32) -> Result<(), &'static str>,
    pub remove: fn(client_id: u32) -> Result<(), &'static str>,
    pub event: Option<fn(port_id: u32, event: HsiEvent)>,
}

/// HSI event (Linux `enum hsi_event`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HsiEvent {
    BreakDetected,
    Error,
    RxData,
    TxError,
    RxError,
}

// ── Registry ────────────────────────────────────────────────────────────

static CTRL_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static PORT_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static CLIENT_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DRIVER_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static HSI_CTRLS: RwLock<BTreeMap<u32, HsiController>> = RwLock::new(BTreeMap::new());
static HSI_PORTS: RwLock<BTreeMap<u32, HsiPort>> = RwLock::new(BTreeMap::new());
static HSI_CLIENTS: RwLock<BTreeMap<u32, HsiClient>> = RwLock::new(BTreeMap::new());
static HSI_DRIVERS: RwLock<BTreeMap<u32, HsiDriver>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register an HSI controller.
pub fn register_controller(name: &str, num_ports: u32) -> Result<u32, &'static str> {
    let id = CTRL_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let ctrl = HsiController {
        id,
        name: String::from(name),
        port_ids: Vec::new(),
        num_ports,
    };
    HSI_CTRLS.write().insert(id, ctrl);
    Ok(id)
}

/// Create a port on an HSI controller.
pub fn create_port(ctrl_id: u32, name: &str, ops: HsiPortOps) -> Result<u32, &'static str> {
    let port_id = PORT_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let port = HsiPort {
        id: port_id,
        controller_id: ctrl_id,
        name: String::from(name),
        ops,
        client_ids: Vec::new(),
        claim_count: 0,
        claimed_by: None,
    };
    HSI_PORTS.write().insert(port_id, port);

    let mut ctrls = HSI_CTRLS.write();
    if let Some(ctrl) = ctrls.get_mut(&ctrl_id) {
        ctrl.port_ids.push(port_id);
    }
    Ok(port_id)
}

/// Register an HSI client on a port.
pub fn register_client(port_id: u32, name: &str) -> Result<u32, &'static str> {
    let client_id = CLIENT_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let client = HsiClient {
        id: client_id,
        port_id,
        name: String::from(name),
        config: None,
        driver_name: None,
        bound: false,
    };
    HSI_CLIENTS.write().insert(client_id, client);

    let mut ports = HSI_PORTS.write();
    if let Some(port) = ports.get_mut(&port_id) {
        port.client_ids.push(client_id);
    }

    try_match_driver(client_id)?;
    Ok(client_id)
}

/// Claim an HSI port (Linux `hsi_claim_port`).
pub fn claim_port(port_id: u32, client_id: u32) -> Result<(), &'static str> {
    let mut ports = HSI_PORTS.write();
    let port = ports.get_mut(&port_id).ok_or("HSI port not found")?;
    if port.claimed_by.is_some() && port.claimed_by != Some(client_id) {
        return Err("HSI port already claimed");
    }
    port.claimed_by = Some(client_id);
    port.claim_count += 1;
    Ok(())
}

/// Release an HSI port (Linux `hsi_release_port`).
pub fn release_port(port_id: u32, client_id: u32) -> Result<(), &'static str> {
    let release_fn = {
        let ports = HSI_PORTS.read();
        let port = ports.get(&port_id).ok_or("HSI port not found")?;
        if port.claimed_by != Some(client_id) {
            return Err("HSI port not claimed by this client");
        }
        port.ops.release
    };
    (release_fn)(port_id, client_id)?;

    let mut ports = HSI_PORTS.write();
    if let Some(port) = ports.get_mut(&port_id) {
        port.claim_count = port.claim_count.saturating_sub(1);
        if port.claim_count == 0 {
            port.claimed_by = None;
        }
    }
    Ok(())
}

/// Setup HSI client configuration (Linux `hsi_setup`).
pub fn setup_client(client_id: u32, config: HsiConfig) -> Result<(), &'static str> {
    let (port_id, setup_fn) = {
        let clients = HSI_CLIENTS.read();
        let client = clients.get(&client_id).ok_or("HSI client not found")?;
        let ports = HSI_PORTS.read();
        let port = ports.get(&client.port_id).ok_or("HSI port not found")?;
        (client.port_id, port.ops.setup)
    };
    (setup_fn)(port_id, client_id, &config)?;

    let mut clients = HSI_CLIENTS.write();
    if let Some(client) = clients.get_mut(&client_id) {
        client.config = Some(config);
    }
    Ok(())
}

/// Start TX on an HSI client (Linux `hsi_start_tx`).
pub fn start_tx(client_id: u32) -> Result<(), &'static str> {
    let (port_id, start_fn) = {
        let clients = HSI_CLIENTS.read();
        let client = clients.get(&client_id).ok_or("HSI client not found")?;
        let ports = HSI_PORTS.read();
        let port = ports.get(&client.port_id).ok_or("HSI port not found")?;
        (client.port_id, port.ops.start_tx)
    };
    (start_fn)(port_id, client_id)
}

/// Stop TX on an HSI client (Linux `hsi_stop_tx`).
pub fn stop_tx(client_id: u32) -> Result<(), &'static str> {
    let (port_id, stop_fn) = {
        let clients = HSI_CLIENTS.read();
        let client = clients.get(&client_id).ok_or("HSI client not found")?;
        let ports = HSI_PORTS.read();
        let port = ports.get(&client.port_id).ok_or("HSI port not found")?;
        (client.port_id, port.ops.stop_tx)
    };
    (stop_fn)(port_id, client_id)
}

/// Async read from HSI client (Linux `hsi_async_read`).
pub fn async_read(client_id: u32, buf: &mut [u8]) -> Result<usize, &'static str> {
    let (port_id, read_fn) = {
        let clients = HSI_CLIENTS.read();
        let client = clients.get(&client_id).ok_or("HSI client not found")?;
        let ports = HSI_PORTS.read();
        let port = ports.get(&client.port_id).ok_or("HSI port not found")?;
        (client.port_id, port.ops.async_read)
    };
    (read_fn)(port_id, client_id, buf)
}

/// Async write to HSI client (Linux `hsi_async_write`).
pub fn async_write(client_id: u32, data: &[u8]) -> Result<usize, &'static str> {
    let (port_id, write_fn) = {
        let clients = HSI_CLIENTS.read();
        let client = clients.get(&client_id).ok_or("HSI client not found")?;
        let ports = HSI_PORTS.read();
        let port = ports.get(&client.port_id).ok_or("HSI port not found")?;
        (client.port_id, port.ops.async_write)
    };
    (write_fn)(port_id, client_id, data)
}

/// Register an HSI driver.
pub fn register_driver(driver: HsiDriver) -> Result<u32, &'static str> {
    let id = DRIVER_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let drv_name = driver.name.clone();
    HSI_DRIVERS.write().insert(id, driver);

    let client_ids: Vec<u32> = {
        let clients = HSI_CLIENTS.read();
        clients
            .iter()
            .filter(|(_, c)| !c.bound && c.name.contains(&drv_name))
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
        let clients = HSI_CLIENTS.read();
        let client = match clients.get(&client_id) {
            Some(c) if !c.bound => c,
            _ => return Ok(()),
        };
        let client_name = client.name.clone();

        let drivers = HSI_DRIVERS.read();
        let mut found: Option<(fn(u32) -> Result<(), &'static str>, String)> = None;
        for (_, drv) in drivers.iter() {
            if client_name.contains(&drv.name) {
                found = Some((drv.probe, drv.name.clone()));
                break;
            }
        }
        found
    };

    if let Some((probe_fn, drv_name)) = matched {
        (probe_fn)(client_id)?;
        let mut clients = HSI_CLIENTS.write();
        if let Some(client) = clients.get_mut(&client_id) {
            client.bound = true;
            client.driver_name = Some(drv_name);
        }
    }
    Ok(())
}

/// List all HSI controllers.
pub fn list_controllers() -> Vec<(u32, String, u32)> {
    HSI_CTRLS
        .read()
        .iter()
        .map(|(id, c)| (*id, c.name.clone(), c.num_ports))
        .collect()
}

/// Count registered controllers.
pub fn controller_count() -> usize {
    HSI_CTRLS.read().len()
}

// ── Software HSI ────────────────────────────────────────────────────────

fn sw_start_tx(_port_id: u32, _client_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_stop_tx(_port_id: u32, _client_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_release(_port_id: u32, _client_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_flush(_port_id: u32, _client_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_async_read(_port_id: u32, _client_id: u32, buf: &mut [u8]) -> Result<usize, &'static str> {
    for b in buf.iter_mut() {
        *b = 0;
    }
    Ok(buf.len())
}
fn sw_async_write(_port_id: u32, _client_id: u32, data: &[u8]) -> Result<usize, &'static str> {
    Ok(data.len())
}
fn sw_setup(_port_id: u32, _client_id: u32, _config: &HsiConfig) -> Result<(), &'static str> {
    Ok(())
}

/// Software HSI port ops.
pub fn software_hsi_port_ops() -> HsiPortOps {
    HsiPortOps {
        start_tx: sw_start_tx,
        stop_tx: sw_stop_tx,
        release: sw_release,
        flush: sw_flush,
        async_read: sw_async_read,
        async_write: sw_async_write,
        setup: sw_setup,
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
    crate::serial_println!("hsi: subsystem ready");
    Ok(())
}
