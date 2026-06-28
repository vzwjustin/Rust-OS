//! Copy live rootfs content onto the target ext4 partition.

use crate::vfs::{InodeType, VfsError};
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use super::format::Ext4Volume;
use crate::drivers::storage::StorageError;

const CRITICAL_PATHS: &[&str] = &["/bin", "/usr", "/etc", "/lib", "/sbin", "/init", "/var", "/home", "/opt", "/root"];

/// Copy critical rootfs paths from the live VFS onto the formatted root partition.
pub fn copy_rootfs_to_partition(volume: &mut Ext4Volume) -> Result<usize, StorageError> {
    let mut files_copied = 0usize;
    for path in CRITICAL_PATHS {
        files_copied += copy_tree(path, volume, "")?;
    }
    Ok(files_copied)
}

fn copy_tree(
    vfs_path: &str,
    volume: &mut Ext4Volume,
    rel_prefix: &str,
) -> Result<usize, StorageError> {
    let vfs = crate::vfs::get_vfs();
    let inode = match vfs.lookup(vfs_path) {
        Ok(i) => i,
        Err(VfsError::NotFound) => return Ok(0),
        Err(_) => return Ok(0),
    };

    if inode.inode_type() == InodeType::Directory {
        let mut count = 0usize;
        let entries = vfs.readdir(vfs_path).unwrap_or_default();
        for entry in entries {
            if entry.name == "." || entry.name == ".." {
                continue;
            }
            let child_vfs = if vfs_path.ends_with('/') {
                format!("{}{}", vfs_path, entry.name)
            } else {
                format!("{}/{}", vfs_path, entry.name)
            };
            let child_rel = if rel_prefix.is_empty() {
                entry.name.clone()
            } else {
                format!("{}/{}", rel_prefix, entry.name)
            };
            if entry.inode_type == InodeType::Directory {
                count += copy_tree(&child_vfs, volume, &child_rel)?;
            } else if entry.inode_type == InodeType::File {
                if let Ok(data) = read_vfs_file(&child_vfs) {
                    let target = format!("/{}", child_rel);
                    volume.write_file(&target, &data)?;
                    count += 1;
                }
            }
        }
        Ok(count)
    } else if inode.inode_type() == InodeType::File {
        let data = read_vfs_file(vfs_path)?;
        let name = vfs_path.rsplit('/').next().unwrap_or("file");
        let target = if rel_prefix.is_empty() {
            format!("/{}", name)
        } else {
            format!("/{}", rel_prefix)
        };
        volume.write_file(&target, &data)?;
        Ok(1)
    } else {
        Ok(0)
    }
}

fn read_vfs_file(path: &str) -> Result<Vec<u8>, StorageError> {
    let vfs = crate::vfs::get_vfs();
    let inode = vfs.lookup(path).map_err(|_| StorageError::MediaError)?;
    let stat = inode.stat().map_err(|_| StorageError::MediaError)?;
    let size = stat.size as usize;
    if size > 16 * 1024 * 1024 {
        // Stream large files in 4 MiB chunks
        return read_vfs_file_chunked(path);
    }
    let mut buf = vec![0u8; size.max(1)];
    let read = inode
        .read_at(0, &mut buf)
        .map_err(|_| StorageError::MediaError)?;
    buf.truncate(read);
    Ok(buf)
}

fn read_vfs_file_chunked(path: &str) -> Result<Vec<u8>, StorageError> {
    let vfs = crate::vfs::get_vfs();
    let inode = vfs.lookup(path).map_err(|_| StorageError::MediaError)?;
    let stat = inode.stat().map_err(|_| StorageError::MediaError)?;
    let size = stat.size as usize;
    let mut buf = vec![0u8; size];
    let mut offset = 0;
    while offset < size {
        let chunk_size = core::cmp::min(size - offset, 4 * 1024 * 1024);
        let read = inode
            .read_at(offset as u64, &mut buf[offset..offset + chunk_size])
            .map_err(|_| StorageError::MediaError)?;
        if read == 0 {
            break;
        }
        offset += read;
    }
    buf.truncate(offset);
    Ok(buf)
}
