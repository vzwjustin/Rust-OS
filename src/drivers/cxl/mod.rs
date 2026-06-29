//! CXL (Compute Express Link) subsystem
//!
//! Provides CXL bus for coherent interconnect between host and devices.
//! Mirrors Linux's `drivers/cxl/`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// CXL device type (Linux `enum cxl_devtype`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CxlDevType {
    Memdev,
    Port,
    Decoder,
    Endpoint,
    Region,
    HdmDecoder,
}

/// CXL device (Linux `struct cxl_dev`).
pub struct CxlDevice {
    pub id: u32,
    pub name: String,
    pub dev_type: CxlDevType,
    pub parent_id: Option<u32>,
    pub child_ids: Vec<u32>,
    pub state: CxlDevState,
    pub serial: u64,
}

/// CXL device state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CxlDevState {
    Unbound,
    Bound,
    ProbeFailed,
    Dead,
}

/// CXL memory device (Linux `struct cxl_memdev`).
pub struct CxlMemdev {
    pub id: u32,
    pub dev_id: u32,
    pub name: String,
    pub serial: u64,
    pub pmem_size: u64,
    pub ram_size: u64,
    pub payload_max: u32,
    pub ram_range: (u64, u64),
    pub pmem_range: (u64, u64),
    pub host_bridge: u32,
}

/// CXL port (Linux `struct cxl_port`).
pub struct CxlPort {
    pub id: u32,
    pub dev_id: u32,
    pub name: String,
    pub port_type: CxlPortType,
    pub parent_dport: Option<u32>,
    pub host_bridge_id: Option<u32>,
    pub decoder_ids: Vec<u32>,
}

/// CXL port type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CxlPortType {
    Root,
    Switch,
    Endpoint,
}

/// CXL decoder (Linux `struct cxl_decoder`).
pub struct CxlDecoder {
    pub id: u32,
    pub dev_id: u32,
    pub port_id: u32,
    pub name: String,
    pub start: u64,
    pub size: u64,
    pub interleave_ways: u32,
    pub interleave_granularity: u32,
    pub target_count: u32,
    pub target_ids: Vec<u32>,
    pub decode_mode: CxlDecodeMode,
    pub locked: bool,
}

/// CXL decode mode (Linux `enum cxl_decoder_mode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CxlDecodeMode {
    None,
    Ram,
    Pmem,
    Mixed,
}

/// CXL region (Linux `struct cxl_region`).
pub struct CxlRegion {
    pub id: u32,
    pub dev_id: u32,
    pub name: String,
    pub uuid: [u8; 16],
    pub size: u64,
    pub interleave_ways: u32,
    pub interleave_granularity: u32,
    pub target_ids: Vec<u32>,
    pub decode_mode: CxlDecodeMode,
    pub active: bool,
}

/// CXL mailbox commands (Linux `struct cxl_mbox_cmd`).
#[derive(Debug, Clone)]
pub struct CxlMboxCmd {
    pub opcode: u32,
    pub payload_in: Vec<u8>,
    pub payload_out: Vec<u8>,
    pub return_code: u32,
}

/// CXL operations.
pub struct CxlOps {
    pub mbox_send: fn(memdev_id: u32, cmd: &mut CxlMboxCmd) -> Result<(), &'static str>,
    pub get_health: fn(memdev_id: u32) -> Result<CxlHealthInfo, &'static str>,
}

/// CXL health info (Linux `struct cxl_memdev_health_info`).
#[derive(Debug, Clone)]
pub struct CxlHealthInfo {
    pub maintenance_required: bool,
    pub performance_degraded: bool,
    pub hw_replacement_needed: bool,
    pub media_normal: bool,
    pub media_not_ready: bool,
    pub media_persistence_lost: bool,
    pub media_data_lost: bool,
    pub media_powerloss_persistence_loss: bool,
    pub media_shutdown: bool,
}

// ── Registry ────────────────────────────────────────────────────────────

