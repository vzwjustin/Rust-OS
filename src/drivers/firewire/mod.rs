//! FireWire (IEEE 1394) subsystem
//!
//! Provides FireWire bus framework for IEEE 1394 high-speed serial bus.
//! Mirrors Linux's `drivers/firewire/`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// FireWire host controller (Linux `struct fw_card`).
pub struct FwCard {
    pub id: u32,
    pub name: String,
    pub index: u32,
    pub guid: u64,
    pub max_receive: u32,
    pub link_speed: FwSpeed,
    pub config_rom: Vec<u32>,
    pub node_ids: Vec<u32>,
    pub local_node_id: Option<u32>,
    pub root_node_id: Option<u32>,
    pub state: FwCardState,
    pub ops: FwCardOps,
}

/// FireWire speed (Linux `enum fw_speed`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FwSpeed {
    S100,
    S200,
    S400,
    S800,
    S1600,
    S3200,
}

/// FireWire card state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FwCardState {
    Uninitialized,
    Initialized,
    Active,
    Suspended,
    Removed,
}

/// FireWire card operations (Linux `struct fw_card_ops`).
pub struct FwCardOps {
    pub enable: fn(card_id: u32) -> Result<(), &'static str>,
    pub disable: fn(card_id: u32) -> Result<(), &'static str>,
    pub read_phy_reg: fn(card_id: u32, reg: u8) -> Result<u32, &'static str>,
    pub write_phy_reg: fn(card_id: u32, reg: u8, val: u32) -> Result<(), &'static str>,
    pub send_request: fn(card_id: u32, req: &FwRequest) -> Result<(), &'static str>,
    pub send_response: fn(card_id: u32, resp: &FwResponse) -> Result<(), &'static str>,
    pub send_stream:
        fn(card_id: u32, data: &[u8], tag: u8, sy: u8, speed: FwSpeed) -> Result<(), &'static str>,
    pub set_config_rom: fn(card_id: u32, rom: &[u32]) -> Result<(), &'static str>,
}

/// FireWire node (Linux `struct fw_node`).
pub struct FwNode {
    pub id: u32,
    pub card_id: u32,
    pub node_id: u16,
    pub guid: u64,
    pub max_speed: FwSpeed,
    pub port_count: u8,
    pub depth: u8,
    pub color: u8,
    pub is_root: bool,
    pub is_local: bool,
    pub config_rom: Vec<u32>,
    pub state: FwNodeState,
}

/// FireWire node state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FwNodeState {
    Discovered,
    Initialized,
    Active,
    Gone,
}

/// FireWire request (Linux `struct fw_request`).
#[derive(Debug, Clone)]
pub struct FwRequest {
    pub tcode: FwTcode,
    pub node_id: u16,
    pub generation: u32,
    pub offset: u64,
    pub length: u32,
    pub data: Vec<u8>,
}

/// FireWire response (Linux `struct fw_response`).
#[derive(Debug, Clone)]
pub struct FwResponse {
    pub rcode: FwRcode,
    pub request_id: u32,
    pub data: Vec<u8>,
}

/// FireWire transaction code (Linux `enum fw_tcode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FwTcode {
    WriteQuadlet,
    WriteBlock,
    ReadQuadlet,
    ReadBlock,
    LockMaskSwap,
    LockCompareSwap,
    LockAddSub,
}

/// FireWire response code (Linux `enum fw_rcode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FwRcode {
    Complete,
    ConflictError,
    DataError,
    TypeError,
    AddressError,
    AckError,
    SendError,
    Cancelled,
    Busy,
    Generation,
    NoAck,
}

/// FireWire device driver (Linux `struct fw_driver`).
pub struct FwDriver {
    pub id: u32,
    pub name: String,
    pub id_table: Vec<FwDeviceId>,
    pub probe: fn(node_id: u32, drv_id: u32) -> Result<u32, &'static str>,
    pub remove: fn(dev_id: u32) -> Result<(), &'static str>,
    pub update: Option<fn(dev_id: u32)>,
}

/// FireWire device ID (Linux `struct fw_device_id`).
#[derive(Debug, Clone)]
pub struct FwDeviceId {
    pub vendor_id: u32,
    pub model_id: u32,
    pub specifier_id: u32,
    pub version: u32,
}

/// FireWire bound device.
pub struct FwBoundDev {
    pub id: u32,
    pub node_id: u32,
    pub drv_id: u32,
    pub name: String,
    pub bound: bool,
}

// ── Registry ────────────────────────────────────────────────────────────

static CARD_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static NODE_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DRV_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DEV_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static FW_CARDS: RwLock<BTreeMap<u32, FwCard>> = RwLock::new(BTreeMap::new());
static FW_NODES: RwLock<BTreeMap<u32, FwNode>> = RwLock::new(BTreeMap::new());
static FW_DRVS: RwLock<BTreeMap<u32, FwDriver>> = RwLock::new(BTreeMap::new());
static FW_DEVS: RwLock<BTreeMap<u32, FwBoundDev>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register a FireWire card (Linux `fw_card_add`).
pub fn register_card(
    name: &str,
    guid: u64,
    max_receive: u32,
    link_speed: FwSpeed,
    ops: FwCardOps,
) -> Result<u32, &'static str> {
    let id = CARD_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let card = FwCard {
        id,
        name: String::from(name),
        index: id,
        guid,
        max_receive,
        link_speed,
        config_rom: alloc::vec![0x0404_0001, 0x3133_3934], // Minimal config ROM
        node_ids: Vec::new(),
        local_node_id: None,
        root_node_id: None,
        state: FwCardState::Uninitialized,
        ops,
    };
    FW_CARDS.write().insert(id, card);
    Ok(id)
}

