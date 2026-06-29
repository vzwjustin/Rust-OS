//! SLIMpro processor subsystem
//!
//! Provides SLIMpro coprocessor mailbox and IPC interface.
//! Mirrors Linux's `drivers/firmware/bcm47xx/slimproc.c`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// SLIMpro processor (Linux `struct slimproc`).
pub struct SlimProc {
    pub id: u32,
    pub name: String,
    pub ops: SlimProcOps,
    pub state: SlimProcState,
    pub firmware_loaded: bool,
    pub mailbox_ids: Vec<u32>,
}

/// SLIMpro processor state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlimProcState {
    Offline,
    Booting,
    Running,
    Stopped,
    Error,
}

/// SLIMpro operations.
pub struct SlimProcOps {
    pub boot: fn(proc_id: u32) -> Result<(), &'static str>,
    pub shutdown: fn(proc_id: u32) -> Result<(), &'static str>,
    pub load_firmware: fn(proc_id: u32, firmware: &[u8]) -> Result<(), &'static str>,
    pub send_msg: fn(proc_id: u32, msg: &SlimProcMsg) -> Result<(), &'static str>,
    pub recv_msg: fn(proc_id: u32) -> Result<SlimProcMsg, &'static str>,
    pub get_status: fn(proc_id: u32) -> Result<SlimProcStatus, &'static str>,
}

/// SLIMpro message.
#[derive(Debug, Clone)]
pub struct SlimProcMsg {
    pub cmd: u32,
    pub arg0: u32,
    pub arg1: u32,
    pub arg2: u32,
    pub data: Vec<u8>,
}

/// SLIMpro status.
#[derive(Debug, Clone)]
pub struct SlimProcStatus {
    pub state: SlimProcState,
    pub pc: u64,
    pub running: bool,
    pub last_error: u32,
}

// ── Registry ────────────────────────────────────────────────────────────

static PROC_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static SLIMPROCS: RwLock<BTreeMap<u32, SlimProc>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register a SLIMpro coprocessor.
pub fn register_processor(name: &str, ops: SlimProcOps) -> Result<u32, &'static str> {
    let id = PROC_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let proc = SlimProc {
        id,
        name: String::from(name),
        ops,
        state: SlimProcState::Offline,
        firmware_loaded: false,
        mailbox_ids: Vec::new(),
    };
    SLIMPROCS.write().insert(id, proc);
    Ok(id)
}

/// Boot a SLIMpro coprocessor (Linux `slimproc_boot`).
pub fn boot_processor(proc_id: u32) -> Result<(), &'static str> {
    let boot_fn = {
        let procs = SLIMPROCS.read();
        let proc = procs.get(&proc_id).ok_or("SLIMpro processor not found")?;
        proc.ops.boot
    };

    {
        let mut procs = SLIMPROCS.write();
        if let Some(proc) = procs.get_mut(&proc_id) {
            proc.state = SlimProcState::Booting;
        }
    }

    (boot_fn)(proc_id)?;

    let mut procs = SLIMPROCS.write();
    if let Some(proc) = procs.get_mut(&proc_id) {
        proc.state = SlimProcState::Running;
    }
    Ok(())
}

/// Shutdown a SLIMpro coprocessor.
pub fn shutdown_processor(proc_id: u32) -> Result<(), &'static str> {
    let shutdown_fn = {
        let procs = SLIMPROCS.read();
        let proc = procs.get(&proc_id).ok_or("SLIMpro processor not found")?;
        proc.ops.shutdown
    };
    (shutdown_fn)(proc_id)?;

    let mut procs = SLIMPROCS.write();
    if let Some(proc) = procs.get_mut(&proc_id) {
        proc.state = SlimProcState::Stopped;
    }
    Ok(())
}

/// Load firmware onto a SLIMpro coprocessor.
pub fn load_firmware(proc_id: u32, firmware: &[u8]) -> Result<(), &'static str> {
    let load_fn = {
        let procs = SLIMPROCS.read();
        let proc = procs.get(&proc_id).ok_or("SLIMpro processor not found")?;
        proc.ops.load_firmware
    };
    (load_fn)(proc_id, firmware)?;

    let mut procs = SLIMPROCS.write();
    if let Some(proc) = procs.get_mut(&proc_id) {
        proc.firmware_loaded = true;
    }
    Ok(())
}

