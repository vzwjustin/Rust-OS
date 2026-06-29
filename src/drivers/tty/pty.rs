//! Pseudoterminal master/slave pairs wired to N_TTY line discipline.

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::Mutex;

use super::{n_tty, Termios, TtyPort, WinSize};
use crate::linux_compat::{LinuxError, LinuxResult};
use crate::vfs::{self, InodeType, VfsError, VfsResult};

static NEXT_PTY_ID: AtomicU32 = AtomicU32::new(0);

/// Legacy synthetic fd -> (pty_id, is_master) for posix_openpt/openpty syscalls.
static LEGACY_FD_MAP: Mutex<BTreeMap<i32, (u32, bool)>> = Mutex::new(BTreeMap::new());

struct PtyPair {
    id: u32,
    master: TtyPort,
    slave: TtyPort,
    unlocked: bool,
    granted: bool,
    session_id: i32,
    foreground_pgrp: i32,
}

static PTY_REGISTRY: Mutex<BTreeMap<u32, PtyPair>> = Mutex::new(BTreeMap::new());

pub fn init() {
    NEXT_PTY_ID.store(0, Ordering::Relaxed);
    PTY_REGISTRY.lock().clear();
    LEGACY_FD_MAP.lock().clear();
}

fn current_pid() -> i32 {
    crate::process::current_pid() as i32
}

fn with_pair<F, R>(id: u32, f: F) -> Option<R>
where
    F: FnOnce(&mut PtyPair) -> R,
{
    PTY_REGISTRY.lock().get_mut(&id).map(f)
}

/// Create a new PTY pair; returns (id, master_fd, slave_fd) for legacy syscall API.
pub fn create_pair() -> LinuxResult<(u32, i32, i32)> {
    let id = NEXT_PTY_ID.fetch_add(1, Ordering::SeqCst);
    let termios = Termios::default();
    let pair = PtyPair {
        id,
        master: TtyPort::new(termios),
        slave: TtyPort::new(termios),
        unlocked: false,
        granted: false,
        session_id: current_pid(),
        foreground_pgrp: current_pid(),
    };
    PTY_REGISTRY.lock().insert(id, pair);

    let master_fd = allocate_legacy_fd(id, true)?;
    let slave_fd = allocate_legacy_fd(id, false)?;

    install_pts_node(id).map_err(|_| LinuxError::EIO)?;

    Ok((id, master_fd, slave_fd))
}

fn allocate_legacy_fd(id: u32, is_master: bool) -> LinuxResult<i32> {
    static NEXT_FD: AtomicU32 = AtomicU32::new(0x4000);
    loop {
        let fd = NEXT_FD.fetch_add(1, Ordering::SeqCst) as i32;
        if fd < 0 {
            continue;
        }
        let mut map = LEGACY_FD_MAP.lock();
        if !map.contains_key(&fd) {
            map.insert(fd, (id, is_master));
            return Ok(fd);
        }
    }
}

pub fn legacy_lookup(fd: i32) -> Option<(u32, bool)> {
    LEGACY_FD_MAP.lock().get(&fd).copied()
}

pub fn dup_legacy_fd(oldfd: i32, newfd: i32) -> LinuxResult<i32> {
    let mapping = legacy_lookup(oldfd).ok_or(LinuxError::EBADF)?;
    LEGACY_FD_MAP.lock().insert(newfd, mapping);
    Ok(newfd)
}

pub fn try_read_legacy_fd(fd: i32, buf: &mut [u8]) -> Option<LinuxResult<isize>> {
    let (id, is_master) = legacy_lookup(fd)?;
    if is_master {
        Some(read_master(id, buf))
    } else {
        Some(read_slave(id, buf))
    }
}

pub fn try_write_legacy_fd(fd: i32, buf: &[u8]) -> Option<LinuxResult<isize>> {
    let (id, is_master) = legacy_lookup(fd)?;
    if is_master {
        Some(write_master(id, buf))
    } else {
        Some(write_slave(id, buf))
    }
}

pub fn read_master(id: u32, buf: &mut [u8]) -> LinuxResult<isize> {
    let n = with_pair(id, |pair| {
        let mut echo = None;
        n_tty::tty_read(&mut pair.master, buf, &mut echo)
    })
    .ok_or(LinuxError::ENOTTY)?;
    if n == 0 {
        return Err(LinuxError::EAGAIN);
    }
    Ok(n as isize)
}

