//! NVMe (Non-Volatile Memory Express) host driver subsystem
//!
//! Provides NVMe controller discovery, admin queue, I/O queues, and namespace management.
//! Mirrors Linux's `drivers/nvme/host/core.c`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// NVMe controller (Linux `struct nvme_ctrl`).
pub struct NvmeCtrl {
    pub id: u32,
    pub name: String,
    pub ops: NvmeOps,
    pub state: NvmeCtrlState,
    pub instance: u32,
    pub queue_count: u32,
    pub admin_q_depth: u32,
    pub io_q_depth: u32,
    pub max_transfer_size: u32,
    pub namespaces: Vec<u32>,
    pub serial: String,
    pub model: String,
    pub firmware: String,
    pub cntrltype: u8,
}

/// NVMe controller state (Linux `enum nvme_ctrl_state`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NvmeCtrlState {
    New,
    Connecting,
    Live,
    Resetting,
    Disconnecting,
    Dead,
}

/// NVMe namespace (Linux `struct nvme_ns`).
pub struct NvmeNs {
    pub id: u32,
    pub ctrl_id: u32,
    pub ns_id: u32,
    pub lba_size: u32,
    pub ns_size: u64,
    pub capacity: u64,
    pub active: bool,
}

/// NVMe operations (Linux `struct nvme_ctrl_ops`).
pub struct NvmeOps {
    pub init: fn(ctrl_id: u32) -> Result<(), &'static str>,
    pub shutdown: fn(ctrl_id: u32) -> Result<(), &'static str>,
    pub submit_admin_cmd: fn(ctrl_id: u32, cmd: &NvmeCmd) -> Result<NvmeCompletion, &'static str>,
    pub submit_io_cmd:
        fn(ctrl_id: u32, qid: u32, cmd: &NvmeCmd) -> Result<NvmeCompletion, &'static str>,
    pub create_io_queue: fn(ctrl_id: u32, qid: u32, depth: u32) -> Result<(), &'static str>,
    pub delete_io_queue: fn(ctrl_id: u32, qid: u32) -> Result<(), &'static str>,
}

/// NVMe command (Linux `struct nvme_command`).
#[derive(Debug, Clone)]
pub struct NvmeCmd {
    pub opcode: u8,
    pub flags: u8,
    pub ns_id: u32,
    pub cdw2_3: u64,
    pub metadata: u64,
    pub prp1: u64,
    pub prp2: u64,
    pub cdw10_15: [u32; 6],
}

/// NVMe completion (Linux `struct nvme_completion`).
#[derive(Debug, Clone)]
pub struct NvmeCompletion {
    pub result: u64,
    pub sq_id: u16,
    pub cmd_id: u16,
    pub status: u16,
}

// ── Registry ────────────────────────────────────────────────────────────

static CTRL_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static NS_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static NVME_CTRLS: RwLock<BTreeMap<u32, NvmeCtrl>> = RwLock::new(BTreeMap::new());
static NVME_NAMESPACES: RwLock<BTreeMap<u32, NvmeNs>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register an NVMe controller.
pub fn register_ctrl(name: &str, ops: NvmeOps) -> Result<u32, &'static str> {
    let id = CTRL_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let ctrl = NvmeCtrl {
        id,
        name: String::from(name),
        ops,
        state: NvmeCtrlState::New,
        instance: id,
        queue_count: 1,
        admin_q_depth: 32,
        io_q_depth: 256,
        max_transfer_size: 128 * 1024,
        namespaces: Vec::new(),
        serial: String::from("SW_NVME001"),
        model: String::from("sw-nvme-256g"),
        firmware: String::from("1.0.0"),
        cntrltype: 0,
    };
    NVME_CTRLS.write().insert(id, ctrl);
    Ok(id)
}

