//! OF (Open Firmware) / Device Tree subsystem
//!
//! Provides device tree parsing, node enumeration, and property access.
//! Mirrors Linux's `drivers/of/`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// Device tree node (Linux `struct device_node`).
pub struct DeviceNode {
    pub id: u32,
    pub name: String,
    pub full_name: String,
    pub compatible: Vec<String>,
    pub phandle: u32,
    pub parent_id: Option<u32>,
    pub child_ids: Vec<u32>,
    pub properties: BTreeMap<String, Vec<u8>>,
    pub driver_name: Option<String>,
    pub bound: bool,
}

/// OF driver (Linux `struct of_driver` via `struct platform_driver`).
pub struct OfDriver {
    pub name: String,
    pub compatible: Vec<String>,
    pub probe: fn(node_id: u32) -> Result<(), &'static str>,
    pub remove: fn(node_id: u32) -> Result<(), &'static str>,
}

/// OF property (Linux `struct property`).
#[derive(Debug, Clone)]
pub struct OfProperty {
    pub name: String,
    pub value: Vec<u8>,
}

// ── Registry ────────────────────────────────────────────────────────────

static NODE_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DRIVER_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static OF_NODES: RwLock<BTreeMap<u32, DeviceNode>> = RwLock::new(BTreeMap::new());
static OF_DRIVERS: RwLock<BTreeMap<u32, OfDriver>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register a device tree node.
pub fn register_node(
    name: &str,
    full_name: &str,
    compatible: Vec<String>,
    parent_id: Option<u32>,
) -> Result<u32, &'static str> {
    let id = NODE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let node = DeviceNode {
        id,
        name: String::from(name),
        full_name: String::from(full_name),
        compatible,
        phandle: id + 1,
        parent_id,
        child_ids: Vec::new(),
        properties: BTreeMap::new(),
        driver_name: None,
        bound: false,
    };
    OF_NODES.write().insert(id, node);

    if let Some(pid) = parent_id {
        let mut nodes = OF_NODES.write();
        if let Some(parent) = nodes.get_mut(&pid) {
            parent.child_ids.push(id);
        }
    }

    try_match_driver(id)?;
    Ok(id)
}

/// Add a property to a device tree node.
pub fn add_property(node_id: u32, name: &str, value: Vec<u8>) -> Result<(), &'static str> {
    let mut nodes = OF_NODES.write();
    let node = nodes
        .get_mut(&node_id)
        .ok_or("Device tree node not found")?;
    node.properties.insert(String::from(name), value);
    Ok(())
}

/// Get a property from a device tree node.
pub fn get_property(node_id: u32, name: &str) -> Result<Vec<u8>, &'static str> {
    let nodes = OF_NODES.read();
    let node = nodes.get(&node_id).ok_or("Device tree node not found")?;
    node.properties
        .get(name)
        .cloned()
        .ok_or("Property not found")
}

/// Read a u32 property (Linux `of_property_read_u32`).
pub fn read_u32(node_id: u32, name: &str) -> Result<u32, &'static str> {
    let val = get_property(node_id, name)?;
    if val.len() < 4 {
        return Err("Property too short for u32");
    }
    Ok(u32::from_be_bytes([val[0], val[1], val[2], val[3]]))
}

/// Read a string property (Linux `of_property_read_string`).
pub fn read_string(node_id: u32, name: &str) -> Result<String, &'static str> {
    let val = get_property(node_id, name)?;
    let end = val.iter().position(|&b| b == 0).unwrap_or(val.len());
    let s = alloc::string::String::from_utf8_lossy(&val[..end]);
    Ok(s.into_owned())
}

/// Find a node by compatible string (Linux `of_find_compatible_node`).
pub fn find_compatible(compatible: &str) -> Option<u32> {
    let nodes = OF_NODES.read();
    for (id, node) in nodes.iter() {
        if node.compatible.iter().any(|c| c == compatible) {
            return Some(*id);
        }
    }
    None
}

/// Get child nodes (Linux `of_get_next_child`).
pub fn get_children(node_id: u32) -> Result<Vec<u32>, &'static str> {
    let nodes = OF_NODES.read();
    let node = nodes.get(&node_id).ok_or("Device tree node not found")?;
    Ok(node.child_ids.clone())
}

/// Get parent node (Linux `of_get_parent`).
pub fn get_parent(node_id: u32) -> Result<Option<u32>, &'static str> {
    let nodes = OF_NODES.read();
    let node = nodes.get(&node_id).ok_or("Device tree node not found")?;
    Ok(node.parent_id)
}

