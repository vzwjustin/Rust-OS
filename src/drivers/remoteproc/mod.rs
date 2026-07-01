//! Remoteproc (Remote Processor) subsystem
//!
//! Provides a framework for loading firmware onto and managing coprocessors.
//! Mirrors Linux's `drivers/remoteproc/`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// RPROC state (Linux `enum rproc_state`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RprocState {
    Offline,
    Suspended,
    Running,
    Crashed,
    Detached,
}

/// RPROC operations (Linux `struct rproc_ops`).
pub struct RprocOps {
    pub prepare: fn(rproc_id: u32) -> Result<(), &'static str>,
    pub unprepare: fn(rproc_id: u32) -> Result<(), &'static str>,
    pub start: fn(rproc_id: u32) -> Result<(), &'static str>,
    pub stop: fn(rproc_id: u32) -> Result<(), &'static str>,
    pub attach: fn(rproc_id: u32) -> Result<(), &'static str>,
    pub detach: fn(rproc_id: u32) -> Result<(), &'static str>,
    pub kick: fn(rproc_id: u32, vqid: u32) -> Result<(), &'static str>,
    pub load: fn(rproc_id: u32, fw: &[u8]) -> Result<u64, &'static str>,
    pub get_loaded_rsc_table: fn(rproc_id: u32) -> Result<Vec<u8>, &'static str>,
}

/// Remote processor device (Linux `struct rproc`).
pub struct Rproc {
    pub id: u32,
    pub name: String,
    pub firmware: String,
    pub state: RprocState,
    pub ops: RprocOps,
    pub mem_table: Vec<RprocMem>,
    pub vring_count: u32,
    pub boot_addr: u64,
}

/// RPROC memory entry (Linux `struct rproc_mem_entry`).
#[derive(Debug, Clone)]
pub struct RprocMem {
    pub name: String,
    pub da: u64,
    pub pa: u64,
    pub size: u64,
}

// ── Registry ────────────────────────────────────────────────────────────

static RPROC_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static RPROCS: RwLock<BTreeMap<u32, Rproc>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register a remote processor.
pub fn register_rproc(
    name: &str,
    firmware: &str,
    ops: RprocOps,
    mem_table: Vec<RprocMem>,
    vring_count: u32,
) -> Result<u32, &'static str> {
    let id = RPROC_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let rproc = Rproc {
        id,
        name: String::from(name),
        firmware: String::from(firmware),
        state: RprocState::Offline,
        ops,
        mem_table,
        vring_count,
        boot_addr: 0,
    };
    RPROCS.write().insert(id, rproc);
    Ok(id)
}

/// Unregister a remote processor.
pub fn unregister_rproc(rproc_id: u32) -> Result<(), &'static str> {
    RPROCS.write().remove(&rproc_id).ok_or("rproc not found")?;
    Ok(())
}

/// Boot a remote processor (Linux `rproc_boot`).
pub fn boot(rproc_id: u32) -> Result<(), &'static str> {
    let (prepare_fn, start_fn) = {
        let rprocs = RPROCS.read();
        let rproc = rprocs.get(&rproc_id).ok_or("rproc not found")?;
        if rproc.state == RprocState::Running {
            return Ok(());
        }
        (rproc.ops.prepare, rproc.ops.start)
    };
    (prepare_fn)(rproc_id)?;
    (start_fn)(rproc_id)?;
    let mut rprocs = RPROCS.write();
    if let Some(rproc) = rprocs.get_mut(&rproc_id) {
        rproc.state = RprocState::Running;
    }
    Ok(())
}

/// Shutdown a remote processor (Linux `rproc_shutdown`).
pub fn shutdown(rproc_id: u32) -> Result<(), &'static str> {
    let (stop_fn, unprepare_fn) = {
        let rprocs = RPROCS.read();
        let rproc = rprocs.get(&rproc_id).ok_or("rproc not found")?;
        if rproc.state != RprocState::Running {
            return Ok(());
        }
        (rproc.ops.stop, rproc.ops.unprepare)
    };
    (stop_fn)(rproc_id)?;
    (unprepare_fn)(rproc_id)?;
    let mut rprocs = RPROCS.write();
    if let Some(rproc) = rprocs.get_mut(&rproc_id) {
        rproc.state = RprocState::Offline;
    }
    Ok(())
}

/// Attach to a running remote processor (Linux `rproc_attach`).
pub fn attach(rproc_id: u32) -> Result<(), &'static str> {
    let attach_fn = {
        let rprocs = RPROCS.read();
        let rproc = rprocs.get(&rproc_id).ok_or("rproc not found")?;
        rproc.ops.attach
    };
    (attach_fn)(rproc_id)?;
    let mut rprocs = RPROCS.write();
    if let Some(rproc) = rprocs.get_mut(&rproc_id) {
        rproc.state = RprocState::Running;
    }
    Ok(())
}

