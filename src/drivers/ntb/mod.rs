//! NTB (Non-Transparent Bridge) subsystem
//!
//! Provides NTB framework for bridging two separate host domains via
//! shared memory windows. Mirrors Linux's `drivers/ntb/ntb.c`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// NTB device (Linux `struct ntb_dev`).
pub struct NtbDev {
    pub id: u32,
    pub name: String,
    pub ops: NtbOps,
    pub port: NtbPort,
    pub peer_count: u32,
    pub mw_count: u32,
    pub link: bool,
    pub ctx: Option<u64>,
}

/// NTB port (Linux `enum ntb_default_port`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NtbPort {
    Primary,
    Secondary,
}

/// NTB operations (Linux `struct ntb_dev_ops`).
pub struct NtbOps {
    pub port_number: fn(dev_id: u32) -> Result<u32, &'static str>,
    pub peer_port_count: fn(dev_id: u32) -> Result<u32, &'static str>,
    pub peer_port_number: fn(dev_id: u32, peer_idx: u32) -> Result<u32, &'static str>,
    pub link_is_up: fn(dev_id: u32) -> Result<bool, &'static str>,
    pub link_enable: fn(dev_id: u32) -> Result<(), &'static str>,
    pub link_disable: fn(dev_id: u32) -> Result<(), &'static str>,
    pub mw_count: fn(dev_id: u32, peer: u32) -> Result<u32, &'static str>,
    pub mw_get_align: fn(dev_id: u32, peer: u32, mw_idx: u32) -> Result<NtbMwAlign, &'static str>,
    pub mw_set_trans:
        fn(dev_id: u32, peer: u32, mw_idx: u32, addr: u64, size: u64) -> Result<(), &'static str>,
    pub mw_clear_trans: fn(dev_id: u32, peer: u32, mw_idx: u32) -> Result<(), &'static str>,
    pub peer_mw_count: fn(dev_id: u32) -> Result<u32, &'static str>,
    pub peer_mw_get_addr: fn(dev_id: u32, mw_idx: u32) -> Result<u64, &'static str>,
    pub db_set: fn(dev_id: u32, bits: u64) -> Result<(), &'static str>,
    pub db_clear: fn(dev_id: u32, bits: u64) -> Result<(), &'static str>,
    pub db_read: fn(dev_id: u32) -> Result<u64, &'static str>,
    pub db_set_mask: fn(dev_id: u32, bits: u64) -> Result<(), &'static str>,
    pub db_clear_mask: fn(dev_id: u32, bits: u64) -> Result<(), &'static str>,
    pub db_mask: fn(dev_id: u32) -> Result<u64, &'static str>,
    pub peer_db_set: fn(dev_id: u32, bits: u64) -> Result<(), &'static str>,
    pub spad_count: fn(dev_id: u32) -> Result<u32, &'static str>,
    pub spad_write: fn(dev_id: u32, idx: u32, val: u32) -> Result<(), &'static str>,
    pub spad_read: fn(dev_id: u32, idx: u32) -> Result<u32, &'static str>,
    pub peer_spad_write: fn(dev_id: u32, idx: u32, val: u32) -> Result<(), &'static str>,
    pub peer_spad_read: fn(dev_id: u32, idx: u32) -> Result<u32, &'static str>,
}

/// NTB memory window alignment (Linux `struct ntb_mw_align`).
#[derive(Debug, Clone)]
pub struct NtbMwAlign {
    pub addr_align: u64,
    pub size_align: u64,
    pub size_max: u64,
}

/// NTB client (Linux `struct ntb_client`).
pub struct NtbClient {
    pub id: u32,
    pub name: String,
    pub probe: fn(dev_id: u32) -> Result<(), &'static str>,
    pub remove: fn(dev_id: u32) -> Result<(), &'static str>,
    pub link_event: Option<fn(dev_id: u32, up: bool)>,
}

// ── Registry ────────────────────────────────────────────────────────────