/// Initialize an NVMe controller (Linux `nvme_init_ctrl`).
pub fn init_ctrl(ctrl_id: u32) -> Result<(), &'static str> {
    let init_fn = {
        let ctrls = NVME_CTRLS.read();
        let ctrl = ctrls.get(&ctrl_id).ok_or("NVMe controller not found")?;
        ctrl.ops.init
    };

    {
        let mut ctrls = NVME_CTRLS.write();
        let ctrl = ctrls.get_mut(&ctrl_id).ok_or("NVMe controller not found")?;
        ctrl.state = NvmeCtrlState::Connecting;
    }

    (init_fn)(ctrl_id)?;

    // Create an I/O queue
    let create_q_fn = {
        let ctrls = NVME_CTRLS.read();
        let ctrl = ctrls.get(&ctrl_id).ok_or("NVMe controller not found")?;
        ctrl.ops.create_io_queue
    };
    (create_q_fn)(ctrl_id, 1, 256)?;

    let mut ctrls = NVME_CTRLS.write();
    if let Some(ctrl) = ctrls.get_mut(&ctrl_id) {
        ctrl.state = NvmeCtrlState::Live;
        ctrl.queue_count = 2;

        // Create a namespace
        let ns_id = NS_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
        let ns = NvmeNs {
            id: ns_id,
            ctrl_id,
            ns_id: 1,
            lba_size: 512,
            ns_size: 256 * 1024 * 1024 * 1024 / 512,
            capacity: 256 * 1024 * 1024 * 1024,
            active: true,
        };
        NVME_NAMESPACES.write().insert(ns_id, ns);
        ctrl.namespaces.push(ns_id);
    }
    Ok(())
}

/// Shutdown an NVMe controller.
pub fn shutdown_ctrl(ctrl_id: u32) -> Result<(), &'static str> {
    let shutdown_fn = {
        let ctrls = NVME_CTRLS.read();
        let ctrl = ctrls.get(&ctrl_id).ok_or("NVMe controller not found")?;
        ctrl.ops.shutdown
    };

    {
        let mut ctrls = NVME_CTRLS.write();
        if let Some(ctrl) = ctrls.get_mut(&ctrl_id) {
            ctrl.state = NvmeCtrlState::Disconnecting;
        }
    }

    (shutdown_fn)(ctrl_id)?;

    let mut ctrls = NVME_CTRLS.write();
    if let Some(ctrl) = ctrls.get_mut(&ctrl_id) {
        ctrl.state = NvmeCtrlState::Dead;
    }
    Ok(())
}

/// Submit an admin command.
pub fn submit_admin_cmd(ctrl_id: u32, cmd: &NvmeCmd) -> Result<NvmeCompletion, &'static str> {
    let submit_fn = {
        let ctrls = NVME_CTRLS.read();
        let ctrl = ctrls.get(&ctrl_id).ok_or("NVMe controller not found")?;
        if ctrl.state != NvmeCtrlState::Live {
            return Err("NVMe controller not live");
        }
        ctrl.ops.submit_admin_cmd
    };
    (submit_fn)(ctrl_id, cmd)
}

/// Submit an I/O command.
pub fn submit_io_cmd(
    ctrl_id: u32,
    qid: u32,
    cmd: &NvmeCmd,
) -> Result<NvmeCompletion, &'static str> {
    let submit_fn = {
        let ctrls = NVME_CTRLS.read();
        let ctrl = ctrls.get(&ctrl_id).ok_or("NVMe controller not found")?;
        if ctrl.state != NvmeCtrlState::Live {
            return Err("NVMe controller not live");
        }
        ctrl.ops.submit_io_cmd
    };
    (submit_fn)(ctrl_id, qid, cmd)
}

/// Read from an NVMe namespace.
pub fn read_ns(ns_id: u32, lba: u64, buf: &mut [u8]) -> Result<usize, &'static str> {
    let (ctrl_id, lba_size) = {
        let namespaces = NVME_NAMESPACES.read();
        let ns = namespaces.get(&ns_id).ok_or("NVMe namespace not found")?;
        if !ns.active {
            return Err("NVMe namespace not active");
        }
        (ns.ctrl_id, ns.lba_size)
    };

    let mut cdw10_15 = [0u32; 6];
    cdw10_15[0] = (lba & 0xFFFFFFFF) as u32;
    cdw10_15[1] = (lba >> 32) as u32;
    let nlb = (buf.len() / lba_size as usize) as u32;
    cdw10_15[2] = nlb - 1;

    let cmd = NvmeCmd {
        opcode: 0x02, // NVMe Cmd Read
        flags: 0,
        ns_id: 1,
        cdw2_3: 0,
        metadata: 0,
        prp1: 0,
        prp2: 0,
        cdw10_15,
    };

    let completion = submit_io_cmd(ctrl_id, 1, &cmd)?;
    if completion.status != 0 {
        return Err("NVMe read failed");
    }
    Ok(buf.len())
}

