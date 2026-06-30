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
static NEXT_SIGNALFD_ID: AtomicU32 = AtomicU32::new(1);

/// Initialize special fd subsystem.
///
/// Clears all per-fd state tables and resets the ID counters so the
/// subsystem starts from a known empty state. This is idempotent and safe
/// to call during both early boot and subsystem restart.
pub fn init_special_fd() {
    EVENTFD_BY_ID.write().clear();
    TIMERFD_BY_ID.write().clear();
    EPOLL_BY_ID.write().clear();
    SIGNALFD_BY_ID.write().clear();
    NEXT_EVENT_ID.store(1, Ordering::SeqCst);
    NEXT_TIMER_ID.store(1, Ordering::SeqCst);
    NEXT_EPOLL_ID.store(1, Ordering::SeqCst);
    NEXT_SIGNALFD_ID.store(1, Ordering::SeqCst);
}

struct EventFdState {
    value: AtomicU64,
    flags: i32,
}

struct TimerFdState {
    expires_ns: AtomicU64,
    interval_ns: u64,
    armed: AtomicU64,
    /// Number of expirations since last successful read.
    overrun: AtomicU64,
}

/// State for a signalfd: the signal mask to watch and the owning pid.
struct SignalFdState {
    mask: AtomicU64,
    pid: u32,
}

#[derive(Clone)]
struct EpollEntry {
    fd: i32,
    events: u32,
    data: u64,
}

#[derive(Clone)]
struct EpollState {
    entries: Vec<EpollEntry>,
}

static EVENTFD_BY_ID: RwLock<BTreeMap<u32, EventFdState>> = RwLock::new(BTreeMap::new());
static TIMERFD_BY_ID: RwLock<BTreeMap<u32, TimerFdState>> = RwLock::new(BTreeMap::new());
static EPOLL_BY_ID: RwLock<BTreeMap<u32, EpollState>> = RwLock::new(BTreeMap::new());
static SIGNALFD_BY_ID: RwLock<BTreeMap<u32, SignalFdState>> = RwLock::new(BTreeMap::new());

fn root_inode() -> alloc::sync::Arc<dyn vfs::InodeOps> {
    vfs::get_vfs().lookup("/").expect("root")
}

pub fn register_special(kind: FdKind, flags: u32) -> LinuxResult<i32> {
    let inode = root_inode();
    vfs::vfs_open_special(inode, flags, kind).map_err(|_| LinuxError::EMFILE)
}

/// Register an inotify instance as a special fd.
pub fn register_inotify(id: u32, flags: u32) -> i32 {
    match register_special(FdKind::Inotify(id), flags) {
        Ok(fd) => fd,
        Err(_) => -12, // ENFILE
    }
}

/// Get the inotify instance ID from a file descriptor.
pub fn get_inotify_id(fd: i32) -> Option<u32> {
    match vfs::vfs_fd_kind(fd) {
        Ok(FdKind::Inotify(id)) => Some(id),
        _ => None,
    }
}

/// Register a pidfd as a special fd.
pub fn register_pidfd(id: u32, flags: u32) -> i32 {
    match register_special(FdKind::Pidfd(id), flags) {
        Ok(fd) => fd,
        Err(_) => -12, // ENFILE
    }
}

/// Get the pidfd instance ID from a file descriptor.
pub fn get_pidfd_id(fd: i32) -> Option<u32> {
    match vfs::vfs_fd_kind(fd) {
        Ok(FdKind::Pidfd(id)) => Some(id),
        _ => None,
    }
}

/// Register an io_uring instance as a special fd.
pub fn register_io_uring(id: u32, flags: u32) -> i32 {
    match register_special(FdKind::IoUring(id), flags) {
        Ok(fd) => fd,
        Err(_) => -12,
    }
}

/// Get the io_uring instance ID from a file descriptor.
pub fn get_io_uring_id(fd: i32) -> Option<u32> {
    match vfs::vfs_fd_kind(fd) {
        Ok(FdKind::IoUring(id)) => Some(id),
        _ => None,
    }
}

