//! sysfs virtual filesystem implementation
//!
//! sysfs is an in-memory filesystem that exports information about
//! devices, drivers, and other kernel subsystems to userspace.

use super::{
    get_current_time, DirectoryEntry, FileMetadata, FilePermissions, FileSystem, FileSystemStats,
    FileSystemType, FileType, FsError, FsResult, InodeNumber, OpenFlags,
};
use alloc::{
    collections::BTreeMap,
    string::{String, ToString},
    vec::Vec,
};
use spin::RwLock;

/// sysfs Inode
#[derive(Debug, Clone)]
struct SysfsInode {
    metadata: FileMetadata,
    content: Vec<u8>,
    entries: BTreeMap<String, InodeNumber>,
    /// Callback triggered when the file is written to.
    write_callback: Option<fn(data: &[u8]) -> FsResult<()>>,
}

impl SysfsInode {
    fn new_file(inode: InodeNumber, permissions: FilePermissions, content: Vec<u8>) -> Self {
        Self {
            metadata: FileMetadata {
                inode,
                file_type: FileType::Regular,
                size: content.len() as u64,
                permissions,
                uid: 0,
                gid: 0,
                created: get_current_time(),
                modified: get_current_time(),
                accessed: get_current_time(),
                link_count: 1,
                device_id: None,
            },
            content,
            entries: BTreeMap::new(),
            write_callback: None,
        }
    }

    fn new_directory(inode: InodeNumber, permissions: FilePermissions) -> Self {
        let mut entries = BTreeMap::new();
        entries.insert(".".to_string(), inode);
        Self {
            metadata: FileMetadata {
                inode,
                file_type: FileType::Directory,
                size: 0,
                permissions,
                uid: 0,
                gid: 0,
                created: get_current_time(),
                modified: get_current_time(),
                accessed: get_current_time(),
                link_count: 2,
                device_id: None,
            },
            content: Vec::new(),
            entries,
            write_callback: None,
        }
    }
}

/// sysfs filesystem
#[derive(Debug)]
pub struct SysFs {
    inodes: RwLock<BTreeMap<InodeNumber, SysfsInode>>,
    next_inode: RwLock<u64>,
}

impl SysFs {
    /// Create a new sysfs instance and populate it with devices and subsystems.
    pub fn new() -> Self {
        let s = Self {
            inodes: RwLock::new(BTreeMap::new()),
            next_inode: RwLock::new(1),
        };

        // Create root directory (Inode 1)
        s.create_dir_node(1, FilePermissions::from_octal(0o755));

        // Build directory structure:
        // /sys/bus/pci/devices/
        // /sys/class/net/
        // /sys/power/state
        let bus_inode = s
            .mkdir_internal(1, "bus", FilePermissions::from_octal(0o755))
            .unwrap();
        let pci_inode = s
            .mkdir_internal(bus_inode, "pci", FilePermissions::from_octal(0o755))
            .unwrap();
        let pci_devices_inode = s
            .mkdir_internal(pci_inode, "devices", FilePermissions::from_octal(0o755))
            .unwrap();

        let class_inode = s
            .mkdir_internal(1, "class", FilePermissions::from_octal(0o755))
            .unwrap();
        let _net_inode = s
            .mkdir_internal(class_inode, "net", FilePermissions::from_octal(0o755))
            .unwrap();

        let power_inode = s
            .mkdir_internal(1, "power", FilePermissions::from_octal(0o755))
            .unwrap();

        // /sys/power/state
        let state_content = b"freeze mem disk off\n".to_vec();
        let state_inode = s
            .create_file_internal(
                power_inode,
                "state",
                FilePermissions::from_octal(0o644),
                state_content,
            )
            .unwrap();

        // Wire up write callback for /sys/power/state
        {
            let mut inodes = s.inodes.write();
            if let Some(inode) = inodes.get_mut(&state_inode) {
                inode.write_callback = Some(|data| {
                    let cmd = core::str::from_utf8(data)
                        .map_err(|_| FsError::InvalidArgument)?
                        .trim();
                    crate::power::request_state(cmd).map_err(|_| FsError::IoError)?;
                    Ok(())
                });
            }
        }

        // Populate PCI devices dynamically
        let pci_devices = crate::pci::get_all_devices();
        for dev in pci_devices {
            let dev_name =
                alloc::format!("0000:{:02x}:{:02x}.{:x}", dev.bus, dev.device, dev.function);
            if let Ok(dev_inode) = s.mkdir_internal(
                pci_devices_inode,
                &dev_name,
                FilePermissions::from_octal(0o755),
            ) {
                let _ = s.create_file_internal(
                    dev_inode,
                    "vendor",
                    FilePermissions::from_octal(0o444),
                    alloc::format!("0x{:04x}\n", dev.vendor_id).into_bytes(),
                );
                let _ = s.create_file_internal(
                    dev_inode,
                    "device",
                    FilePermissions::from_octal(0o444),
                    alloc::format!("0x{:04x}\n", dev.device_id).into_bytes(),
                );
                let _ = s.create_file_internal(
                    dev_inode,
                    "class",
                    FilePermissions::from_octal(0o444),
                    alloc::format!(
                        "0x{:02x}{:02x}{:02x}\n",
                        dev.class,
                        dev.subclass,
                        dev.prog_if
                    )
                    .into_bytes(),
                );
                let _ = s.create_file_internal(
                    dev_inode,
                    "name",
                    FilePermissions::from_octal(0o444),
                    alloc::format!("{}\n", dev.name).into_bytes(),
                );
            }
        }

        s
    }

