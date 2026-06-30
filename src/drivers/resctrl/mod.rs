//! Resctrl (Resource Control) driver subsystem
//!
//! Provides resource allocation and monitoring framework (Intel RDT/AMD QoS).
//! Mirrors Linux's `drivers/resctrl/`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// Resource type (Linux `enum resctrl_resource`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceType {
    L3Cache,
    L2Cache,
    MemoryBandwidth,
    L3CacheCode,
    L3CacheData,
}

/// Resource control (Linux `struct rdt_resource`).
pub struct ResctrlResource {
    pub id: u32,
    pub res_type: ResourceType,
    pub name: String,
    pub num_closids: u32,
    pub cbm_len: u32,
    pub min_cbm_bits: u32,
    pub cache_level: u32,
    pub domains: Vec<u32>,
}

/// Resource domain (Linux `struct rdt_domain`).
pub struct ResctrlDomain {
    pub id: u32,
    pub resource_id: u32,
    pub cpu_mask: u64,
    pub cbm: Vec<u64>,
}

/// Control group (Linux `struct rdtgroup`).
pub struct ResctrlGroup {
    pub id: u32,
    pub name: String,
    pub closid: u32,
    pub parent_id: Option<u32>,
    pub mode: GroupMode,
}

/// Group mode (Linux `enum rdtgrp_mode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupMode {
    Exclusive,
    Shareable,
    PseudoLocked,
    Pseudo,
}

// ── Registry ────────────────────────────────────────────────────────────

static RES_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DOM_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static GRP_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static RESCTRL_RESOURCES: RwLock<BTreeMap<u32, ResctrlResource>> = RwLock::new(BTreeMap::new());
static RESCTRL_DOMAINS: RwLock<BTreeMap<u32, ResctrlDomain>> = RwLock::new(BTreeMap::new());
static RESCTRL_GROUPS: RwLock<BTreeMap<u32, ResctrlGroup>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register a resource (Linux `rdt_resource_init`).
pub fn register_resource(
    res_type: ResourceType,
    name: &str,
    num_closids: u32,
    cbm_len: u32,
    min_cbm_bits: u32,
    cache_level: u32,
) -> Result<u32, &'static str> {
    let id = RES_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let res = ResctrlResource {
        id,
        res_type,
        name: String::from(name),
        num_closids,
        cbm_len,
        min_cbm_bits,
        cache_level,
        domains: Vec::new(),
    };
    RESCTRL_RESOURCES.write().insert(id, res);
    Ok(id)
}

/// Register a domain for a resource (Linux `domain_add_cpu`).
pub fn register_domain(resource_id: u32, cpu_mask: u64) -> Result<u32, &'static str> {
    if !RESCTRL_RESOURCES.read().contains_key(&resource_id) {
        return Err("Resource not found");
    }
    let id = DOM_ID_COUNTER.fetch_add(1, Ordering::SeqCst);

    let num_closids = {
        let resources = RESCTRL_RESOURCES.read();
        resources
            .get(&resource_id)
            .map(|r| r.num_closids)
            .unwrap_or(1)
    };

    let domain = ResctrlDomain {
        id,
        resource_id,
        cpu_mask,
        cbm: alloc::vec![0u64; num_closids as usize],
    };
    RESCTRL_DOMAINS.write().insert(id, domain);
    let mut resources = RESCTRL_RESOURCES.write();
    if let Some(res) = resources.get_mut(&resource_id) {
        res.domains.push(id);
    }
    Ok(id)
}

/// Create a control group (Linux `mkdir` on resctrlfs).
pub fn create_group(name: &str, closid: u32, parent_id: Option<u32>) -> Result<u32, &'static str> {
    let id = GRP_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let group = ResctrlGroup {
        id,
        name: String::from(name),
        closid,
        parent_id,
        mode: GroupMode::Exclusive,
    };
    RESCTRL_GROUPS.write().insert(id, group);
    Ok(id)
}

/// Set the CBM (Cache Bit Mask) for a domain and CLOSID.
pub fn set_cbm(domain_id: u32, closid: u32, mask: u64) -> Result<(), &'static str> {
    let mut domains = RESCTRL_DOMAINS.write();
    let domain = domains.get_mut(&domain_id).ok_or("Domain not found")?;
    if closid as usize >= domain.cbm.len() {
        return Err("CLOSID out of range");
    }
    domain.cbm[closid as usize] = mask;
    Ok(())
}

/// Get the CBM for a domain and CLOSID.
pub fn get_cbm(domain_id: u32, closid: u32) -> Result<u64, &'static str> {
    let domains = RESCTRL_DOMAINS.read();
    let domain = domains.get(&domain_id).ok_or("Domain not found")?;
    domain
        .cbm
        .get(closid as usize)
        .copied()
        .ok_or("CLOSID out of range")
}

/// List all resources.
pub fn list_resources() -> Vec<(u32, ResourceType, String, u32, u32)> {
    RESCTRL_RESOURCES
        .read()
        .iter()
        .map(|(id, r)| (*id, r.res_type, r.name.clone(), r.num_closids, r.cbm_len))
        .collect()
}

/// List all groups.
pub fn list_groups() -> Vec<(u32, String, u32, GroupMode)> {
    RESCTRL_GROUPS
        .read()
        .iter()
        .map(|(id, g)| (*id, g.name.clone(), g.closid, g.mode))
        .collect()
}

/// Count resources.
pub fn resource_count() -> usize {
    RESCTRL_RESOURCES.read().len()
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    if !RESCTRL_RESOURCES.read().is_empty() {
        return Ok(());
    }

    let l3_id = register_resource(ResourceType::L3Cache, "L3", 16, 20, 2, 3)?;
    register_domain(l3_id, 0xFF)?;
    create_group("root", 0, None)?;

    crate::serial_println!(
        "resctrl: L3 cache resource registered (id={}, 16 CLOSIDs, 20-bit CBM)",
        l3_id
    );
    Ok(())
}