/// Register a fanotify instance as a special fd.
pub fn register_fanotify(id: u32, flags: u32) -> i32 {
    match register_special(FdKind::Fanotify(id), flags) {
        Ok(fd) => fd,
        Err(_) => -12,
    }
}

/// Get the fanotify instance ID from a file descriptor.
pub fn get_fanotify_id(fd: i32) -> Option<u32> {
    match vfs::vfs_fd_kind(fd) {
        Ok(FdKind::Fanotify(id)) => Some(id),
        _ => None,
    }
}

/// Register a fs context as a special fd.
pub fn register_fs_context(id: u32, flags: u32) -> i32 {
    match register_special(FdKind::FsContext(id), flags) {
        Ok(fd) => fd,
        Err(_) => -12,
    }
}

/// Get the fs context ID from a file descriptor.
pub fn get_fs_context_id(fd: i32) -> Option<u32> {
    match vfs::vfs_fd_kind(fd) {
        Ok(FdKind::FsContext(id)) => Some(id),
        _ => None,
    }
}

/// Register a mount object as a special fd.
pub fn register_mount_object(id: u32, flags: u32) -> i32 {
    match register_special(FdKind::MountObject(id), flags) {
        Ok(fd) => fd,
        Err(_) => -12,
    }
}

/// Get the mount object ID from a file descriptor.
pub fn get_mount_object_id(fd: i32) -> Option<u32> {
    match vfs::vfs_fd_kind(fd) {
        Ok(FdKind::MountObject(id)) => Some(id),
        _ => None,
    }
}

/// Register a Landlock ruleset as a special fd.
pub fn register_landlock_ruleset(id: u32, flags: u32) -> i32 {
    match register_special(FdKind::LandlockRuleset(id), flags) {
        Ok(fd) => fd,
        Err(_) => -12,
    }
}

/// Get the Landlock ruleset ID from a file descriptor.
pub fn get_landlock_ruleset_id(fd: i32) -> Option<u32> {
    match vfs::vfs_fd_kind(fd) {
        Ok(FdKind::LandlockRuleset(id)) => Some(id),
        _ => None,
    }
}

/// Register a BPF map as a special fd.
pub fn register_bpf_map(id: u32, flags: u32) -> i32 {
    match register_special(FdKind::BpfMap(id), flags) {
        Ok(fd) => fd,
        Err(_) => -12,
    }
}

/// Get the BPF map ID from a file descriptor.
pub fn get_bpf_map_id(fd: i32) -> Option<u32> {
    match vfs::vfs_fd_kind(fd) {
        Ok(FdKind::BpfMap(id)) => Some(id),
        _ => None,
    }
}

/// Register a BPF program as a special fd.
pub fn register_bpf_prog(id: u32, flags: u32) -> i32 {
    match register_special(FdKind::BpfProg(id), flags) {
        Ok(fd) => fd,
        Err(_) => -12,
    }
}

/// Get the BPF program ID from a file descriptor.
pub fn get_bpf_prog_id(fd: i32) -> Option<u32> {
    match vfs::vfs_fd_kind(fd) {
        Ok(FdKind::BpfProg(id)) => Some(id),
        _ => None,
    }
}

/// Register a perf event as a special fd.
pub fn register_perf_event(id: u32, flags: u32) -> i32 {
    match register_special(FdKind::PerfEvent(id), flags) {
        Ok(fd) => fd,
        Err(_) => -12,
    }
}

/// Get the perf event ID from a file descriptor.
pub fn get_perf_event_id(fd: i32) -> Option<u32> {
    match vfs::vfs_fd_kind(fd) {
        Ok(FdKind::PerfEvent(id)) => Some(id),
        _ => None,
    }
}

/// Register userfaultfd instance special fd.
pub fn register_userfaultfd(id: u32, flags: u32) -> i32 {
    match register_special(FdKind::Userfaultfd(id), flags) {
        Ok(fd) => fd,
        Err(_) => -12,
    }
}

