//! RapidIO fabric registry.
//!
//! Rust-owned mirror of Linux `drivers/rapidio/`: local mports, discovered
//! devices, and routing entries are tracked in kernel-owned tables.

use alloc::{collections::BTreeMap, string::String, vec::Vec};
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

#[derive(Debug, Clone)]
pub struct RapidIoMport {
    pub id: u32,
    pub name: String,
    pub host_device_id: u16,
    pub link_up: bool,
}

#[derive(Debug, Clone)]
pub struct RapidIoDevice {
    pub id: u32,
    pub mport_id: u32,
    pub dest_id: u16,
    pub vendor: u16,
    pub device: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RapidIoRoute {
    pub mport_id: u32,
    pub dest_id: u16,
    pub next_hop: u16,
}

static MPORT_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DEVICE_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static RIO_MPORTS: RwLock<BTreeMap<u32, RapidIoMport>> = RwLock::new(BTreeMap::new());
static RIO_DEVICES: RwLock<BTreeMap<u32, RapidIoDevice>> = RwLock::new(BTreeMap::new());
static RIO_ROUTES: RwLock<BTreeMap<(u32, u16), RapidIoRoute>> = RwLock::new(BTreeMap::new());

pub fn register_mport(name: &str, host_device_id: u16) -> Result<u32, &'static str> {
    let id = MPORT_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    RIO_MPORTS.write().insert(
        id,
        RapidIoMport {
            id,
            name: String::from(name),
            host_device_id,
            link_up: true,
        },
    );
    Ok(id)
}

pub fn set_link_state(mport_id: u32, link_up: bool) -> Result<(), &'static str> {
    let mut mports = RIO_MPORTS.write();
    let mport = mports.get_mut(&mport_id).ok_or("RapidIO mport not found")?;
    mport.link_up = link_up;
    Ok(())
}

pub fn add_device(
    mport_id: u32,
    dest_id: u16,
    vendor: u16,
    device: u16,
) -> Result<u32, &'static str> {
    if !RIO_MPORTS.read().contains_key(&mport_id) {
        return Err("RapidIO mport not found");
    }

    let id = DEVICE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    RIO_DEVICES.write().insert(
        id,
        RapidIoDevice {
            id,
            mport_id,
            dest_id,
            vendor,
            device,
        },
    );
    Ok(id)
}

pub fn add_route(mport_id: u32, dest_id: u16, next_hop: u16) -> Result<(), &'static str> {
    if !RIO_MPORTS.read().contains_key(&mport_id) {
        return Err("RapidIO mport not found");
    }

    RIO_ROUTES.write().insert(
        (mport_id, dest_id),
        RapidIoRoute {
            mport_id,
            dest_id,
            next_hop,
        },
    );
    Ok(())
}

pub fn resolve_route(mport_id: u32, dest_id: u16) -> Result<RapidIoRoute, &'static str> {
    RIO_ROUTES
        .read()
        .get(&(mport_id, dest_id))
        .copied()
        .ok_or("RapidIO route not found")
}

pub fn enumerate_devices(mport_id: u32) -> Vec<RapidIoDevice> {
    RIO_DEVICES
        .read()
        .values()
        .filter(|device| device.mport_id == mport_id)
        .cloned()
        .collect()
}

pub fn device_count() -> usize {
    RIO_DEVICES.read().len()
}

pub fn init() -> Result<(), &'static str> {
    if !RIO_MPORTS.read().is_empty() {
        return Ok(());
    }

    let mport_id = register_mport("sw-rio-mport0", 0)?;
    add_device(mport_id, 1, 0x0001, 0x0001)?;
    add_route(mport_id, 1, 1)?;
    crate::serial_println!("rapidio: software mport registered with 1 device");
    Ok(())
}
