//! Minimal io_uring support.
//!
//! This implements a synchronous io_uring core for normal file descriptors.
//! It supports setup, mmap of SQ/CQ/SQE regions, enter, and basic SQE opcodes.

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::string::String;
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
const IORING_OP_READV: u8 = 1;
const IORING_OP_WRITEV: u8 = 2;
const IORING_OP_FSYNC: u8 = 3;
const IORING_OP_READ_FIXED: u8 = 4;
const IORING_OP_WRITE_FIXED: u8 = 5;
const IORING_OP_POLL_ADD: u8 = 6;
const IORING_OP_POLL_REMOVE: u8 = 7;
const IORING_OP_SYNC_FILE_RANGE: u8 = 8;
const IORING_OP_SENDMSG: u8 = 9;
const IORING_OP_RECVMSG: u8 = 10;
const IORING_OP_TIMEOUT: u8 = 11;
const IORING_OP_TIMEOUT_REMOVE: u8 = 12;
const IORING_OP_ACCEPT: u8 = 13;
const IORING_OP_ASYNC_CANCEL: u8 = 14;
const IORING_OP_LINK_TIMEOUT: u8 = 15;
const IORING_OP_CONNECT: u8 = 16;
const IORING_OP_FALLOCATE: u8 = 17;
const IORING_OP_OPENAT: u8 = 18;
const IORING_OP_CLOSE: u8 = 19;
const IORING_OP_FILES_UPDATE: u8 = 20;
const IORING_OP_STATX: u8 = 21;
const IORING_OP_READ: u8 = 22;
const IORING_OP_WRITE: u8 = 23;
const IORING_OP_FADVISE: u8 = 24;
const IORING_OP_MADVISE: u8 = 25;
const IORING_OP_SEND: u8 = 26;
const IORING_OP_RECV: u8 = 27;
const IORING_OP_OPENAT2: u8 = 28;
const IORING_OP_EPOLL_CTL: u8 = 29;
const IORING_OP_SPLICE: u8 = 30;
const IORING_OP_TEE: u8 = 31;
const IORING_OP_SHUTDOWN: u8 = 32;
const IORING_OP_RENAMEAT: u8 = 33;
const IORING_OP_UNLINKAT: u8 = 34;
const IORING_OP_MKDIRAT: u8 = 35;
const IORING_OP_LINKAT: u8 = 37;
const IORING_OP_SYMLINKAT: u8 = 38;

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
struct IoVec {
    base: *mut u8,
    len: usize,
}

impl IoUringSqe {
    /// Return the open_flags field (rw_flags reinterpreted for open/send/recv).
    fn open_flags(&self) -> u32 {
        self.rw_flags
    }

    /// Return the second address field (addr3 for operations needing two pointers).
    fn addr2(&self) -> u64 {
        self.addr3
    }
}

fn c_str_to_string(ptr: *const u8) -> String {
    if ptr.is_null() {
        return String::new();
    }
    let mut len = 0usize;
    unsafe {
        while *ptr.add(len) != 0 {
            len += 1;
            if len > 4096 {
                break;
            }
        }
    }
    let bytes = unsafe { core::slice::from_raw_parts(ptr, len) };
    String::from_utf8_lossy(bytes).into_owned()
}

