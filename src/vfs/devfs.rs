//! Device nodes under `/dev` for the syscall-facing VFS.

extern crate alloc;

use alloc::sync::Arc;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use super::{InodeOps, InodeType, Stat, VfsError, VfsResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DevKind {
    Null,
    Zero,
    Random,
    URandom,
    Full,
    Console,
}

struct DevInode {
    ino: u64,
    kind: DevKind,
    mode: u32,
    prng: AtomicU64,
}

impl DevInode {
    fn new(ino: u64, kind: DevKind, mode: u32) -> Arc<Self> {
        Arc::new(Self {
            ino,
            kind,
            mode,
            prng: AtomicU64::new(0x1234_5678_9abc_def0),
        })
    }

    fn fill_random(&self, buf: &mut [u8]) {
        let mut state = self.prng.load(Ordering::Relaxed);
        for byte in buf.iter_mut() {
            state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            *byte = (state >> 16) as u8;
        }
        self.prng.store(state, Ordering::Relaxed);
    }
}

impl InodeOps for DevInode {
    fn read_at(&self, _offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        match self.kind {
            DevKind::Null => Ok(0),
            DevKind::Zero | DevKind::Full => {
                buf.fill(0);
                Ok(buf.len())
            }
            DevKind::Random | DevKind::URandom => {
                self.fill_random(buf);
                Ok(buf.len())
            }
            DevKind::Console => Ok(0),
        }
    }

    fn write_at(&self, _offset: u64, buf: &[u8]) -> VfsResult<usize> {
        match self.kind {
            DevKind::Null | DevKind::Zero | DevKind::Random | DevKind::URandom => Ok(buf.len()),
            DevKind::Full => Err(VfsError::NoSpace),
            DevKind::Console => {
                if let Ok(text) = core::str::from_utf8(buf) {
                    crate::serial_print!("{text}");
                } else {
                    for &b in buf {
                        crate::serial_print!("{}", b as char);
                    }
                }
                Ok(buf.len())
            }
        }
    }

    fn stat(&self) -> VfsResult<Stat> {
        let (major, minor) = match self.kind {
            DevKind::Null => (1, 3),
            DevKind::Zero => (1, 5),
            DevKind::Random => (1, 8),
            DevKind::URandom => (1, 9),
            DevKind::Full => (1, 7),
            DevKind::Console => (5, 1),
        };
        Ok(Stat {
            ino: self.ino,
            inode_type: InodeType::CharDevice,
            size: 0,
            blksize: 4096,
            blocks: 0,
            mode: self.mode,
            nlink: 1,
            uid: 0,
            gid: 0,
            rdev: ((major as u64) << 8) | minor as u64,
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

    fn lookup(&self, _name: &str) -> VfsResult<Arc<dyn InodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn create(
        &self,
        _name: &str,
        _inode_type: InodeType,
        _mode: u32,
    ) -> VfsResult<Arc<dyn InodeOps>> {
        Err(VfsError::ReadOnly)
    }

    fn unlink(&self, _name: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn link(&self, _name: &str, _target: Arc<dyn InodeOps>) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn rename(
        &self,
        _old_name: &str,
        _new_dir: Arc<dyn InodeOps>,
        _new_name: &str,
    ) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn readdir(&self) -> VfsResult<alloc::vec::Vec<super::DirEntry>> {
        Err(VfsError::NotDirectory)
    }

    fn inode_type(&self) -> InodeType {
        InodeType::CharDevice
    }

    fn attach_child(&self, _name: &str, _child: Arc<dyn InodeOps>) -> VfsResult<()> {
        Err(VfsError::NotDirectory)
    }
}

static NEXT_DEV_INO: AtomicU32 = AtomicU32::new(10_000);

fn alloc_dev_ino() -> u64 {
    NEXT_DEV_INO.fetch_add(1, Ordering::Relaxed) as u64
}

fn attach(dev_dir: &Arc<dyn InodeOps>, name: &str, inode: Arc<dyn InodeOps>) -> VfsResult<()> {
    dev_dir.attach_child(name, inode)
}

/// Populate `/dev` with standard Linux device nodes.
pub fn install_dev(root: Arc<dyn InodeOps>) -> VfsResult<()> {
    let dev = root.lookup("dev")?;
    attach(&dev, "null", DevInode::new(alloc_dev_ino(), DevKind::Null, 0o666))?;
    attach(&dev, "zero", DevInode::new(alloc_dev_ino(), DevKind::Zero, 0o666))?;
    attach(
        &dev,
        "random",
        DevInode::new(alloc_dev_ino(), DevKind::Random, 0o644),
    )?;
    attach(
        &dev,
        "urandom",
        DevInode::new(alloc_dev_ino(), DevKind::URandom, 0o644),
    )?;
    attach(&dev, "full", DevInode::new(alloc_dev_ino(), DevKind::Full, 0o666))?;
    attach(
        &dev,
        "console",
        DevInode::new(alloc_dev_ino(), DevKind::Console, 0o600),
    )?;
    attach(&dev, "tty", DevInode::new(alloc_dev_ino(), DevKind::Console, 0o666))?;
    Ok(())
}
