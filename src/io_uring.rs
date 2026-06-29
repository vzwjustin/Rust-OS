//! Minimal io_uring support.
//!
//! This implements a synchronous io_uring core for normal file descriptors.
//! It supports setup, mmap of SQ/CQ/SQE regions, enter, and basic SQE opcodes.

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::cmp;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

use crate::linux_compat::{self, LinuxError, LinuxResult};
use crate::memory::user_space::UserSpaceMemory;
use crate::vfs::{self, FdKind};

const PAGE_SIZE: usize = 4096;
const MAX_ENTRIES: u32 = 4096;

const IORING_SETUP_IOPOLL: u32 = 1 << 0;
const IORING_SETUP_SQPOLL: u32 = 1 << 1;
const IORING_SETUP_SQ_AFF: u32 = 1 << 2;
const IORING_SETUP_CQSIZE: u32 = 1 << 3;
const IORING_SETUP_CLAMP: u32 = 1 << 4;
const IORING_SETUP_R_DISABLED: u32 = 1 << 6;

const SUPPORTED_SETUP_FLAGS: u32 =
    IORING_SETUP_CQSIZE | IORING_SETUP_CLAMP | IORING_SETUP_R_DISABLED;

const IORING_OFF_SQ_RING: u64 = 0;
const IORING_OFF_CQ_RING: u64 = 0x0800_0000;
const IORING_OFF_SQES: u64 = 0x1000_0000;

const SQ_HEAD: usize = 0;
const SQ_TAIL: usize = 4;
const SQ_RING_MASK: usize = 8;
const SQ_RING_ENTRIES: usize = 12;
const SQ_FLAGS: usize = 16;
const SQ_DROPPED: usize = 20;
const SQ_ARRAY: usize = 24;

const CQ_HEAD: usize = 0;
const CQ_TAIL: usize = 4;
const CQ_RING_MASK: usize = 8;
const CQ_RING_ENTRIES: usize = 12;
const CQ_OVERFLOW: usize = 16;
const CQ_CQES: usize = 20;

const IORING_OP_NOP: u8 = 0;
const IORING_OP_FSYNC: u8 = 3;
const IORING_OP_CLOSE: u8 = 19;
const IORING_OP_READ: u8 = 22;
const IORING_OP_WRITE: u8 = 23;

