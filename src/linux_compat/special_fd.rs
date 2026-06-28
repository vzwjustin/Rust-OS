//! Special file descriptors: pipes, eventfd, timerfd, epoll, and poll multiplexing.

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use spin::RwLock;

use super::types::PollFd;
use super::{LinuxError, LinuxResult};
use crate::process;
use crate::process::ipc::get_ipc_manager;
use crate::time;
use crate::vfs::{self, FdKind, OpenFlags};

/// Poll event bits (match Linux epoll/poll)
pub mod poll_events {
    pub const POLLIN: i16 = 0x001;
    pub const POLLPRI: i16 = 0x002;
    pub const POLLOUT: i16 = 0x004;
    pub const POLLERR: i16 = 0x008;
    pub const POLLHUP: i16 = 0x010;
    pub const POLLNVAL: i16 = 0x020;
}

static NEXT_EVENT_ID: AtomicU32 = AtomicU32::new(1);
static NEXT_TIMER_ID: AtomicU32 = AtomicU32::new(1);
static NEXT_EPOLL_ID: AtomicU32 = AtomicU32::new(1);

/// Initialize special fd subsystem.
pub fn init_special_fd() {}

struct EventFdState {
    value: AtomicU64,
    #[allow(dead_code)]
    flags: i32,
}

struct TimerFdState {
    expires_ns: AtomicU64,
    interval_ns: u64,
    armed: AtomicU64,
}

#[derive(Clone)]
struct EpollEntry {
    fd: i32,
    events: u32,
}

#[derive(Clone)]
struct EpollState {
    entries: Vec<EpollEntry>,
}

static EVENTFD_BY_ID: RwLock<BTreeMap<u32, EventFdState>> = RwLock::new(BTreeMap::new());
static TIMERFD_BY_ID: RwLock<BTreeMap<u32, TimerFdState>> = RwLock::new(BTreeMap::new());
static EPOLL_BY_ID: RwLock<BTreeMap<u32, EpollState>> = RwLock::new(BTreeMap::new());

fn placeholder_inode() -> alloc::sync::Arc<dyn vfs::InodeOps> {
    vfs::get_vfs().lookup("/").expect("root")
}

fn register_special(kind: FdKind, flags: u32) -> LinuxResult<i32> {
    let inode = placeholder_inode();
    vfs::vfs_open_special(inode, flags, kind).map_err(|_| LinuxError::EMFILE)
}

/// Read from a special fd if applicable.
pub fn try_read(fd: i32, buf: &mut [u8]) -> Option<LinuxResult<isize>> {
    let kind = vfs::vfs_fd_kind(fd).ok()?;
    match kind {
        FdKind::PipeRead(pipe_id) => {
            let ipc = get_ipc_manager();
            match ipc.pipe_read(pipe_id, buf) {
                Ok(0) if buf.is_empty() => Some(Ok(0)),
                Ok(0) => Some(Err(LinuxError::EAGAIN)),
                Ok(n) => Some(Ok(n as isize)),
                Err(_) => Some(Err(LinuxError::EPIPE)),
            }
        }
        FdKind::EventFd(id) => {
            if buf.len() < 8 {
                return Some(Err(LinuxError::EINVAL));
            }
            let table = EVENTFD_BY_ID.read();
            let event = table.get(&id)?;
            let val = event.value.swap(0, Ordering::SeqCst);
            if val == 0 {
                return Some(Err(LinuxError::EAGAIN));
            }
            buf[..8].copy_from_slice(&val.to_le_bytes());
            Some(Ok(8))
        }
        FdKind::TimerFd(id) => {
            if buf.len() < 8 {
                return Some(Err(LinuxError::EINVAL));
            }
            let table = TIMERFD_BY_ID.read();
            let timer = table.get(&id)?;
            let now = time::uptime_ns();
            if timer.armed.load(Ordering::SeqCst) == 0
                || now < timer.expires_ns.load(Ordering::SeqCst)
            {
                return Some(Err(LinuxError::EAGAIN));
            }
            timer.armed.store(0, Ordering::SeqCst);
            if timer.interval_ns > 0 {
                timer
                    .expires_ns
                    .store(now + timer.interval_ns, Ordering::SeqCst);
                timer.armed.store(1, Ordering::SeqCst);
            }
            buf[..8].copy_from_slice(&1u64.to_le_bytes());
            Some(Ok(8))
        }
        FdKind::Socket(socket_id) => {
            let mut sock = crate::net::network_stack().get_socket(socket_id)?;
            match sock.recv(buf) {
                Ok(n) => Some(Ok(n as isize)),
                Err(crate::net::NetworkError::Timeout) => Some(Err(LinuxError::EAGAIN)),
                Err(e) => Some(Err(super::socket_ops::net_err_to_linux(e))),
            }
        }
        _ => None,
    }
}

