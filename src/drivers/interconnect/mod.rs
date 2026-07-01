//! Interconnect subsystem
//!
//! Provides interconnect framework for managing on-chip bus bandwidth and QoS.
//! Mirrors Linux's `drivers/interconnect/interconnect-core.c`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// Interconnect node (Linux `struct icc_node`).
pub struct IccNode {
    pub id: u32,
    pub name: String,
    pub provider_id: u32,
    pub avg_bw: u64,
    pub peak_bw: u64,
    pub init_avg_bw: u64,
    pub init_peak_bw: u64,
    pub links: Vec<u32>,
    pub data: Option<u64>,
}

/// Interconnect provider (Linux `struct icc_provider`).
pub struct IccProvider {
    pub id: u32,
    pub name: String,
    pub nodes: Vec<u32>,
    pub ops: IccProviderOps,
    pub inter_set: bool,
}

/// Interconnect provider operations (Linux `struct icc_provider_ops`).
pub struct IccProviderOps {
    pub aggregate: fn(
        provider_id: u32,
        node_id: u32,
        avg_bw: u64,
        peak_bw: u64,
    ) -> Result<(u64, u64), &'static str>,
    pub set:
        fn(provider_id: u32, path: &[u32], avg_bw: u64, peak_bw: u64) -> Result<(), &'static str>,
    pub get_bw: fn(provider_id: u32, node_id: u32) -> Result<(u64, u64), &'static str>,
    pub enable: fn(provider_id: u32) -> Result<(), &'static str>,
    pub disable: fn(provider_id: u32) -> Result<(), &'static str>,
}

/// Interconnect path (Linux `struct icc_path`).
pub struct IccPath {
    pub id: u32,
    pub node_ids: Vec<u32>,
    pub num_nodes: u32,
    pub reqs: Vec<IccPathRequest>,
}

/// Path bandwidth request.
#[derive(Debug, Clone)]
pub struct IccPathRequest {
    pub avg_bw: u64,
    pub peak_bw: u64,
    pub tag: u64,
}

// ── Registry ────────────────────────────────────────────────────────────

static PROVIDER_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static NODE_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static PATH_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static ICC_PROVIDERS: RwLock<BTreeMap<u32, IccProvider>> = RwLock::new(BTreeMap::new());
static ICC_NODES: RwLock<BTreeMap<u32, IccNode>> = RwLock::new(BTreeMap::new());
static ICC_PATHS: RwLock<BTreeMap<u32, IccPath>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register an interconnect provider.
pub fn register_provider(
    name: &str,
    ops: IccProviderOps,
    inter_set: bool,
) -> Result<u32, &'static str> {
    let id = PROVIDER_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let provider = IccProvider {
        id,
        name: String::from(name),
        nodes: Vec::new(),
        ops,
        inter_set,
    };
    ICC_PROVIDERS.write().insert(id, provider);
    Ok(id)
}

/// Create a node on an interconnect provider.
pub fn create_node(
    provider_id: u32,
    name: &str,
    init_avg_bw: u64,
    init_peak_bw: u64,
) -> Result<u32, &'static str> {
    let node_id = NODE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let node = IccNode {
        id: node_id,
        name: String::from(name),
        provider_id,
        avg_bw: init_avg_bw,
        peak_bw: init_peak_bw,
        init_avg_bw,
        init_peak_bw,
        links: Vec::new(),
        data: None,
    };
    ICC_NODES.write().insert(node_id, node);

    let mut providers = ICC_PROVIDERS.write();
    let provider = providers
        .get_mut(&provider_id)
        .ok_or("ICC provider not found")?;
    provider.nodes.push(node_id);
    Ok(node_id)
}

/// Link two nodes (create a directed edge in the interconnect graph).
pub fn link_nodes(src_id: u32, dst_id: u32) -> Result<(), &'static str> {
    let mut nodes = ICC_NODES.write();
    let src = nodes.get_mut(&src_id).ok_or("Source node not found")?;
    src.links.push(dst_id);
    Ok(())
}

/// Find a path between two nodes (BFS through the interconnect graph).
pub fn find_path(src_id: u32, dst_id: u32) -> Result<Vec<u32>, &'static str> {
    if src_id == dst_id {
        let mut path = Vec::new();
        path.push(src_id);
        return Ok(path);
    }

    let nodes = ICC_NODES.read();

    // BFS
    let mut visited: Vec<u32> = Vec::new();
    let mut queue: Vec<(u32, Vec<u32>)> = Vec::new();
    visited.push(src_id);
    let mut initial_path = Vec::new();
    initial_path.push(src_id);
    queue.push((src_id, initial_path));

    while let Some((current, path)) = queue.pop() {
        let node = match nodes.get(&current) {
            Some(n) => n,
            None => continue,
        };
        for &link in &node.links {
            if link == dst_id {
                let mut result = path.clone();
                result.push(link);
                return Ok(result);
            }
            if !visited.contains(&link) {
                visited.push(link);
                let mut new_path = path.clone();
                new_path.push(link);
                queue.push((link, new_path));
            }
        }
    }

    Err("No interconnect path found")
}

