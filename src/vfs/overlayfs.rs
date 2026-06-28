//! Minimal overlay filesystem: upper tmpfs over an existing VFS lower layer.

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::string::ToString;
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::Mutex;

use super::{
    get_vfs, ramfs, DirEntry, InodeOps, InodeType, Stat, StatFs, SuperblockOps, VfsError, VfsResult,
};

lazy_static::lazy_static! {
    static ref OVERLAY_UPPERS: Mutex<BTreeMap<String, Arc<ramfs::RamFs>>> = Mutex::new(BTreeMap::new());
}

/// Parse `lowerdir=...,upperdir=...,workdir=...` mount options.
pub fn parse_overlay_options(data: &str) -> Option<(String, String)> {
    let mut lower = String::new();
    let mut upper = String::new();
    for part in data.split(',') {
        if let Some(v) = part.strip_prefix("lowerdir=") {
            lower = String::from(v.trim());
        } else if let Some(v) = part.strip_prefix("upperdir=") {
            upper = String::from(v.trim());
        }
    }
    if lower.is_empty() {
        None
    } else {
        Some((lower, upper))
    }
}

struct OverlayInode {
    mount: String,
    rel: String,
    lower: String,
    ino: u64,
    inode_type: InodeType,
}

impl OverlayInode {
    fn full_lower(&self) -> String {
        if self.lower == "/" {
            format!("/{}", self.rel.trim_start_matches('/'))
        } else {
            format!(
                "{}/{}",
                self.lower.trim_end_matches('/'),
                self.rel.trim_start_matches('/')
            )
        }
    }

    fn full_mount(&self) -> String {
        if self.rel.is_empty() || self.rel == "/" {
            self.mount.clone()
        } else {
            format!(
                "{}/{}",
                self.mount.trim_end_matches('/'),
                self.rel.trim_start_matches('/')
            )
        }
    }
}

impl InodeOps for OverlayInode {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        let upper_path = self.full_mount();
        if let Ok(upper) = get_vfs().lookup(&upper_path) {
            if upper.inode_type() == InodeType::File {
                return upper.read_at(offset, buf);
            }
        }
        get_vfs().lookup(&self.full_lower())?.read_at(offset, buf)
    }

    fn write_at(&self, offset: u64, buf: &[u8]) -> VfsResult<usize> {
        ensure_upper_parent(&self.full_mount())?;
        let upper_path = self.full_mount();
        if get_vfs().lookup(&upper_path).is_err() {
            copy_lower_to_upper(&self.full_lower(), &upper_path)?;
        }
        get_vfs().lookup(&upper_path)?.write_at(offset, buf)
    }

    fn stat(&self) -> VfsResult<Stat> {
        let upper_path = self.full_mount();
        if let Ok(upper) = get_vfs().lookup(&upper_path) {
            return upper.stat();
        }
        get_vfs().lookup(&self.full_lower())?.stat()
    }

    fn truncate(&self, size: u64) -> VfsResult<()> {
        get_vfs().lookup(&self.full_mount())?.truncate(size)
    }

    fn sync(&self) -> VfsResult<()> {
        Ok(())
    }

    fn lookup(&self, name: &str) -> VfsResult<Arc<dyn InodeOps>> {
        if self.inode_type != InodeType::Directory {
            return Err(VfsError::NotDirectory);
        }
        let rel = if self.rel.is_empty() || self.rel == "/" {
            name.to_string()
        } else {
            format!("{}/{}", self.rel.trim_start_matches('/'), name)
        };
        let lower_path = self.full_lower_for_rel(&rel);
        let upper_path = self.full_mount_for_rel(&rel);
        let inode_type = if get_vfs().lookup(&upper_path).is_ok() {
            get_vfs().lookup(&upper_path)?.inode_type()
        } else {
            get_vfs().lookup(&lower_path)?.inode_type()
        };
        Ok(Arc::new(OverlayInode {
            mount: self.mount.clone(),
            rel,
            lower: self.lower.clone(),
            ino: 0,
            inode_type,
        }))
    }

    fn create(&self, name: &str, inode_type: InodeType, mode: u32) -> VfsResult<Arc<dyn InodeOps>> {
        let path = self.full_mount_for_rel(name);
        ensure_upper_parent(&path)?;
        get_vfs()
            .lookup(&self.full_mount())?
            .create(name, inode_type, mode)
    }

    fn unlink(&self, name: &str) -> VfsResult<()> {
        let path = self.full_mount_for_rel(name);
        if get_vfs().lookup(&path).is_ok() {
            return get_vfs().unlink(&path);
        }
        Err(VfsError::ReadOnly)
    }

    fn link(&self, _name: &str, _target: Arc<dyn InodeOps>) -> VfsResult<()> {
        Err(VfsError::NotSupported)
    }

    fn rename(
        &self,
        _old_name: &str,
        _new_dir: Arc<dyn InodeOps>,
        _new_name: &str,
    ) -> VfsResult<()> {
        Err(VfsError::NotSupported)
    }

    fn readdir(&self) -> VfsResult<Vec<DirEntry>> {
        let mut names = BTreeMap::new();
        if let Ok(entries) = get_vfs().readdir(&self.full_lower()) {
            for e in entries {
                names.insert(e.name.clone(), e);
            }
        }
        if let Ok(entries) = get_vfs().readdir(&self.full_mount()) {
            for e in entries {
                names.insert(e.name.clone(), e);
            }
        }
        Ok(names.into_values().collect())
    }

    fn inode_type(&self) -> InodeType {
        self.inode_type
    }
}