/// Write to a special fd if applicable.
pub fn try_write(fd: i32, buf: &[u8]) -> Option<LinuxResult<isize>> {
    let kind = vfs::vfs_fd_kind(fd).ok()?;
    match kind {
        FdKind::PipeWrite(pipe_id) => {
            let ipc = get_ipc_manager();
            match ipc.pipe_write(pipe_id, buf) {
                Ok(0) if buf.is_empty() => Some(Ok(0)),
                Ok(0) => Some(Err(LinuxError::EAGAIN)),
                Ok(n) => Some(Ok(n as isize)),
                Err(_) => Some(Err(LinuxError::EPIPE)),
            }
        }
        FdKind::EventFd(id) => {
            if buf.len() != 8 {
                return Some(Err(LinuxError::EINVAL));
            }
            let mut bytes = [0u8; 8];
            bytes.copy_from_slice(buf);
            let add = u64::from_le_bytes(bytes);
            let table = EVENTFD_BY_ID.read();
            let event = table.get(&id)?;
            event.value.fetch_add(add, Ordering::SeqCst);
            Some(Ok(8))
        }
        FdKind::Socket(socket_id) => {
            let mut sock = crate::net::network_stack().get_socket(socket_id)?;
            match sock.send(buf) {
                Ok(n) => Some(Ok(n as isize)),
                Err(e) => Some(Err(super::socket_ops::net_err_to_linux(e))),
            }
        }
        _ => None,
    }
}

/// Close special fd state if applicable.
pub fn try_close(fd: i32) -> Option<LinuxResult<()>> {
    let kind = vfs::vfs_fd_kind(fd).ok()?;
    match kind {
        FdKind::PipeRead(pipe_id) => {
            let _ = get_ipc_manager().close_pipe(pipe_id, true, false);
            Some(Ok(()))
        }
        FdKind::PipeWrite(pipe_id) => {
            let _ = get_ipc_manager().close_pipe(pipe_id, false, true);
            Some(Ok(()))
        }
        FdKind::EventFd(id) => {
            EVENTFD_BY_ID.write().remove(&id);
            Some(Ok(()))
        }
        FdKind::TimerFd(id) => {
            TIMERFD_BY_ID.write().remove(&id);
            Some(Ok(()))
        }
        FdKind::Epoll(id) => {
            EPOLL_BY_ID.write().remove(&id);
            Some(Ok(()))
        }
        FdKind::Socket(socket_id) => {
            if let Some(mut sock) = crate::net::network_stack().get_socket(socket_id) {
                let _ = sock.close();
            }
            crate::net::network_stack().close_socket(socket_id).ok();
            Some(Ok(()))
        }
        _ => None,
    }
}
pub fn poll_revents(fd: i32, events: i16) -> i16 {
    if fd < 0 {
        return poll_events::POLLNVAL;
    }

    let Ok(kind) = vfs::vfs_fd_kind(fd) else {
        return poll_events::POLLNVAL;
    };

    let mut revents = 0i16;
    match kind {
        FdKind::Regular | FdKind::Directory { .. } => {
            if events & poll_events::POLLIN != 0 {
                revents |= poll_events::POLLIN;
            }
            if events & poll_events::POLLOUT != 0 {
                revents |= poll_events::POLLOUT;
            }
        }
        FdKind::PipeRead(pipe_id) => {
            let ipc = get_ipc_manager();
            if events & poll_events::POLLIN != 0 && ipc.pipe_has_data(pipe_id) {
                revents |= poll_events::POLLIN;
            }
        }
        FdKind::PipeWrite(pipe_id) => {
            let ipc = get_ipc_manager();
            if events & poll_events::POLLOUT != 0 && ipc.pipe_has_space(pipe_id) {
                revents |= poll_events::POLLOUT;
            }
        }
        FdKind::EventFd(id) => {
            if events & poll_events::POLLIN != 0 {
                if let Some(event) = EVENTFD_BY_ID.read().get(&id) {
                    if event.value.load(Ordering::SeqCst) > 0 {
                        revents |= poll_events::POLLIN;
                    }
                }
            }
        }
        FdKind::TimerFd(id) => {
            if events & poll_events::POLLIN != 0 {
                if let Some(timer) = TIMERFD_BY_ID.read().get(&id) {
                    if timer.armed.load(Ordering::SeqCst) != 0
                        && time::uptime_ns() >= timer.expires_ns.load(Ordering::SeqCst)
                    {
                        revents |= poll_events::POLLIN;
                    }
                }
            }
        }
        FdKind::Socket(socket_id) => {
            if let Some(sock) = crate::net::network_stack().get_socket(socket_id) {
                if events & poll_events::POLLIN != 0 && sock.has_data() {
                    revents |= poll_events::POLLIN;
                }
                if events & poll_events::POLLOUT != 0 && sock.can_send() {
                    revents |= poll_events::POLLOUT;
                }
            }
        }
        FdKind::Epoll(_) | FdKind::Signalfd(_) | FdKind::Inotify(_) => {}
    }
    revents
}

