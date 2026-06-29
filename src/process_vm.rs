//! Process memory syscalls.
//!
//! RustOS currently exposes safe user-memory copying for the current address
//! space only. These handlers therefore implement the full iovec walk for the
//! current process and reject true remote address-space access until the VM
//! layer provides remote page-table pin/copy primitives.

extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;

use crate::linux_compat::memory_ops;
use crate::linux_compat::{LinuxError, LinuxResult};
use crate::memory::user_space::UserSpaceMemory;
use crate::process::{self, ProcessState};

const MAX_IOV: usize = 1024;
const COPY_CHUNK: usize = 4096;

#[derive(Clone, Copy)]
struct UserIov {
    base: u64,
    len: usize,
}

fn read_usize(addr: u64) -> LinuxResult<usize> {
    let mut bytes = [0u8; core::mem::size_of::<usize>()];
    UserSpaceMemory::copy_from_user(addr, &mut bytes).map_err(|_| LinuxError::EFAULT)?;
    Ok(usize::from_ne_bytes(bytes))
}

fn read_iovecs(addr: u64, count: usize) -> LinuxResult<Vec<UserIov>> {
    if count > MAX_IOV {
        return Err(LinuxError::EINVAL);
    }
    if count == 0 {
        return Ok(Vec::new());
    }
    if addr == 0 {
        return Err(LinuxError::EFAULT);
    }

    let mut out = Vec::with_capacity(count);
    let stride = core::mem::size_of::<usize>() * 2;
    for i in 0..count {
        let entry = addr + (i * stride) as u64;
        let base = read_usize(entry)? as u64;
        let len = read_usize(entry + core::mem::size_of::<usize>() as u64)?;
        out.push(UserIov { base, len });
    }
    Ok(out)
}

fn check_target_pid(pid: i32) -> LinuxResult<u32> {
    if pid <= 0 {
        return Err(LinuxError::ESRCH);
    }
    let pid = pid as u32;
    if process::get_process_manager().get_process(pid).is_none() {
        return Err(LinuxError::ESRCH);
    }
    Ok(pid)
}

fn ensure_current_target(pid: u32) -> LinuxResult<()> {
    if pid == process::current_pid() {
        Ok(())
    } else {
        Err(LinuxError::EPERM)
    }
}

fn copy_between_iovecs(
    local: &[UserIov],
    remote: &[UserIov],
    write_remote: bool,
) -> LinuxResult<isize> {
    let mut local_idx = 0usize;
    let mut remote_idx = 0usize;
    let mut local_off = 0usize;
    let mut remote_off = 0usize;
    let mut total = 0usize;
    let mut buf = vec![0u8; COPY_CHUNK];

    while local_idx < local.len() && remote_idx < remote.len() {
        if local[local_idx].len == local_off {
            local_idx += 1;
            local_off = 0;
            continue;
        }
        if remote[remote_idx].len == remote_off {
            remote_idx += 1;
            remote_off = 0;
            continue;
        }

        let local_left = local[local_idx].len - local_off;
        let remote_left = remote[remote_idx].len - remote_off;
        let n = core::cmp::min(core::cmp::min(local_left, remote_left), COPY_CHUNK);
        if n == 0 {
            break;
        }

        let local_addr = local[local_idx].base + local_off as u64;
        let remote_addr = remote[remote_idx].base + remote_off as u64;
        let step = if write_remote {
            match UserSpaceMemory::copy_from_user(local_addr, &mut buf[..n])
                .and_then(|_| UserSpaceMemory::copy_to_user(remote_addr, &buf[..n]))
            {
                Ok(()) => n,
                Err(_) if total > 0 => return Ok(total as isize),
                Err(_) => return Err(LinuxError::EFAULT),
            }
        } else {
            match UserSpaceMemory::copy_from_user(remote_addr, &mut buf[..n])
                .and_then(|_| UserSpaceMemory::copy_to_user(local_addr, &buf[..n]))
            {
                Ok(()) => n,
                Err(_) if total > 0 => return Ok(total as isize),
                Err(_) => return Err(LinuxError::EFAULT),
            }
        };

        total += step;
        local_off += step;
        remote_off += step;
    }

    Ok(total as isize)
}

pub fn process_vm_readv(
    pid: i32,
    local_iov: u64,
    liovcnt: usize,
    remote_iov: u64,
    riovcnt: usize,
    flags: u64,
) -> LinuxResult<isize> {
    if flags != 0 {
        return Err(LinuxError::EINVAL);
    }
    let pid = check_target_pid(pid)?;
    ensure_current_target(pid)?;
    let local = read_iovecs(local_iov, liovcnt)?;
    let remote = read_iovecs(remote_iov, riovcnt)?;
    copy_between_iovecs(&local, &remote, false)
}

pub fn process_vm_writev(
    pid: i32,
    local_iov: u64,
    liovcnt: usize,
    remote_iov: u64,
    riovcnt: usize,
    flags: u64,
) -> LinuxResult<isize> {
    if flags != 0 {
        return Err(LinuxError::EINVAL);
    }
    let pid = check_target_pid(pid)?;
    ensure_current_target(pid)?;
    let local = read_iovecs(local_iov, liovcnt)?;
    let remote = read_iovecs(remote_iov, riovcnt)?;
    copy_between_iovecs(&local, &remote, true)
}

pub fn process_madvise(
    pidfd: i32,
    iov: u64,
    vlen: usize,
    advice: i32,
    flags: u32,
) -> LinuxResult<isize> {
    if flags != 0 {
        return Err(LinuxError::EINVAL);
    }
    let target = crate::pidfd::get_pid(pidfd).ok_or(LinuxError::EBADF)?;
    ensure_current_target(target)?;
    let iovecs = read_iovecs(iov, vlen)?;
    let mut total = 0usize;
    for entry in iovecs {
        if entry.len == 0 {
            continue;
        }
        memory_ops::madvise(entry.base as *mut u8, entry.len, advice)?;
        total = total.saturating_add(entry.len);
    }
    Ok(total as isize)
}

pub fn process_mrelease(pidfd: i32, flags: u32) -> LinuxResult<i32> {
    if flags != 0 {
        return Err(LinuxError::EINVAL);
    }
    let target = crate::pidfd::get_pid(pidfd).ok_or(LinuxError::EBADF)?;
    let Some(pcb) = process::get_process_manager().get_process(target) else {
        return Err(LinuxError::ESRCH);
    };
    if !matches!(pcb.state, ProcessState::Zombie | ProcessState::Terminated) {
        return Err(LinuxError::EBUSY);
    }

    process::get_process_manager()
        .with_process_mut(target, |process| {
            process.memory.heap_size = 0;
            process.memory.stack_size = 0;
            process.memory.vm_size = 0;
            process.locked_pages = 0;
        })
        .ok_or(LinuxError::ESRCH)?;
    Ok(0)
}