/// Set bandwidth on an interconnect path (Linux `icc_set_bw`).
pub fn set_bw(path_id: u32, avg_bw: u64, peak_bw: u64) -> Result<(), &'static str> {
    let (node_ids, provider_id) = {
        let paths = ICC_PATHS.read();
        let path = paths.get(&path_id).ok_or("ICC path not found")?;
        let nodes = ICC_NODES.read();
        let first_node = nodes
            .get(path.node_ids.first().ok_or("Empty path")?)
            .ok_or("Node not found")?;
        (path.node_ids.clone(), first_node.provider_id)
    };

    // Aggregate bandwidth on each node in the path
    let mut aggregated_avg = avg_bw;
    let mut aggregated_peak = peak_bw;

    let aggregate_fn = {
        let providers = ICC_PROVIDERS.read();
        let provider = providers
            .get(&provider_id)
            .ok_or("ICC provider not found")?;
        provider.ops.aggregate
    };

    for &node_id in &node_ids {
        let (agg_avg, agg_peak) =
            (aggregate_fn)(provider_id, node_id, aggregated_avg, aggregated_peak)?;
        aggregated_avg = agg_avg;
        aggregated_peak = agg_peak;

        let mut nodes = ICC_NODES.write();
        if let Some(node) = nodes.get_mut(&node_id) {
            node.avg_bw = aggregated_avg;
            node.peak_bw = aggregated_peak;
        }
    }

    // Apply the bandwidth setting
    let set_fn = {
        let providers = ICC_PROVIDERS.read();
        let provider = providers
            .get(&provider_id)
            .ok_or("ICC provider not found")?;
        provider.ops.set
    };
    (set_fn)(provider_id, &node_ids, aggregated_avg, aggregated_peak)
}

/// Get a path and register it (Linux `icc_get`).
pub fn get_path(src_id: u32, dst_id: u32) -> Result<u32, &'static str> {
    let node_ids = find_path(src_id, dst_id)?;

    let path_id = PATH_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let path = IccPath {
        id: path_id,
        num_nodes: node_ids.len() as u32,
        node_ids,
        reqs: Vec::new(),
    };
    ICC_PATHS.write().insert(path_id, path);
    Ok(path_id)
}

/// Put (release) an interconnect path (Linux `icc_put`).
pub fn put_path(path_id: u32) -> Result<(), &'static str> {
    // Reset bandwidth to initial values
    set_bw(path_id, 0, 0)?;
    ICC_PATHS.write().remove(&path_id);
    Ok(())
}

/// Enable an interconnect provider.
pub fn enable_provider(provider_id: u32) -> Result<(), &'static str> {
    let enable_fn = {
        let providers = ICC_PROVIDERS.read();
        let provider = providers
            .get(&provider_id)
            .ok_or("ICC provider not found")?;
        provider.ops.enable
    };
    (enable_fn)(provider_id)
}

/// Disable an interconnect provider.
pub fn disable_provider(provider_id: u32) -> Result<(), &'static str> {
    let disable_fn = {
        let providers = ICC_PROVIDERS.read();
        let provider = providers
            .get(&provider_id)
            .ok_or("ICC provider not found")?;
        provider.ops.disable
    };
    (disable_fn)(provider_id)
}

/// List all registered providers.
pub fn list_providers() -> Vec<(u32, String, usize)> {
    ICC_PROVIDERS
        .read()
        .iter()
        .map(|(id, p)| (*id, p.name.clone(), p.nodes.len()))
        .collect()
}

/// Get node bandwidth.
pub fn get_node_bw(node_id: u32) -> Result<(u64, u64), &'static str> {
    let nodes = ICC_NODES.read();
    let node = nodes.get(&node_id).ok_or("ICC node not found")?;
    Ok((node.avg_bw, node.peak_bw))
}

// ── Software interconnect ───────────────────────────────────────────────

fn sw_aggregate(
    _prov_id: u32,
    _node_id: u32,
    _avg_bw: u64,
    _peak_bw: u64,
) -> Result<(u64, u64), &'static str> {
    Err("software interconnect provider not available")
}
fn sw_set(_prov_id: u32, _path: &[u32], _avg_bw: u64, _peak_bw: u64) -> Result<(), &'static str> {
    Err("software interconnect provider not available")
}
fn sw_get_bw(_prov_id: u32, _node_id: u32) -> Result<(u64, u64), &'static str> {
    Err("software interconnect provider not available")
}
fn sw_enable(_prov_id: u32) -> Result<(), &'static str> {
    Err("software interconnect provider not available")
}
fn sw_disable(_prov_id: u32) -> Result<(), &'static str> {
    Err("software interconnect provider not available")
}

/// Software interconnect ops for callers that need an explicit unsupported backend.
pub fn software_icc_ops() -> IccProviderOps {
    IccProviderOps {
        aggregate: sw_aggregate,
        set: sw_set,
        get_bw: sw_get_bw,
        enable: sw_enable,
        disable: sw_disable,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("interconnect: framework ready (no software provider)");
    Ok(())
}