static DEV_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static CLIENT_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static NTB_DEVS: RwLock<BTreeMap<u32, NtbDev>> = RwLock::new(BTreeMap::new());
static NTB_CLIENTS: RwLock<BTreeMap<u32, NtbClient>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register an NTB device.
pub fn register_device(
    name: &str,
    ops: NtbOps,
    port: NtbPort,
    peer_count: u32,
    mw_count: u32,
) -> Result<u32, &'static str> {
    if name.is_empty() {
        return Err("NTB device name is empty");
    }
    if peer_count == 0 {
        return Err("NTB device has no peers");
    }
    if mw_count == 0 {
        return Err("NTB device has no memory windows");
    }

    let mut devs = NTB_DEVS.write();
    if devs.values().any(|dev| dev.name == name) {
        return Err("NTB device already registered");
    }

    let id = DEV_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dev = NtbDev {
        id,
        name: String::from(name),
        ops,
        port,
        peer_count,
        mw_count,
        link: false,
        ctx: None,
    };
    devs.insert(id, dev);
    Ok(id)
}

/// Enable NTB link (Linux `ntb_link_enable`).
pub fn link_enable(dev_id: u32) -> Result<(), &'static str> {
    let enable_fn = {
        let devs = NTB_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("NTB device not found")?;
        dev.ops.link_enable
    };
    (enable_fn)(dev_id)?;

    let mut devs = NTB_DEVS.write();
    if let Some(dev) = devs.get_mut(&dev_id) {
        dev.link = true;
    }
    notify_link_event(dev_id, true);
    Ok(())
}

/// Disable NTB link (Linux `ntb_link_disable`).
pub fn link_disable(dev_id: u32) -> Result<(), &'static str> {
    let disable_fn = {
        let devs = NTB_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("NTB device not found")?;
        dev.ops.link_disable
    };
    (disable_fn)(dev_id)?;

    let mut devs = NTB_DEVS.write();
    if let Some(dev) = devs.get_mut(&dev_id) {
        dev.link = false;
    }
    notify_link_event(dev_id, false);
    Ok(())
}

/// Check if link is up (Linux `ntb_link_is_up`).
pub fn link_is_up(dev_id: u32) -> Result<bool, &'static str> {
    let devs = NTB_DEVS.read();
    let dev = devs.get(&dev_id).ok_or("NTB device not found")?;
    Ok(dev.link)
}

/// Set memory window translation (Linux `ntb_mw_set_trans`).
pub fn mw_set_trans(
    dev_id: u32,
    peer: u32,
    mw_idx: u32,
    addr: u64,
    size: u64,
) -> Result<(), &'static str> {
    if addr == 0 || size == 0 {
        return Err("NTB memory window translation is empty");
    }

    let (set_fn, align_fn, peer_count, mw_count) = {
        let devs = NTB_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("NTB device not found")?;
        (
            dev.ops.mw_set_trans,
            dev.ops.mw_get_align,
            dev.peer_count,
            dev.mw_count,
        )
    };
    if peer >= peer_count {
        return Err("NTB peer index out of range");
    }
    if mw_idx >= mw_count {
        return Err("NTB memory window index out of range");
    }

    let align = (align_fn)(dev_id, peer, mw_idx)?;
    if align.addr_align != 0 && addr % align.addr_align != 0 {
        return Err("NTB memory window address is misaligned");
    }
    if align.size_align != 0 && size % align.size_align != 0 {
        return Err("NTB memory window size is misaligned");
    }
    if align.size_max != 0 && size > align.size_max {
        return Err("NTB memory window size too large");
    }

    (set_fn)(dev_id, peer, mw_idx, addr, size)
}

/// Clear memory window translation (Linux `ntb_mw_clear_trans`).
pub fn mw_clear_trans(dev_id: u32, peer: u32, mw_idx: u32) -> Result<(), &'static str> {
    let clear_fn = {
        let devs = NTB_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("NTB device not found")?;
        if peer >= dev.peer_count {
            return Err("NTB peer index out of range");
        }
        if mw_idx >= dev.mw_count {
            return Err("NTB memory window index out of range");
        }
        dev.ops.mw_clear_trans
    };
    (clear_fn)(dev_id, peer, mw_idx)
}

