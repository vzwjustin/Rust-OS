//! Fusion MPT message-passing subsystem (mirrors Linux `drivers/message/`)
//!
//! Models LSI Fusion-MPT host adapters: a request/reply message frame queue
//! used to issue SCSI I/O to attached targets. Provides adapter registration,
//! target enumeration, and SCSI command submission with a reply post path.

use alloc::collections::BTreeMap;
use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MptReplyStatus {
    Success,
    DeviceNotThere,
    CheckCondition,
}

#[derive(Clone)]
pub struct MptReply {
    pub context: u32,
    pub status: MptReplyStatus,
    pub data: Vec<u8>,
}

#[derive(Clone)]
struct MptTarget {
    bus: u8,
    target_id: u8,
    lun: u8,
}

struct MptAdapter {
    id: u32,
    name: String,
    /// MPT product (e.g. 0x0030 = SAS1068).
    product_id: u16,
    targets: Vec<MptTarget>,
    reply_queue: VecDeque<MptReply>,
    requests: u64,
}

// ── Registry ──────────────────────────────────────────────────────────────

static ADAPTERS: RwLock<BTreeMap<u32, MptAdapter>> = RwLock::new(BTreeMap::new());
static NEXT_ID: AtomicU32 = AtomicU32::new(0);
static NEXT_CONTEXT: AtomicU32 = AtomicU32::new(1);

// ── Public API ──────────────────────────────────────────────────────────

pub fn register_adapter(name: &str, product_id: u16) -> u32 {
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    ADAPTERS.write().insert(
        id,
        MptAdapter {
            id,
            name: String::from(name),
            product_id,
            targets: Vec::new(),
            reply_queue: VecDeque::new(),
            requests: 0,
        },
    );
    id
}

pub fn add_target(adapter_id: u32, bus: u8, target_id: u8, lun: u8) -> Result<(), &'static str> {
    let mut adapters = ADAPTERS.write();
    let a = adapters
        .get_mut(&adapter_id)
        .ok_or("mpt: adapter not found")?;
    a.targets.push(MptTarget {
        bus,
        target_id,
        lun,
    });
    Ok(())
}

/// Issue a SCSI command request frame; posts a reply onto the reply queue and
/// returns the message context that identifies it.
pub fn scsi_io_request(
    adapter_id: u32,
    bus: u8,
    target_id: u8,
    lun: u8,
    cdb: &[u8],
) -> Result<u32, &'static str> {
    let mut adapters = ADAPTERS.write();
    let a = adapters
        .get_mut(&adapter_id)
        .ok_or("mpt: adapter not found")?;
    let present = a
        .targets
        .iter()
        .any(|t| t.bus == bus && t.target_id == target_id && t.lun == lun);
    let context = NEXT_CONTEXT.fetch_add(1, Ordering::SeqCst);
    let status = if present {
        MptReplyStatus::Success
    } else {
        MptReplyStatus::DeviceNotThere
    };
    // Model a TEST UNIT READY / INQUIRY style empty good reply.
    let data = if present && cdb.first() == Some(&0x12) {
        // INQUIRY: minimal standard data with peripheral type 0 (disk).
        let mut v = alloc::vec![0u8; 36];
        v[2] = 0x05; // SPC-3
        v
    } else {
        Vec::new()
    };
    a.reply_queue.push_back(MptReply {
        context,
        status,
        data,
    });
    a.requests += 1;
    Ok(context)
}

/// Pop the next posted reply frame, if any.
pub fn poll_reply(adapter_id: u32) -> Option<MptReply> {
    ADAPTERS
        .write()
        .get_mut(&adapter_id)
        .and_then(|a| a.reply_queue.pop_front())
}

pub fn target_count(adapter_id: u32) -> usize {
    ADAPTERS
        .read()
        .get(&adapter_id)
        .map(|a| a.targets.len())
        .unwrap_or(0)
}

pub fn adapter_count() -> usize {
    ADAPTERS.read().len()
}

/// Initialize the Fusion-MPT layer with a software SAS adapter and one disk.
pub fn init() -> Result<(), &'static str> {
    if !ADAPTERS.read().is_empty() {
        return Ok(());
    }
    let a = register_adapter("mptsas0", 0x0030);
    add_target(a, 0, 0, 0)?;
    crate::serial_println!(
        "message: Fusion-MPT adapter mptsas0, {} target(s)",
        target_count(a)
    );
    Ok(())
}
