//! Bluetooth subsystem
//!
//! Provides Bluetooth HCI framework for Bluetooth adapters and protocols.
//! Mirrors Linux's `net/bluetooth/`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// Bluetooth adapter (Linux `struct hci_dev`).
pub struct HciDev {
    pub id: u32,
    pub name: String,
    pub bdaddr: [u8; 6],
    pub dev_type: HciDevType,
    pub bus: HciBus,
    pub state: HciState,
    pub features: [u8; 8],
    pub pkt_type: u16,
    pub link_policy: u16,
    pub voice_setting: u16,
    pub conn_ids: Vec<u32>,
    pub ops: HciOps,
}

/// HCI device type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HciDevType {
    Bredr,
    Amp,
    Primary,
}

/// HCI bus type (Linux `enum hci_bus`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HciBus {
    Virtual,
    Usb,
    Pcmcia,
    Uart,
    Sdio,
    Spi,
    I2c,
    Smd,
    Other,
}

/// HCI state (Linux `enum hci_state`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HciState {
    Reset,
    Down,
    Initialized,
    Running,
    Closing,
}

/// HCI operations (Linux `struct hci_dev_ops`).
pub struct HciOps {
    pub open: fn(dev_id: u32) -> Result<(), &'static str>,
    pub close: fn(dev_id: u32) -> Result<(), &'static str>,
    pub flush: fn(dev_id: u32) -> Result<(), &'static str>,
    pub send_cmd: fn(dev_id: u32, opcode: u16, data: &[u8]) -> Result<(), &'static str>,
    pub send_acl: fn(dev_id: u32, handle: u16, data: &[u8]) -> Result<(), &'static str>,
    pub setup: fn(dev_id: u32) -> Result<(), &'static str>,
}

/// Bluetooth connection (Linux `struct hci_conn`).
pub struct HciConn {
    pub id: u32,
    pub dev_id: u32,
    pub handle: u16,
    pub bdaddr: [u8; 6],
    pub type_: HciConnType,
    pub state: HciConnState,
    pub role: HciRole,
    pub link_key: Option<[u8; 16]>,
    pub interval: u16,
    pub latency: u16,
    pub timeout: u16,
}

/// HCI connection type (Linux `enum hci_conn_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HciConnType {
    Acl,
    Sco,
    Le,
    Iso,
}

/// HCI connection state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HciConnState {
    Open,
    Connecting,
    Connected,
    Disconnected,
    Closing,
}

/// HCI role (Linux `enum hci_role`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HciRole {
    Master,
    Slave,
}

/// Bluetooth L2CAP channel (Linux `struct l2cap_chan`).
pub struct L2capChan {
    pub id: u32,
    pub conn_id: u32,
    pub scid: u16,
    pub dcid: u16,
    pub psm: u16,
    pub state: L2capState,
    pub mode: L2capMode,
    pub imtu: u16,
    pub omtu: u16,
}

/// L2CAP channel state (Linux `enum l2cap_chan_state`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum L2capState {
    Open,
    Connect,
    Connected,
    Disconnect,
    Closed,
}

/// L2CAP mode (Linux `enum l2cap_mode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum L2capMode {
    Basic,
    Lem,
    Ertm,
    Streaming,
}

// ── Registry ────────────────────────────────────────────────────────────

static DEV_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static CONN_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static CHAN_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static HCI_DEVS: RwLock<BTreeMap<u32, HciDev>> = RwLock::new(BTreeMap::new());
static HCI_CONNS: RwLock<BTreeMap<u32, HciConn>> = RwLock::new(BTreeMap::new());
static L2CAP_CHANS: RwLock<BTreeMap<u32, L2capChan>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register an HCI device (Linux `hci_register_dev`).
pub fn register_device(
    name: &str,
    bdaddr: [u8; 6],
    dev_type: HciDevType,
    bus: HciBus,
    ops: HciOps,
) -> Result<u32, &'static str> {
    let id = DEV_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dev = HciDev {
        id,
        name: String::from(name),
        bdaddr,
        dev_type,
        bus,
        state: HciState::Down,
        features: [0; 8],
        pkt_type: 0x0008 | 0x0010,    // ACL + SCO
        link_policy: 0x0001 | 0x0002, // REPEATING + HOLD
        voice_setting: 0x0060,
        conn_ids: Vec::new(),
        ops,
    };
    HCI_DEVS.write().insert(id, dev);
    Ok(id)
}

/// Open an HCI device (Linux `hci_open_dev`).
pub fn open_device(dev_id: u32) -> Result<(), &'static str> {
    let (open_fn, setup_fn) = {
        let devs = HCI_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("HCI device not found")?;
        (dev.ops.open, dev.ops.setup)
    };
    (open_fn)(dev_id)?;

    let mut devs = HCI_DEVS.write();
    if let Some(dev) = devs.get_mut(&dev_id) {
        dev.state = HciState::Initialized;
    }
    drop(devs);

    (setup_fn)(dev_id)?;

    let mut devs = HCI_DEVS.write();
    if let Some(dev) = devs.get_mut(&dev_id) {
        dev.state = HciState::Running;
    }
    Ok(())
}

/// Close an HCI device (Linux `hci_close_dev`).
pub fn close_device(dev_id: u32) -> Result<(), &'static str> {
    let close_fn = {
        let devs = HCI_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("HCI device not found")?;
        dev.ops.close
    };
    (close_fn)(dev_id)?;

    let mut devs = HCI_DEVS.write();
    if let Some(dev) = devs.get_mut(&dev_id) {
        dev.state = HciState::Closing;
    }
    Ok(())
}

