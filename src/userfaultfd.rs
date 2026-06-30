//! userfaultfd syscall and ioctl support.
//!
//! This implements the fd lifecycle, API negotiation, range registration, and
//! event queue semantics. The VM page-fault path delegates registered missing
//! page faults here so userspace can service them with `UFFDIO_COPY`,
//! `UFFDIO_ZEROPAGE`, and `UFFDIO_WAKE`.

extern crate alloc;

use alloc::collections::{BTreeMap, VecDeque};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

use crate::linux_compat::{LinuxError, LinuxResult};
use crate::memory::user_space::UserSpaceMemory;
use crate::vfs;

const UFFD_API: u64 = 0xAA;

const UFFDIO_API: u64 = 0xc018_aa3f;
const UFFDIO_REGISTER: u64 = 0xc020_aa00;
const UFFDIO_UNREGISTER: u64 = 0x8010_aa01;
const UFFDIO_WAKE: u64 = 0x8010_aa02;
const UFFDIO_COPY: u64 = 0xc028_aa03;
const UFFDIO_ZEROPAGE: u64 = 0xc020_aa04;
const UFFDIO_MOVE: u64 = 0xc028_aa05;
const UFFDIO_WRITEPROTECT: u64 = 0xc018_aa06;
const UFFDIO_CONTINUE: u64 = 0xc020_aa07;
const UFFDIO_POISON: u64 = 0xc020_aa08;

const _UFFDIO_REGISTER: u64 = 0;
const _UFFDIO_UNREGISTER: u64 = 1;
const _UFFDIO_WAKE: u64 = 2;
const _UFFDIO_COPY: u64 = 3;
const _UFFDIO_ZEROPAGE: u64 = 4;
const _UFFDIO_MOVE: u64 = 5;
const _UFFDIO_WRITEPROTECT: u64 = 6;
const _UFFDIO_CONTINUE: u64 = 7;
const _UFFDIO_POISON: u64 = 8;
const _UFFDIO_API: u64 = 0x3f;

const API_IOCTLS: u64 =
    (1u64 << _UFFDIO_API) | (1u64 << _UFFDIO_REGISTER) | (1u64 << _UFFDIO_UNREGISTER);
const RANGE_IOCTLS: u64 = (1u64 << _UFFDIO_WAKE)
    | (1u64 << _UFFDIO_COPY)
    | (1u64 << _UFFDIO_ZEROPAGE)
    | (1u64 << _UFFDIO_MOVE)
    | (1u64 << _UFFDIO_WRITEPROTECT)
    | (1u64 << _UFFDIO_CONTINUE)
    | (1u64 << _UFFDIO_POISON);

const UFFDIO_REGISTER_MODE_MISSING: u64 = 1 << 0;
const UFFDIO_REGISTER_MODE_WP: u64 = 1 << 1;
const UFFDIO_REGISTER_MODE_MINOR: u64 = 1 << 2;
const SUPPORTED_REGISTER_MODES: u64 = UFFDIO_REGISTER_MODE_MISSING;

const UFFD_EVENT_PAGEFAULT: u8 = 0x12;
const UFFD_PAGEFAULT_FLAG_WRITE: u64 = 1 << 0;
const UFFD_PAGEFAULT_FLAG_WP: u64 = 1 << 1;
const UFFD_PAGEFAULT_FLAG_MINOR: u64 = 1 << 2;
const SUPPORTED_PAGEFAULT_FLAGS: u64 = UFFD_PAGEFAULT_FLAG_WRITE;

const O_CLOEXEC: u32 = 0x80000;
const O_NONBLOCK: u32 = 0x800;
const VALID_USERFAULTFD_FLAGS: u32 = O_CLOEXEC | O_NONBLOCK;

static NEXT_ID: AtomicU32 = AtomicU32::new(1);
static USERFAULTFDS: RwLock<BTreeMap<u32, UserfaultfdState>> = RwLock::new(BTreeMap::new());

