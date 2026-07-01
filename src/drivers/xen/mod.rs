//! Xen hypervisor backend subsystem
//!
//! Provides Xen paravirtualized device backend framework.
//! Mirrors Linux's `drivers/xen/`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// Xen backend device (Linux `struct xenbus_device`).
pub struct XenDevice {
    pub id: u32,
    pub name: String,
    pub dev_type: XenDevType,
    pub frontend_id: u32,
    pub state: XenBusState,
    pub backend_path: String,
    pub frontend_path: String,
    pub driver_name: Option<String>,
    pub bound: bool,
}

/// Xen device type (Linux `enum xenbus_state` subset).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XenDevType {
    Vbd, // Virtual block device
    Vif, // Virtual network interface
    Console,
    Pvcalls,
    Ninepfs,
    Tpm,
    Tmem,
}

/// Xen bus state (Linux `enum xenbus_state`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XenBusState {
    Unknown,
    Initialising,
    InitWait,
    Initialised,
    Connected,
    Closing,
    Closed,
    Reconfiguring,
    Reconfigured,
}

/// Xen driver (Linux `struct xenbus_driver`).
pub struct XenDriver {
    pub name: String,
    pub id_table: Vec<XenDeviceId>,
    pub probe: fn(dev_id: u32) -> Result<(), &'static str>,
    pub remove: fn(dev_id: u32) -> Result<(), &'static str>,
    pub otherend_changed: Option<fn(dev_id: u32, state: XenBusState)>,
}

/// Xen device ID (Linux `struct xenbus_device_id`).
#[derive(Debug, Clone)]
pub struct XenDeviceId {
    pub dev_type: XenDevType,
}

/// Xen store (shared configuration store).
pub struct XenStore {
    pub id: u32,
    pub entries: BTreeMap<String, String>,
}

/// Xen store watch (Linux `struct xenbus_watch`).
pub struct XenWatch {
    pub path: String,
    pub token: u32,
    pub triggered: u32,
}

/// Xen grant-table entry.
#[derive(Debug, Clone)]
pub struct XenGrant {
    pub domid: u32,
    pub gfn: u64,
    pub readonly: bool,
}

// ── Registry ────────────────────────────────────────────────────────────

static DEV_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DRIVER_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static WATCH_TOKEN_COUNTER: AtomicU32 = AtomicU32::new(0);
static GRANT_REF_COUNTER: AtomicU32 = AtomicU32::new(1);

static XEN_DEVICES: RwLock<BTreeMap<u32, XenDevice>> = RwLock::new(BTreeMap::new());
static XEN_DRIVERS: RwLock<BTreeMap<u32, XenDriver>> = RwLock::new(BTreeMap::new());
static XEN_WATCHES: RwLock<BTreeMap<u32, XenWatch>> = RwLock::new(BTreeMap::new());
static XEN_GRANTS: RwLock<BTreeMap<u32, XenGrant>> = RwLock::new(BTreeMap::new());
static XEN_STORE: RwLock<XenStore> = RwLock::new(XenStore {
    id: 0,
    entries: BTreeMap::new(),
});

// ── Public API ──────────────────────────────────────────────────────────

/// Register a Xen backend device.
pub fn register_device(
    name: &str,
    dev_type: XenDevType,
    frontend_id: u32,
    backend_path: &str,
    frontend_path: &str,
) -> Result<u32, &'static str> {
    if name.trim().is_empty() {
        return Err("Xen device name required");
    }
    if backend_path.trim().is_empty() || frontend_path.trim().is_empty() {
        return Err("Xen device path required");
    }

    let mut devices = XEN_DEVICES.write();
    if devices.values().any(|dev| {
        dev.name == name || dev.backend_path == backend_path || dev.frontend_path == frontend_path
    }) {
        return Err("Xen device already registered");
    }

    let id = DEV_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dev = XenDevice {
        id,
        name: String::from(name),
        dev_type,
        frontend_id,
        state: XenBusState::Initialising,
        backend_path: String::from(backend_path),
        frontend_path: String::from(frontend_path),
        driver_name: None,
        bound: false,
    };
    devices.insert(id, dev);
    drop(devices);

    if let Err(err) = try_match_driver(id) {
        XEN_DEVICES.write().remove(&id);
        return Err(err);
    }
    Ok(id)
}

/// Set Xen bus state (Linux `xenbus_switch_state`).
pub fn set_state(dev_id: u32, state: XenBusState) -> Result<(), &'static str> {
    let cb_fn = {
        let mut devices = XEN_DEVICES.write();
        let dev = devices.get_mut(&dev_id).ok_or("Xen device not found")?;
        let old_state = dev.state;
        if !valid_xenbus_transition(old_state, state) {
            return Err("Invalid Xen bus state transition");
        }
        dev.state = state;
        if old_state == state {
            return Ok(());
        }
        let state_key = alloc::format!("{}/state", dev.backend_path);
        let drv_name = dev.driver_name.clone();
        drop(devices);

        store_write(&state_key, xenbus_state_name(state))?;

        if let Some(dn) = drv_name {
            let drivers = XEN_DRIVERS.read();
            drivers
                .iter()
                .find(|(_, d)| d.name == dn)
                .and_then(|(_, d)| d.otherend_changed)
        } else {
            None
        }
    };

    if let Some(cb) = cb_fn {
        cb(dev_id, state);
    }
    Ok(())
}