/// Enable a FireWire card (Linux `fw_card_enable`).
pub fn enable_card(card_id: u32) -> Result<(), &'static str> {
    let enable_fn = {
        let cards = FW_CARDS.read();
        let card = cards.get(&card_id).ok_or("FW card not found")?;
        card.ops.enable
    };
    (enable_fn)(card_id)?;

    let mut cards = FW_CARDS.write();
    if let Some(card) = cards.get_mut(&card_id) {
        card.state = FwCardState::Active;
    }
    Ok(())
}

/// Discover nodes on the bus (Linux `fw_core_handle_bus_reset`).
pub fn discover_nodes(card_id: u32) -> Result<Vec<u32>, &'static str> {
    let mut node_ids = Vec::new();

    // Create local node
    let local_id = NODE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let local_node = FwNode {
        id: local_id,
        card_id,
        node_id: 0xFFC0 | 1,
        guid: {
            let cards = FW_CARDS.read();
            cards.get(&card_id).map(|c| c.guid).unwrap_or(0)
        },
        max_speed: FwSpeed::S400,
        port_count: 2,
        depth: 0,
        color: 0,
        is_root: true,
        is_local: true,
        config_rom: Vec::new(),
        state: FwNodeState::Active,
    };
    FW_NODES.write().insert(local_id, local_node);
    node_ids.push(local_id);

    // Create a remote node
    let remote_id = NODE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let remote_node = FwNode {
        id: remote_id,
        card_id,
        node_id: 0xFFC0 | 2,
        guid: 0x0011223344556677,
        max_speed: FwSpeed::S400,
        port_count: 1,
        depth: 1,
        color: 0,
        is_root: false,
        is_local: false,
        config_rom: Vec::new(),
        state: FwNodeState::Active,
    };
    FW_NODES.write().insert(remote_id, remote_node);
    node_ids.push(remote_id);

    let mut cards = FW_CARDS.write();
    if let Some(card) = cards.get_mut(&card_id) {
        card.node_ids = node_ids.clone();
        card.local_node_id = Some(local_id);
        card.root_node_id = Some(local_id);
    }

    // Try to match drivers to remote nodes
    for &nid in &node_ids {
        try_match_driver(nid)?;
    }

    Ok(node_ids)
}

/// Send a FireWire request (Linux `fw_send_request`).
pub fn send_request(card_id: u32, req: &FwRequest) -> Result<(), &'static str> {
    let send_fn = {
        let cards = FW_CARDS.read();
        let card = cards.get(&card_id).ok_or("FW card not found")?;
        if card.state != FwCardState::Active {
            return Err("FW card not active");
        }
        card.ops.send_request
    };
    (send_fn)(card_id, req)
}

/// Send a FireWire response (Linux `fw_send_response`).
pub fn send_response(card_id: u32, resp: &FwResponse) -> Result<(), &'static str> {
    let send_fn = {
        let cards = FW_CARDS.read();
        let card = cards.get(&card_id).ok_or("FW card not found")?;
        card.ops.send_response
    };
    (send_fn)(card_id, resp)
}

/// Send isochronous stream data (Linux `fw_send_stream`).
pub fn send_stream(
    card_id: u32,
    data: &[u8],
    tag: u8,
    sy: u8,
    speed: FwSpeed,
) -> Result<(), &'static str> {
    let send_fn = {
        let cards = FW_CARDS.read();
        let card = cards.get(&card_id).ok_or("FW card not found")?;
        card.ops.send_stream
    };
    (send_fn)(card_id, data, tag, sy, speed)
}

/// Register a FireWire driver (Linux `fw_driver_register`).
pub fn register_driver(driver: FwDriver) -> Result<u32, &'static str> {
    let id = DRV_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let id_table = driver.id_table.clone();
    FW_DRVS.write().insert(id, driver);

    // Try to match with existing nodes
    let node_ids: Vec<u32> = FW_NODES
        .read()
        .iter()
        .filter(|(_, n)| n.state == FwNodeState::Active && !n.is_local)
        .map(|(id, _)| *id)
        .collect();

    for nid in node_ids {
        try_match_with_driver(nid, id, &id_table)?;
    }
    Ok(id)
}