pub fn write_master(id: u32, buf: &[u8]) -> LinuxResult<isize> {
    with_pair(id, |pair| {
        let mut echo = None;
        n_tty::tty_push_input(&mut pair.slave, buf, &mut echo);
    })
    .ok_or(LinuxError::ENOTTY)?;
    Ok(buf.len() as isize)
}

pub fn read_slave(id: u32, buf: &mut [u8]) -> LinuxResult<isize> {
    let n = with_pair(id, |pair| {
        let mut echo = None;
        n_tty::tty_read(&mut pair.slave, buf, &mut echo)
    })
    .ok_or(LinuxError::ENOTTY)?;
    if n == 0 {
        return Err(LinuxError::EAGAIN);
    }
    Ok(n as isize)
}

pub fn write_slave(id: u32, buf: &[u8]) -> LinuxResult<isize> {
    with_pair(id, |pair| {
        let processed = n_tty::process_output(pair.slave.termios, buf);
        pair.master.read_buf.extend_from_slice(&processed);
    })
    .ok_or(LinuxError::ENOTTY)?;
    Ok(buf.len() as isize)
}

pub fn pending_read(id: u32, is_master: bool) -> usize {
    with_pair(id, |pair| {
        if is_master {
            pair.master.pending_read()
        } else {
            pair.slave.pending_read()
        }
    })
    .unwrap_or(0)
}

pub fn get_termios(id: u32, is_master: bool) -> Option<Termios> {
    with_pair(id, |pair| {
        if is_master {
            pair.master.termios
        } else {
            pair.slave.termios
        }
    })
}

pub fn set_termios(id: u32, is_master: bool, termios: Termios) -> LinuxResult<()> {
    with_pair(id, |pair| {
        if is_master {
            pair.master.termios = termios;
        } else {
            pair.slave.termios = termios;
        }
    })
    .ok_or(LinuxError::ENOTTY)?;
    Ok(())
}

pub fn get_winsize(id: u32) -> Option<WinSize> {
    with_pair(id, |pair| pair.slave.winsize)
}

pub fn set_winsize(id: u32, winsize: WinSize) -> LinuxResult<()> {
    with_pair(id, |pair| {
        pair.master.winsize = winsize;
        pair.slave.winsize = winsize;
    })
    .ok_or(LinuxError::ENOTTY)?;
    Ok(())
}

pub fn flush(id: u32, is_master: bool, queue: i32) -> LinuxResult<()> {
    with_pair(id, |pair| {
        if is_master {
            match queue {
                0 => n_tty::tty_flush(&mut pair.master, 0),
                1 => n_tty::tty_flush(&mut pair.slave, 1),
                2 => {
                    n_tty::tty_flush(&mut pair.master, 0);
                    n_tty::tty_flush(&mut pair.slave, 1);
                }
                _ => {}
            }
        } else {
            match queue {
                0 => n_tty::tty_flush(&mut pair.slave, 0),
                1 => n_tty::tty_flush(&mut pair.master, 1),
                2 => {
                    n_tty::tty_flush(&mut pair.slave, 0);
                    n_tty::tty_flush(&mut pair.master, 1);
                }
                _ => {}
            }
        }
    })
    .ok_or(LinuxError::ENOTTY)?;
    Ok(())
}

pub fn flow(id: u32, is_master: bool, action: i32) -> LinuxResult<()> {
    with_pair(id, |pair| {
        if is_master {
            n_tty::tty_flow(&mut pair.slave, action);
        } else {
            n_tty::tty_flow(&mut pair.master, action);
        }
    })
    .ok_or(LinuxError::ENOTTY)?;
    Ok(())
}

pub fn drain(id: u32, is_master: bool) -> LinuxResult<()> {
    with_pair(id, |pair| {
        if is_master {
            pair.slave.raw_in.clear();
        } else {
            pair.master.read_buf.clear();
        }
    })
    .ok_or(LinuxError::ENOTTY)?;
    Ok(())
}

pub fn set_unlocked(id: u32) -> LinuxResult<()> {
    with_pair(id, |pair| pair.unlocked = true).ok_or(LinuxError::ENOTTY)?;
    Ok(())
}

pub fn set_granted(id: u32) -> LinuxResult<()> {
    with_pair(id, |pair| pair.granted = true).ok_or(LinuxError::ENOTTY)?;
    Ok(())
}

pub fn is_unlocked(id: u32) -> bool {
    with_pair(id, |pair| pair.unlocked).unwrap_or(false)
}

pub fn get_session(id: u32) -> Option<i32> {
    with_pair(id, |pair| pair.session_id)
}