/// Read from Xen store (Linux `xenbus_read`).
pub fn store_read(key: &str) -> Result<String, &'static str> {
    let store = XEN_STORE.read();
    store
        .entries
        .get(key)
        .cloned()
        .ok_or("Xen store key not found")
}

/// Write to Xen store (Linux `xenbus_write`).
pub fn store_write(key: &str, value: &str) -> Result<(), &'static str> {
    if key.trim().is_empty() {
        return Err("Xen store key required");
    }

    let mut store = XEN_STORE.write();
    store.entries.insert(String::from(key), String::from(value));
    drop(store);
    fire_watch(key);
    Ok(())
}

/// Watch a Xen store path (Linux `xenbus_watch`).
pub fn store_watch(path: &str) -> Result<u32, &'static str> {
    if path.trim().is_empty() {
        return Err("Xen watch path required");
    }

    let token = WATCH_TOKEN_COUNTER.fetch_add(1, Ordering::SeqCst);
    let watch = XenWatch {
        path: String::from(path),
        token,
        triggered: 0,
    };
    XEN_WATCHES.write().insert(token, watch);
    store_write(&alloc::format!("watch/{}/path", token), path)?;
    Ok(token)
}

/// Unregister a Xen store watch (Linux `unregister_xenbus_watch`).
pub fn unwatch(token: u32) -> Result<(), &'static str> {
    XEN_WATCHES
        .write()
        .remove(&token)
        .ok_or("Watch not found")?;
    Ok(())
}

/// Fire a watch for a given path (called when xenstore data changes).
pub fn fire_watch(path: &str) {
    let mut watches = XEN_WATCHES.write();
    for (_, w) in watches.iter_mut() {
        if path == w.path || path.starts_with(&alloc::format!("{}/", w.path)) {
            w.triggered += 1;
        }
    }
}

/// List all active watches.
pub fn list_watches() -> Vec<(u32, String, u32)> {
    XEN_WATCHES
        .read()
        .iter()
        .map(|(t, w)| (*t, w.path.clone(), w.triggered))
        .collect()
}

/// Grant access to a page (Linux `gnttab_grant_foreign_access`).
pub fn grant_access(domid: u32, gfn: u64, readonly: bool) -> Result<u32, &'static str> {
    if domid == u32::MAX {
        return Err("Invalid Xen domain id");
    }

    let grant_ref = GRANT_REF_COUNTER
        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |current| {
            if current == u32::MAX {
                None
            } else {
                Some(current + 1)
            }
        })
        .map_err(|_| "Xen grant reference exhausted")?;
    XEN_GRANTS.write().insert(
        grant_ref,
        XenGrant {
            domid,
            gfn,
            readonly,
        },
    );
    Ok(grant_ref)
}

/// End grant access (Linux `gnttab_end_foreign_access`).
pub fn end_grant_access(grant_ref: u32) -> Result<(), &'static str> {
    XEN_GRANTS
        .write()
        .remove(&grant_ref)
        .ok_or("Xen grant reference not found")?;
    Ok(())
}

/// Register a Xen driver.
pub fn register_driver(driver: XenDriver) -> Result<u32, &'static str> {
    if driver.name.trim().is_empty() {
        return Err("Xen driver name required");
    }
    if driver.id_table.is_empty() {
        return Err("Xen driver id table required");
    }

    let mut drivers = XEN_DRIVERS.write();
    if drivers.values().any(|drv| drv.name == driver.name) {
        return Err("Xen driver already registered");
    }

    let id = DRIVER_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let driver_name = driver.name.clone();
    let id_table = driver.id_table.clone();
    drivers.insert(id, driver);
    drop(drivers);

    let device_ids: Vec<u32> = {
        let devices = XEN_DEVICES.read();
        devices
            .iter()
            .filter(|(_, d)| !d.bound && id_table.iter().any(|id| id.dev_type == d.dev_type))
            .map(|(id, _)| *id)
            .collect()
    };
    for dev_id in device_ids {
        if let Err(err) = try_match_driver(dev_id) {
            XEN_DRIVERS.write().remove(&id);
            let mut devices = XEN_DEVICES.write();
            for (_, dev) in devices.iter_mut() {
                if dev.driver_name.as_deref() == Some(driver_name.as_str())
                    && id_table.iter().any(|idt| idt.dev_type == dev.dev_type)
                {
                    dev.bound = false;
                    dev.driver_name = None;
                }
            }
            return Err(err);
        }
    }
    Ok(id)
}

