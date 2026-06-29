//! Overlay filesystem: merges a lower read-only tree with an upper writable layer.
//!
//! Supports copy-up on write, whiteout files (`.wh.` prefix and char device 0:0
//! with `trusted.overlay.whiteout`), and opaque directories (`.wh..wh.opq` or
//! `trusted.overlay.opaque` xattr).

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::string::ToString;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use spin::Mutex;

use super::{
    get_vfs, DirEntry, InodeOps, InodeType, Stat, StatFs, SuperblockOps, VfsError, VfsResult,
};

const WH_PREFIX: &str = ".wh.";
const OPAQUE_MARKER: &str = ".wh..wh.opq";
const XATTR_WHITEOUT: &str = "trusted.overlay.whiteout";
const XATTR_OPAQUE: &str = "trusted.overlay.opaque";

lazy_static::lazy_static! {
    static ref OVERLAY_UPPERS: Mutex<BTreeMap<String, Arc<super::ramfs::RamFs>>> =
        Mutex::new(BTreeMap::new());
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

fn join_path(base: &str, rel: &str) -> String {
    let rel = rel.trim_start_matches('/');
    if rel.is_empty() {
        base.trim_end_matches('/').to_string()
    } else if base == "/" {
        format!("/{rel}")
    } else {
        format!("{}/{}", base.trim_end_matches('/'), rel)
    }
}

fn parent_rel(rel: &str) -> Option<&str> {
    rel.rsplit_once('/').map(|(p, _)| p)
}

fn child_rel(parent_rel: &str, name: &str) -> String {
    if parent_rel.is_empty() {
        name.to_string()
    } else {
        format!("{parent_rel}/{name}")
    }
}

fn path_exists(path: &str) -> bool {
    super::vfs_lstat(path).is_ok()
}

fn ensure_upper_parent(upper_base: &str, rel: &str) -> VfsResult<()> {
    let Some(parent) = parent_rel(rel) else {
        return Ok(());
    };
    let upper_parent = join_path(upper_base, parent);
    if path_exists(&upper_parent) {
        return Ok(());
    }
    ensure_upper_parent(upper_base, parent)?;
    let _ = super::vfs_mkdir(&upper_parent, 0o755);
    Ok(())
}

fn is_whiteout_path(path: &str, name: &str) -> bool {
    if name.starts_with(WH_PREFIX) && name != OPAQUE_MARKER {
        return true;
    }
    if let Ok(stat) = super::vfs_lstat(path) {
        if stat.inode_type == InodeType::CharDevice && stat.rdev == 0 {
            return true;
        }
        if super::vfs_getxattr(path, XATTR_WHITEOUT).is_ok() {
            return true;
        }
    }
    false
}

fn whiteout_hides_name(upper_dir: &str, name: &str) -> bool {
    let wh_path = join_path(upper_dir, &format!("{WH_PREFIX}{name}"));
    is_whiteout_path(&wh_path, &format!("{WH_PREFIX}{name}"))
}

fn is_opaque_upper_dir(upper_dir: &str) -> bool {
    if path_exists(&join_path(upper_dir, OPAQUE_MARKER)) {
        return true;
    }
    super::vfs_getxattr(upper_dir, XATTR_OPAQUE)
        .map(|v| v == b"y")
        .unwrap_or(false)
}

fn create_whiteout(upper_dir: &str, name: &str) -> VfsResult<()> {
    let wh_name = format!("{WH_PREFIX}{name}");
    let wh_path = join_path(upper_dir, &wh_name);
    if path_exists(&wh_path) {
        return Ok(());
    }

    if get_vfs().mknod(&wh_path, InodeType::CharDevice, 0).is_ok() {
        let _ = super::vfs_setxattr(&wh_path, XATTR_WHITEOUT, b"y", true);
        return Ok(());
    }

    const O_WRONLY: u32 = 1;
    const O_CREAT: u32 = 64;
    let fd = super::vfs_open(&wh_path, O_WRONLY | O_CREAT, 0o000)?;
    let _ = super::vfs_close(fd);
    super::vfs_setxattr(&wh_path, XATTR_WHITEOUT, b"y", true)
}

fn remove_whiteout(upper_dir: &str, name: &str) -> VfsResult<()> {
    let wh_path = join_path(upper_dir, &format!("{WH_PREFIX}{name}"));
    if path_exists(&wh_path) {
        let _ = super::vfs_unlink(&wh_path);
    }
    Ok(())
}

fn mark_opaque(upper_dir: &str) -> VfsResult<()> {
    let marker = join_path(upper_dir, OPAQUE_MARKER);
    if !path_exists(&marker) {
        const O_WRONLY: u32 = 1;
        const O_CREAT: u32 = 64;
        let fd = super::vfs_open(&marker, O_WRONLY | O_CREAT, 0o000)?;
        let _ = super::vfs_close(fd);
    }
    super::vfs_setxattr(upper_dir, XATTR_OPAQUE, b"y", true)
}

fn copy_up_file(lower_base: &str, upper_base: &str, rel: &str) -> VfsResult<()> {
    let lower_path = join_path(lower_base, rel);
    let upper_path = join_path(upper_base, rel);
    if path_exists(&upper_path) {
        return Ok(());
    }

    if let Some(parent) = parent_rel(rel) {
        ensure_upper_parent(upper_base, rel)?;
        let _ = parent;
    }

    let stat = super::vfs_lstat(&lower_path).map_err(|_| VfsError::NotFound)?;
    if stat.inode_type == InodeType::Directory {
        let _ = super::vfs_mkdir(&upper_path, stat.mode & 0o7777);
        return Ok(());
    }

    let mut buf = vec![0u8; stat.size as usize];
    const O_RDONLY: u32 = 0;
    let fd_in = super::vfs_open(&lower_path, O_RDONLY, 0)?;
    let read = super::vfs_read(fd_in, &mut buf)?;
    let _ = super::vfs_close(fd_in);

    const O_WRONLY: u32 = 1;
    const O_CREAT: u32 = 64;
    const O_TRUNC: u32 = 512;
    let fd_out = super::vfs_open(
        &upper_path,
        O_WRONLY | O_CREAT | O_TRUNC,
        stat.mode & 0o7777,
    )?;
    let _ = super::vfs_write(fd_out, &buf[..read]);
    let _ = super::vfs_close(fd_out);
    Ok(())
}

struct OverlayInode {
    mount: String,
    lower: String,
    upper: String,
    rel: String,
    inode_type: InodeType,
}

impl OverlayInode {
    fn lower_path(&self) -> String {
        join_path(&self.lower, &self.rel)
    }

    fn upper_path(&self) -> String {
        join_path(&self.upper, &self.rel)
    }

    fn upper_dir(&self) -> String {
        join_path(&self.upper, &self.rel)
    }

    fn resolve_visible_stat(&self) -> VfsResult<Stat> {
        let upper = self.upper_path();
        if path_exists(&upper) {
            return super::vfs_lstat(&upper);
        }
        let name = self.rel.rsplit('/').next().unwrap_or("");
        let upper_parent = parent_rel(&self.rel)
            .map(|p| join_path(&self.upper, p))
            .unwrap_or_else(|| self.upper.clone());
        if whiteout_hides_name(&upper_parent, name) {
            return Err(VfsError::NotFound);
        }
        super::vfs_lstat(&self.lower_path())
    }

    fn pick_read_path(&self) -> VfsResult<String> {
        let upper = self.upper_path();
        if path_exists(&upper) {
            return Ok(upper);
        }
        let name = self.rel.rsplit('/').next().unwrap_or("");
        let upper_parent = parent_rel(&self.rel)
            .map(|p| join_path(&self.upper, p))
            .unwrap_or_else(|| self.upper.clone());
        if whiteout_hides_name(&upper_parent, name) {
            return Err(VfsError::NotFound);
        }
        Ok(self.lower_path())
    }
}

impl InodeOps for OverlayInode {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        let path = self.pick_read_path()?;
        get_vfs().lookup(&path)?.read_at(offset, buf)
    }

    fn write_at(&self, offset: u64, buf: &[u8]) -> VfsResult<usize> {
        if self.inode_type == InodeType::Directory {
            return Err(VfsError::IsDirectory);
        }
        copy_up_file(&self.lower, &self.upper, &self.rel)?;
        get_vfs().lookup(&self.upper_path())?.write_at(offset, buf)
    }

    fn stat(&self) -> VfsResult<Stat> {
        self.resolve_visible_stat()
    }

    fn truncate(&self, size: u64) -> VfsResult<()> {
        copy_up_file(&self.lower, &self.upper, &self.rel)?;
        get_vfs().lookup(&self.upper_path())?.truncate(size)
    }

    fn sync(&self) -> VfsResult<()> {
        Ok(())
    }

    fn lookup(&self, name: &str) -> VfsResult<Arc<dyn InodeOps>> {
        if self.inode_type != InodeType::Directory {
            return Err(VfsError::NotDirectory);
        }

        let rel = child_rel(&self.rel, name);
        let upper_dir = self.upper_dir();

        if whiteout_hides_name(&upper_dir, name) {
            return Err(VfsError::NotFound);
        }

        let upper_child = join_path(&self.upper, &rel);
        if path_exists(&upper_child) {
            let stat = super::vfs_lstat(&upper_child)?;
            return Ok(Arc::new(OverlayInode {
                mount: self.mount.clone(),
                lower: self.lower.clone(),
                upper: self.upper.clone(),
                rel,
                inode_type: stat.inode_type,
            }));
        }

        if is_opaque_upper_dir(&upper_dir) {
            return Err(VfsError::NotFound);
        }

        let lower_child = join_path(&self.lower, &rel);
        let stat = super::vfs_lstat(&lower_child).map_err(|_| VfsError::NotFound)?;
        Ok(Arc::new(OverlayInode {
            mount: self.mount.clone(),
            lower: self.lower.clone(),
            upper: self.upper.clone(),
            rel,
            inode_type: stat.inode_type,
        }))
    }

    fn create(&self, name: &str, inode_type: InodeType, mode: u32) -> VfsResult<Arc<dyn InodeOps>> {
        let rel = child_rel(&self.rel, name);
        ensure_upper_parent(&self.upper, &rel)?;
        remove_whiteout(&self.upper_dir(), name)?;

        let upper_path = join_path(&self.upper, &rel);
        match inode_type {
            InodeType::Directory => {
                let _ = super::vfs_mkdir(&upper_path, mode);
                let lower_path = join_path(&self.lower, &rel);
                if path_exists(&lower_path) {
                    let _ = mark_opaque(&upper_path);
                }
            }
            InodeType::File => {
                const O_WRONLY: u32 = 1;
                const O_CREAT: u32 = 64;
                let fd = super::vfs_open(&upper_path, O_WRONLY | O_CREAT, mode)?;
                let _ = super::vfs_close(fd);
            }
            _ => {
                get_vfs().mknod(&upper_path, inode_type, mode)?;
            }
        }

        Ok(Arc::new(OverlayInode {
            mount: self.mount.clone(),
            lower: self.lower.clone(),
            upper: self.upper.clone(),
            rel,
            inode_type,
        }))
    }

    fn unlink(&self, name: &str) -> VfsResult<()> {
        let rel = child_rel(&self.rel, name);
        let upper_path = join_path(&self.upper, &rel);
        let lower_path = join_path(&self.lower, &rel);
        let upper_dir = self.upper_dir();

        if path_exists(&upper_path) {
            super::vfs_unlink(&upper_path)?;
        }
        if path_exists(&lower_path) {
            ensure_upper_parent(&self.upper, &rel)?;
            create_whiteout(&upper_dir, name)?;
        }
        Ok(())
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
        let upper_dir = self.upper_dir();
        let lower_dir = self.lower_path();
        let opaque = path_exists(&upper_dir) && is_opaque_upper_dir(&upper_dir);

        let mut merged: BTreeMap<String, DirEntry> = BTreeMap::new();

        if !opaque && path_exists(&lower_dir) {
            if let Ok(entries) = super::vfs_readdir(&lower_dir) {
                for entry in entries {
                    if entry.name.starts_with(WH_PREFIX) {
                        continue;
                    }
                    merged.insert(entry.name.clone(), entry);
                }
            }
        }

        if path_exists(&upper_dir) {
            if let Ok(entries) = super::vfs_readdir(&upper_dir) {
                for entry in entries {
                    let name = entry.name.clone();
                    if name.starts_with(WH_PREFIX) {
                        if name == OPAQUE_MARKER {
                            continue;
                        }
                        if let Some(hidden) = name.strip_prefix(WH_PREFIX) {
                            merged.remove(hidden);
                        }
                        continue;
                    }
                    if is_whiteout_path(&join_path(&upper_dir, &name), &name) {
                        continue;
                    }
                    merged.insert(name, entry);
                }
            }
        }

        Ok(merged.into_values().collect())
    }

    fn inode_type(&self) -> InodeType {
        self.inode_type
    }
}

pub struct OverlayMount {
    mount: String,
    lower: String,
    upper: String,
    root: Arc<dyn InodeOps>,
}

impl OverlayMount {
    pub fn new(mount: String, lower: String, upper: String) -> Self {
        let root = Arc::new(OverlayInode {
            mount: mount.clone(),
            lower: lower.clone(),
            upper: upper.clone(),
            rel: String::new(),
            inode_type: InodeType::Directory,
        });
        Self {
            mount,
            lower,
            upper,
            root,
        }
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
        get_vfs().statfs(&self.upper)
    }
}

pub fn mount_overlay(lower: &str, target: &str, upper: &str, work: &str) -> VfsResult<()> {
    let upper_dir = if upper.is_empty() {
        format!("{target}/.upper")
    } else {
        upper.to_string()
    };
    let _ = super::vfs_mkdir(&upper_dir, 0o755);
    if !work.is_empty() {
        let _ = super::vfs_mkdir(work, 0o755);
    }
    let sb = Arc::new(OverlayMount::new(
        String::from(target),
        String::from(lower),
        upper_dir,
    ));
    get_vfs().mount(target, sb)
}