/// Write to an NVMe namespace.
pub fn write_ns(ns_id: u32, lba: u64, data: &[u8]) -> Result<usize, &'static str> {
    let (ctrl_id, lba_size) = {
        let namespaces = NVME_NAMESPACES.read();
        let ns = namespaces.get(&ns_id).ok_or("NVMe namespace not found")?;
        if !ns.active {
            return Err("NVMe namespace not active");
        }
        (ns.ctrl_id, ns.lba_size)
    };

    let mut cdw10_15 = [0u32; 6];
    cdw10_15[0] = (lba & 0xFFFFFFFF) as u32;
    cdw10_15[1] = (lba >> 32) as u32;
    let nlb = (data.len() / lba_size as usize) as u32;
    cdw10_15[2] = nlb - 1;

    let cmd = NvmeCmd {
        opcode: 0x01, // NVMe Cmd Write
        flags: 0,
        ns_id: 1,
        cdw2_3: 0,
        metadata: 0,
        prp1: 0,
        prp2: 0,
        cdw10_15,
    };

    let completion = submit_io_cmd(ctrl_id, 1, &cmd)?;
    if completion.status != 0 {
        return Err("NVMe write failed");
    }
    Ok(data.len())
}

/// List all NVMe controllers.
pub fn list_ctrls() -> Vec<(u32, String, NvmeCtrlState)> {
    NVME_CTRLS
        .read()
        .iter()
        .map(|(id, c)| (*id, c.name.clone(), c.state))
        .collect()
}

/// List namespaces for a controller.
pub fn list_namespaces(ctrl_id: u32) -> Result<Vec<(u32, u32, u64, u32)>, &'static str> {
    let ctrls = NVME_CTRLS.read();
    let ctrl = ctrls.get(&ctrl_id).ok_or("NVMe controller not found")?;
    let namespaces = NVME_NAMESPACES.read();
    let mut result = Vec::new();
    for &ns_id in &ctrl.namespaces {
        if let Some(ns) = namespaces.get(&ns_id) {
            result.push((ns_id, ns.ns_id, ns.capacity, ns.lba_size));
        }
    }
    Ok(result)
}

/// Count registered controllers.
pub fn ctrl_count() -> usize {
    NVME_CTRLS.read().len()
}

// ── Software NVMe ───────────────────────────────────────────────────────

fn sw_init(_ctrl_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_shutdown(_ctrl_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_admin_cmd(_ctrl_id: u32, _cmd: &NvmeCmd) -> Result<NvmeCompletion, &'static str> {
    Ok(NvmeCompletion {
        result: 0,
        sq_id: 0,
        cmd_id: 0,
        status: 0,
    })
}
fn sw_io_cmd(_ctrl_id: u32, _qid: u32, _cmd: &NvmeCmd) -> Result<NvmeCompletion, &'static str> {
    Ok(NvmeCompletion {
        result: 0,
        sq_id: 1,
        cmd_id: 0,
        status: 0,
    })
}
fn sw_create_q(_ctrl_id: u32, _qid: u32, _depth: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_delete_q(_ctrl_id: u32, _qid: u32) -> Result<(), &'static str> {
    Ok(())
}

/// Software NVMe ops.
pub fn software_nvme_ops() -> NvmeOps {
    NvmeOps {
        init: sw_init,
        shutdown: sw_shutdown,
        submit_admin_cmd: sw_admin_cmd,
        submit_io_cmd: sw_io_cmd,
        create_io_queue: sw_create_q,
        delete_io_queue: sw_delete_q,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    let ops = software_nvme_ops();
    let ctrl_id = register_ctrl("sw-nvme0", ops)?;
    init_ctrl(ctrl_id)?;
    Ok(())
}