/// Set doorbell bits (Linux `ntb_db_set`).
pub fn db_set(dev_id: u32, bits: u64) -> Result<(), &'static str> {
    let set_fn = {
        let devs = NTB_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("NTB device not found")?;
        dev.ops.db_set
    };
    (set_fn)(dev_id, bits)
}

/// Clear doorbell bits (Linux `ntb_db_clear`).
pub fn db_clear(dev_id: u32, bits: u64) -> Result<(), &'static str> {
    let clear_fn = {
        let devs = NTB_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("NTB device not found")?;
        dev.ops.db_clear
    };
    (clear_fn)(dev_id, bits)
}

/// Read doorbell (Linux `ntb_db_read`).
pub fn db_read(dev_id: u32) -> Result<u64, &'static str> {
    let read_fn = {
        let devs = NTB_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("NTB device not found")?;
        dev.ops.db_read
    };
    (read_fn)(dev_id)
}

/// Set peer doorbell (Linux `ntb_peer_db_set`).
pub fn peer_db_set(dev_id: u32, bits: u64) -> Result<(), &'static str> {
    let set_fn = {
        let devs = NTB_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("NTB device not found")?;
        dev.ops.peer_db_set
    };
    (set_fn)(dev_id, bits)
}

/// Write scratchpad register (Linux `ntb_spad_write`).
pub fn spad_write(dev_id: u32, idx: u32, val: u32) -> Result<(), &'static str> {
    let write_fn = {
        let devs = NTB_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("NTB device not found")?;
        let count = (dev.ops.spad_count)(dev_id)?;
        if idx >= count {
            return Err("NTB scratchpad index out of range");
        }
        dev.ops.spad_write
    };
    (write_fn)(dev_id, idx, val)
}

/// Read scratchpad register (Linux `ntb_spad_read`).
pub fn spad_read(dev_id: u32, idx: u32) -> Result<u32, &'static str> {
    let read_fn = {
        let devs = NTB_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("NTB device not found")?;
        let count = (dev.ops.spad_count)(dev_id)?;
        if idx >= count {
            return Err("NTB scratchpad index out of range");
        }
        dev.ops.spad_read
    };
    (read_fn)(dev_id, idx)
}

/// Register an NTB client driver.
pub fn register_client(mut client: NtbClient) -> Result<u32, &'static str> {
    let id = CLIENT_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let probe_fn = client.probe;
    client.id = id;
    NTB_CLIENTS.write().insert(id, client);

    // Probe all existing devices
    let dev_ids: Vec<u32> = NTB_DEVS.read().keys().copied().collect();
    for dev_id in dev_ids {
        let _ = (probe_fn)(dev_id);
    }
    Ok(id)
}

/// Notify link event to all clients.
fn notify_link_event(dev_id: u32, up: bool) {
    let cbs: Vec<fn(u32, bool)> = {
        let clients = NTB_CLIENTS.read();
        clients.iter().filter_map(|(_, c)| c.link_event).collect()
    };
    for cb in cbs {
        cb(dev_id, up);
    }
}

/// List all NTB devices.
pub fn list_devices() -> Vec<(u32, String, NtbPort, bool)> {
    NTB_DEVS
        .read()
        .iter()
        .map(|(id, d)| (*id, d.name.clone(), d.port, d.link))
        .collect()
}

/// Count registered devices.
pub fn device_count() -> usize {
    NTB_DEVS.read().len()
}

// ── Software NTB ────────────────────────────────────────────────────────

