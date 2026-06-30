//! Greybus subsystem (mirrors Linux `drivers/greybus/`)
//!
//! Models the Greybus interface/bundle/connection hierarchy: an interface
//! exposes bundles, each bundle owns CPort connections to a protocol, and
//! operations are exchanged over a connection as request/response messages.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Protocols (subset of Greybus protocol ids) ────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GbProtocol {
    Control,
    Gpio,
    I2c,
    Spi,
    Uart,
    Vendor,
}

#[derive(Clone)]
struct Connection {
    cport_id: u16,
    protocol: GbProtocol,
    operations: u64,
}

#[derive(Clone)]
struct Bundle {
    bundle_id: u8,
    class: u8,
    connections: Vec<Connection>,
}

struct Interface {
    id: u32,
    vendor_id: u32,
    product_id: u32,
    bundles: Vec<Bundle>,
}

// ── Registry ──────────────────────────────────────────────────────────────

static INTERFACES: RwLock<BTreeMap<u32, Interface>> = RwLock::new(BTreeMap::new());
static NEXT_ID: AtomicU32 = AtomicU32::new(0);

// ── Public API ──────────────────────────────────────────────────────────

pub fn add_interface(vendor_id: u32, product_id: u32) -> u32 {
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    INTERFACES.write().insert(
        id,
        Interface {
            id,
            vendor_id,
            product_id,
            bundles: Vec::new(),
        },
    );
    id
}

pub fn add_bundle(iface_id: u32, bundle_id: u8, class: u8) -> Result<(), &'static str> {
    let mut ifaces = INTERFACES.write();
    let iface = ifaces
        .get_mut(&iface_id)
        .ok_or("greybus: interface not found")?;
    if iface.bundles.iter().any(|b| b.bundle_id == bundle_id) {
        return Err("greybus: bundle id in use");
    }
    iface.bundles.push(Bundle {
        bundle_id,
        class,
        connections: Vec::new(),
    });
    Ok(())
}

pub fn add_connection(
    iface_id: u32,
    bundle_id: u8,
    cport_id: u16,
    protocol: GbProtocol,
) -> Result<(), &'static str> {
    let mut ifaces = INTERFACES.write();
    let iface = ifaces
        .get_mut(&iface_id)
        .ok_or("greybus: interface not found")?;
    let bundle = iface
        .bundles
        .iter_mut()
        .find(|b| b.bundle_id == bundle_id)
        .ok_or("greybus: bundle not found")?;
    bundle.connections.push(Connection {
        cport_id,
        protocol,
        operations: 0,
    });
    Ok(())
}

/// Send an operation over a connection identified by its CPort.
pub fn operation(iface_id: u32, cport_id: u16, _request: &[u8]) -> Result<(), &'static str> {
    let mut ifaces = INTERFACES.write();
    let iface = ifaces
        .get_mut(&iface_id)
        .ok_or("greybus: interface not found")?;
    for bundle in iface.bundles.iter_mut() {
        if let Some(conn) = bundle
            .connections
            .iter_mut()
            .find(|c| c.cport_id == cport_id)
        {
            conn.operations += 1;
            return Ok(());
        }
    }
    Err("greybus: cport not connected")
}

pub fn bundle_count(iface_id: u32) -> usize {
    INTERFACES
        .read()
        .get(&iface_id)
        .map(|i| i.bundles.len())
        .unwrap_or(0)
}

pub fn interface_count() -> usize {
    INTERFACES.read().len()
}

/// Initialize Greybus with a sample interface exposing a control bundle.
pub fn init() -> Result<(), &'static str> {
    if !INTERFACES.read().is_empty() {
        return Ok(());
    }
    let iface = add_interface(0x0001, 0x0001);
    add_bundle(iface, 0, 0x00)?;
    add_connection(iface, 0, 0, GbProtocol::Control)?;
    crate::serial_println!("greybus: interface up, {} bundle(s)", bundle_count(iface));
    Ok(())
}
