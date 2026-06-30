//! Android driver subsystem
//!
//! Mirrors Linux's `drivers/android/` — the Binder IPC driver plus ashmem
//! (anonymous shared memory). Binder provides process-to-process transaction
//! delivery over reference-counted nodes; ashmem provides named, pinnable
//! shared memory regions backed by in-kernel buffers.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Binder types ──────────────────────────────────────────────────────────

/// A binder transaction handed from one process to a node in another.
#[derive(Debug, Clone)]
pub struct BinderTransaction {
    pub id: u32,
    pub from_proc: u32,
    pub to_node: u32,
    pub code: u32,
    pub oneway: bool,
    pub data: Vec<u8>,
}

/// A binder node: an addressable object owned by a process.
#[derive(Debug, Clone)]
pub struct BinderNode {
    pub id: u32,
    pub owner: u32,
    pub ptr: u64,
    pub descriptor: String,
    pub strong_refs: u32,
}

/// A binder process context: its owned nodes and pending transaction inbox.
#[derive(Debug, Clone)]
pub struct BinderProcess {
    pub id: u32,
    pub name: String,
    pub nodes: Vec<u32>,
    pub inbox: Vec<BinderTransaction>,
}

// ── Ashmem types ────────────────────────────────────────────────────────────

/// A named anonymous shared-memory region.
#[derive(Debug, Clone)]
pub struct AshmemRegion {
    pub id: u32,
    pub name: String,
    pub size: usize,
    pub data: Vec<u8>,
    pub pinned: bool,
}

// ── Registries ──────────────────────────────────────────────────────────────

static BINDER_PROCS: RwLock<BTreeMap<u32, BinderProcess>> = RwLock::new(BTreeMap::new());
static BINDER_NODES: RwLock<BTreeMap<u32, BinderNode>> = RwLock::new(BTreeMap::new());
static ASHMEM_REGIONS: RwLock<BTreeMap<u32, AshmemRegion>> = RwLock::new(BTreeMap::new());

static NEXT_PROC_ID: AtomicU32 = AtomicU32::new(0);
static NEXT_NODE_ID: AtomicU32 = AtomicU32::new(0);
static NEXT_TXN_ID: AtomicU32 = AtomicU32::new(1);
static NEXT_ASHMEM_ID: AtomicU32 = AtomicU32::new(0);

// ── Binder API ────────────────────────────────────────────────────────────

/// Open a new binder process context and return its id.
pub fn open_process(name: &str) -> u32 {
    let id = NEXT_PROC_ID.fetch_add(1, Ordering::SeqCst);
    BINDER_PROCS.write().insert(
        id,
        BinderProcess {
            id,
            name: String::from(name),
            nodes: Vec::new(),
            inbox: Vec::new(),
        },
    );
    id
}

/// Close a binder process, removing it and all nodes it owns.
pub fn close_process(proc_id: u32) -> Result<(), &'static str> {
    let mut procs = BINDER_PROCS.write();
    let process = procs.remove(&proc_id).ok_or("binder: process not found")?;
    let mut nodes = BINDER_NODES.write();
    for node_id in &process.nodes {
        nodes.remove(node_id);
    }
    Ok(())
}

/// Create a binder node owned by `proc_id` and return its node id.
pub fn add_node(proc_id: u32, descriptor: &str) -> Result<u32, &'static str> {
    let mut procs = BINDER_PROCS.write();
    let process = procs.get_mut(&proc_id).ok_or("binder: process not found")?;

    let id = NEXT_NODE_ID.fetch_add(1, Ordering::SeqCst);
    let ptr = 0xB1_0000_0000u64 | (id as u64);
    BINDER_NODES.write().insert(
        id,
        BinderNode {
            id,
            owner: proc_id,
            ptr,
            descriptor: String::from(descriptor),
            strong_refs: 1,
        },
    );
    process.nodes.push(id);
    Ok(id)
}

/// Look up the owning process of a node.
fn node_owner(node_id: u32) -> Result<u32, &'static str> {
    BINDER_NODES
        .read()
        .get(&node_id)
        .map(|n| n.owner)
        .ok_or("binder: node not found")
}

