//! IPACK (IndustryPack) bus subsystem
//!
//! Provides IndustryPack carrier bus for mezzanine I/O boards.
//! Mirrors Linux's `drivers/ipack/ipack.c`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// IPack device (Linux `struct ipack_device`).
pub struct IpackDevice {
    pub id: u32,
    pub bus_id: u32,
    pub slot: u8,
    pub name: String,
    pub vendor_id: u8,
    pub device_id: u8,
    pub vendor_name: String,
    pub device_name: String,
    pub speed: u32,
    pub mem_base: u64,
    pub io_base: u64,
    pub irq: u32,
    pub driver_name: Option<String>,
    pub bound: bool,
}

/// IPack driver (Linux `struct ipack_driver`).
pub struct IpackDriver {
    pub name: String,
    pub id_table: Vec<IpackDeviceId>,
    pub probe: fn(device_id: u32) -> Result<(), &'static str>,
    pub remove: fn(device_id: u32) -> Result<(), &'static str>,
}

/// IPack device ID (Linux `struct ipack_device_id`).
#[derive(Debug, Clone)]
pub struct IpackDeviceId {
    pub vendor_id: u8,
    pub device_id: u8,
}

/// IPack bus (Linux `struct ipack_bus`).
pub struct IpackBus {
    pub id: u32,
    pub name: String,
    pub ops: IpackBusOps,
    pub device_ids: Vec<u32>,
    pub slots: u8,
    pub bus_nr: u32,
}

/// IPack bus operations (Linux `struct ipack_bus_ops`).
pub struct IpackBusOps {
    pub read_id: fn(bus_id: u32, slot: u8) -> Result<(u8, u8, String, String), &'static str>,
    pub map_space: fn(
        bus_id: u32,
        device_id: u32,
        space: IpackSpace,
        addr: u64,
        size: u64,
    ) -> Result<(), &'static str>,
    pub unmap_space: fn(bus_id: u32, device_id: u32, space: IpackSpace) -> Result<(), &'static str>,
    pub request_irq:
        fn(bus_id: u32, device_id: u32, handler: fn(device_id: u32)) -> Result<(), &'static str>,
    pub free_irq: fn(bus_id: u32, device_id: u32) -> Result<(), &'static str>,
}

/// IPack address space (Linux `enum ipack_space`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpackSpace {
    Mem,
    Io,
    Id,
    Int,
    Dma,
}

// ── Registry ────────────────────────────────────────────────────────────

static BUS_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DEVICE_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DRIVER_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static IPACK_BUSES: RwLock<BTreeMap<u32, IpackBus>> = RwLock::new(BTreeMap::new());
static IPACK_DEVICES: RwLock<BTreeMap<u32, IpackDevice>> = RwLock::new(BTreeMap::new());
static IPACK_DRIVERS: RwLock<BTreeMap<u32, IpackDriver>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register an IPack bus.
pub fn register_bus(
    name: &str,
    ops: IpackBusOps,
    slots: u8,
    bus_nr: u32,
) -> Result<u32, &'static str> {
    let id = BUS_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let bus = IpackBus {
        id,
        name: String::from(name),
        ops,
        device_ids: Vec::new(),
        slots,
        bus_nr,
    };
    IPACK_BUSES.write().insert(id, bus);
    Ok(id)
}

/// Enumerate devices on an IPack bus (Linux `ipack_bus_add_dev`).
pub fn enumerate_devices(bus_id: u32) -> Result<Vec<u32>, &'static str> {
    let (slots, read_id_fn) = {
        let buses = IPACK_BUSES.read();
        let bus = buses.get(&bus_id).ok_or("IPack bus not found")?;
        (bus.slots, bus.ops.read_id)
    };

    let mut registered = Vec::new();
    for slot in 0..slots {
        match (read_id_fn)(bus_id, slot) {
            Ok((vid, did, vname, dname)) => {
                let dev_id = DEVICE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
                let dev = IpackDevice {
                    id: dev_id,
                    bus_id,
                    slot,
                    name: alloc::format!("ipack-{}-{}", bus_id, slot),
                    vendor_id: vid,
                    device_id: did,
                    vendor_name: vname,
                    device_name: dname,
                    speed: 8_000_000,
                    mem_base: 0,
                    io_base: 0,
                    irq: slot as u32,
                    driver_name: None,
                    bound: false,
                };
                IPACK_DEVICES.write().insert(dev_id, dev);

                let mut buses = IPACK_BUSES.write();
                if let Some(bus) = buses.get_mut(&bus_id) {
                    bus.device_ids.push(dev_id);
                }
                registered.push(dev_id);
                try_match_driver(dev_id)?;
            }
            Err(_) => continue,
        }
    }
    Ok(registered)
}

/// Map address space for a device (Linux `ipack_bus_ops.map_space`).
pub fn map_space(
    bus_id: u32,
    device_id: u32,
    space: IpackSpace,
    addr: u64,
    size: u64,
) -> Result<(), &'static str> {
    let map_fn = {
        let buses = IPACK_BUSES.read();
        let bus = buses.get(&bus_id).ok_or("IPack bus not found")?;
        bus.ops.map_space
    };
    (map_fn)(bus_id, device_id, space, addr, size)
}