/// Register an OF driver.
pub fn register_driver(driver: OfDriver) -> Result<u32, &'static str> {
    let id = DRIVER_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let compatible = driver.compatible.clone();
    OF_DRIVERS.write().insert(id, driver);

    let node_ids: Vec<u32> = {
        let nodes = OF_NODES.read();
        nodes
            .iter()
            .filter(|(_, n)| !n.bound && n.compatible.iter().any(|c| compatible.contains(c)))
            .map(|(id, _)| *id)
            .collect()
    };
    for node_id in node_ids {
        try_match_driver(node_id)?;
    }
    Ok(id)
}

/// Try to match a node with a driver.
fn try_match_driver(node_id: u32) -> Result<(), &'static str> {
    let matched = {
        let nodes = OF_NODES.read();
        let node = match nodes.get(&node_id) {
            Some(n) if !n.bound => n,
            _ => return Ok(()),
        };
        let node_compat = node.compatible.clone();

        let drivers = OF_DRIVERS.read();
        let mut found: Option<(fn(u32) -> Result<(), &'static str>, String)> = None;
        for (_, drv) in drivers.iter() {
            for drv_compat in &drv.compatible {
                if node_compat.iter().any(|c| c == drv_compat) {
                    found = Some((drv.probe, drv.name.clone()));
                    break;
                }
            }
            if found.is_some() {
                break;
            }
        }
        found
    };

    if let Some((probe_fn, drv_name)) = matched {
        (probe_fn)(node_id)?;
        let mut nodes = OF_NODES.write();
        if let Some(node) = nodes.get_mut(&node_id) {
            node.bound = true;
            node.driver_name = Some(drv_name);
        }
    }
    Ok(())
}

/// List all device tree nodes.
pub fn list_nodes() -> Vec<(u32, String, String, bool)> {
    OF_NODES
        .read()
        .iter()
        .map(|(id, n)| (*id, n.name.clone(), n.full_name.clone(), n.bound))
        .collect()
}

/// Count registered nodes.
pub fn node_count() -> usize {
    OF_NODES.read().len()
}

// ── Init ────────────────────────────────────────────────────────────────

fn null_probe(_node_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn null_remove(_node_id: u32) -> Result<(), &'static str> {
    Ok(())
}

pub fn init() -> Result<(), &'static str> {
    // Create a simple device tree:
    // / (root)
    //   ├── cpus
    //   │   └── cpu@0
    //   ├── uart@09000000
    //   └── interrupt-controller@0

    let mut root_compat = Vec::new();
    root_compat.push(String::from("simple-bus"));
    let root_id = register_node("/", "/", root_compat, None)?;

    let mut cpus_compat = Vec::new();
    cpus_compat.push(String::from("cpus"));
    let cpus_id = register_node("cpus", "/cpus", cpus_compat, Some(root_id))?;

    let mut cpu_compat = Vec::new();
    cpu_compat.push(String::from("arm,cortex-a72"));
    let cpu_id = register_node("cpu@0", "/cpus/cpu@0", cpu_compat, Some(cpus_id))?;
    add_property(cpu_id, "reg", 0u32.to_be_bytes().to_vec())?;
    add_property(cpu_id, "device_type", b"cpu\0".to_vec())?;

    let mut uart_compat = Vec::new();
    uart_compat.push(String::from("arm,pl011"));
    uart_compat.push(String::from("arm,primecell"));
    let uart_id = register_node(
        "uart@09000000",
        "/uart@09000000",
        uart_compat,
        Some(root_id),
    )?;
    add_property(uart_id, "reg", {
        let mut v = Vec::new();
        v.extend_from_slice(&0x09000000u32.to_be_bytes());
        v.extend_from_slice(&0x1000u32.to_be_bytes());
        v
    })?;
    add_property(uart_id, "interrupts", {
        let mut v = Vec::new();
        v.extend_from_slice(&0u32.to_be_bytes()); // GIC_SPI
        v.extend_from_slice(&37u32.to_be_bytes()); // IRQ 37
        v.extend_from_slice(&4u32.to_be_bytes()); // LEVEL_HIGH
        v
    })?;

    // Register a PL011 driver
    let mut drv_compat = Vec::new();
    drv_compat.push(String::from("arm,pl011"));
    let driver = OfDriver {
        name: String::from("pl011-of-drv"),
        compatible: drv_compat,
        probe: null_probe,
        remove: null_remove,
    };
    register_driver(driver)?;

    Ok(())
}
