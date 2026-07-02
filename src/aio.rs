//! AIO — POSIX asynchronous I/O context
//!
//! Ported from Linux fs/aio.c.
//! Provides io_setup, io_submit, io_getevents, io_cancel, io_destroy.
//! Uses a simple completion model: submitted requests are processed synchronously
//! and completion events are queued for retrieval.

use crate::memory::user_space::UserSpaceMemory;
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::{Mutex, RwLock};

// ── AIO context handle (fake pointer — we encode the ID) ────────────────

pub type AioContext = u64;

// ── iocb structure (matches Linux struct iocb) ──────────────────────────

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct IoCb {
    pub aio_data: u64,
    pub aio_key: u32,
    pub aio_rw_flags: i32,
    pub aio_lio_opcode: u16,
    pub aio_reqprio: i16,
    pub aio_fildes: u32,
    pub aio_buf: u64,
    pub aio_nbytes: u64,
    pub aio_offset: i64,
    pub aio_reserved2: u64,
    pub aio_flags: u32,
    pub aio_resfd: u32,
}

// ── io_event structure (matches Linux struct io_event) ──────────────────

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct IoEvent {
    pub data: u64,
    pub obj: u64,
    pub res: i64,
    pub res2: i64,
}

// ── AIO opcodes ─────────────────────────────────────────────────────────

pub const IO_CMD_PREAD: u16 = 0;
pub const IO_CMD_PWRITE: u16 = 1;
pub const IO_CMD_FSYNC: u16 = 2;
pub const IO_CMD_FDSYNC: u16 = 3;
pub const IO_CMD_POLL: u16 = 5;
pub const IO_CMD_NOOP: u16 = 6;
pub const IO_CMD_PREADV: u16 = 7;
pub const IO_CMD_PWRITEV: u16 = 8;

// ── AIO context state ───────────────────────────────────────────────────

pub struct AioCtx {
    pub id: u32,
    pub max_events: u32,
    pub events: Vec<IoEvent>,
    pub pending: u32,
}

// ── Global state ────────────────────────────────────────────────────────

static AIO_CONTEXTS: RwLock<BTreeMap<u32, Mutex<AioCtx>>> = RwLock::new(BTreeMap::new());
static NEXT_CTX_ID: AtomicU32 = AtomicU32::new(1);

fn ctx_id_from_handle(ctx: AioContext) -> Option<u32> {
    if ctx == 0 {
        return None;
    }
    Some(ctx as u32)
}

fn handle_from_ctx_id(id: u32) -> AioContext {
    id as u64
}

// ── Syscall implementations ─────────────────────────────────────────────

/// io_setup — create an AIO context
pub fn io_setup(max_events: u32, ctx_idp: *mut AioContext) -> i32 {
    if max_events == 0 || max_events > 65536 {
        return -22;
    }
    if ctx_idp.is_null() {
        return -14;
    }

    let id = NEXT_CTX_ID.fetch_add(1, Ordering::SeqCst);
    let ctx = AioCtx {
        id,
        max_events,
        events: Vec::new(),
        pending: 0,
    };
    AIO_CONTEXTS.write().insert(id, Mutex::new(ctx));

    let handle = handle_from_ctx_id(id);
    // SAFETY: `handle` is a stack-local `Copy` struct; the pointer is valid
    // and the length is `size_of::<AioContext>()`.
    let handle_bytes = unsafe {
        core::slice::from_raw_parts(
            (&handle as *const AioContext).cast::<u8>(),
            core::mem::size_of::<AioContext>(),
        )
    };
    if UserSpaceMemory::copy_to_user(ctx_idp as u64, handle_bytes).is_err() {
        AIO_CONTEXTS.write().remove(&id);
        return -14;
    }
    0
}

/// io_destroy — destroy an AIO context
pub fn io_destroy(ctx: AioContext) -> i32 {
    let id = match ctx_id_from_handle(ctx) {
        Some(id) => id,
        None => return -22,
    };
    AIO_CONTEXTS.write().remove(&id);
    0
}

/// io_submit — submit AIO requests
pub fn io_submit(ctx: AioContext, nr: i64, iocbpp: *const *const IoCb) -> i32 {
    if nr <= 0 || nr > 65536 {
        return -22;
    }
    let id = match ctx_id_from_handle(ctx) {
        Some(id) => id,
        None => return -22,
    };
    if iocbpp.is_null() {
        return -14;
    }

    let contexts = AIO_CONTEXTS.read();
    let ctx_mutex = match contexts.get(&id) {
        Some(c) => c,
        None => return -22,
    };

    let mut submitted = 0i32;
    for i in 0..nr as usize {
        let mut ptr_bytes = [0u8; core::mem::size_of::<u64>()];
        let ptr_addr =
            (iocbpp as u64).saturating_add((i * core::mem::size_of::<*const IoCb>()) as u64);
        if UserSpaceMemory::copy_from_user(ptr_addr, &mut ptr_bytes).is_err() {
            return -14;
        }
        let iocb_ptr = u64::from_ne_bytes(ptr_bytes) as *const IoCb;
        if iocb_ptr.is_null() {
            continue;
        }
        let mut iocb = IoCb::default();
        // SAFETY: `iocb` is a stack-local `Default` value; the pointer is
        // valid for writes and the length is `size_of::<IoCb>()`.
        let iocb_bytes = unsafe {
            core::slice::from_raw_parts_mut(
                (&mut iocb as *mut IoCb).cast::<u8>(),
                core::mem::size_of::<IoCb>(),
            )
        };
        if UserSpaceMemory::copy_from_user(iocb_ptr as u64, iocb_bytes).is_err() {
            return -14;
        }

        // Process the request synchronously
        let res = process_iocb(&iocb);

        // Queue completion event
        let event = IoEvent {
            data: iocb.aio_data,
            obj: iocb_ptr as u64,
            res,
            res2: 0,
        };

        {
            let mut ctx = ctx_mutex.lock();
            if ctx.events.len() < ctx.max_events as usize {
                ctx.events.push(event);
            }
        }
        submitted += 1;
    }

    submitted
}