impl OverlayInode {
    fn full_lower_for_rel(&self, rel: &str) -> String {
        format!(
            "{}/{}",
            self.lower.trim_end_matches('/'),
            rel.trim_start_matches('/')
        )
    }

    fn full_mount_for_rel(&self, rel: &str) -> String {
        format!(
            "{}/{}",
            self.mount.trim_end_matches('/'),
            rel.trim_start_matches('/')
        )
    }
}

pub struct OverlayMount {
    mount: String,
    lower: String,
    root: Arc<dyn InodeOps>,
}

impl OverlayMount {
    pub fn new(mount: String, lower: String) -> Self {
        let root = Arc::new(OverlayInode {
            mount: mount.clone(),
            rel: String::new(),
            lower: lower.clone(),
            ino: 1,
            inode_type: InodeType::Directory,
        });
        Self { mount, lower, root }
    }
}

impl SuperblockOps for OverlayMount {
    fn root(&self) -> Arc<dyn InodeOps> {
        Arc::clone(&self.root)
    }

    fn sync_fs(&self) -> VfsResult<()> {
        Ok(())
    }

    fn statfs(&self) -> VfsResult<StatFs> {
        get_vfs().statfs(&self.lower)
    }
}

pub fn mount_overlay(lower: &str, target: &str, _upper: &str, _work: &str) -> VfsResult<()> {
    let sb = Arc::new(OverlayMount::new(String::from(target), String::from(lower)));
    get_vfs().mount(target, sb)
}

fn ensure_upper_parent(path: &str) -> VfsResult<()> {
    let path = path.trim_end_matches('/');
    if let Some((parent, _)) = path.rsplit_once('/') {
        if !parent.is_empty() && get_vfs().lookup(parent).is_err() {
            let _ = crate::vfs::vfs_mkdir(parent, 0o755);
        }
    }
    Ok(())
}

fn copy_lower_to_upper(lower: &str, upper: &str) -> VfsResult<()> {
    let lower_inode = get_vfs().lookup(lower)?;
    if lower_inode.inode_type() == InodeType::File {
        let stat = lower_inode.stat()?;
        let mut buf = Vec::new();
        buf.resize(stat.size as usize, 0);
        let _ = lower_inode.read_at(0, &mut buf);
        const O_WRONLY: u32 = 1;
        const O_CREAT: u32 = 64;
        const O_TRUNC: u32 = 512;
        if let Ok(fd) = crate::vfs::vfs_open(upper, O_WRONLY | O_CREAT | O_TRUNC, stat.mode) {
            let _ = crate::vfs::vfs_write(fd, &buf);
            let _ = crate::vfs::vfs_close(fd);
        }
    } else {
        let _ = crate::vfs::vfs_mkdir(upper, 0o755);
    }
    Ok(())
}