/// Send an HCI command (Linux `hci_send_cmd`).
pub fn send_cmd(dev_id: u32, opcode: u16, data: &[u8]) -> Result<(), &'static str> {
    let send_fn = {
        let devs = HCI_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("HCI device not found")?;
        if dev.state != HciState::Running {
            return Err("HCI device not running");
        }
        dev.ops.send_cmd
    };
    (send_fn)(dev_id, opcode, data)
}

/// Create a connection (Linux `hci_connect`).
pub fn create_connection(
    dev_id: u32,
    bdaddr: [u8; 6],
    conn_type: HciConnType,
) -> Result<u32, &'static str> {
    let conn_id = CONN_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let handle = (conn_id as u16) | 0x1000;
    let conn = HciConn {
        id: conn_id,
        dev_id,
        handle,
        bdaddr,
        type_: conn_type,
        state: HciConnState::Connecting,
        role: HciRole::Master,
        link_key: None,
        interval: 0x0006,
        latency: 0x0000,
        timeout: 0x07D0,
    };
    HCI_CONNS.write().insert(conn_id, conn);

    let mut devs = HCI_DEVS.write();
    if let Some(dev) = devs.get_mut(&dev_id) {
        dev.conn_ids.push(conn_id);
    }

    // Simulate connection completion
    let mut conns = HCI_CONNS.write();
    if let Some(c) = conns.get_mut(&conn_id) {
        c.state = HciConnState::Connected;
    }

    Ok(conn_id)
}

/// Disconnect (Linux `hci_disconnect`).
pub fn disconnect(conn_id: u32) -> Result<(), &'static str> {
    let dev_id = {
        let mut conns = HCI_CONNS.write();
        let conn = conns.get_mut(&conn_id).ok_or("HCI connection not found")?;
        conn.state = HciConnState::Disconnected;
        conn.dev_id
    };

    let mut devs = HCI_DEVS.write();
    if let Some(dev) = devs.get_mut(&dev_id) {
        dev.conn_ids.retain(|&id| id != conn_id);
    }
    Ok(())
}

/// Create an L2CAP channel (Linux `l2cap_chan_create`).
pub fn create_l2cap_channel(conn_id: u32, psm: u16) -> Result<u32, &'static str> {
    let chan_id = CHAN_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let chan = L2capChan {
        id: chan_id,
        conn_id,
        scid: 0x0040 + chan_id as u16,
        dcid: 0x0040 + chan_id as u16,
        psm,
        state: L2capState::Connect,
        mode: L2capMode::Basic,
        imtu: 672,
        omtu: 672,
    };
    L2CAP_CHANS.write().insert(chan_id, chan);

    // Simulate connection
    let mut chans = L2CAP_CHANS.write();
    if let Some(c) = chans.get_mut(&chan_id) {
        c.state = L2capState::Connected;
    }
    Ok(chan_id)
}

/// Close an L2CAP channel (Linux `l2cap_chan_close`).
pub fn close_l2cap_channel(chan_id: u32) -> Result<(), &'static str> {
    let mut chans = L2CAP_CHANS.write();
    if let Some(c) = chans.get_mut(&chan_id) {
        c.state = L2capState::Closed;
    }
    Ok(())
}

/// Send data over an L2CAP channel (Linux `l2cap_sock_sendmsg`).
pub fn l2cap_send(chan_id: u32, data: &[u8]) -> Result<usize, &'static str> {
    let (conn_id, _dcid) = {
        let chans = L2CAP_CHANS.read();
        let chan = chans.get(&chan_id).ok_or("L2CAP channel not found")?;
        if chan.state != L2capState::Connected {
            return Err("L2CAP channel not connected");
        }
        (chan.conn_id, chan.dcid)
    };

    let (dev_id, handle) = {
        let conns = HCI_CONNS.read();
        let conn = conns.get(&conn_id).ok_or("HCI connection not found")?;
        if conn.state != HciConnState::Connected {
            return Err("HCI connection not active");
        }
        (conn.dev_id, conn.handle)
    };

    let send_fn = {
        let devs = HCI_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("HCI device not found")?;
        dev.ops.send_acl
    };
    (send_fn)(dev_id, handle, data)?;
    Ok(data.len())
}

/// List all HCI devices.
pub fn list_devices() -> Vec<(u32, String, HciState, usize)> {
    HCI_DEVS
        .read()
        .iter()
        .map(|(id, d)| (*id, d.name.clone(), d.state, d.conn_ids.len()))
        .collect()
}

/// Count registered devices.
pub fn device_count() -> usize {
    HCI_DEVS.read().len()
}

// ── Software Bluetooth ──────────────────────────────────────────────────

fn sw_open(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_close(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_flush(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_send_cmd(_dev_id: u32, _opcode: u16, _data: &[u8]) -> Result<(), &'static str> {
    Ok(())
}
fn sw_send_acl(_dev_id: u32, _handle: u16, _data: &[u8]) -> Result<(), &'static str> {
    Ok(())
}
fn sw_setup(dev_id: u32) -> Result<(), &'static str> {
    let mut devs = HCI_DEVS.write();
    if let Some(dev) = devs.get_mut(&dev_id) {
        dev.features[0] = 0xFF;
        dev.features[1] = 0xFF;
    }
    Ok(())
}

/// Software HCI ops.
pub fn software_hci_ops() -> HciOps {
    HciOps {
        open: sw_open,
        close: sw_close,
        flush: sw_flush,
        send_cmd: sw_send_cmd,
        send_acl: sw_send_acl,
        setup: sw_setup,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    if !HCI_DEVS.read().is_empty() {
        return Ok(());
    }

    let ops = software_hci_ops();
    let dev_id = register_device("sw-hci0", [0; 6], HciDevType::Primary, HciBus::Virtual, ops)?;
    crate::serial_println!("bt: software HCI device registered (id={})", dev_id);
    Ok(())
}