    fn alloc_inode(&self) -> InodeNumber {
        let mut next = self.next_inode.write();
        let inode = *next;
        *next += 1;
        inode
    }

    fn create_dir_node(&self, inode: InodeNumber, permissions: FilePermissions) {
        let mut inodes = self.inodes.write();
        inodes.insert(inode, SysfsInode::new_directory(inode, permissions));
    }

    fn mkdir_internal(
        &self,
        parent_inode: InodeNumber,
        name: &str,
        permissions: FilePermissions,
    ) -> FsResult<InodeNumber> {
        let mut inodes = self.inodes.write();

        let parent = inodes.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        if parent.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        if parent.entries.contains_key(name) {
            return Err(FsError::AlreadyExists);
        }

        let child_inode = {
            let mut next = self.next_inode.write();
            let inode = *next;
            *next += 1;
            inode
        };

        let mut child = SysfsInode::new_directory(child_inode, permissions);
        child.entries.insert("..".to_string(), parent_inode);

        inodes.insert(child_inode, child);

        // Re-borrow parent
        let parent = inodes.get_mut(&parent_inode).unwrap();
        parent.entries.insert(name.to_string(), child_inode);
        parent.metadata.link_count += 1;

        Ok(child_inode)
    }

    fn create_file_internal(
        &self,
        parent_inode: InodeNumber,
        name: &str,
        permissions: FilePermissions,
        content: Vec<u8>,
    ) -> FsResult<InodeNumber> {
        let mut inodes = self.inodes.write();

        let parent = inodes.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        if parent.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        if parent.entries.contains_key(name) {
            return Err(FsError::AlreadyExists);
        }

        let child_inode = {
            let mut next = self.next_inode.write();
            let inode = *next;
            *next += 1;
            inode
        };

        let child = SysfsInode::new_file(child_inode, permissions, content);
        inodes.insert(child_inode, child);

        let parent = inodes.get_mut(&parent_inode).unwrap();
        parent.entries.insert(name.to_string(), child_inode);

        Ok(child_inode)
    }
}

impl FileSystem for SysFs {
    fn fs_type(&self) -> FileSystemType {
        FileSystemType::SysFs
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        let inodes = self.inodes.read();
        Ok(FileSystemStats {
            total_blocks: 0,
            free_blocks: 0,
            available_blocks: 0,
            total_inodes: inodes.len() as u64,
            free_inodes: 0,
            block_size: 4096,
            max_filename_length: 255,
        })
    }