/// Send a transaction from `from_proc` to the process owning `to_node`.
///
/// The transaction is queued into the target owner's inbox. Non-oneway
/// transactions conceptually expect a reply via [`reply`]; the transaction id
/// is returned so the caller can correlate that reply.
pub fn transact(
    from_proc: u32,
    to_node: u32,
    code: u32,
    oneway: bool,
    data: Vec<u8>,
) -> Result<u32, &'static str> {
    if !BINDER_PROCS.read().contains_key(&from_proc) {
        return Err("binder: sender process not found");
    }
    let owner = node_owner(to_node)?;

    let id = NEXT_TXN_ID.fetch_add(1, Ordering::SeqCst);
    let txn = BinderTransaction {
        id,
        from_proc,
        to_node,
        code,
        oneway,
        data,
    };

    let mut procs = BINDER_PROCS.write();
    let target = procs
        .get_mut(&owner)
        .ok_or("binder: target process not found")?;
    target.inbox.push(txn);
    Ok(id)
}

/// Deliver a reply for a non-oneway transaction back to the originator.
///
/// The reply is modeled as a transaction queued into the original sender's
/// inbox, carrying the same id and a reply marker code.
pub fn reply(transaction_id: u32, from_proc: u32, data: Vec<u8>) -> Result<(), &'static str> {
    // Find the original sender by scanning live inboxes for the txn id.
    let mut procs = BINDER_PROCS.write();
    let mut originator: Option<u32> = None;
    for process in procs.values() {
        if process.inbox.iter().any(|t| t.id == transaction_id) {
            // The transaction still sitting in an inbox means it has not been
            // polled yet; its sender is recorded on the transaction itself.
            originator = process
                .inbox
                .iter()
                .find(|t| t.id == transaction_id)
                .map(|t| t.from_proc);
            break;
        }
    }
    let dest = originator.ok_or("binder: transaction not found for reply")?;
    if !procs.contains_key(&from_proc) {
        return Err("binder: replying process not found");
    }

    let reply_txn = BinderTransaction {
        id: transaction_id,
        from_proc,
        to_node: 0,
        code: BINDER_REPLY_CODE,
        oneway: false,
        data,
    };
    let target = procs
        .get_mut(&dest)
        .ok_or("binder: reply destination not found")?;
    target.inbox.push(reply_txn);
    Ok(())
}

/// Reserved transaction code used to mark replies.
pub const BINDER_REPLY_CODE: u32 = 0xFFFF_FFFF;

/// Drain and return all pending transactions for a process.
pub fn poll(proc_id: u32) -> Vec<BinderTransaction> {
    let mut procs = BINDER_PROCS.write();
    match procs.get_mut(&proc_id) {
        Some(process) => core::mem::take(&mut process.inbox),
        None => Vec::new(),
    }
}

/// Number of transactions currently waiting in a process inbox.
pub fn inbox_len(proc_id: u32) -> usize {
    BINDER_PROCS
        .read()
        .get(&proc_id)
        .map(|p| p.inbox.len())
        .unwrap_or(0)
}

/// Increment the strong reference count of a node.
pub fn ref_node(node_id: u32) -> Result<u32, &'static str> {
    let mut nodes = BINDER_NODES.write();
    let node = nodes.get_mut(&node_id).ok_or("binder: node not found")?;
    node.strong_refs = node.strong_refs.saturating_add(1);
    Ok(node.strong_refs)
}

/// Decrement a node's strong reference count, removing it at zero.
pub fn unref_node(node_id: u32) -> Result<u32, &'static str> {
    let mut nodes = BINDER_NODES.write();
    let (owner, remaining) = {
        let node = nodes.get_mut(&node_id).ok_or("binder: node not found")?;
        node.strong_refs = node.strong_refs.saturating_sub(1);
        (node.owner, node.strong_refs)
    };

    if remaining == 0 {
        nodes.remove(&node_id);
        drop(nodes);
        if let Some(process) = BINDER_PROCS.write().get_mut(&owner) {
            process.nodes.retain(|&n| n != node_id);
        }
    }
    Ok(remaining)
}

/// Look up a node's descriptor by id.
pub fn node_descriptor(node_id: u32) -> Option<String> {
    BINDER_NODES
        .read()
        .get(&node_id)
        .map(|n| n.descriptor.clone())
}

pub fn process_count() -> usize {
    BINDER_PROCS.read().len()
}

pub fn node_count() -> usize {
    BINDER_NODES.read().len()
}