static DEV_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static MEMDEV_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static PORT_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DECODER_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static REGION_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static CXL_DEVS: RwLock<BTreeMap<u32, CxlDevice>> = RwLock::new(BTreeMap::new());
static CXL_MEMDEVS: RwLock<BTreeMap<u32, CxlMemdev>> = RwLock::new(BTreeMap::new());
static CXL_PORTS: RwLock<BTreeMap<u32, CxlPort>> = RwLock::new(BTreeMap::new());
static CXL_DECODERS: RwLock<BTreeMap<u32, CxlDecoder>> = RwLock::new(BTreeMap::new());
static CXL_REGIONS: RwLock<BTreeMap<u32, CxlRegion>> = RwLock::new(BTreeMap::new());
static CXL_OPS: RwLock<BTreeMap<u32, CxlOps>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register a CXL device.
pub fn register_device(
    name: &str,
    dev_type: CxlDevType,
    parent_id: Option<u32>,
    serial: u64,
) -> Result<u32, &'static str> {
    let id = DEV_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dev = CxlDevice {
        id,
        name: String::from(name),
        dev_type,
        parent_id,
        child_ids: Vec::new(),
        state: CxlDevState::Unbound,
        serial,
    };
    CXL_DEVS.write().insert(id, dev);

    if let Some(pid) = parent_id {
        let mut devs = CXL_DEVS.write();
        if let Some(parent) = devs.get_mut(&pid) {
            parent.child_ids.push(id);
        }
    }
    Ok(id)
}

/// Register a CXL memory device.
pub fn register_memdev(
    name: &str,
    serial: u64,
    pmem_size: u64,
    ram_size: u64,
    host_bridge: u32,
    ops: CxlOps,
) -> Result<u32, &'static str> {
    let dev_id = register_device(name, CxlDevType::Memdev, None, serial)?;

    let memdev_id = MEMDEV_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let memdev = CxlMemdev {
        id: memdev_id,
        dev_id,
        name: String::from(name),
        serial,
        pmem_size,
        ram_size,
        payload_max: 256,
        ram_range: (0, ram_size),
        pmem_range: (ram_size, ram_size + pmem_size),
        host_bridge,
    };
    CXL_MEMDEVS.write().insert(memdev_id, memdev);
    CXL_OPS.write().insert(memdev_id, ops);
    Ok(memdev_id)
}

/// Register a CXL port.
pub fn register_port(
    name: &str,
    port_type: CxlPortType,
    parent_dport: Option<u32>,
    host_bridge_id: Option<u32>,
) -> Result<u32, &'static str> {
    let dev_id = register_device(name, CxlDevType::Port, None, 0)?;

    let port_id = PORT_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let port = CxlPort {
        id: port_id,
        dev_id,
        name: String::from(name),
        port_type,
        parent_dport,
        host_bridge_id,
        decoder_ids: Vec::new(),
    };
    CXL_PORTS.write().insert(port_id, port);
    Ok(port_id)
}

/// Register a CXL decoder.
pub fn register_decoder(
    port_id: u32,
    start: u64,
    size: u64,
    interleave_ways: u32,
    interleave_granularity: u32,
    decode_mode: CxlDecodeMode,
) -> Result<u32, &'static str> {
    let dev_id = register_device(
        &alloc::format!("decoder{}", DECODER_ID_COUNTER.load(Ordering::SeqCst)),
        CxlDevType::Decoder,
        None,
        0,
    )?;

    let decoder_id = DECODER_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let decoder = CxlDecoder {
        id: decoder_id,
        dev_id,
        port_id,
        name: alloc::format!("decoder{}.{}", port_id, decoder_id),
        start,
        size,
        interleave_ways,
        interleave_granularity,
        target_count: 0,
        target_ids: Vec::new(),
        decode_mode,
        locked: false,
    };
    CXL_DECODERS.write().insert(decoder_id, decoder);

    let mut ports = CXL_PORTS.write();
    if let Some(port) = ports.get_mut(&port_id) {
        port.decoder_ids.push(decoder_id);
    }
    Ok(decoder_id)
}

/// Register a CXL region.
pub fn register_region(
    name: &str,
    size: u64,
    interleave_ways: u32,
    interleave_granularity: u32,
    target_ids: Vec<u32>,
    decode_mode: CxlDecodeMode,
) -> Result<u32, &'static str> {
    let dev_id = register_device(name, CxlDevType::Region, None, 0)?;

    let region_id = REGION_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let region = CxlRegion {
        id: region_id,
        dev_id,
        name: String::from(name),
        uuid: [0u8; 16],
        size,
        interleave_ways,
        interleave_granularity,
        target_ids,
        decode_mode,
        active: false,
    };
    CXL_REGIONS.write().insert(region_id, region);
    Ok(region_id)
}