fn sw_port_number(_dev_id: u32) -> Result<u32, &'static str> {
    Ok(0)
}
fn sw_peer_port_count(_dev_id: u32) -> Result<u32, &'static str> {
    Ok(1)
}
fn sw_peer_port_number(_dev_id: u32, _peer_idx: u32) -> Result<u32, &'static str> {
    Ok(1)
}
fn sw_link_is_up(dev_id: u32) -> Result<bool, &'static str> {
    let devs = NTB_DEVS.read();
    Ok(devs.get(&dev_id).map(|d| d.link).unwrap_or(false))
}
fn sw_link_enable(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_link_disable(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_mw_count(_dev_id: u32, _peer: u32) -> Result<u32, &'static str> {
    Ok(2)
}
fn sw_mw_get_align(_dev_id: u32, _peer: u32, _mw_idx: u32) -> Result<NtbMwAlign, &'static str> {
    Ok(NtbMwAlign {
        addr_align: 4096,
        size_align: 4096,
        size_max: 1024 * 1024,
    })
}
fn sw_mw_set_trans(
    _dev_id: u32,
    _peer: u32,
    _mw_idx: u32,
    _addr: u64,
    _size: u64,
) -> Result<(), &'static str> {
    Ok(())
}
fn sw_mw_clear_trans(_dev_id: u32, _peer: u32, _mw_idx: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_peer_mw_count(_dev_id: u32) -> Result<u32, &'static str> {
    Ok(2)
}
fn sw_peer_mw_get_addr(_dev_id: u32, mw_idx: u32) -> Result<u64, &'static str> {
    Ok(0x40000000 + (mw_idx as u64) * 0x100000)
}
fn sw_db_set(_dev_id: u32, _bits: u64) -> Result<(), &'static str> {
    Ok(())
}
fn sw_db_clear(_dev_id: u32, _bits: u64) -> Result<(), &'static str> {
    Ok(())
}
fn sw_db_read(_dev_id: u32) -> Result<u64, &'static str> {
    Ok(0)
}
fn sw_db_set_mask(_dev_id: u32, _bits: u64) -> Result<(), &'static str> {
    Ok(())
}
fn sw_db_clear_mask(_dev_id: u32, _bits: u64) -> Result<(), &'static str> {
    Ok(())
}
fn sw_db_mask(_dev_id: u32) -> Result<u64, &'static str> {
    Ok(0xFFFF)
}
fn sw_peer_db_set(_dev_id: u32, _bits: u64) -> Result<(), &'static str> {
    Ok(())
}
fn sw_spad_count(_dev_id: u32) -> Result<u32, &'static str> {
    Ok(16)
}
fn sw_spad_write(_dev_id: u32, _idx: u32, _val: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_spad_read(_dev_id: u32, _idx: u32) -> Result<u32, &'static str> {
    Ok(0)
}
fn sw_peer_spad_write(_dev_id: u32, _idx: u32, _val: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_peer_spad_read(_dev_id: u32, _idx: u32) -> Result<u32, &'static str> {
    Ok(0)
}

/// Software NTB ops.
pub fn software_ntb_ops() -> NtbOps {
    NtbOps {
        port_number: sw_port_number,
        peer_port_count: sw_peer_port_count,
        peer_port_number: sw_peer_port_number,
        link_is_up: sw_link_is_up,
        link_enable: sw_link_enable,
        link_disable: sw_link_disable,
        mw_count: sw_mw_count,
        mw_get_align: sw_mw_get_align,
        mw_set_trans: sw_mw_set_trans,
        mw_clear_trans: sw_mw_clear_trans,
        peer_mw_count: sw_peer_mw_count,
        peer_mw_get_addr: sw_peer_mw_get_addr,
        db_set: sw_db_set,
        db_clear: sw_db_clear,
        db_read: sw_db_read,
        db_set_mask: sw_db_set_mask,
        db_clear_mask: sw_db_clear_mask,
        db_mask: sw_db_mask,
        peer_db_set: sw_peer_db_set,
        spad_count: sw_spad_count,
        spad_write: sw_spad_write,
        spad_read: sw_spad_read,
        peer_spad_write: sw_peer_spad_write,
        peer_spad_read: sw_peer_spad_read,
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
    if !NTB_DEVS.read().is_empty() {
        return Ok(());
    }
    register_device(
        "software-ntb",
        software_ntb_ops(),
        NtbPort::Primary,
        1, // peer_count
        2, // mw_count
    )?;
    crate::serial_println!("ntb: software device registered");
    crate::serial_println!("ntb: subsystem ready");
    Ok(())
}