// ── Ashmem API ──────────────────────────────────────────────────────────────

/// Create a zero-filled named shared-memory region and return its id.
pub fn ashmem_create(name: &str, size: usize) -> u32 {
    let id = NEXT_ASHMEM_ID.fetch_add(1, Ordering::SeqCst);
    ASHMEM_REGIONS.write().insert(
        id,
        AshmemRegion {
            id,
            name: String::from(name),
            size,
            data: vec![0u8; size],
            pinned: true,
        },
    );
    id
}

/// Read `len` bytes at `offset` from an ashmem region.
pub fn ashmem_read(id: u32, offset: usize, len: usize) -> Result<Vec<u8>, &'static str> {
    let regions = ASHMEM_REGIONS.read();
    let region = regions.get(&id).ok_or("ashmem: region not found")?;
    let end = offset.checked_add(len).ok_or("ashmem: range overflow")?;
    if end > region.size {
        return Err("ashmem: read out of bounds");
    }
    Ok(region.data[offset..end].to_vec())
}

/// Write `bytes` at `offset` into an ashmem region.
pub fn ashmem_write(id: u32, offset: usize, bytes: &[u8]) -> Result<(), &'static str> {
    let mut regions = ASHMEM_REGIONS.write();
    let region = regions.get_mut(&id).ok_or("ashmem: region not found")?;
    let end = offset
        .checked_add(bytes.len())
        .ok_or("ashmem: range overflow")?;
    if end > region.size {
        return Err("ashmem: write out of bounds");
    }
    if region.data.len() < region.size {
        region.data.resize(region.size, 0);
    }
    region.data[offset..end].copy_from_slice(bytes);
    Ok(())
}

/// Pin a region, protecting it from purge.
pub fn ashmem_pin(id: u32) -> Result<(), &'static str> {
    let mut regions = ASHMEM_REGIONS.write();
    let region = regions.get_mut(&id).ok_or("ashmem: region not found")?;
    region.pinned = true;
    Ok(())
}

/// Unpin a region, allowing it to be purged under memory pressure.
pub fn ashmem_unpin(id: u32) -> Result<(), &'static str> {
    let mut regions = ASHMEM_REGIONS.write();
    let region = regions.get_mut(&id).ok_or("ashmem: region not found")?;
    region.pinned = false;
    Ok(())
}

/// Purge (free the backing pages of) an unpinned region. Pinned regions are
/// rejected. The region descriptor remains but its data is dropped to zero len.
pub fn ashmem_purge(id: u32) -> Result<(), &'static str> {
    let mut regions = ASHMEM_REGIONS.write();
    let region = regions.get_mut(&id).ok_or("ashmem: region not found")?;
    if region.pinned {
        return Err("ashmem: cannot purge pinned region");
    }
    region.data = Vec::new();
    Ok(())
}

/// Whether a region currently has its backing buffer resident.
pub fn ashmem_is_resident(id: u32) -> bool {
    ASHMEM_REGIONS
        .read()
        .get(&id)
        .map(|r| !r.data.is_empty() || r.size == 0)
        .unwrap_or(false)
}

pub fn ashmem_count() -> usize {
    ASHMEM_REGIONS.read().len()
}

// ── Init ────────────────────────────────────────────────────────────────────

/// Initialize the Android subsystem: bring up the binder context manager and a
/// sample ashmem region. Idempotent.
pub fn init() -> Result<(), &'static str> {
    if !BINDER_PROCS.read().is_empty() {
        return Ok(());
    }

    // The servicemanager is the binder context manager, owning node 0.
    let sm = open_process("servicemanager");
    let ctx_node = add_node(sm, "context-manager")?;
    debug_assert_eq!(ctx_node, 0, "context manager must be node 0");

    // A second process so transaction routing has somewhere to go.
    let app = open_process("system_server");
    let svc = add_node(app, "activity")?;

    // Sample one-way transaction: app announces a service to servicemanager.
    let _ = transact(app, ctx_node, 1, true, b"addService:activity".to_vec())?;
    let _ = ref_node(svc)?;

    // Sample ashmem region.
    let _region = ashmem_create("android-sample", 4096);

    crate::serial_println!(
        "android: binder ready ({} procs), ashmem ready",
        process_count()
    );
    Ok(())
}