/// Send a mailbox command to a CXL memory device (Linux `cxl_mbox_send`).
pub fn mbox_send(memdev_id: u32, cmd: &mut CxlMboxCmd) -> Result<(), &'static str> {
    let send_fn = {
        let ops = CXL_OPS.read();
        let dev_ops = ops.get(&memdev_id).ok_or("CXL ops not found")?;
        dev_ops.mbox_send
    };
    (send_fn)(memdev_id, cmd)
}

/// Get health info for a CXL memory device.
pub fn get_health(memdev_id: u32) -> Result<CxlHealthInfo, &'static str> {
    let health_fn = {
        let ops = CXL_OPS.read();
        let dev_ops = ops.get(&memdev_id).ok_or("CXL ops not found")?;
        dev_ops.get_health
    };
    (health_fn)(memdev_id)
}

/// Activate a CXL region.
pub fn activate_region(region_id: u32) -> Result<(), &'static str> {
    let mut regions = CXL_REGIONS.write();
    let region = regions.get_mut(&region_id).ok_or("CXL region not found")?;
    region.active = true;
    Ok(())
}

/// List all CXL memory devices.
pub fn list_memdevs() -> Vec<(u32, String, u64, u64)> {
    CXL_MEMDEVS
        .read()
        .iter()
        .map(|(id, m)| (*id, m.name.clone(), m.ram_size, m.pmem_size))
        .collect()
}

/// List all CXL ports.
pub fn list_ports() -> Vec<(u32, String, CxlPortType)> {
    CXL_PORTS
        .read()
        .iter()
        .map(|(id, p)| (*id, p.name.clone(), p.port_type))
        .collect()
}

/// Count registered memory devices.
pub fn memdev_count() -> usize {
    CXL_MEMDEVS.read().len()
}

// ── Software CXL ────────────────────────────────────────────────────────

fn sw_mbox_send(_memdev_id: u32, cmd: &mut CxlMboxCmd) -> Result<(), &'static str> {
    cmd.return_code = 0;
    cmd.payload_out.resize(cmd.payload_in.len(), 0);
    for (i, &b) in cmd.payload_in.iter().enumerate() {
        if i < cmd.payload_out.len() {
            cmd.payload_out[i] = b;
        }
    }
    Ok(())
}
fn sw_get_health(_memdev_id: u32) -> Result<CxlHealthInfo, &'static str> {
    Ok(CxlHealthInfo {
        maintenance_required: false,
        performance_degraded: false,
        hw_replacement_needed: false,
        media_normal: true,
        media_not_ready: false,
        media_persistence_lost: false,
        media_data_lost: false,
        media_powerloss_persistence_loss: false,
        media_shutdown: false,
    })
}

/// Software CXL ops.
pub fn software_cxl_ops() -> CxlOps {
    CxlOps {
        mbox_send: sw_mbox_send,
        get_health: sw_get_health,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    // Register a root port
    let root_port = register_port("cxl_root", CxlPortType::Root, None, Some(0))?;

    // Register an endpoint port
    let ep_port = register_port("cxl_ep0", CxlPortType::Endpoint, Some(root_port), Some(0))?;

    // Register a decoder on the endpoint
    let decoder_id = register_decoder(ep_port, 0, 256 * 1024 * 1024, 1, 256, CxlDecodeMode::Ram)?;

    // Register a memory device
    let ops = software_cxl_ops();
    let memdev_id = register_memdev(
        "cxl_mem0",
        0x1234,
        512 * 1024 * 1024,
        256 * 1024 * 1024,
        0,
        ops,
    )?;

    // Get health info
    let _health = get_health(memdev_id)?;

    // Send a mailbox command (Identify)
    let mut cmd = CxlMboxCmd {
        opcode: 0x0100, // CXL_MBOX_OP_IDENTIFY
        payload_in: Vec::new(),
        payload_out: Vec::new(),
        return_code: 0,
    };
    mbox_send(memdev_id, &mut cmd)?;

    // Register a region
    let mut target_ids = Vec::new();
    target_ids.push(decoder_id);
    let region_id = register_region(
        "region0",
        256 * 1024 * 1024,
        1,
        256,
        target_ids,
        CxlDecodeMode::Ram,
    )?;
    activate_region(region_id)?;

    Ok(())
}