/// Try to match a device with a driver.
fn try_match_driver(device_id: u32) -> Result<(), &'static str> {
    let matched = {
        let devices = XEN_DEVICES.read();
        let dev = match devices.get(&device_id) {
            Some(d) if !d.bound => d,
            _ => return Ok(()),
        };
        let dev_type = dev.dev_type;

        let drivers = XEN_DRIVERS.read();
        let mut found: Option<(fn(u32) -> Result<(), &'static str>, String)> = None;
        for (_, drv) in drivers.iter() {
            for id in &drv.id_table {
                if id.dev_type == dev_type {
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
        let mut devices = XEN_DEVICES.write();
        if let Some(dev) = devices.get_mut(&device_id) {
            dev.bound = true;
            dev.driver_name = Some(drv_name);
        }
    }
    Ok(())
}

fn valid_xenbus_transition(old: XenBusState, new: XenBusState) -> bool {
    if old == new {
        return true;
    }

    match old {
        XenBusState::Unknown => matches!(new, XenBusState::Initialising | XenBusState::Closed),
        XenBusState::Initialising => matches!(
            new,
            XenBusState::InitWait | XenBusState::Initialised | XenBusState::Closing
        ),
        XenBusState::InitWait => matches!(
            new,
            XenBusState::Initialised | XenBusState::Connected | XenBusState::Closing
        ),
        XenBusState::Initialised => matches!(
            new,
            XenBusState::Connected | XenBusState::Closing | XenBusState::Reconfiguring
        ),
        XenBusState::Connected => {
            matches!(new, XenBusState::Closing | XenBusState::Reconfiguring)
        }
        XenBusState::Closing => matches!(new, XenBusState::Closed),
        XenBusState::Closed => matches!(new, XenBusState::Initialising),
        XenBusState::Reconfiguring => {
            matches!(new, XenBusState::Reconfigured | XenBusState::Closing)
        }
        XenBusState::Reconfigured => matches!(new, XenBusState::Connected | XenBusState::Closing),
    }
}

fn xenbus_state_name(state: XenBusState) -> &'static str {
    match state {
        XenBusState::Unknown => "Unknown",
        XenBusState::Initialising => "Initialising",
        XenBusState::InitWait => "InitWait",
        XenBusState::Initialised => "Initialised",
        XenBusState::Connected => "Connected",
        XenBusState::Closing => "Closing",
        XenBusState::Closed => "Closed",
        XenBusState::Reconfiguring => "Reconfiguring",
        XenBusState::Reconfigured => "Reconfigured",
    }
}

/// List all Xen devices.
pub fn list_devices() -> Vec<(u32, String, XenDevType, XenBusState, bool)> {
    XEN_DEVICES
        .read()
        .iter()
        .map(|(id, d)| (*id, d.name.clone(), d.dev_type, d.state, d.bound))
        .collect()
}

/// Count registered devices.
pub fn device_count() -> usize {
    XEN_DEVICES.read().len()
}

// ── Init ────────────────────────────────────────────────────────────────

fn null_probe(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn null_remove(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}

pub fn init() -> Result<(), &'static str> {
    if device_count() > 0 {
        return Ok(());
    }

    // Write some initial Xen store entries
    store_write("domid", "0")?;
    store_write("name", "RustOS-Dom0")?;

    // Register a virtual block device backend
    register_device(
        "vbd-0",
        XenDevType::Vbd,
        1,
        "backend/vbd/1/51712",
        "frontend/vbd/1/51712",
    )?;

    // Register a virtual network interface backend
    register_device(
        "vif-0",
        XenDevType::Vif,
        1,
        "backend/vif/1/0",
        "frontend/vif/1/0",
    )?;

    // Register drivers
    let mut id_table = Vec::new();
    id_table.push(XenDeviceId {
        dev_type: XenDevType::Vbd,
    });
    let vbd_driver = XenDriver {
        name: String::from("sw-xen-vbd"),
        id_table,
        probe: null_probe,
        remove: null_remove,
        otherend_changed: None,
    };
    register_driver(vbd_driver)?;

    let mut id_table2 = Vec::new();
    id_table2.push(XenDeviceId {
        dev_type: XenDevType::Vif,
    });
    let vif_driver = XenDriver {
        name: String::from("sw-xen-vif"),
        id_table: id_table2,
        probe: null_probe,
        remove: null_remove,
        otherend_changed: None,
    };
    register_driver(vif_driver)?;

    // Transition devices to connected state
    let dev_ids: Vec<u32> = XEN_DEVICES.read().keys().copied().collect();
    for did in dev_ids {
        set_state(did, XenBusState::Initialised)?;
        set_state(did, XenBusState::Connected)?;
    }

    Ok(())
}