    fn create(&self, _path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        Err(FsError::ReadOnly)
    }

    fn open(&self, path: &str, _flags: OpenFlags) -> FsResult<InodeNumber> {
        let inodes = self.inodes.read();
        let mut current_inode = 1;

        let parts = path.split('/').filter(|s| !s.is_empty());
        for part in parts {
            let inode = inodes.get(&current_inode).ok_or(FsError::NotFound)?;
            if inode.metadata.file_type != FileType::Directory {
                return Err(FsError::NotFound);
            }
            current_inode = *inode.entries.get(part).ok_or(FsError::NotFound)?;
        }

        Ok(current_inode)
    }

    fn read(&self, inode: InodeNumber, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        let inodes = self.inodes.read();
        let node = inodes.get(&inode).ok_or(FsError::NotFound)?;
        if node.metadata.file_type != FileType::Regular {
            return Err(FsError::IsADirectory);
        }

        if offset >= node.content.len() as u64 {
            return Ok(0);
        }

        let start = offset as usize;
        let end = core::cmp::min(node.content.len(), start + buffer.len());
        let len = end - start;
        buffer[..len].copy_from_slice(&node.content[start..end]);
        Ok(len)
    }

    fn write(&self, inode: InodeNumber, offset: u64, buffer: &[u8]) -> FsResult<usize> {
        let mut inodes = self.inodes.write();
        let node = inodes.get_mut(&inode).ok_or(FsError::NotFound)?;
        if node.metadata.file_type != FileType::Regular {
            return Err(FsError::IsADirectory);
        }

        // If a callback is registered, trigger it
        if let Some(cb) = node.write_callback {
            cb(buffer)?;
        }

        // Write to content buffer
        let start = offset as usize;
        let end = start + buffer.len();
        if end > node.content.len() {
            node.content.resize(end, 0);
        }
        node.content[start..end].copy_from_slice(buffer);
        node.metadata.size = node.content.len() as u64;
        node.metadata.modified = get_current_time();

        Ok(buffer.len())
    }

    fn metadata(&self, inode: InodeNumber) -> FsResult<FileMetadata> {
        let inodes = self.inodes.read();
        let node = inodes.get(&inode).ok_or(FsError::NotFound)?;
        Ok(node.metadata.clone())
    }

    fn set_metadata(&self, inode: InodeNumber, metadata: &FileMetadata) -> FsResult<()> {
        let mut inodes = self.inodes.write();
        let node = inodes.get_mut(&inode).ok_or(FsError::NotFound)?;
        node.metadata.permissions = metadata.permissions;
        node.metadata.modified = get_current_time();
        Ok(())
    }

    fn mkdir(&self, _path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        Err(FsError::ReadOnly)
    }

    fn rmdir(&self, _path: &str) -> FsResult<()> {
        Err(FsError::ReadOnly)
    }

    fn unlink(&self, _path: &str) -> FsResult<()> {
        Err(FsError::ReadOnly)
    }

    fn readdir(&self, inode: InodeNumber) -> FsResult<Vec<DirectoryEntry>> {
        let inodes = self.inodes.read();
        let node = inodes.get(&inode).ok_or(FsError::NotFound)?;
        if node.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }

        let mut entries = Vec::new();
        for (name, &child_inode) in node.entries.iter() {
            let child_node = inodes.get(&child_inode).unwrap();
            entries.push(DirectoryEntry {
                name: name.clone(),
                inode: child_inode,
                file_type: child_node.metadata.file_type,
            });
        }
        Ok(entries)
    }

    fn rename(&self, _old_path: &str, _new_path: &str) -> FsResult<()> {
        Err(FsError::ReadOnly)
    }

    fn symlink(&self, _target: &str, _link_path: &str) -> FsResult<()> {
        Err(FsError::ReadOnly)
    }

    fn readlink(&self, _path: &str) -> FsResult<String> {
        Err(FsError::NotSupported)
    }

    fn sync(&self) -> FsResult<()> {
        Ok(())
    }
}