/// poll - wait for events on file descriptors
pub fn poll(fds: *mut PollFd, nfds: u64, timeout_ms: i32) -> LinuxResult<i32> {
    if fds.is_null() && nfds > 0 {
        return Err(LinuxError::EFAULT);
    }

    let deadline = if timeout_ms >= 0 {
        Some(time::uptime_ns() + timeout_ms as u64 * 1_000_000)
    } else {
        None
    };

    loop {
        let mut ready = 0i32;
        unsafe {
            for i in 0..nfds {
                let entry = &mut *fds.add(i as usize);
                entry.revents = poll_revents(entry.fd, entry.events);
                if entry.revents != 0 {
                    ready += 1;
                }
            }
        }

        if ready > 0 {
            return Ok(ready);
        }

        if timeout_ms == 0 {
            return Ok(0);
        }

        if let Some(deadline) = deadline {
            if time::uptime_ns() >= deadline {
                return Ok(0);
            }
        }

        let _ = process::scheduler::yield_cpu();
    }
}

/// epoll_create1 - create epoll instance
pub fn epoll_create1(_flags: i32) -> LinuxResult<i32> {
    let id = NEXT_EPOLL_ID.fetch_add(1, Ordering::SeqCst);
    EPOLL_BY_ID.write().insert(
        id,
        EpollState {
            entries: Vec::new(),
        },
    );
    register_special(FdKind::Epoll(id), OpenFlags::RDONLY)
}

/// epoll_ctl - modify interest list
pub fn epoll_ctl(epfd: i32, op: i32, fd: i32, event: *mut u8) -> LinuxResult<i32> {
    const EPOLL_CTL_ADD: i32 = 1;
    const EPOLL_CTL_DEL: i32 = 2;
    const EPOLL_CTL_MOD: i32 = 3;

    let FdKind::Epoll(epoll_id) = vfs::vfs_fd_kind(epfd).map_err(|_| LinuxError::EBADF)? else {
        return Err(LinuxError::EBADF);
    };

    let mut table = EPOLL_BY_ID.write();
    let state = table.get_mut(&epoll_id).ok_or(LinuxError::EBADF)?;

    match op {
        EPOLL_CTL_ADD => {
            if event.is_null() {
                return Err(LinuxError::EFAULT);
            }
            let events = unsafe { *(event as *const u32) };
            if state.entries.iter().any(|e| e.fd == fd) {
                return Err(LinuxError::EEXIST);
            }
            state.entries.push(EpollEntry { fd, events });
            Ok(0)
        }
        EPOLL_CTL_DEL => {
            state.entries.retain(|e| e.fd != fd);
            Ok(0)
        }
        EPOLL_CTL_MOD => {
            if event.is_null() {
                return Err(LinuxError::EFAULT);
            }
            let events = unsafe { *(event as *const u32) };
            if let Some(entry) = state.entries.iter_mut().find(|e| e.fd == fd) {
                entry.events = events;
                Ok(0)
            } else {
                Err(LinuxError::ENOENT)
            }
        }
        _ => Err(LinuxError::EINVAL),
    }
}

/// epoll_wait - wait for epoll events
pub fn epoll_wait(epfd: i32, events: *mut u8, maxevents: i32, timeout_ms: i32) -> LinuxResult<i32> {
    if events.is_null() || maxevents <= 0 {
        return Err(LinuxError::EINVAL);
    }

    let FdKind::Epoll(epoll_id) = vfs::vfs_fd_kind(epfd).map_err(|_| LinuxError::EBADF)? else {
        return Err(LinuxError::EBADF);
    };

    let deadline = if timeout_ms >= 0 {
        Some(time::uptime_ns() + timeout_ms as u64 * 1_000_000)
    } else {
        None
    };

    loop {
        let state = EPOLL_BY_ID.read().get(&epoll_id).cloned();
        let Some(state) = state else {
            return Err(LinuxError::EBADF);
        };

        let mut out = 0i32;
        for entry in &state.entries {
            if out >= maxevents {
                break;
            }
            let revents = poll_revents(entry.fd, entry.events as i16) as u32;
            if revents != 0 {
                unsafe {
                    let off = out as usize * 12;
                    *(events.add(off) as *mut u32) = entry.events;
                    *(events.add(off + 4) as *mut u64) = entry.fd as u64;
                    *(events.add(off + 8) as *mut u32) = revents;
                }
                out += 1;
            }
        }

        if out > 0 {
            return Ok(out);
        }

        if timeout_ms == 0 {
            return Ok(0);
        }

        if let Some(deadline) = deadline {
            if time::uptime_ns() >= deadline {
                return Ok(0);
            }
        }

        let _ = process::scheduler::yield_cpu();
    }
}

