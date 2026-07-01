//! Restartable sequence registration.
//!
//! This provides the Linux `rseq(2)` registration contract for userspace that
//! probes rseq support. RustOS currently runs a single CPU ID domain for user
//! threads, so registered areas are kept updated with CPU/node/mm-cid zero.

extern crate alloc;

use alloc::collections::BTreeMap;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

use crate::linux_compat::{LinuxError, LinuxResult};
use crate::memory::user_space::UserSpaceMemory;
use crate::process;

const RSEQ_MIN_LEN: u32 = 32;
const RSEQ_ALIGN: u64 = 32;
const RSEQ_FLAG_UNREGISTER: u32 = 1 << 0;
const RSEQ_FLAG_SLICE_EXT_DEFAULT_ON: u32 = 1 << 1;
const VALID_FLAGS: u32 = RSEQ_FLAG_UNREGISTER | RSEQ_FLAG_SLICE_EXT_DEFAULT_ON;
const RSEQ_CPU_ID_UNINITIALIZED: u32 = u32::MAX;

const CPU_ID_START_OFF: u64 = 0;
const CPU_ID_OFF: u64 = 4;
const RSEQ_CS_OFF: u64 = 8;
const FLAGS_OFF: u64 = 16;
const NODE_ID_OFF: u64 = 20;
const MM_CID_OFF: u64 = 24;
const SLICE_CTRL_OFF: u64 = 28;

static REGISTRATIONS: RwLock<BTreeMap<u32, RseqRegistration>> = RwLock::new(BTreeMap::new());
static NEXT_MM_CID: AtomicU32 = AtomicU32::new(1);

#[derive(Clone, Copy)]
struct RseqRegistration {
    ptr: u64,
    len: u32,
    signature: u32,
    mm_cid: u32,
}

fn write_u32(addr: u64, value: u32) -> LinuxResult<()> {
    UserSpaceMemory::copy_to_user(addr, &value.to_ne_bytes()).map_err(|_| LinuxError::EFAULT)
}

fn write_u64(addr: u64, value: u64) -> LinuxResult<()> {
    UserSpaceMemory::copy_to_user(addr, &value.to_ne_bytes()).map_err(|_| LinuxError::EFAULT)
}

fn validate(ptr: u64, len: u32, flags: u32) -> LinuxResult<()> {
    if flags & !VALID_FLAGS != 0 {
        return Err(LinuxError::EINVAL);
    }
    if ptr == 0 {
        return Err(LinuxError::EFAULT);
    }
    if ptr & (RSEQ_ALIGN - 1) != 0 {
        return Err(LinuxError::EINVAL);
    }
    if len < RSEQ_MIN_LEN {
        return Err(LinuxError::EINVAL);
    }
    Ok(())
}

fn publish(reg: RseqRegistration, flags: u32) -> LinuxResult<()> {
    write_u32(reg.ptr + CPU_ID_START_OFF, RSEQ_CPU_ID_UNINITIALIZED)?;
    write_u32(reg.ptr + CPU_ID_OFF, RSEQ_CPU_ID_UNINITIALIZED)?;
    write_u64(reg.ptr + RSEQ_CS_OFF, 0)?;
    write_u32(reg.ptr + FLAGS_OFF, flags & RSEQ_FLAG_SLICE_EXT_DEFAULT_ON)?;
    write_u32(reg.ptr + NODE_ID_OFF, 0)?;
    write_u32(reg.ptr + MM_CID_OFF, reg.mm_cid)?;
    write_u32(reg.ptr + SLICE_CTRL_OFF, 0)?;
    Ok(())
}

pub fn rseq(ptr: u64, len: u32, flags: u32, signature: u32) -> LinuxResult<i32> {
    validate(ptr, len, flags)?;
    let pid = process::current_pid();

    if flags & RSEQ_FLAG_UNREGISTER != 0 {
        let mut regs = REGISTRATIONS.write();
        let Some(existing) = regs.get(&pid).copied() else {
            return Err(LinuxError::EINVAL);
        };
        if existing.ptr != ptr || existing.len != len || existing.signature != signature {
            return Err(LinuxError::EINVAL);
        }
        regs.remove(&pid);
        return Ok(0);
    }

    let mut regs = REGISTRATIONS.write();
    if regs.contains_key(&pid) {
        return Err(LinuxError::EBUSY);
    }
    let reg = RseqRegistration {
        ptr,
        len,
        signature,
        mm_cid: NEXT_MM_CID.fetch_add(1, Ordering::SeqCst),
    };
    publish(reg, flags)?;
    regs.insert(pid, reg);
    Ok(0)
}

pub fn clear_for_pid(pid: u32) {
    REGISTRATIONS.write().remove(&pid);
}

pub fn init() {
    REGISTRATIONS.write().clear();
    NEXT_MM_CID.store(1, Ordering::SeqCst);
}