/// Unmap address space for a device.
pub fn unmap_space(bus_id: u32, device_id: u32, space: IpackSpace) -> Result<(), &'static str> {
    let unmap_fn = {
        let buses = IPACK_BUSES.read();
        let bus = buses.get(&bus_id).ok_or("IPack bus not found")?;
        bus.ops.unmap_space
    };
    (unmap_fn)(bus_id, device_id, space)
}

/// Register an IPack driver.
pub fn register_driver(driver: IpackDriver) -> Result<u32, &'static str> {
    let id = DRIVER_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let id_table = driver.id_table.clone();
    IPACK_DRIVERS.write().insert(id, driver);

    let device_ids: Vec<u32> = {
        let devices = IPACK_DEVICES.read();
        devices
            .iter()
            .filter(|(_, d)| {
                !d.bound
                    && id_table
                        .iter()
                        .any(|id| id.vendor_id == d.vendor_id && id.device_id == d.device_id)
            })
            .map(|(id, _)| *id)
            .collect()
    };
    for dev_id in device_ids {
        try_match_driver(dev_id)?;
    }
    Ok(id)
}

/// Try to match a device with a driver.
fn try_match_driver(device_id: u32) -> Result<(), &'static str> {
    let matched = {
        let devices = IPACK_DEVICES.read();
        let dev = match devices.get(&device_id) {
            Some(d) if !d.bound => d,
            _ => return Ok(()),
        };
        let vid = dev.vendor_id;
        let did = dev.device_id;

        let drivers = IPACK_DRIVERS.read();
        let mut found: Option<(fn(u32) -> Result<(), &'static str>, String)> = None;
        for (_, drv) in drivers.iter() {
            for id in &drv.id_table {
                if id.vendor_id == vid && id.device_id == did {
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
        (probe_fn)(device_id)?;
        let mut devices = IPACK_DEVICES.write();
        if let Some(dev) = devices.get_mut(&device_id) {
            dev.bound = true;
            dev.driver_name = Some(drv_name);
        }
    }
    Ok(())
}

/// List all IPack buses.
pub fn list_buses() -> Vec<(u32, String, u8, u32)> {
    IPACK_BUSES
        .read()
        .iter()
        .map(|(id, b)| (*id, b.name.clone(), b.slots, b.bus_nr))
        .collect()
}

/// List devices on a bus.
pub fn list_devices(bus_id: u32) -> Result<Vec<(u32, String, u8, u8, bool)>, &'static str> {
    let buses = IPACK_BUSES.read();
    let bus = buses.get(&bus_id).ok_or("IPack bus not found")?;
    let devices = IPACK_DEVICES.read();
    let mut result = Vec::new();
    for &dev_id in &bus.device_ids {
        if let Some(dev) = devices.get(&dev_id) {
            result.push((
                dev_id,
                dev.name.clone(),
                dev.vendor_id,
                dev.device_id,
                dev.bound,
            ));
        }
    }
    Ok(result)
}

/// Count registered buses.
pub fn bus_count() -> usize {
    IPACK_BUSES.read().len()
}

// ── Software IPack ──────────────────────────────────────────────────────

fn sw_read_id(_bus_id: u32, slot: u8) -> Result<(u8, u8, String, String), &'static str> {
    if slot == 0 {
        Ok((
            0x01,
            0x02,
            String::from("TEWS"),
            String::from("TEWS-IP-TMP"),
        ))
    } else {
        Err("No device in slot")
    }
}
fn sw_map_space(
    _bus_id: u32,
    _device_id: u32,
    _space: IpackSpace,
    _addr: u64,
    _size: u64,
) -> Result<(), &'static str> {
    Ok(())
}
fn sw_unmap_space(_bus_id: u32, _device_id: u32, _space: IpackSpace) -> Result<(), &'static str> {
    Ok(())
}
fn sw_request_irq(_bus_id: u32, _device_id: u32, _handler: fn(u32)) -> Result<(), &'static str> {
    Ok(())
}
fn sw_free_irq(_bus_id: u32, _device_id: u32) -> Result<(), &'static str> {
    Ok(())
}

/// Software IPack bus ops.
pub fn software_ipack_ops() -> IpackBusOps {
    IpackBusOps {
        read_id: sw_read_id,
        map_space: sw_map_space,
        unmap_space: sw_unmap_space,
        request_irq: sw_request_irq,
        free_irq: sw_free_irq,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

fn null_probe(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn null_remove(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}

pub fn init() -> Result<(), &'static str> {
    let ops = software_ipack_ops();
    let bus_id = register_bus("sw-ipack-0", ops, 4, 0)?;
    enumerate_devices(bus_id)?;

    // Register a driver for TEWS TMP module
    let mut id_table = Vec::new();
    id_table.push(IpackDeviceId {
        vendor_id: 0x01,
        device_id: 0x02,
    });
    let driver = IpackDriver {
        name: String::from("sw-ipack-tmp-drv"),
        id_table,
        probe: null_probe,
        remove: null_remove,
    };
    register_driver(driver)?;

    Ok(())
}