/// Detach from a remote processor (Linux `rproc_detach`).
pub fn detach(rproc_id: u32) -> Result<(), &'static str> {
    let detach_fn = {
        let rprocs = RPROCS.read();
        let rproc = rprocs.get(&rproc_id).ok_or("rproc not found")?;
        rproc.ops.detach
    };
    (detach_fn)(rproc_id)?;
    let mut rprocs = RPROCS.write();
    if let Some(rproc) = rprocs.get_mut(&rproc_id) {
        rproc.state = RprocState::Detached;
    }
    Ok(())
}

/// Kick a virtqueue on the remote processor (Linux `rproc_vq_kick`).
pub fn kick(rproc_id: u32, vqid: u32) -> Result<(), &'static str> {
    let kick_fn = {
        let rprocs = RPROCS.read();
        let rproc = rprocs.get(&rproc_id).ok_or("rproc not found")?;
        rproc.ops.kick
    };
    (kick_fn)(rproc_id, vqid)
}

/// Load firmware onto a remote processor (Linux `rproc_load_segments`).
pub fn load_firmware(rproc_id: u32, fw: &[u8]) -> Result<u64, &'static str> {
    let load_fn = {
        let rprocs = RPROCS.read();
        let rproc = rprocs.get(&rproc_id).ok_or("rproc not found")?;
        rproc.ops.load
    };
    let boot_addr = (load_fn)(rproc_id, fw)?;
    let mut rprocs = RPROCS.write();
    if let Some(rproc) = rprocs.get_mut(&rproc_id) {
        rproc.boot_addr = boot_addr;
    }
    Ok(boot_addr)
}

/// Get the resource table from a remote processor.
pub fn get_rsc_table(rproc_id: u32) -> Result<Vec<u8>, &'static str> {
    let rsc_fn = {
        let rprocs = RPROCS.read();
        let rproc = rprocs.get(&rproc_id).ok_or("rproc not found")?;
        rproc.ops.get_loaded_rsc_table
    };
    (rsc_fn)(rproc_id)
}

/// List all remote processors.
pub fn list_rprocs() -> Vec<(u32, String, RprocState, u32)> {
    RPROCS
        .read()
        .iter()
        .map(|(id, r)| (*id, r.name.clone(), r.state, r.vring_count))
        .collect()
}

/// Count registered remote processors.
pub fn rproc_count() -> usize {
    RPROCS.read().len()
}

// ── Software remoteproc ─────────────────────────────────────────────────

fn sw_prepare(_id: u32) -> Result<(), &'static str> {
    Ok(())
}

fn sw_unprepare(_id: u32) -> Result<(), &'static str> {
    Ok(())
}

fn sw_start(_id: u32) -> Result<(), &'static str> {
    Ok(())
}

fn sw_stop(_id: u32) -> Result<(), &'static str> {
    Ok(())
}

fn sw_attach(_id: u32) -> Result<(), &'static str> {
    Ok(())
}

fn sw_detach(_id: u32) -> Result<(), &'static str> {
    Ok(())
}

fn sw_kick(_id: u32, _vqid: u32) -> Result<(), &'static str> {
    Ok(())
}

fn sw_load(_id: u32, fw: &[u8]) -> Result<u64, &'static str> {
    Ok(fw.len() as u64)
}

fn sw_get_rsc_table(_id: u32) -> Result<Vec<u8>, &'static str> {
    Ok(Vec::new())
}

/// Software remoteproc ops.
pub fn software_rproc_ops() -> RprocOps {
    RprocOps {
        prepare: sw_prepare,
        unprepare: sw_unprepare,
        start: sw_start,
        stop: sw_stop,
        attach: sw_attach,
        detach: sw_detach,
        kick: sw_kick,
        load: sw_load,
        get_loaded_rsc_table: sw_get_rsc_table,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    if !RPROCS.read().is_empty() {
        return Ok(());
    }

    let ops = software_rproc_ops();
    let mem_table = alloc::vec![RprocMem {
        name: String::from("sw-rproc-mem0"),
        da: 0,
        pa: 0,
        size: 0x10000,
    }];
    let id = register_rproc("sw-rproc", "sw-firmware.elf", ops, mem_table, 2)?;
    crate::serial_println!(
        "remoteproc: software rproc registered (id={}, 2 vrings)",
        id
    );
    Ok(())
}
