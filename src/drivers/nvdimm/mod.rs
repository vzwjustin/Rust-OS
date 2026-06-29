//! NVDIMM (Non-Volatile DIMM) subsystem
//!
//! Provides NVDIMM region and namespace management for persistent memory.
//! Mirrors Linux's `drivers/nvdimm/core.c`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// NVDIMM type (Linux `enum nvdimm_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NvdimmType {
    Mem, // Persistent memory (pmem)
    Blk, // Block-mode NVDIMM
    Dax, // Direct access
}

/// NVDIMM bus (Linux `struct nvdimm_bus`).
pub struct NvdimmBus {
    pub id: u32,
    pub name: String,
    pub provider: String,
    pub dimm_ids: Vec<u32>,
    pub region_ids: Vec<u32>,
}

/// NVDIMM device (Linux `struct nvdimm`).
pub struct Nvdimm {
    pub id: u32,
    pub name: String,
    pub bus_id: u32,
    pub size: u64,
    pub ntype: NvdimmType,
    pub state: NvdimmState,
    pub provider_data: Option<u64>,
}

/// NVDIMM state (Linux `enum nvdimm_state`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NvdimmState {
    Unknown,
    Active,
    Idle,
    Disabled,
    Error,
}

/// NVDIMM region (Linux `struct nd_region`).
pub struct NvdimmRegion {
    pub id: u32,
    pub name: String,
    pub bus_id: u32,
    pub ntype: NvdimmType,
    pub dimm_ids: Vec<u32>,
    pub size: u64,
    pub align: u64,
    pub n_mappings: u32,
    pub namespaces: Vec<u32>,
    pub ro: bool,
}

/// NVDIMM namespace (Linux `struct nd_namespace_*`).
pub struct NvdimmNamespace {
    pub id: u32,
    pub name: String,
    pub region_id: u32,
    pub size: u64,
    pub uuid: [u8; 16],
    pub sector_size: u32,
    pub ntype: NvdimmType,
    pub active: bool,
    pub ro: bool,
}

/// NVDIMM bus operations (Linux `struct nvdimm_bus_ops`).
pub struct NvdimmBusOps {
    pub probe: fn(bus_id: u32) -> Result<(), &'static str>,
    pub dimm_init: fn(dimm_id: u32) -> Result<(), &'static str>,
    pub dimm_release: fn(dimm_id: u32) -> Result<(), &'static str>,
    pub region_probe: fn(region_id: u32) -> Result<(), &'static str>,
    pub region_release: fn(region_id: u32) -> Result<(), &'static str>,
}

// ── Registry ────────────────────────────────────────────────────────────

static BUS_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DIMM_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static REGION_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static NS_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static NVDIMM_BUSES: RwLock<BTreeMap<u32, NvdimmBus>> = RwLock::new(BTreeMap::new());
static NVDIMM_DIMMS: RwLock<BTreeMap<u32, Nvdimm>> = RwLock::new(BTreeMap::new());
static NVDIMM_REGIONS: RwLock<BTreeMap<u32, NvdimmRegion>> = RwLock::new(BTreeMap::new());
static NVDIMM_NAMESPACES: RwLock<BTreeMap<u32, NvdimmNamespace>> = RwLock::new(BTreeMap::new());
static NVDIMM_BUS_OPS: RwLock<BTreeMap<u32, NvdimmBusOps>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register an NVDIMM bus.
pub fn register_bus(name: &str, provider: &str, ops: NvdimmBusOps) -> Result<u32, &'static str> {
    let id = BUS_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let bus = NvdimmBus {
        id,
        name: String::from(name),
        provider: String::from(provider),
        dimm_ids: Vec::new(),
        region_ids: Vec::new(),
    };
    NVDIMM_BUSES.write().insert(id, bus);
    NVDIMM_BUS_OPS.write().insert(id, ops);
    Ok(id)
}

/// Register a DIMM on an NVDIMM bus.
pub fn register_dimm(
    bus_id: u32,
    name: &str,
    size: u64,
    ntype: NvdimmType,
) -> Result<u32, &'static str> {
    let dimm_id = DIMM_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dimm = Nvdimm {
        id: dimm_id,
        name: String::from(name),
        bus_id,
        size,
        ntype,
        state: NvdimmState::Idle,
        provider_data: None,
    };
    NVDIMM_DIMMS.write().insert(dimm_id, dimm);

    let mut buses = NVDIMM_BUSES.write();
    let bus = buses.get_mut(&bus_id).ok_or("NVDIMM bus not found")?;
    bus.dimm_ids.push(dimm_id);

    // Call dimm_init
    let init_fn = {
        let ops = NVDIMM_BUS_OPS.read();
        let bus_ops = ops.get(&bus_id).ok_or("NVDIMM bus ops not found")?;
        bus_ops.dimm_init
    };
    (init_fn)(dimm_id)?;

    let mut dimms = NVDIMM_DIMMS.write();
    if let Some(d) = dimms.get_mut(&dimm_id) {
        d.state = NvdimmState::Active;
    }
    Ok(dimm_id)
}