static NEXT_RING_ID: AtomicU32 = AtomicU32::new(1);
static RINGS: RwLock<BTreeMap<u32, IoUring>> = RwLock::new(BTreeMap::new());

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct IoSqringOffsets {
    head: u32,
    tail: u32,
    ring_mask: u32,
    ring_entries: u32,
    flags: u32,
    dropped: u32,
    array: u32,
    resv1: u32,
    user_addr: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct IoCqringOffsets {
    head: u32,
    tail: u32,
    ring_mask: u32,
    ring_entries: u32,
    overflow: u32,
    cqes: u32,
    flags: u32,
    resv1: u32,
    user_addr: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct IoUringParams {
    pub sq_entries: u32,
    pub cq_entries: u32,
    pub flags: u32,
    pub sq_thread_cpu: u32,
    pub sq_thread_idle: u32,
    pub features: u32,
    pub wq_fd: u32,
    pub resv: [u32; 3],
    pub sq_off: IoSqringOffsets,
    pub cq_off: IoCqringOffsets,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct IoUringSqe {
    opcode: u8,
    flags: u8,
    ioprio: u16,
    fd: i32,
    off: u64,
    addr: u64,
    len: u32,
    rw_flags: u32,
    user_data: u64,
    buf_index: u16,
    personality: u16,
    splice_fd_in: i32,
    addr3: u64,
    pad2: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct IoUringCqe {
    user_data: u64,
    res: i32,
    flags: u32,
}

#[derive(Debug, Clone)]
struct IoUring {
    sq_entries: u32,
    cq_entries: u32,
    sq_ring_addr: Option<usize>,
    cq_ring_addr: Option<usize>,
    sqes_addr: Option<usize>,
}

pub fn init() {
    RINGS.write().clear();
    NEXT_RING_ID.store(1, Ordering::SeqCst);
    crate::serial_println!("[io_uring] io_uring subsystem initialized");
}

fn round_up_pow2(value: u32) -> Option<u32> {
    if value == 0 {
        return None;
    }
    if value > MAX_ENTRIES {
        return Some(MAX_ENTRIES);
    }
    Some(value.next_power_of_two())
}

fn round_up_page(size: usize) -> usize {
    (size + PAGE_SIZE - 1) & !(PAGE_SIZE - 1)
}

fn sq_ring_size(entries: u32) -> usize {
    round_up_page(SQ_ARRAY + entries as usize * core::mem::size_of::<u32>())
}

fn cq_ring_size(entries: u32) -> usize {
    round_up_page(CQ_CQES + entries as usize * core::mem::size_of::<IoUringCqe>())
}

fn sqes_size(entries: u32) -> usize {
    round_up_page(entries as usize * core::mem::size_of::<IoUringSqe>())
}

fn copy_from_user<T: Copy + Default>(addr: u64) -> LinuxResult<T> {
    if addr == 0 {
        return Err(LinuxError::EFAULT);
    }
    let mut value = T::default();
    let bytes = unsafe {
        core::slice::from_raw_parts_mut(
            (&mut value as *mut T) as *mut u8,
            core::mem::size_of::<T>(),
        )
    };
    UserSpaceMemory::copy_from_user(addr, bytes).map_err(|_| LinuxError::EFAULT)?;
    Ok(value)
}

fn copy_to_user<T: Copy>(addr: u64, value: &T) -> LinuxResult<()> {
    if addr == 0 {
        return Err(LinuxError::EFAULT);
    }
    let bytes = unsafe {
        core::slice::from_raw_parts((value as *const T) as *const u8, core::mem::size_of::<T>())
    };
    UserSpaceMemory::copy_to_user(addr, bytes).map_err(|_| LinuxError::EFAULT)
}

fn read_u32(addr: usize) -> LinuxResult<u32> {
    let mut bytes = [0u8; 4];
    UserSpaceMemory::copy_from_user(addr as u64, &mut bytes).map_err(|_| LinuxError::EFAULT)?;
    Ok(u32::from_le_bytes(bytes))
}

fn write_u32(addr: usize, value: u32) -> LinuxResult<()> {
    UserSpaceMemory::copy_to_user(addr as u64, &value.to_le_bytes()).map_err(|_| LinuxError::EFAULT)
}

fn zero_user(addr: usize, len: usize) -> LinuxResult<()> {
    let zeros = [0u8; 256];
    let mut done = 0usize;
    while done < len {
        let n = cmp::min(zeros.len(), len - done);
        UserSpaceMemory::copy_to_user((addr + done) as u64, &zeros[..n])
            .map_err(|_| LinuxError::EFAULT)?;
        done += n;
    }
    Ok(())
}

pub fn setup(entries: u32, params: *mut u8) -> LinuxResult<i32> {
    if params.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let mut p: IoUringParams = copy_from_user(params as u64)?;
    if entries == 0 {
        return Err(LinuxError::EINVAL);
    }
    if p.flags & !SUPPORTED_SETUP_FLAGS != 0
        || p.flags & (IORING_SETUP_IOPOLL | IORING_SETUP_SQPOLL | IORING_SETUP_SQ_AFF) != 0
    {
        return Err(LinuxError::EINVAL);
    }

    let sq_entries = round_up_pow2(entries).ok_or(LinuxError::EINVAL)?;
    let requested_cq = if p.flags & IORING_SETUP_CQSIZE != 0 {
        p.cq_entries
    } else {
        sq_entries.saturating_mul(2)
    };
    let cq_entries = round_up_pow2(requested_cq).ok_or(LinuxError::EINVAL)?;

    let id = NEXT_RING_ID.fetch_add(1, Ordering::SeqCst);
    RINGS.write().insert(
        id,
        IoUring {
            sq_entries,
            cq_entries,
            sq_ring_addr: None,
            cq_ring_addr: None,
            sqes_addr: None,
        },
    );

    p.sq_entries = sq_entries;
    p.cq_entries = cq_entries;
    p.features = 0;
    p.sq_off = IoSqringOffsets {
        head: SQ_HEAD as u32,
        tail: SQ_TAIL as u32,
        ring_mask: SQ_RING_MASK as u32,
        ring_entries: SQ_RING_ENTRIES as u32,
        flags: SQ_FLAGS as u32,
        dropped: SQ_DROPPED as u32,
        array: SQ_ARRAY as u32,
        resv1: 0,
        user_addr: 0,
    };
    p.cq_off = IoCqringOffsets {
        head: CQ_HEAD as u32,
        tail: CQ_TAIL as u32,
        ring_mask: CQ_RING_MASK as u32,
        ring_entries: CQ_RING_ENTRIES as u32,
        overflow: CQ_OVERFLOW as u32,
        cqes: CQ_CQES as u32,
        flags: 0,
        resv1: 0,
        user_addr: 0,
    };
    copy_to_user(params as u64, &p)?;

    let fd = linux_compat::special_fd::register_io_uring(id, vfs::OpenFlags::RDWR);
    if fd < 0 {
        RINGS.write().remove(&id);
        return Err(LinuxError::EMFILE);
    }
    Ok(fd)
}

pub fn io_uring_setup(entries: u32, params: *mut IoUringParams) -> i32 {
    match setup(entries, params as *mut u8) {
        Ok(fd) => fd,
        Err(e) => -(e as i32),
    }
}

pub fn close_ring(id: u32) {
    RINGS.write().remove(&id);
}

pub fn mmap(fd: i32, offset: u64, addr: usize, len: usize) -> LinuxResult<bool> {
    let id = match vfs::vfs_fd_kind(fd) {
        Ok(FdKind::IoUring(id)) => id,
        _ => return Ok(false),
    };

    let mut rings = RINGS.write();
    let ring = rings.get_mut(&id).ok_or(LinuxError::EBADF)?;
    match offset {
        IORING_OFF_SQ_RING => {
            if len < sq_ring_size(ring.sq_entries) {
                return Err(LinuxError::EINVAL);
            }
            init_sq_ring(addr, ring.sq_entries)?;
            ring.sq_ring_addr = Some(addr);
        }
        IORING_OFF_CQ_RING => {
            if len < cq_ring_size(ring.cq_entries) {
                return Err(LinuxError::EINVAL);
            }
            init_cq_ring(addr, ring.cq_entries)?;
            ring.cq_ring_addr = Some(addr);
        }
        IORING_OFF_SQES => {
            if len < sqes_size(ring.sq_entries) {
                return Err(LinuxError::EINVAL);
            }
            zero_user(addr, sqes_size(ring.sq_entries))?;
            ring.sqes_addr = Some(addr);
        }
        _ => return Err(LinuxError::EINVAL),
    }
    Ok(true)
}

fn init_sq_ring(addr: usize, entries: u32) -> LinuxResult<()> {
    zero_user(addr, sq_ring_size(entries))?;
    write_u32(addr + SQ_RING_MASK, entries - 1)?;
    write_u32(addr + SQ_RING_ENTRIES, entries)?;
    for i in 0..entries {
        write_u32(addr + SQ_ARRAY + i as usize * 4, i)?;
    }
    Ok(())
}

fn init_cq_ring(addr: usize, entries: u32) -> LinuxResult<()> {
    zero_user(addr, cq_ring_size(entries))?;
    write_u32(addr + CQ_RING_MASK, entries - 1)?;
    write_u32(addr + CQ_RING_ENTRIES, entries)?;
    Ok(())
}

pub fn enter(
    fd: i32,
    to_submit: u32,
    _min_complete: u32,
    _flags: u32,
    _arg: u64,
    _sigsz: usize,
) -> LinuxResult<i32> {
    let id = linux_compat::special_fd::get_io_uring_id(fd).ok_or(LinuxError::EBADF)?;
    let ring = RINGS.read().get(&id).cloned().ok_or(LinuxError::EBADF)?;
    let sq_ring = ring.sq_ring_addr.ok_or(LinuxError::EINVAL)?;
    let cq_ring = ring.cq_ring_addr.ok_or(LinuxError::EINVAL)?;
    let sqes = ring.sqes_addr.ok_or(LinuxError::EINVAL)?;

    let sq_head = read_u32(sq_ring + SQ_HEAD)?;
    let sq_tail = read_u32(sq_ring + SQ_TAIL)?;
    let available = sq_tail.wrapping_sub(sq_head);
    let submit = cmp::min(to_submit, available);
    let mut completed = 0u32;

    for n in 0..submit {
        let sq_index = (sq_head.wrapping_add(n)) & (ring.sq_entries - 1);
        let sqe_index = read_u32(sq_ring + SQ_ARRAY + sq_index as usize * 4)?;
        if sqe_index >= ring.sq_entries {
            write_u32(
                sq_ring + SQ_DROPPED,
                read_u32(sq_ring + SQ_DROPPED)?.saturating_add(1),
            )?;
            continue;
        }
        let sqe_addr = sqes + sqe_index as usize * core::mem::size_of::<IoUringSqe>();
        let sqe: IoUringSqe = copy_from_user(sqe_addr as u64)?;
        let res = execute_sqe(&sqe);
        push_cqe(
            cq_ring,
            ring.cq_entries,
            IoUringCqe {
                user_data: sqe.user_data,
                res,
                flags: 0,
            },
        )?;
        completed += 1;
    }

    write_u32(sq_ring + SQ_HEAD, sq_head.wrapping_add(submit))?;
    Ok(completed as i32)
}

pub fn io_uring_enter(fd: i32, to_submit: u32, min_complete: u32, flags: u32) -> i32 {
    match enter(fd, to_submit, min_complete, flags, 0, 0) {
        Ok(done) => done,
        Err(e) => -(e as i32),
    }
}

fn execute_sqe(sqe: &IoUringSqe) -> i32 {
    match sqe.opcode {
        IORING_OP_NOP => 0,
        IORING_OP_READ => {
            match linux_compat::file_ops::read(sqe.fd, sqe.addr as *mut u8, sqe.len as usize) {
                Ok(n) => n as i32,
                Err(e) => -(e as i32),
            }
        }
        IORING_OP_WRITE => {
            match linux_compat::file_ops::write(sqe.fd, sqe.addr as *const u8, sqe.len as usize) {
                Ok(n) => n as i32,
                Err(e) => -(e as i32),
            }
        }
        IORING_OP_FSYNC => match linux_compat::file_ops::fsync(sqe.fd) {
            Ok(v) => v,
            Err(e) => -(e as i32),
        },
        IORING_OP_CLOSE => match linux_compat::file_ops::close(sqe.fd) {
            Ok(v) => v,
            Err(e) => -(e as i32),
        },
        _ => -(LinuxError::ENOSYS as i32),
    }
}

fn push_cqe(cq_ring: usize, entries: u32, cqe: IoUringCqe) -> LinuxResult<()> {
    let head = read_u32(cq_ring + CQ_HEAD)?;
    let tail = read_u32(cq_ring + CQ_TAIL)?;
    if tail.wrapping_sub(head) >= entries {
        let overflow = read_u32(cq_ring + CQ_OVERFLOW)?;
        write_u32(cq_ring + CQ_OVERFLOW, overflow.saturating_add(1))?;
        return Ok(());
    }
    let index = tail & (entries - 1);
    let cqe_addr = cq_ring + CQ_CQES + index as usize * core::mem::size_of::<IoUringCqe>();
    copy_to_user(cqe_addr as u64, &cqe)?;
    write_u32(cq_ring + CQ_TAIL, tail.wrapping_add(1))
}

pub fn register(_fd: i32, opcode: u32, _arg: u64, nr_args: u32) -> LinuxResult<i32> {
    if nr_args != 0 {
        return Err(LinuxError::EINVAL);
    }
    match opcode {
        0 => Ok(0),
        _ => Err(LinuxError::EINVAL),
    }
}

pub fn io_uring_register(fd: i32, opcode: u32, arg: u64, nr_args: u32) -> i32 {
    match register(fd, opcode, arg, nr_args) {
        Ok(v) => v,
        Err(e) => -(e as i32),
    }
}