/// io_getevents — retrieve completion events
pub fn io_getevents(
    ctx: AioContext,
    min_nr: i64,
    nr: i64,
    events: *mut IoEvent,
    _timeout: *const u8,
) -> i32 {
    if nr <= 0 || events.is_null() {
        return -22;
    }
    let id = match ctx_id_from_handle(ctx) {
        Some(id) => id,
        None => return -22,
    };

    let contexts = AIO_CONTEXTS.read();
    let ctx_mutex = match contexts.get(&id) {
        Some(c) => c,
        None => return -22,
    };
    let mut ctx = ctx_mutex.lock();

    // Wait until we have min_nr events (for now, just return what's available)
    if (ctx.events.len() as i64) < min_nr {
        return -11; // EAGAIN
    }

    let count = core::cmp::min(nr as usize, ctx.events.len());
    for i in 0..count {
        let event = ctx.events.remove(0);
        // SAFETY: `event` is a stack-local `Copy` struct; the pointer is
        // valid and the length is `size_of::<IoEvent>()`.
        let event_bytes = unsafe {
            core::slice::from_raw_parts(
                (&event as *const IoEvent).cast::<u8>(),
                core::mem::size_of::<IoEvent>(),
            )
        };
        let event_addr =
            (events as u64).saturating_add((i * core::mem::size_of::<IoEvent>()) as u64);
        if UserSpaceMemory::copy_to_user(event_addr, event_bytes).is_err() {
            return -14;
        }
    }

    count as i32
}

/// io_cancel — cancel an AIO request
pub fn io_cancel(ctx: AioContext, iocb: *const IoCb, result: *mut IoEvent) -> i32 {
    let id = match ctx_id_from_handle(ctx) {
        Some(id) => id,
        None => return -22,
    };
    if iocb.is_null() {
        return -14;
    }

    // Search for the event matching this iocb
    let contexts = AIO_CONTEXTS.read();
    if let Some(ctx_mutex) = contexts.get(&id) {
        let mut ctx = ctx_mutex.lock();
        let target = iocb as u64;
        if let Some(pos) = ctx.events.iter().position(|e| e.obj == target) {
            let event = ctx.events.remove(pos);
            if !result.is_null() {
                // SAFETY: `event` is a stack-local `Copy` struct; the pointer
                // is valid and the length is `size_of::<IoEvent>()`.
                let event_bytes = unsafe {
                    core::slice::from_raw_parts(
                        (&event as *const IoEvent).cast::<u8>(),
                        core::mem::size_of::<IoEvent>(),
                    )
                };
                if UserSpaceMemory::copy_to_user(result as u64, event_bytes).is_err() {
                    return -14;
                }
            }
            return 0;
        }
    }
    -2 // ENOENT — not found (already completed or not submitted)
}

// ── Internal ────────────────────────────────────────────────────────────

/// Maximum per-operation transfer size accepted from user space. This caps the
/// user-controlled `aio_nbytes` value to prevent unbounded kernel allocations.
const MAX_AIO_TRANSFER: usize = 65536;

fn process_iocb(iocb: &IoCb) -> i64 {
    let vfs = crate::vfs::get_vfs();
    match iocb.aio_lio_opcode {
        IO_CMD_PREAD => {
            if iocb.aio_buf == 0 || iocb.aio_nbytes == 0 {
                return -22;
            }
            // Reject user-controlled sizes that would trigger an unbounded
            // kernel allocation (OOM), capped at MAX_AIO_TRANSFER.
            if iocb.aio_nbytes as usize > MAX_AIO_TRANSFER {
                return -22;
            }
            let len = iocb.aio_nbytes as usize;
            let mut buf = Vec::new();
            buf.resize(len, 0);
            match vfs.pread(iocb.aio_fildes as i32, &mut buf, iocb.aio_offset as u64) {
                Ok(n) => {
                    if UserSpaceMemory::copy_to_user(iocb.aio_buf, &buf[..n]).is_err() {
                        -14
                    } else {
                        n as i64
                    }
                }
                Err(_) => -5, // EIO
            }
        }
        IO_CMD_PWRITE => {
            if iocb.aio_buf == 0 || iocb.aio_nbytes == 0 {
                return -22;
            }
            // Reject user-controlled sizes that would trigger an unbounded
            // kernel allocation (OOM), capped at MAX_AIO_TRANSFER.
            if iocb.aio_nbytes as usize > MAX_AIO_TRANSFER {
                return -22;
            }
            let len = iocb.aio_nbytes as usize;
            let mut buf = Vec::new();
            buf.resize(len, 0);
            if UserSpaceMemory::copy_from_user(iocb.aio_buf, &mut buf).is_err() {
                return -14;
            }
            match vfs.pwrite(iocb.aio_fildes as i32, &buf, iocb.aio_offset as u64) {
                Ok(n) => n as i64,
                Err(_) => -5,
            }
        }
        IO_CMD_FSYNC | IO_CMD_FDSYNC => match vfs.sync_all() {
            Ok(()) => 0,
            Err(_) => -5,
        },
        IO_CMD_NOOP => 0,
        IO_CMD_POLL => 0, // Simplified — always ready
        _ => -22,         // EINVAL — unknown opcode
    }
}

/// Initialize the AIO subsystem.
pub fn init() {
    crate::serial_println!("[aio] AIO subsystem initialized");
}