/// pipe - create pipe with VFS fds
pub fn pipe(pipefd: *mut [i32; 2]) -> LinuxResult<i32> {
    if pipefd.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let ipc = get_ipc_manager();
    let (pipe_id, _) = ipc.create_pipe().map_err(|_| LinuxError::EMFILE)?;

    let read_fd = register_special(FdKind::PipeRead(pipe_id), OpenFlags::RDONLY)?;
    let write_fd = register_special(FdKind::PipeWrite(pipe_id), OpenFlags::WRONLY)?;

    unsafe {
        (*pipefd)[0] = read_fd;
        (*pipefd)[1] = write_fd;
    }
    Ok(0)
}

/// pipe2 - create pipe (flags ignored for now)
pub fn pipe2(pipefd: *mut [i32; 2], _flags: i32) -> LinuxResult<i32> {
    pipe(pipefd)
}

/// eventfd2 - create eventfd as VFS fd
pub fn eventfd2(initval: u32, flags: i32) -> LinuxResult<i32> {
    let id = NEXT_EVENT_ID.fetch_add(1, Ordering::SeqCst);
    EVENTFD_BY_ID.write().insert(
        id,
        EventFdState {
            value: AtomicU64::new(initval as u64),
            flags,
        },
    );
    register_special(FdKind::EventFd(id), OpenFlags::RDWR)
}

/// timerfd_create - create timerfd as VFS fd
pub fn timerfd_create(clockid: i32, flags: i32) -> LinuxResult<i32> {
    use super::types::clock;
    if clockid != clock::CLOCK_REALTIME && clockid != clock::CLOCK_MONOTONIC {
        return Err(LinuxError::EINVAL);
    }
    let id = NEXT_TIMER_ID.fetch_add(1, Ordering::SeqCst);
    TIMERFD_BY_ID.write().insert(
        id,
        TimerFdState {
            expires_ns: AtomicU64::new(0),
            interval_ns: 0,
            armed: AtomicU64::new(0),
        },
    );
    let _ = flags;
    register_special(FdKind::TimerFd(id), OpenFlags::RDWR)
}

#[repr(C)]
#[derive(Clone, Copy)]
struct ITimerSpec {
    it_interval_sec: u64,
    it_interval_nsec: u64,
    it_value_sec: u64,
    it_value_nsec: u64,
}

/// timerfd_settime - arm timer
pub fn timerfd_settime(
    fd: i32,
    _flags: i32,
    new_value: *const u8,
    old_value: *mut u8,
) -> LinuxResult<i32> {
    if new_value.is_null() {
        return Err(LinuxError::EFAULT);
    }
    let FdKind::TimerFd(id) = vfs::vfs_fd_kind(fd).map_err(|_| LinuxError::EBADF)? else {
        return Err(LinuxError::EBADF);
    };

    let spec = unsafe { *(new_value as *const ITimerSpec) };
    let mut table = TIMERFD_BY_ID.write();
    let timer = table.get_mut(&id).ok_or(LinuxError::EBADF)?;

    if !old_value.is_null() {
        unsafe {
            *(old_value as *mut ITimerSpec) = ITimerSpec {
                it_interval_sec: timer.interval_ns / 1_000_000_000,
                it_interval_nsec: timer.interval_ns % 1_000_000_000,
                it_value_sec: 0,
                it_value_nsec: 0,
            };
        }
    }

    timer.interval_ns = spec.it_interval_sec * 1_000_000_000 + spec.it_interval_nsec;
    let value_ns = spec.it_value_sec * 1_000_000_000 + spec.it_value_nsec;
    if value_ns == 0 {
        timer.armed.store(0, Ordering::SeqCst);
    } else {
        timer
            .expires_ns
            .store(time::uptime_ns() + value_ns, Ordering::SeqCst);
        timer.armed.store(1, Ordering::SeqCst);
    }
    Ok(0)
}

/// timerfd_gettime - read timer state
pub fn timerfd_gettime(fd: i32, curr_value: *mut u8) -> LinuxResult<i32> {
    if curr_value.is_null() {
        return Err(LinuxError::EFAULT);
    }
    let FdKind::TimerFd(id) = vfs::vfs_fd_kind(fd).map_err(|_| LinuxError::EBADF)? else {
        return Err(LinuxError::EBADF);
    };
    let table = TIMERFD_BY_ID.read();
    let timer = table.get(&id).ok_or(LinuxError::EBADF)?;
    unsafe {
        *(curr_value as *mut ITimerSpec) = ITimerSpec {
            it_interval_sec: timer.interval_ns / 1_000_000_000,
            it_interval_nsec: timer.interval_ns % 1_000_000_000,
            it_value_sec: 0,
            it_value_nsec: 0,
        };
    }
    Ok(0)
}
