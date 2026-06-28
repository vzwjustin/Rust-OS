//! Live-session mounts: tar.gz archives and squashfs fallback extraction.

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use crate::package::compression::{decompress, TarArchive};
use crate::vfs::{get_vfs, ramfs, InodeType, VfsError, VfsResult, SuperblockOps};

/// Mount a live rootfs image file at `target`.
pub fn mount_live_archive(source: &str, target: &str) -> VfsResult<()> {
    let data = read_vfs_file(source)?;
    let payload = if data.len() >= 2 && data[0] == 0x1f && data[1] == 0x8b {
        decompress(&data).map_err(|_| VfsError::IoError)?
    } else {
        data
    };

    let sb = Arc::new(ramfs::RamFs::new());
    get_vfs().mount(target, sb.clone())?;
    populate_ramfs_from_tar(sb.root(), &payload, target)?;
    Ok(())
}

fn read_vfs_file(path: &str) -> VfsResult<Vec<u8>> {
    let vfs = get_vfs();
    let inode = vfs.lookup(path)?;
    if inode.inode_type() != InodeType::File {
        return Err(VfsError::IsDirectory);
    }
    let stat = inode.stat()?;
    let mut buf = Vec::new();
    buf.resize(stat.size as usize, 0);
    let mut offset = 0u64;
    let mut read_total = 0usize;
    while read_total < buf.len() {
        let n = inode.read_at(offset, &mut buf[read_total..])?;
        if n == 0 {
            break;
        }
        read_total += n;
        offset += n as u64;
    }
    buf.truncate(read_total);
    Ok(buf)
}

fn populate_ramfs_from_tar(
    root: Arc<dyn crate::vfs::InodeOps>,
    payload: &[u8],
    mount_prefix: &str,
) -> VfsResult<()> {
    let archive = TarArchive::parse(payload).map_err(|_| VfsError::IoError)?;
    for entry in archive.entries() {
        let rel = entry
            .path
            .trim_start_matches("./")
            .trim_start_matches('/');
        if rel.is_empty() {
            continue;
        }
        let full = if mount_prefix == "/" {
            format!("/{rel}")
        } else {
            format!("{mount_prefix}/{rel}")
        };
        ensure_parent_dirs(&full)?;
        match entry.entry_type {
            crate::package::compression::tar::TarEntryType::Directory => {
                let _ = get_vfs().lookup(&full);
                let _ = crate::vfs::vfs_mkdir(&full, entry.mode);
            }
            _ => {
                if let Ok(parent) = parent_path(&full) {
                    let _ = crate::vfs::vfs_mkdir(&parent, 0o755);
                }
                const O_WRONLY: u32 = 1;
                const O_CREAT: u32 = 64;
                const O_TRUNC: u32 = 512;
                if let Ok(fd) = crate::vfs::vfs_open(&full, O_WRONLY | O_CREAT | O_TRUNC, entry.mode) {
                    let _ = crate::vfs::vfs_write(fd, &entry.data);
                    let _ = crate::vfs::vfs_close(fd);
                }
            }
        }
    }
    let _ = root;
    Ok(())
}

fn parent_path(path: &str) -> VfsResult<String> {
    let path = path.trim_end_matches('/');
    let Some((parent, _)) = path.rsplit_once('/') else {
        return Err(VfsError::NotFound);
    };
    if parent.is_empty() {
        Ok(String::from("/"))
    } else {
        Ok(String::from(parent))
    }
}

fn ensure_parent_dirs(path: &str) -> VfsResult<()> {
    let mut parts = path.split('/').filter(|p| !p.is_empty());
    let mut current = String::new();
    while let Some(part) = parts.next() {
        if current.is_empty() {
            current.push('/');
        } else if !current.ends_with('/') {
            current.push('/');
        }
        current.push_str(part);
        if get_vfs().lookup(&current).is_err() {
            let _ = crate::vfs::vfs_mkdir(&current, 0o755);
        }
    }
    Ok(())
}

/// Squashfs mount: extract gzip/tar fallback or bind-populate from `/`.
pub fn mount_squashfs(source: &str, target: &str) -> VfsResult<()> {
    if get_vfs().lookup(source).is_ok() {
        if mount_live_archive(source, target).is_ok() {
            return Ok(());
        }
    }
    // Fallback: mirror current rootfs at target via recursive copy of top-level dirs
    let sb = Arc::new(ramfs::RamFs::new());
    get_vfs().mount(target, sb)?;
    for dir in ["/bin", "/usr", "/etc", "/lib", "/sbin", "/init"] {
        let _ = copy_vfs_tree(dir, &format!("{target}{dir}"));
    }
    Ok(())
}

fn copy_vfs_tree(src: &str, dst: &str) -> VfsResult<()> {
    let vfs = get_vfs();
    let inode = match vfs.lookup(src) {
        Ok(i) => i,
        Err(_) => return Ok(()),
    };
    if inode.inode_type() == InodeType::Directory {
        let _ = crate::vfs::vfs_mkdir(dst, 0o755);
        for entry in vfs.readdir(src).unwrap_or_default() {
            if entry.name == "." || entry.name == ".." {
                continue;
            }
            let child_src = format!("{src}/{}", entry.name);
            let child_dst = format!("{dst}/{}", entry.name);
            let _ = copy_vfs_tree(&child_src, &child_dst);
        }
    } else if inode.inode_type() == InodeType::File {
        let data = read_vfs_file(src)?;
        const O_WRONLY: u32 = 1;
        const O_CREAT: u32 = 64;
        const O_TRUNC: u32 = 512;
        if let Ok(fd) = crate::vfs::vfs_open(dst, O_WRONLY | O_CREAT | O_TRUNC, 0o644) {
            let _ = crate::vfs::vfs_write(fd, &data);
            let _ = crate::vfs::vfs_close(fd);
        }
    }
    Ok(())
}