#[derive(Clone, Copy)]
struct Registration {
    start: u64,
    len: u64,
    mode: u64,
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
struct UffdioApi {
    api: u64,
    features: u64,
    ioctls: u64,
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
struct UffdioRange {
    start: u64,
    len: u64,
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
struct UffdioRegister {
    range: UffdioRange,
    mode: u64,
    ioctls: u64,
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
struct UffdioCopy {
    dst: u64,
    src: u64,
    len: u64,
    mode: u64,
    copy: i64,
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
struct UffdioZeropage {
    range: UffdioRange,
    mode: u64,
    zeropage: i64,
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
struct UffdioMove {
    dst: u64,
    src: u64,
    len: u64,
    mode: u64,
    move_: i64,
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
struct UffdioWriteprotect {
    range: UffdioRange,
    mode: u64,
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
struct UffdioContinue {
    range: UffdioRange,
    mode: u64,
    mapped: i64,
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
struct UffdioPoison {
    range: UffdioRange,
    mode: u64,
    updated: i64,
}

#[derive(Clone, Copy)]
#[repr(C, packed)]
pub struct UffdMsg {
    event: u8,
    reserved1: u8,
    reserved2: u16,
    reserved3: u32,
    arg0: u64,
    arg1: u64,
    arg2: u64,
}

impl UffdMsg {
    fn pagefault(address: u64, flags: u64, ptid: u32) -> Self {
        Self {
            event: UFFD_EVENT_PAGEFAULT,
            reserved1: 0,
            reserved2: 0,
            reserved3: 0,
            arg0: flags,
            arg1: address,
            arg2: ptid as u64,
        }
    }
}

struct UserfaultfdState {
    api_enabled: bool,
    flags: u32,
    registrations: Vec<Registration>,
    events: VecDeque<UffdMsg>,
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

fn validate_range(range: UffdioRange) -> LinuxResult<()> {
    if range.start == 0 || range.len == 0 {
        return Err(LinuxError::EINVAL);
    }
    if range.start & 0xfff != 0 || range.len & 0xfff != 0 {
        return Err(LinuxError::EINVAL);
    }
    if range.start.checked_add(range.len).is_none() {
        return Err(LinuxError::EINVAL);
    }
    Ok(())
}

fn ranges_overlap(a: Registration, b: Registration) -> bool {
    let a_end = a.start.saturating_add(a.len);
    let b_end = b.start.saturating_add(b.len);
    a.start < b_end && b.start < a_end
}

fn require_ready(state: &UserfaultfdState) -> LinuxResult<()> {
    if !state.api_enabled {
        return Err(LinuxError::EINVAL);
    }
    Ok(())
}

pub fn userfaultfd(flags: u32) -> i32 {
    if flags & !VALID_USERFAULTFD_FLAGS != 0 {
        return -(LinuxError::EINVAL as i32);
    }

    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    USERFAULTFDS.write().insert(
        id,
        UserfaultfdState {
            api_enabled: false,
            flags,
            registrations: Vec::new(),
            events: VecDeque::new(),
        },
    );

    let mut fd_flags = vfs::OpenFlags::RDWR;
    if flags & O_CLOEXEC != 0 {
        fd_flags |= vfs::OpenFlags::CLOEXEC;
    }
    if flags & O_NONBLOCK != 0 {
        fd_flags |= vfs::OpenFlags::NONBLOCK;
    }

    crate::linux_compat::special_fd::register_userfaultfd(id, fd_flags)
}

pub fn close_userfaultfd(id: u32) {
    USERFAULTFDS.write().remove(&id);
}

pub fn init() {
    USERFAULTFDS.write().clear();
    NEXT_ID.store(1, Ordering::SeqCst);
}

pub fn has_events(id: u32) -> bool {
    USERFAULTFDS
        .read()
        .get(&id)
        .map(|state| !state.events.is_empty())
        .unwrap_or(false)
}

pub fn read_events(id: u32, buf: &mut [u8]) -> LinuxResult<isize> {
    if buf.len() < core::mem::size_of::<UffdMsg>() {
        return Err(LinuxError::EINVAL);
    }

    let mut table = USERFAULTFDS.write();
    let state = table.get_mut(&id).ok_or(LinuxError::EBADF)?;
    let Some(msg) = state.events.pop_front() else {
        return Err(LinuxError::EAGAIN);
    };

    let msg_bytes = unsafe {
        core::slice::from_raw_parts(
            (&msg as *const UffdMsg) as *const u8,
            core::mem::size_of::<UffdMsg>(),
        )
    };
    buf[..msg_bytes.len()].copy_from_slice(msg_bytes);
    Ok(msg_bytes.len() as isize)
}

pub fn queue_pagefault(id: u32, address: u64, flags: u64, thread_id: u32) -> LinuxResult<()> {
    if flags & !SUPPORTED_PAGEFAULT_FLAGS != 0 {
        return Err(LinuxError::EINVAL);
    }

    let mut table = USERFAULTFDS.write();
    let state = table.get_mut(&id).ok_or(LinuxError::EBADF)?;
    if !state
        .registrations
        .iter()
        .any(|reg| address >= reg.start && address < reg.start.saturating_add(reg.len))
    {
        return Err(LinuxError::ENOENT);
    }

    state
        .events
        .push_back(UffdMsg::pagefault(address, flags, thread_id));
    Ok(())
}

/// Queue a page-fault event for the first userfaultfd registration covering
/// `address`. Returns true when the fault was claimed by userfaultfd.
pub fn handle_page_fault(address: u64, error_code: u64, thread_id: u32) -> bool {
    let pagefault_flags = if error_code & 0x2 != 0 {
        UFFD_PAGEFAULT_FLAG_WRITE
    } else {
        0
    };
    if pagefault_flags & !SUPPORTED_PAGEFAULT_FLAGS != 0 {
        return false;
    }

    let mut table = USERFAULTFDS.write();
    for state in table.values_mut() {
        if !state.api_enabled {
            continue;
        }
        let registered = state.registrations.iter().any(|reg| {
            reg.mode & UFFDIO_REGISTER_MODE_MISSING != 0
                && address >= reg.start
                && address < reg.start.saturating_add(reg.len)
        });
        if !registered {
            continue;
        }
        let already_queued = state
            .events
            .iter()
            .any(|msg| msg.event == UFFD_EVENT_PAGEFAULT && msg.arg1 == address);
        if !already_queued {
            state
                .events
                .push_back(UffdMsg::pagefault(address, pagefault_flags, thread_id));
        }
        return true;
    }

    false
}

pub fn ioctl(fd: i32, request: u64, argp: u64) -> Option<LinuxResult<i32>> {
    let id = crate::linux_compat::special_fd::get_userfaultfd_id(fd)?;
    Some(ioctl_by_id(id, request, argp))
}

fn ioctl_by_id(id: u32, request: u64, argp: u64) -> LinuxResult<i32> {
    match request {
        UFFDIO_API => {
            let mut api: UffdioApi = copy_from_user(argp)?;
            if api.api != UFFD_API {
                return Err(LinuxError::EINVAL);
            }
            if api.features != 0 {
                return Err(LinuxError::EINVAL);
            }
            api.features = 0;
            api.ioctls = API_IOCTLS;
            let mut table = USERFAULTFDS.write();
            let state = table.get_mut(&id).ok_or(LinuxError::EBADF)?;
            state.api_enabled = true;
            copy_to_user(argp, &api)?;
            Ok(0)
        }
        UFFDIO_REGISTER => {
            let mut reg: UffdioRegister = copy_from_user(argp)?;
            validate_range(reg.range)?;
            if reg.mode == 0 || reg.mode & !SUPPORTED_REGISTER_MODES != 0 {
                return Err(LinuxError::EINVAL);
            }
            let new_reg = Registration {
                start: reg.range.start,
                len: reg.range.len,
                mode: reg.mode,
            };
            let mut table = USERFAULTFDS.write();
            let state = table.get_mut(&id).ok_or(LinuxError::EBADF)?;
            require_ready(state)?;
            if state
                .registrations
                .iter()
                .any(|old| ranges_overlap(*old, new_reg))
            {
                return Err(LinuxError::EEXIST);
            }
            state.registrations.push(new_reg);
            reg.ioctls = RANGE_IOCTLS;
            copy_to_user(argp, &reg)?;
            Ok(0)
        }
        UFFDIO_UNREGISTER => {
            let range: UffdioRange = copy_from_user(argp)?;
            validate_range(range)?;
            let mut table = USERFAULTFDS.write();
            let state = table.get_mut(&id).ok_or(LinuxError::EBADF)?;
            require_ready(state)?;
            let before = state.registrations.len();
            let end = range.start + range.len;
            state
                .registrations
                .retain(|reg| reg.start >= end || reg.start.saturating_add(reg.len) <= range.start);
            if state.registrations.len() == before {
                return Err(LinuxError::ENOENT);
            }
            Ok(0)
        }
        UFFDIO_WAKE => {
            let range: UffdioRange = copy_from_user(argp)?;
            validate_range(range)?;
            let mut table = USERFAULTFDS.write();
            let state = table.get_mut(&id).ok_or(LinuxError::EBADF)?;
            require_ready(state)?;
            let end = range.start + range.len;
            state.events.retain(|msg| {
                msg.event != UFFD_EVENT_PAGEFAULT || msg.arg1 < range.start || msg.arg1 >= end
            });
            Ok(0)
        }
        UFFDIO_COPY => {
            let mut copy: UffdioCopy = copy_from_user(argp)?;
            validate_range(UffdioRange {
                start: copy.dst,
                len: copy.len,
            })?;
            if copy.src == 0 || copy.len == 0 {
                return Err(LinuxError::EINVAL);
            }
            // Reject a source range that wraps the address space before reading
            // from it (the dst range is already checked by validate_range).
            copy.src.checked_add(copy.len).ok_or(LinuxError::EINVAL)?;
            if !range_registered(id, copy.dst, copy.len)? {
                return Err(LinuxError::ENOENT);
            }
            copy_user_bytes(copy.src, copy.dst, copy.len as usize)?;
            copy.copy = copy.len as i64;
            copy_to_user(argp, &copy)?;
            Ok(0)
        }
        UFFDIO_ZEROPAGE => {
            let mut zero: UffdioZeropage = copy_from_user(argp)?;
            validate_range(zero.range)?;
            if !range_registered(id, zero.range.start, zero.range.len)? {
                return Err(LinuxError::ENOENT);
            }
            zero_user_bytes(zero.range.start, zero.range.len as usize)?;
            zero.zeropage = zero.range.len as i64;
            copy_to_user(argp, &zero)?;
            Ok(0)
        }
        UFFDIO_MOVE | UFFDIO_WRITEPROTECT | UFFDIO_CONTINUE | UFFDIO_POISON => {
            Err(LinuxError::ENOTSUP)
        }
        _ => Err(LinuxError::ENOTTY),
    }
}

fn range_registered(id: u32, start: u64, len: u64) -> LinuxResult<bool> {
    let table = USERFAULTFDS.read();
    let state = table.get(&id).ok_or(LinuxError::EBADF)?;
    require_ready(state)?;
    let end = start.checked_add(len).ok_or(LinuxError::EINVAL)?;
    Ok(state
        .registrations
        .iter()
        .any(|reg| start >= reg.start && end <= reg.start.saturating_add(reg.len)))
}

fn copy_user_bytes(src: u64, dst: u64, len: usize) -> LinuxResult<()> {
    const CHUNK: usize = 256;
    let mut done = 0usize;
    let mut buf = [0u8; CHUNK];
    while done < len {
        let n = core::cmp::min(CHUNK, len - done);
        UserSpaceMemory::copy_from_user(src + done as u64, &mut buf[..n])
            .map_err(|_| LinuxError::EFAULT)?;
        UserSpaceMemory::copy_to_user(dst + done as u64, &buf[..n])
            .map_err(|_| LinuxError::EFAULT)?;
        done += n;
    }
    Ok(())
}

fn zero_user_bytes(dst: u64, len: usize) -> LinuxResult<()> {
    const CHUNK: usize = 256;
    let mut done = 0usize;
    let zeros = [0u8; CHUNK];
    while done < len {
        let n = core::cmp::min(CHUNK, len - done);
        UserSpaceMemory::copy_to_user(dst + done as u64, &zeros[..n])
            .map_err(|_| LinuxError::EFAULT)?;
        done += n;
    }
    Ok(())
}