pub fn get_foreground(id: u32) -> Option<i32> {
    with_pair(id, |pair| pair.foreground_pgrp)
}

pub fn set_foreground(id: u32, pgrp: i32) -> LinuxResult<()> {
    with_pair(id, |pair| pair.foreground_pgrp = pgrp).ok_or(LinuxError::ENOTTY)?;
    Ok(())
}

pub fn slave_name(id: u32) -> String {
    format!("/dev/pts/{}", id)
}

/// Open /dev/ptmx: create pair and return VFS fd for master.
pub fn open_ptmx(flags: u32) -> VfsResult<i32> {
    let (id, _legacy_master, _legacy_slave) = create_pair().map_err(|_| VfsError::IoError)?;
    with_pair(id, |pair| {
        pair.unlocked = true;
        pair.granted = true;
    });
    let root = vfs::get_vfs().lookup("/")?;
    let inode = root.lookup("dev")?.lookup("ptmx")?;
    let open_flags = vfs::OpenFlags::new(flags);
    vfs::vfs_open_special(inode, open_flags.bits(), vfs::FdKind::PtyMaster(id))
}

/// Open /dev/pts/N slave if the pair exists.
pub fn open_pts_slave(id: u32, flags: u32) -> VfsResult<i32> {
    if !PTY_REGISTRY.lock().contains_key(&id) {
        return Err(VfsError::NotFound);
    }
    let path = format!("/dev/pts/{}", id);
    let inode = vfs::get_vfs().lookup(&path)?;
    let open_flags = vfs::OpenFlags::new(flags);
    vfs::vfs_open_special(inode, open_flags.bits(), vfs::FdKind::PtySlave(id))
}

fn install_pts_node(id: u32) -> VfsResult<()> {
    let pts_dir = vfs::get_vfs().lookup("/dev/pts")?;
    let name = format!("{}", id);
    if pts_dir.lookup(&name).is_ok() {
        return Ok(());
    }
    let inode = PtySlaveInode::new(id);
    pts_dir.attach_child(&name, inode)
}

/// Dynamic /dev/pts/N inode.
struct PtySlaveInode {
    ino: u64,
    id: u32,
}

impl PtySlaveInode {
    fn new(id: u32) -> Arc<Self> {
        static NEXT_INO: AtomicU32 = AtomicU32::new(20_000);
        Arc::new(Self {
            ino: NEXT_INO.fetch_add(1, Ordering::Relaxed) as u64,
            id,
        })
    }
}

impl vfs::InodeOps for PtySlaveInode {
    fn read_at(&self, _offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        match read_slave(self.id, buf) {
            Ok(n) if n > 0 => Ok(n as usize),
            Ok(_) => Ok(0),
            Err(_) => Ok(0),
        }
    }

    fn write_at(&self, _offset: u64, buf: &[u8]) -> VfsResult<usize> {
        match write_slave(self.id, buf) {
            Ok(n) if n >= 0 => Ok(n as usize),
            _ => Err(VfsError::IoError),
        }
    }

    fn stat(&self) -> VfsResult<vfs::Stat> {
        Ok(vfs::Stat {
            ino: self.ino,
            inode_type: InodeType::CharDevice,
            size: 0,
            blksize: 4096,
            blocks: 0,
            mode: 0o620,
            nlink: 1,
            uid: 0,
            gid: 0,
            rdev: ((136u64) << 8) | self.id as u64,
            atime: 0,
            mtime: 0,
            ctime: 0,
        })
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Err(VfsError::NotSupported)
    }

    fn sync(&self) -> VfsResult<()> {
        Ok(())
    }

    fn lookup(&self, _name: &str) -> VfsResult<Arc<dyn vfs::InodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn create(
        &self,
        _name: &str,
        _inode_type: InodeType,
        _mode: u32,
    ) -> VfsResult<Arc<dyn vfs::InodeOps>> {
        Err(VfsError::ReadOnly)
    }

    fn unlink(&self, _name: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn link(&self, _name: &str, _target: Arc<dyn vfs::InodeOps>) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn rename(
        &self,
        _old_name: &str,
        _new_dir: Arc<dyn vfs::InodeOps>,
        _new_name: &str,
    ) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn readdir(&self) -> VfsResult<Vec<vfs::DirEntry>> {
        Err(VfsError::NotDirectory)
    }

    fn inode_type(&self) -> InodeType {
        InodeType::CharDevice
    }

    fn attach_child(&self, _name: &str, _child: Arc<dyn vfs::InodeOps>) -> VfsResult<()> {
        Err(VfsError::NotDirectory)
    }
}