/// Send a message to a SLIMpro coprocessor.
pub fn send_msg(proc_id: u32, msg: &SlimProcMsg) -> Result<(), &'static str> {
    let send_fn = {
        let procs = SLIMPROCS.read();
        let proc = procs.get(&proc_id).ok_or("SLIMpro processor not found")?;
        if proc.state != SlimProcState::Running {
            return Err("SLIMpro processor not running");
        }
        proc.ops.send_msg
    };
    (send_fn)(proc_id, msg)
}

/// Receive a message from a SLIMpro coprocessor.
pub fn recv_msg(proc_id: u32) -> Result<SlimProcMsg, &'static str> {
    let recv_fn = {
        let procs = SLIMPROCS.read();
        let proc = procs.get(&proc_id).ok_or("SLIMpro processor not found")?;
        if proc.state != SlimProcState::Running {
            return Err("SLIMpro processor not running");
        }
        proc.ops.recv_msg
    };
    (recv_fn)(proc_id)
}

/// Get processor status.
pub fn get_status(proc_id: u32) -> Result<SlimProcStatus, &'static str> {
    let status_fn = {
        let procs = SLIMPROCS.read();
        let proc = procs.get(&proc_id).ok_or("SLIMpro processor not found")?;
        proc.ops.get_status
    };
    (status_fn)(proc_id)
}

/// Associate a mailbox with a SLIMpro processor.
pub fn associate_mailbox(proc_id: u32, mailbox_id: u32) -> Result<(), &'static str> {
    let mut procs = SLIMPROCS.write();
    let proc = procs
        .get_mut(&proc_id)
        .ok_or("SLIMpro processor not found")?;
    proc.mailbox_ids.push(mailbox_id);
    Ok(())
}

/// List all SLIMpro processors.
pub fn list_processors() -> Vec<(u32, String, SlimProcState)> {
    SLIMPROCS
        .read()
        .iter()
        .map(|(id, p)| (*id, p.name.clone(), p.state))
        .collect()
}

/// Count registered processors.
pub fn processor_count() -> usize {
    SLIMPROCS.read().len()
}

// ── Software SLIMpro ────────────────────────────────────────────────────

fn sw_boot(_proc_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_shutdown(_proc_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_load_firmware(_proc_id: u32, _firmware: &[u8]) -> Result<(), &'static str> {
    Ok(())
}
fn sw_send_msg(_proc_id: u32, _msg: &SlimProcMsg) -> Result<(), &'static str> {
    Ok(())
}
fn sw_recv_msg(_proc_id: u32) -> Result<SlimProcMsg, &'static str> {
    Ok(SlimProcMsg {
        cmd: 0,
        arg0: 0,
        arg1: 0,
        arg2: 0,
        data: Vec::new(),
    })
}
fn sw_get_status(proc_id: u32) -> Result<SlimProcStatus, &'static str> {
    let procs = SLIMPROCS.read();
    let proc = procs.get(&proc_id).ok_or("SLIMpro processor not found")?;
    Ok(SlimProcStatus {
        state: proc.state,
        pc: 0,
        running: proc.state == SlimProcState::Running,
        last_error: 0,
    })
}

/// Software SLIMpro ops.
pub fn software_slimproc_ops() -> SlimProcOps {
    SlimProcOps {
        boot: sw_boot,
        shutdown: sw_shutdown,
        load_firmware: sw_load_firmware,
        send_msg: sw_send_msg,
        recv_msg: sw_recv_msg,
        get_status: sw_get_status,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    let ops = software_slimproc_ops();
    let proc_id = register_processor("sw-slimproc-0", ops)?;

    // Load firmware and boot
    let fw = [0u8; 256];
    load_firmware(proc_id, &fw)?;
    boot_processor(proc_id)?;

    Ok(())
}