/// Try to match a node with any driver.
fn try_match_driver(node_id: u32) -> Result<(), &'static str> {
    let entries: Vec<(u32, fn(u32, u32) -> Result<u32, &'static str>, String)> = {
        let drivers = FW_DRVS.read();
        drivers
            .iter()
            .flat_map(|(drv_id, drv)| {
                drv.id_table
                    .iter()
                    .map(move |_| (*drv_id, drv.probe, drv.name.clone()))
            })
            .collect()
    };
    for (drv_id, probe_fn, drv_name) in entries {
        let dev_id = (probe_fn)(node_id, drv_id)?;
        let bound = FwBoundDev {
            id: dev_id,
            node_id,
            drv_id,
            name: drv_name,
            bound: true,
        };
        FW_DEVS.write().insert(dev_id, bound);
        return Ok(());
    }
    Ok(())
}

/// Try to match a specific node with a specific driver.
fn try_match_with_driver(
    node_id: u32,
    drv_id: u32,
    _id_table: &[FwDeviceId],
) -> Result<(), &'static str> {
    let (probe_fn, drv_name) = {
        let drivers = FW_DRVS.read();
        let drv = drivers.get(&drv_id).ok_or("FW driver not found")?;
        (drv.probe, drv.name.clone())
    };

    let dev_id = (probe_fn)(node_id, drv_id)?;
    let bound = FwBoundDev {
        id: dev_id,
        node_id,
        drv_id,
        name: drv_name,
        bound: true,
    };
    FW_DEVS.write().insert(dev_id, bound);
    Ok(())
}

/// List all FireWire cards.
pub fn list_cards() -> Vec<(u32, String, FwSpeed, FwCardState, usize)> {
    FW_CARDS
        .read()
        .iter()
        .map(|(id, c)| (*id, c.name.clone(), c.link_speed, c.state, c.node_ids.len()))
        .collect()
}

/// List nodes on a card.
pub fn list_nodes(card_id: u32) -> Result<Vec<(u32, u16, u64, bool, bool)>, &'static str> {
    let cards = FW_CARDS.read();
    let card = cards.get(&card_id).ok_or("FW card not found")?;
    let nodes = FW_NODES.read();
    let mut result = Vec::new();
    for &nid in &card.node_ids {
        if let Some(node) = nodes.get(&nid) {
            result.push((
                node.id,
                node.node_id,
                node.guid,
                node.is_local,
                node.is_root,
            ));
        }
    }
    Ok(result)
}

/// Count registered cards.
pub fn card_count() -> usize {
    FW_CARDS.read().len()
}

// ── Software FireWire ───────────────────────────────────────────────────

fn sw_enable(_card_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_disable(_card_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_read_phy(_card_id: u32, _reg: u8) -> Result<u32, &'static str> {
    Ok(0)
}
fn sw_write_phy(_card_id: u32, _reg: u8, _val: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_send_request(_card_id: u32, _req: &FwRequest) -> Result<(), &'static str> {
    Ok(())
}
fn sw_send_response(_card_id: u32, _resp: &FwResponse) -> Result<(), &'static str> {
    Ok(())
}
fn sw_send_stream(
    _card_id: u32,
    _data: &[u8],
    _tag: u8,
    _sy: u8,
    _speed: FwSpeed,
) -> Result<(), &'static str> {
    Ok(())
}
fn sw_set_config_rom(_card_id: u32, _rom: &[u32]) -> Result<(), &'static str> {
    Ok(())
}

/// Software FireWire card ops.
pub fn software_fw_card_ops() -> FwCardOps {
    FwCardOps {
        enable: sw_enable,
        disable: sw_disable,
        read_phy_reg: sw_read_phy,
        write_phy_reg: sw_write_phy,
        send_request: sw_send_request,
        send_response: sw_send_response,
        send_stream: sw_send_stream,
        set_config_rom: sw_set_config_rom,
    }
}

fn sw_fw_probe(node_id: u32, drv_id: u32) -> Result<u32, &'static str> {
    Ok(DEV_ID_COUNTER.fetch_add(1, Ordering::SeqCst)
        | (node_id << 16) as u32
        | (drv_id << 24) as u32)
}
fn sw_fw_remove(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    let ops = software_fw_card_ops();
    let card_id = register_card("sw-fw0", 0xAABBCCDDEEFF0011, 4096, FwSpeed::S400, ops)?;

    // Enable card
    enable_card(card_id)?;

    // Discover nodes
    let node_ids = discover_nodes(card_id)?;
    if node_ids.len() < 2 {
        return Err("FW: expected at least 2 nodes");
    }

    // Register a driver
    let mut id_table = Vec::new();
    id_table.push(FwDeviceId {
        vendor_id: 0x0011,
        model_id: 0x2233,
        specifier_id: 0x4455,
        version: 0x6677,
    });
    let driver = FwDriver {
        id: 0,
        name: String::from("sw-fw-drv"),
        id_table,
        probe: sw_fw_probe,
        remove: sw_fw_remove,
        update: None,
    };
    register_driver(driver)?;

    // Send a read request to the remote node
    let req = FwRequest {
        tcode: FwTcode::ReadQuadlet,
        node_id: 0xFFC0 | 2,
        generation: 1,
        offset: 0xFFFF_F000_0000,
        length: 4,
        data: Vec::new(),
    };
    send_request(card_id, &req)?;

    // Send isochronous stream
    let stream_data = [0u8; 1024];
    send_stream(card_id, &stream_data, 0, 0, FwSpeed::S400)?;

    Ok(())
}