fn is_supported_at_path(dirfd: i32, path: &str) -> bool {
    const AT_FDCWD: i32 = -100;
    path.starts_with('/') || dirfd == AT_FDCWD
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
        IORING_OP_READV => {
            if sqe.off != u64::MAX {
                return -(LinuxError::ENOSYS as i32);
            }
            let iovs = sqe.addr as *const IoVec;
            if iovs.is_null() {
                return -14;
            }
            let mut total = 0usize;
            for i in 0..sqe.len as usize {
                let iov = unsafe { &*iovs.add(i) };
                if iov.base.is_null() && iov.len != 0 {
                    return -14;
                }
                match linux_compat::file_ops::read(sqe.fd, iov.base, iov.len) {
                    Ok(0) => break,
                    Ok(n) => total += n as usize,
                    Err(e) => return -(e as i32),
                }
            }
            total as i32
        }
        IORING_OP_WRITEV => {
            if sqe.off != u64::MAX {
                return -(LinuxError::ENOSYS as i32);
            }
            let iovs = sqe.addr as *const IoVec;
            if iovs.is_null() {
                return -14;
            }
            let mut total = 0usize;
            for i in 0..sqe.len as usize {
                let iov = unsafe { &*iovs.add(i) };
                if iov.base.is_null() && iov.len != 0 {
                    return -14;
                }
                match linux_compat::file_ops::write(sqe.fd, iov.base as *const u8, iov.len) {
                    Ok(0) => break,
                    Ok(n) => total += n as usize,
                    Err(e) => return -(e as i32),
                }
            }
            total as i32
        }
        IORING_OP_READ_FIXED | IORING_OP_WRITE_FIXED => -(LinuxError::ENOSYS as i32),
        IORING_OP_FSYNC => match linux_compat::file_ops::fsync(sqe.fd) {
            Ok(v) => v,
            Err(e) => -(e as i32),
        },
        IORING_OP_SYNC_FILE_RANGE => -(LinuxError::ENOSYS as i32),
        IORING_OP_CLOSE => match linux_compat::file_ops::close(sqe.fd) {
            Ok(v) => v,
            Err(e) => -(e as i32),
        },
        IORING_OP_OPENAT => {
            // openat(dirfd, pathname, flags, mode)
            let path = sqe.addr as *const u8;
            if path.is_null() {
                return -14;
            }
            let ret = linux_compat::file_ops::openat(
                sqe.fd,
                path,
                sqe.open_flags() as i32,
                sqe.len as u32,
            );
            match ret {
                Ok(fd) => fd as i32,
                Err(e) => -(e as i32),
            }
        }
        IORING_OP_OPENAT2 => -(LinuxError::ENOSYS as i32),
        IORING_OP_STATX => {
            // statx(dirfd, pathname, flags, mask, statxbuf)
            let path = sqe.addr as *const u8;
            let statxbuf = sqe.addr2() as *mut u8;
            if path.is_null() || statxbuf.is_null() {
                return -14;
            }
            let path_str = c_str_to_string(path);
            if !is_supported_at_path(sqe.fd, &path_str) {
                return -38;
            }
            match vfs::vfs_stat(&path_str) {
                Ok(stat) => {
                    #[repr(C)]
                    struct StatxTimestamp {
                        tv_sec: i64,
                        tv_nsec: u32,
                        __reserved: i32,
                    }
                    #[repr(C)]
                    struct Statx {
                        stx_mask: u32,
                        stx_blksize: u32,
                        stx_attributes: u64,
                        stx_nlink: u32,
                        stx_uid: u32,
                        stx_gid: u32,
                        stx_mode: u16,
                        __spare0: u16,
                        stx_ino: u64,
                        stx_size: u64,
                        stx_blocks: u64,
                        stx_attributes_mask: u64,
                        stx_atime: StatxTimestamp,
                        stx_btime: StatxTimestamp,
                        stx_ctime: StatxTimestamp,
                        stx_mtime: StatxTimestamp,
                        stx_rdev_major: u32,
                        stx_rdev_minor: u32,
                        stx_dev_major: u32,
                        stx_dev_minor: u32,
                        stx_mnt_id: u64,
                        stx_dio_mem_align: u32,
                        stx_dio_offset_align: u32,
                        __spare3: [u64; 12],
                    }
                    let stx = Statx {
                        stx_mask: 0x7FF,
                        stx_blksize: stat.blksize as u32,
                        stx_attributes: 0,
                        stx_nlink: stat.nlink,
                        stx_uid: stat.uid,
                        stx_gid: stat.gid,
                        stx_mode: stat.mode as u16,
                        __spare0: 0,
                        stx_ino: stat.ino,
                        stx_size: stat.size,
                        stx_blocks: stat.blocks,
                        stx_attributes_mask: 0,
                        stx_atime: StatxTimestamp {
                            tv_sec: stat.atime as i64,
                            tv_nsec: 0,
                            __reserved: 0,
                        },
                        stx_btime: StatxTimestamp {
                            tv_sec: stat.ctime as i64,
                            tv_nsec: 0,
                            __reserved: 0,
                        },
                        stx_ctime: StatxTimestamp {
                            tv_sec: stat.ctime as i64,
                            tv_nsec: 0,
                            __reserved: 0,
                        },
                        stx_mtime: StatxTimestamp {
                            tv_sec: stat.mtime as i64,
                            tv_nsec: 0,
                            __reserved: 0,
                        },
                        stx_rdev_major: 0,
                        stx_rdev_minor: 0,
                        stx_dev_major: 0,
                        stx_dev_minor: 0,
                        stx_mnt_id: 0,
                        stx_dio_mem_align: 0,
                        stx_dio_offset_align: 0,
                        __spare3: [0; 12],
                    };
                    unsafe {
                        core::ptr::write(statxbuf as *mut Statx, stx);
                    }
                    0
                }
                Err(_) => -2,
            }
        }
        IORING_OP_ACCEPT => {
            // accept(sockfd, addr, addrlen, flags)
            match linux_compat::socket_ops::accept(
                sqe.fd,
                sqe.addr as *mut linux_compat::SockAddr,
                sqe.addr2() as *mut u32,
            ) {
                Ok(fd) => fd as i32,
                Err(e) => -(e as i32),
            }
        }
        IORING_OP_CONNECT => {
            match linux_compat::socket_ops::connect(
                sqe.fd,
                sqe.addr as *const linux_compat::SockAddr,
                sqe.len as u32,
            ) {
                Ok(v) => v as i32,
                Err(e) => -(e as i32),
            }
        }
        IORING_OP_SEND => {
            match linux_compat::socket_ops::send(
                sqe.fd,
                sqe.addr as *const u8,
                sqe.len as usize,
                sqe.open_flags() as i32,
            ) {
                Ok(n) => n as i32,
                Err(e) => -(e as i32),
            }
        }
        IORING_OP_SENDMSG => -(LinuxError::ENOSYS as i32),
        IORING_OP_RECV => {
            match linux_compat::socket_ops::recv(
                sqe.fd,
                sqe.addr as *mut u8,
                sqe.len as usize,
                sqe.open_flags() as i32,
            ) {
                Ok(n) => n as i32,
                Err(e) => -(e as i32),
            }
        }
        IORING_OP_RECVMSG => -(LinuxError::ENOSYS as i32),
        IORING_OP_SHUTDOWN => {
            // shutdown(fd, how)
            match linux_compat::socket_ops::shutdown(sqe.fd, sqe.len as i32) {
                Ok(v) => v as i32,
                Err(e) => -(e as i32),
            }
        }
        IORING_OP_TIMEOUT => {
            // timeout(timeout, count, flags) — sleep for the given time
            // sqe.addr points to struct __kernel_timespec
            if sqe.addr == 0 {
                return -14;
            }
            let secs = unsafe { *(sqe.addr as *const u64) };
            let nanos = unsafe { *((sqe.addr as *const u64).add(1)) };
            // Busy-wait approximation — use nanosleep
            let ts = linux_compat::TimeSpec {
                tv_sec: secs as i64,
                tv_nsec: nanos as i64,
            };
            match linux_compat::time_ops::nanosleep(&ts, core::ptr::null_mut()) {
                Ok(_) => 0,
                Err(e) => -(e as i32),
            }
        }
        IORING_OP_TIMEOUT_REMOVE => -(LinuxError::ENOSYS as i32),
        IORING_OP_FALLOCATE => {
            // fallocate(fd, mode, offset, len)
            match linux_compat::file_ops::fallocate(
                sqe.fd,
                sqe.open_flags() as i32,
                sqe.off as i64,
                sqe.len as i64,
            ) {
                Ok(v) => v as i32,
                Err(e) => -(e as i32),
            }
        }
        IORING_OP_FADVISE | IORING_OP_MADVISE => -(LinuxError::ENOSYS as i32),
        IORING_OP_EPOLL_CTL => {
            // epoll_ctl(epfd, op, fd, event)
            match linux_compat::socket_ops::epoll_ctl(
                sqe.fd,
                sqe.open_flags() as i32,
                sqe.len as i32,
                sqe.addr as *mut u8,
            ) {
                Ok(v) => v as i32,
                Err(e) => -(e as i32),
            }
        }
        IORING_OP_RENAMEAT => {
            // renameat(olddirfd, oldpath, newdirfd, newpath, flags)
            let oldpath = sqe.addr as *const u8;
            let newpath = sqe.addr2() as *const u8;
            if oldpath.is_null() || newpath.is_null() {
                return -14;
            }
            let old_str = c_str_to_string(oldpath);
            let new_str = c_str_to_string(newpath);
            if !is_supported_at_path(sqe.fd, &old_str)
                || !is_supported_at_path(sqe.len as i32, &new_str)
            {
                return -38;
            }
            match vfs::vfs_rename(&old_str, &new_str) {
                Ok(()) => 0,
                Err(_) => -2,
            }
        }
        IORING_OP_UNLINKAT => {
            // unlinkat(dirfd, pathname, flags)
            let path = sqe.addr as *const u8;
            if path.is_null() {
                return -14;
            }
            let path_str = c_str_to_string(path);
            if !is_supported_at_path(sqe.fd, &path_str) {
                return -38;
            }
            match vfs::vfs_unlink(&path_str) {
                Ok(()) => 0,
                Err(_) => -2,
            }
        }
        IORING_OP_MKDIRAT => {
            // mkdirat(dirfd, pathname, mode)
            let path = sqe.addr as *const u8;
            if path.is_null() {
                return -14;
            }
            let path_str = c_str_to_string(path);
            if !is_supported_at_path(sqe.fd, &path_str) {
                return -38;
            }
            match vfs::vfs_mkdir(&path_str, sqe.len as u32) {
                Ok(()) => 0,
                Err(_) => -2,
            }
        }
        IORING_OP_SPLICE | IORING_OP_TEE => -(LinuxError::ENOSYS as i32),
        IORING_OP_LINKAT => {
            // linkat(olddirfd, oldpath, newdirfd, newpath, flags)
            let oldpath = sqe.addr as *const u8;
            let newpath = sqe.addr2() as *const u8;
            if oldpath.is_null() || newpath.is_null() {
                return -14;
            }
            match linux_compat::file_ops::linkat(sqe.fd, oldpath, sqe.len as i32, newpath, 0) {
                Ok(v) => v,
                Err(e) => -(e as i32),
            }
        }
        IORING_OP_SYMLINKAT => {
            // symlinkat(target, newdirfd, linkpath)
            let target = sqe.addr as *const u8;
            let linkpath = sqe.addr2() as *const u8;
            if target.is_null() || linkpath.is_null() {
                return -14;
            }
            match linux_compat::file_ops::symlinkat(target, sqe.fd, linkpath) {
                Ok(v) => v,
                Err(e) => -(e as i32),
            }
        }
        IORING_OP_POLL_ADD => {
            // poll_add(fd, poll_mask) — check fd readiness and return revents.
            // In the io_uring ABI, the poll mask is in the rw_flags union member.
            let fd = sqe.fd;
            let poll_mask = sqe.rw_flags as i16;
            let revents = crate::linux_compat::special_fd::poll_revents(fd, poll_mask);
            if revents & crate::linux_compat::special_fd::poll_events::POLLNVAL != 0 {
                return -9; // EBADF
            }
            revents as i32
        }
        IORING_OP_POLL_REMOVE => -(LinuxError::ENOSYS as i32),
        IORING_OP_ASYNC_CANCEL => -(LinuxError::ENOSYS as i32),
        IORING_OP_LINK_TIMEOUT => -(LinuxError::ENOSYS as i32),
        IORING_OP_FILES_UPDATE => -(LinuxError::ENOSYS as i32),
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