/// Get userfaultfd instance ID from file descriptor.
pub fn get_userfaultfd_id(fd: i32) -> Option<u32> {
    match vfs::vfs_fd_kind(fd) {
        Ok(FdKind::Userfaultfd(id)) => Some(id),
        _ => None,
    }
}

/// Register memfd_secret special fd.
pub fn register_memfd_secret(id: u32, flags: u32) -> i32 {
    match register_special(FdKind::MemfdSecret(id), flags) {
        Ok(fd) => fd,
        Err(_) => -12,
    }
}

/// Get memfd_secret ID from file descriptor.
pub fn get_memfd_secret_id(fd: i32) -> Option<u32> {
    match vfs::vfs_fd_kind(fd) {
        Ok(FdKind::MemfdSecret(id)) => Some(id),
        _ => None,
    }
}

/// Register namespace handle special fd.
pub fn register_namespace(id: u32, flags: u32) -> i32 {
    match register_special(FdKind::Namespace(id), flags) {
        Ok(fd) => fd,
        Err(_) => -12,
    }
}

/// Get namespace handle ID from file descriptor.
pub fn get_namespace_id(fd: i32) -> Option<u32> {
    match vfs::vfs_fd_kind(fd) {
        Ok(FdKind::Namespace(id)) => Some(id),
        _ => None,
    }
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
            let is_semaphore = (event.flags & 0x1) != 0;
            if is_semaphore {
                // Semaphore mode: decrement by 1, return 1
                loop {
                    let val = event.value.load(Ordering::SeqCst);
                    if val == 0 {
                        return Some(Err(LinuxError::EAGAIN));
                    }
                    if event
                        .value
                        .compare_exchange(val, val - 1, Ordering::SeqCst, Ordering::SeqCst)
                        .is_ok()
                    {
                        buf[..8].copy_from_slice(&1u64.to_le_bytes());
                        return Some(Ok(8));
                    }
                }
            } else {
                // Normal mode: read counter and reset to 0
                let val = event.value.swap(0, Ordering::SeqCst);
                if val == 0 {
                    return Some(Err(LinuxError::EAGAIN));
                }
                buf[..8].copy_from_slice(&val.to_le_bytes());
                Some(Ok(8))
            }
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
            // Count how many intervals have passed since expiry.
            let expires = timer.expires_ns.load(Ordering::SeqCst);
            let interval = timer.interval_ns;
            let expirations = if interval > 0 {
                let elapsed = now - expires;
                1 + elapsed / interval
            } else {
                1
            };
            // Advance expiry to the next interval boundary (or disarm if one-shot).
            if interval > 0 {
                let next = expires + interval * expirations;
                timer.expires_ns.store(next, Ordering::SeqCst);
                timer.armed.store(1, Ordering::SeqCst);
            } else {
                timer.armed.store(0, Ordering::SeqCst);
            }
            timer.overrun.store(0, Ordering::SeqCst);
            buf[..8].copy_from_slice(&(expirations as u64).to_le_bytes());
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
        FdKind::Signalfd(id) => {
            // signalfd read returns one or more signalfd_siginfo structs.
            // Each is 128 bytes. We return one signal at a time.
            if buf.len() < 128 {
                return Some(Err(LinuxError::EINVAL));
            }
            let table = SIGNALFD_BY_ID.read();
            let state = table.get(&id)?;
            let mask = state.mask.load(Ordering::SeqCst);
            let pid = state.pid;

            // Check pending signals against the signalfd mask
            let process_manager = process::get_process_manager();
            if let Some(pcb) = process_manager.get_process(pid) {
                for &sig in &pcb.pending_signals {
                    let bit = 1u64 << ((sig - 1) & 63);
                    if (mask & bit) != 0 {
                        // Consume this signal
                        drop(table);
                        process_manager.with_process_mut(pid, |p| {
                            p.pending_signals.retain(|s| *s != sig);
                        });
                        // Write signalfd_siginfo (128 bytes, simplified)
                        // Fields: ssi_signo(4), ssi_errno(4), ssi_code(4),
                        // ssi_pid(4), ssi_uid(4), ssi_fd(4), ssi_tid(4),
                        // ssi_band(8), ssi_overrun(4), ssi_trapno(4),
                        // ssi_status(4), ssi_int(4), ssi_ptr(8), ssi_utime(8),
                        // ssi_stime(8), ssi_addr(8), ... padding to 128
                        let mut info = [0u8; 128];
                        info[..4].copy_from_slice(&(sig as u32).to_ne_bytes());
                        // ssi_pid = sender (0 for kernel)
                        info[20..24].copy_from_slice(&0u32.to_ne_bytes());
                        info[24..28].copy_from_slice(&0u32.to_ne_bytes()); // ssi_uid
                        buf[..128].copy_from_slice(&info);
                        return Some(Ok(128));
                    }
                }
            }
            Some(Err(LinuxError::EAGAIN))
        }
        FdKind::Fanotify(_) => {
            let n = crate::fanotify::read_events(fd, buf);
            if n >= 0 {
                Some(Ok(n))
            } else {
                Some(Err(match -n {
                    9 => LinuxError::EBADF,
                    11 => LinuxError::EAGAIN,
                    _ => LinuxError::EINVAL,
                }))
            }
        }
        FdKind::PerfEvent(_) => {
            let n = crate::perf_event::read_event(fd, buf);
            if n >= 0 {
                Some(Ok(n))
            } else {
                Some(Err(match -n {
                    9 => LinuxError::EBADF,
                    22 => LinuxError::EINVAL,
                    _ => LinuxError::EIO,
                }))
            }
        }
        FdKind::Userfaultfd(id) => Some(crate::userfaultfd::read_events(id, buf)),
        FdKind::MemfdSecret(_) => Some(Err(LinuxError::EINVAL)),
        FdKind::Inotify(_) => Some(super::fs_ops::read_inotify_events(fd, buf)),
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
            // Linux eventfd: the counter is u64 and must not overflow.
            // If the add would cause overflow, block or return EAGAIN.
            // Use a CAS loop to handle concurrent writes safely.
            loop {
                let current = event.value.load(Ordering::SeqCst);
                match current.checked_add(add) {
                    Some(val) if val != u64::MAX => {
                        if event
                            .value
                            .compare_exchange(current, val, Ordering::SeqCst, Ordering::SeqCst)
                            .is_ok()
                        {
                            return Some(Ok(8));
                        }
                        // Retry on contention
                    }
                    _ => return Some(Err(LinuxError::EAGAIN)),
                }
            }
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
    if vfs::vfs_fd_ref_count(fd).unwrap_or(1) > 1 {
        return Some(Ok(()));
    }
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
        FdKind::Signalfd(id) => {
            SIGNALFD_BY_ID.write().remove(&id);
            Some(Ok(()))
        }
        FdKind::Pidfd(id) => {
            crate::pidfd::close_pidfd(id);
            Some(Ok(()))
        }
        FdKind::IoUring(id) => {
            crate::io_uring::close_ring(id);
            Some(Ok(()))
        }
        FdKind::Fanotify(id) => {
            crate::fanotify::close_instance(id);
            Some(Ok(()))
        }
        FdKind::Inotify(id) => {
            super::fs_ops::close_inotify(id);
            Some(Ok(()))
        }
        FdKind::LandlockRuleset(id) => {
            crate::landlock::close_ruleset(id);
            Some(Ok(()))
        }
        FdKind::BpfMap(id) => {
            crate::bpf::close_map(id);
            Some(Ok(()))
        }
        FdKind::BpfProg(id) => {
            crate::bpf::close_prog(id);
            Some(Ok(()))
        }
        FdKind::PerfEvent(id) => {
            crate::perf_event::close_event(id);
            Some(Ok(()))
        }
        FdKind::Userfaultfd(id) => {
            crate::userfaultfd::close_userfaultfd(id);
            Some(Ok(()))
        }
        FdKind::MemfdSecret(id) => {
            crate::memfd_secret::close_memfd_secret(id);
            Some(Ok(()))
        }
        FdKind::Namespace(id) => {
            crate::namespace::close_namespace_fd(id);
            Some(Ok(()))
        }
        FdKind::Socket(socket_id) => {
            if let Some(mut sock) = crate::net::network_stack().get_socket(socket_id) {
                let _ = sock.close();
            }
            crate::net::network_stack().close_socket(socket_id).ok();
            Some(Ok(()))
        }
        FdKind::MessageQueue(queue_id) => {
            super::ipc_ops::mq_release(queue_id);
            Some(Ok(()))
        }
        _ => None,
    }
}
pub fn poll_revents(fd: i32, events: i16) -> i16 {
    if fd < 0 {
        return poll_events::POLLNVAL;
    }

    // AF_UNIX sockets are backed by regular VFS fds, so the generic match below
    // would report them as perpetually readable. Report true readiness from the
    // underlying transport instead (the sink side is always writable).
    if crate::net::unix::is_unix_fd(fd) {
        let mut revents = 0i16;
        if events & poll_events::POLLIN != 0 && crate::net::unix::poll_readable(fd) {
            revents |= poll_events::POLLIN;
        }
        if events & poll_events::POLLOUT != 0 {
            revents |= poll_events::POLLOUT;
        }
        return revents;
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
            if let Some(event) = EVENTFD_BY_ID.read().get(&id) {
                if events & poll_events::POLLIN != 0 && event.value.load(Ordering::SeqCst) > 0 {
                    revents |= poll_events::POLLIN;
                }
                // eventfd is writable unless counter is at u64::MAX - 1
                if events & poll_events::POLLOUT != 0
                    && event.value.load(Ordering::SeqCst) < u64::MAX - 1
                {
                    revents |= poll_events::POLLOUT;
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
        FdKind::Signalfd(id) => {
            if events & poll_events::POLLIN != 0 {
                if let Some(state) = SIGNALFD_BY_ID.read().get(&id) {
                    let mask = state.mask.load(Ordering::SeqCst);
                    let pid = state.pid;
                    if let Some(pcb) = process::get_process_manager().get_process(pid) {
                        for &sig in &pcb.pending_signals {
                            let bit = 1u64 << ((sig - 1) & 63);
                            if (mask & bit) != 0 {
                                revents |= poll_events::POLLIN;
                                break;
                            }
                        }
                    }
                }
            }
        }
        FdKind::Pidfd(id) => {
            if events & poll_events::POLLIN != 0 {
                if let Some(pid) = crate::pidfd::get_pid_by_id(id) {
                    if process::get_process_manager().get_process(pid).is_none() {
                        revents |= poll_events::POLLIN;
                    }
                } else {
                    revents |= poll_events::POLLNVAL;
                }
            }
        }
        FdKind::Fanotify(id) => {
            if events & poll_events::POLLIN != 0 && crate::fanotify::has_events(id) {
                revents |= poll_events::POLLIN;
            }
        }
        FdKind::Userfaultfd(id) => {
            if events & poll_events::POLLIN != 0 && crate::userfaultfd::has_events(id) {
                revents |= poll_events::POLLIN;
            }
        }
        FdKind::TtyConsole => {
            if events & poll_events::POLLIN != 0 && crate::drivers::tty::console_pending_read() > 0
            {
                revents |= poll_events::POLLIN;
            }
            if events & poll_events::POLLOUT != 0 {
                revents |= poll_events::POLLOUT;
            }
        }
        FdKind::PtyMaster(id) => {
            if events & poll_events::POLLIN != 0
                && crate::drivers::tty::pty::pending_read(id, true) > 0
            {
                revents |= poll_events::POLLIN;
            }
            if events & poll_events::POLLOUT != 0 {
                revents |= poll_events::POLLOUT;
            }
        }
        FdKind::PtySlave(id) => {
            if events & poll_events::POLLIN != 0
                && crate::drivers::tty::pty::pending_read(id, false) > 0
            {
                revents |= poll_events::POLLIN;
            }
            if events & poll_events::POLLOUT != 0 {
                revents |= poll_events::POLLOUT;
            }
        }
        FdKind::Epoll(_)
        | FdKind::IoUring(_)
        | FdKind::FsContext(_)
        | FdKind::MountObject(_)
        | FdKind::LandlockRuleset(_)
        | FdKind::BpfMap(_)
        | FdKind::BpfProg(_)
        | FdKind::PerfEvent(_)
        | FdKind::MemfdSecret(_)
        | FdKind::Namespace(_) => {}
        FdKind::Inotify(id) => {
            if events & poll_events::POLLIN != 0 {
                if super::fs_ops::inotify_has_events(id) {
                    revents |= poll_events::POLLIN;
                }
            }
        }
        FdKind::MessageQueue(queue_id) => {
            if let Some((has_msgs, has_space)) = super::ipc_ops::mq_poll_state(queue_id) {
                if events & poll_events::POLLIN != 0 && has_msgs {
                    revents |= poll_events::POLLIN;
                }
                if events & poll_events::POLLOUT != 0 && has_space {
                    revents |= poll_events::POLLOUT;
                }
            } else {
                revents |= poll_events::POLLNVAL;
            }
        }
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
pub fn epoll_create1(flags: i32) -> LinuxResult<i32> {
    let id = NEXT_EPOLL_ID.fetch_add(1, Ordering::SeqCst);
    EPOLL_BY_ID.write().insert(
        id,
        EpollState {
            entries: Vec::new(),
        },
    );
    let mut fd_flags: u32 = OpenFlags::RDONLY;
    if (flags & 0o2000000) != 0 {
        // EPOLL_CLOEXEC
        fd_flags |= vfs::OpenFlags::CLOEXEC;
    }
    register_special(FdKind::Epoll(id), fd_flags)
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
            // struct epoll_event (packed, 12 bytes): { u32 events; u64 data; }
            let events = unsafe { *(event as *const u32) };
            let data = unsafe { *(event.add(4) as *const u64) };
            if state.entries.iter().any(|e| e.fd == fd) {
                return Err(LinuxError::EEXIST);
            }
            state.entries.push(EpollEntry { fd, events, data });
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
            let data = unsafe { *(event.add(4) as *const u64) };
            if let Some(entry) = state.entries.iter_mut().find(|e| e.fd == fd) {
                entry.events = events;
                entry.data = data;
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
                    // struct epoll_event (packed, 12 bytes):
                    //   offset 0: u32 events  (actual events that occurred)
                    //   offset 4: u64 data    (user data from epoll_ctl)
                    *(events.add(off) as *mut u32) = revents;
                    *(events.add(off + 4) as *mut u64) = entry.data;
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

/// pipe2 - create pipe with flags
pub fn pipe2(pipefd: *mut [i32; 2], flags: i32) -> LinuxResult<i32> {
    pipe(pipefd)?;
    // Apply O_CLOEXEC / O_NONBLOCK to both pipe ends.
    let pipefd_ref = unsafe { &*pipefd };
    let mut fd_flags: u32 = 0;
    if (flags & 0o2000000) != 0 {
        // O_CLOEXEC
        fd_flags |= vfs::OpenFlags::CLOEXEC;
    }
    if (flags & 0o20000) != 0 {
        // O_NONBLOCK
        fd_flags |= vfs::OpenFlags::NONBLOCK;
    }
    if fd_flags != 0 {
        let _ = vfs::vfs_set_fd_flags(pipefd_ref[0], fd_flags);
        let _ = vfs::vfs_set_fd_flags(pipefd_ref[1], fd_flags);
    }
    Ok(0)
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
    let mut fd_flags: u32 = OpenFlags::RDWR;
    if (flags & 0o2000000) != 0 {
        // EFD_CLOEXEC
        fd_flags |= vfs::OpenFlags::CLOEXEC;
    }
    if (flags & 0o20000) != 0 {
        // EFD_NONBLOCK
        fd_flags |= vfs::OpenFlags::NONBLOCK;
    }
    register_special(FdKind::EventFd(id), fd_flags)
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
            overrun: AtomicU64::new(0),
        },
    );
    let mut fd_flags: u32 = vfs::OpenFlags::RDWR;
    if (flags & 0o2000000) != 0 {
        // TFD_CLOEXEC
        fd_flags |= vfs::OpenFlags::CLOEXEC;
    }
    if (flags & 0o20000) != 0 {
        // TFD_NONBLOCK
        fd_flags |= vfs::OpenFlags::NONBLOCK;
    }
    register_special(FdKind::TimerFd(id), fd_flags)
}

/// signalfd - create or update a signalfd
///
/// Creates a new signalfd watching the signals in `mask`, or updates
/// the mask of an existing signalfd if `fd` is valid (>= 0). The mask
/// is a 64-bit signal set where bit N-1 corresponds to signal N.
pub fn signalfd(fd: i32, mask: u64, flags: i32) -> LinuxResult<i32> {
    // If fd is valid, update the existing signalfd's mask
    if fd >= 0 {
        let kind = vfs::vfs_fd_kind(fd).map_err(|_| LinuxError::EBADF)?;
        match kind {
            FdKind::Signalfd(id) => {
                if let Some(state) = SIGNALFD_BY_ID.write().get(&id) {
                    state.mask.store(mask, Ordering::SeqCst);
                    return Ok(fd);
                }
                return Err(LinuxError::EBADF);
            }
            _ => return Err(LinuxError::EINVAL),
        }
    }

    // Create a new signalfd
    let id = NEXT_SIGNALFD_ID.fetch_add(1, Ordering::SeqCst);
    let pid = process::current_pid();
    SIGNALFD_BY_ID.write().insert(
        id,
        SignalFdState {
            mask: AtomicU64::new(mask),
            pid,
        },
    );
    let mut fd_flags: u32 = OpenFlags::RDWR;
    if (flags & 0o2000000) != 0 {
        // SFD_CLOEXEC
        fd_flags |= vfs::OpenFlags::CLOEXEC;
    }
    if (flags & 0o20000) != 0 {
        // SFD_NONBLOCK
        fd_flags |= vfs::OpenFlags::NONBLOCK;
    }
    register_special(FdKind::Signalfd(id), fd_flags)
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
    flags: i32,
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
        let old_interval_ns = timer.interval_ns;
        let old_armed = timer.armed.load(Ordering::SeqCst);
        let old_expires = timer.expires_ns.load(Ordering::SeqCst);
        let now = time::uptime_ns();
        let old_remaining: u64 = if old_armed != 0 && old_expires > now {
            old_expires - now
        } else {
            0
        };
        unsafe {
            *(old_value as *mut ITimerSpec) = ITimerSpec {
                it_interval_sec: old_interval_ns / 1_000_000_000,
                it_interval_nsec: old_interval_ns % 1_000_000_000,
                it_value_sec: old_remaining / 1_000_000_000,
                it_value_nsec: old_remaining % 1_000_000_000,
            };
        }
    }

    timer.interval_ns = spec.it_interval_sec * 1_000_000_000 + spec.it_interval_nsec;
    let value_ns = spec.it_value_sec * 1_000_000_000 + spec.it_value_nsec;
    if value_ns == 0 {
        timer.armed.store(0, Ordering::SeqCst);
    } else {
        // TFD_TIMER_ABSTIME (1): value is an absolute time on the
        // clock; otherwise it is relative to now.
        let expires = if (flags & 1) != 0 {
            value_ns
        } else {
            time::uptime_ns() + value_ns
        };
        timer.expires_ns.store(expires, Ordering::SeqCst);
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
    let interval_ns = timer.interval_ns;
    let armed = timer.armed.load(Ordering::SeqCst);
    let expires = timer.expires_ns.load(Ordering::SeqCst);
    let now = time::uptime_ns();
    let remaining: u64 = if armed != 0 && expires > now {
        expires - now
    } else {
        0
    };
    unsafe {
        *(curr_value as *mut ITimerSpec) = ITimerSpec {
            it_interval_sec: interval_ns / 1_000_000_000,
            it_interval_nsec: interval_ns % 1_000_000_000,
            it_value_sec: remaining / 1_000_000_000,
            it_value_nsec: remaining % 1_000_000_000,
        };
    }
    Ok(0)
}