/// Create a region on an NVDIMM bus.
pub fn create_region(
    bus_id: u32,
    name: &str,
    ntype: NvdimmType,
    dimm_ids: Vec<u32>,
    size: u64,
    align: u64,
) -> Result<u32, &'static str> {
    let region_id = REGION_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let n_mappings = dimm_ids.len() as u32;
    let region = NvdimmRegion {
        id: region_id,
        name: String::from(name),
        bus_id,
        ntype,
        dimm_ids,
        size,
        align,
        n_mappings,
        namespaces: Vec::new(),
        ro: false,
    };
    NVDIMM_REGIONS.write().insert(region_id, region);

    let mut buses = NVDIMM_BUSES.write();
    let bus = buses.get_mut(&bus_id).ok_or("NVDIMM bus not found")?;
    bus.region_ids.push(region_id);

    // Call region_probe
    let probe_fn = {
        let ops = NVDIMM_BUS_OPS.read();
        let bus_ops = ops.get(&bus_id).ok_or("NVDIMM bus ops not found")?;
        bus_ops.region_probe
    };
    (probe_fn)(region_id)?;
    Ok(region_id)
}

/// Create a namespace within a region.
pub fn create_namespace(
    region_id: u32,
    name: &str,
    size: u64,
    ntype: NvdimmType,
) -> Result<u32, &'static str> {
    let ns_id = NS_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let ns = NvdimmNamespace {
        id: ns_id,
        name: String::from(name),
        region_id,
        size,
        uuid: [0u8; 16],
        sector_size: 512,
        ntype,
        active: true,
        ro: false,
    };
    NVDIMM_NAMESPACES.write().insert(ns_id, ns);

    let mut regions = NVDIMM_REGIONS.write();
    let region = regions
        .get_mut(&region_id)
        .ok_or("NVDIMM region not found")?;
    region.namespaces.push(ns_id);
    Ok(ns_id)
}

/// Delete a namespace.
pub fn delete_namespace(ns_id: u32) -> Result<(), &'static str> {
    let region_id = {
        let ns = NVDIMM_NAMESPACES.read();
        let n = ns.get(&ns_id).ok_or("NVDIMM namespace not found")?;
        n.region_id
    };
    NVDIMM_NAMESPACES.write().remove(&ns_id);

    let mut regions = NVDIMM_REGIONS.write();
    if let Some(region) = regions.get_mut(&region_id) {
        region.namespaces.retain(|&id| id != ns_id);
    }
    Ok(())
}

/// Get DIMM info.
pub fn get_dimm(dimm_id: u32) -> Result<(String, u64, NvdimmType, NvdimmState), &'static str> {
    let dimms = NVDIMM_DIMMS.read();
    let d = dimms.get(&dimm_id).ok_or("NVDIMM not found")?;
    Ok((d.name.clone(), d.size, d.ntype, d.state))
}

/// Get region info.
pub fn get_region(region_id: u32) -> Result<(String, NvdimmType, u64, u32), &'static str> {
    let regions = NVDIMM_REGIONS.read();
    let r = regions.get(&region_id).ok_or("NVDIMM region not found")?;
    Ok((r.name.clone(), r.ntype, r.size, r.n_mappings))
}

/// List namespaces in a region.
pub fn list_namespaces(region_id: u32) -> Result<Vec<(u32, String, u64, bool)>, &'static str> {
    let regions = NVDIMM_REGIONS.read();
    let region = regions.get(&region_id).ok_or("NVDIMM region not found")?;
    let namespaces = NVDIMM_NAMESPACES.read();
    let mut result = Vec::new();
    for &ns_id in &region.namespaces {
        if let Some(ns) = namespaces.get(&ns_id) {
            result.push((ns_id, ns.name.clone(), ns.size, ns.active));
        }
    }
    Ok(result)
}

/// List all NVDIMM buses.
pub fn list_buses() -> Vec<(u32, String, String)> {
    NVDIMM_BUSES
        .read()
        .iter()
        .map(|(id, b)| (*id, b.name.clone(), b.provider.clone()))
        .collect()
}

/// Count registered DIMMs.
pub fn dimm_count() -> usize {
    NVDIMM_DIMMS.read().len()
}

/// Count registered regions.
pub fn region_count() -> usize {
    NVDIMM_REGIONS.read().len()
}

// ── Software NVDIMM ─────────────────────────────────────────────────────

fn sw_probe(_bus_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_dimm_init(_dimm_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_dimm_release(_dimm_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_region_probe(_region_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_region_release(_region_id: u32) -> Result<(), &'static str> {
    Ok(())
}

/// Software NVDIMM bus ops.
pub fn software_nvdimm_ops() -> NvdimmBusOps {
    NvdimmBusOps {
        probe: sw_probe,
        dimm_init: sw_dimm_init,
        dimm_release: sw_dimm_release,
        region_probe: sw_region_probe,
        region_release: sw_region_release,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    let ops = software_nvdimm_ops();
    let bus_id = register_bus("sw-nvdimm-bus", "sw-nvdimm", ops)?;

    // Register two DIMMs
    let dimm0 = register_dimm(bus_id, "nmem0", 4 * 1024 * 1024 * 1024, NvdimmType::Mem)?;
    let dimm1 = register_dimm(bus_id, "nmem1", 4 * 1024 * 1024 * 1024, NvdimmType::Mem)?;

    // Create a region spanning both DIMMs
    let mut dimm_ids = Vec::new();
    dimm_ids.push(dimm0);
    dimm_ids.push(dimm1);
    let region_id = create_region(
        bus_id,
        "region0",
        NvdimmType::Mem,
        dimm_ids,
        8 * 1024 * 1024 * 1024,
        4096,
    )?;

    // Create a namespace in the region
    create_namespace(
        region_id,
        "namespace0",
        4 * 1024 * 1024 * 1024,
        NvdimmType::Mem,
    )?;

    Ok(())
}
