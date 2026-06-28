//! Device filesystem implementation
//!
//! This module provides a device filesystem that exposes system devices
//! as files in the /dev directory. It includes standard devices like
//! null, zero, random, and console.
// Import handled by parent module

use super::{
    DirectoryEntry, FileMetadata, FilePermissions, FileSystem, FileSystemStats, FileSystemType,
    FileType, FsError, FsResult, InodeNumber, OpenFlags,
};
use alloc::{
    collections::BTreeMap,
    string::{String, ToString},
    vec::Vec,
};
use spin::RwLock;

/// Device types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceType {
    /// Null device (/dev/null)
    Null,
    /// Zero device (/dev/zero)
    Zero,
    /// Random device (/dev/random)
    Random,
    /// Pseudo-random device (/dev/urandom)
    URandom,
    /// Console device (/dev/console)
    Console,
    /// Standard input (/dev/stdin)
    Stdin,
    /// Standard output (/dev/stdout)
    Stdout,
    /// Standard error (/dev/stderr)
    Stderr,
    /// Memory device (/dev/mem)
    Memory,
    /// Kernel memory (/dev/kmem)
    KernelMemory,
    /// Full device (/dev/full)
    Full,
}

/// Device node information
#[derive(Debug, Clone)]
struct DeviceNode {
    /// Device type
    device_type: DeviceType,
    /// Device metadata
    metadata: FileMetadata,
    /// Major device number
    major: u32,
    /// Minor device number
    minor: u32,
}

impl DeviceNode {
    /// Create a new character device node
    fn new_char_device(
        inode: InodeNumber,
        device_type: DeviceType,
        major: u32,
        minor: u32,
        permissions: FilePermissions,
    ) -> Self {
        let mut metadata = FileMetadata::new(inode, FileType::CharacterDevice, 0);
        metadata.permissions = permissions;
        metadata.device_id = Some((major << 8) | minor);

        Self {
            device_type,
            metadata,
            major,
            minor,
        }
    }
}

/// Device filesystem
#[derive(Debug)]
pub struct DevFs {
    /// Device nodes
    devices: RwLock<BTreeMap<String, DeviceNode>>,
    /// Root directory metadata
    root_metadata: FileMetadata,
    /// Simple PRNG state for /dev/random
    prng_state: RwLock<u64>,
    /// Next inode number for dynamically added devices
    next_inode: RwLock<u64>,
}

impl DevFs {
    /// Create a new device filesystem
    pub fn new() -> Self {
        let mut devices = BTreeMap::new();
        let root_inode = 1;

        // Create standard device nodes
        devices.insert(
            "null".to_string(),
            DeviceNode::new_char_device(
                2,
                DeviceType::Null,
                1,
                3,
                FilePermissions::from_octal(0o666),
            ),
        );

        devices.insert(
            "zero".to_string(),
            DeviceNode::new_char_device(
                3,
                DeviceType::Zero,
                1,
                5,
                FilePermissions::from_octal(0o666),
            ),
        );

        devices.insert(
            "random".to_string(),
            DeviceNode::new_char_device(
                4,
                DeviceType::Random,
                1,
                8,
                FilePermissions::from_octal(0o644),
            ),
        );

        devices.insert(
            "urandom".to_string(),
            DeviceNode::new_char_device(
                5,
                DeviceType::URandom,
                1,
                9,
                FilePermissions::from_octal(0o644),
            ),
        );

        devices.insert(
            "console".to_string(),
            DeviceNode::new_char_device(
                6,
                DeviceType::Console,
                5,
                1,
                FilePermissions::from_octal(0o600),
            ),
        );

        devices.insert(
            "stdin".to_string(),
            DeviceNode::new_char_device(
                7,
                DeviceType::Stdin,
                1,
                0,
                FilePermissions::from_octal(0o400),
            ),
        );

        devices.insert(
            "stdout".to_string(),
            DeviceNode::new_char_device(
                8,
                DeviceType::Stdout,
                1,
                1,
                FilePermissions::from_octal(0o200),
            ),
        );

        devices.insert(
            "stderr".to_string(),
            DeviceNode::new_char_device(
                9,
                DeviceType::Stderr,
                1,
                2,
                FilePermissions::from_octal(0o200),
            ),
        );

        devices.insert(
            "mem".to_string(),
            DeviceNode::new_char_device(
                10,
                DeviceType::Memory,
                1,
                1,
                FilePermissions::from_octal(0o640),
            ),
        );

        devices.insert(
            "kmem".to_string(),
            DeviceNode::new_char_device(
                11,
                DeviceType::KernelMemory,
                1,
                2,
                FilePermissions::from_octal(0o640),
            ),
        );

        devices.insert(
            "full".to_string(),
            DeviceNode::new_char_device(
                12,
                DeviceType::Full,
                1,
                7,
                FilePermissions::from_octal(0o666),
            ),
        );

        let root_metadata = FileMetadata::new(root_inode, FileType::Directory, 0);

        Self {
            devices: RwLock::new(devices),
            root_metadata,
            prng_state: RwLock::new(0x123456789abcdef0),
            next_inode: RwLock::new(100),
        }
    }

    /// Generate pseudo-random bytes
    fn generate_random(&self, buffer: &mut [u8]) {
        let mut state = self.prng_state.write();

        for byte in buffer.iter_mut() {
            // Simple linear congruential generator
            *state = state.wrapping_mul(1103515245).wrapping_add(12345);
            *byte = (*state >> 16) as u8;
        }
    }

    /// Find device by path
    fn find_device(&self, path: &str) -> Option<DeviceNode> {
        if path == "/" {
            return None; // Root directory
        }

        let path = path.strip_prefix('/').unwrap_or(path);
        let devices = self.devices.read();
        devices.get(path).cloned()
    }

    /// Get device inode by name
    fn get_device_inode(&self, name: &str) -> Option<InodeNumber> {
        let devices = self.devices.read();
        devices.get(name).map(|dev| dev.metadata.inode)
    }

    /// Add a device node dynamically
    pub fn add_device(
        &self,
        name: &str,
        device_type: DeviceType,
        major: u32,
        minor: u32,
        permissions: FilePermissions,
    ) -> FsResult<InodeNumber> {
        let mut devices = self.devices.write();
        if devices.contains_key(name) {
            return Err(FsError::AlreadyExists);
        }

        let mut next_inode = self.next_inode.write();
        let inode = *next_inode;
        *next_inode += 1;

        let node = DeviceNode::new_char_device(inode, device_type, major, minor, permissions);
        devices.insert(name.to_string(), node);
        Ok(inode)
    }

    /// Remove a device node dynamically
    pub fn remove_device(&self, name: &str) -> FsResult<()> {
        let mut devices = self.devices.write();
        if devices.remove(name).is_none() {
            return Err(FsError::NotFound);
        }
        Ok(())
    }
}

impl FileSystem for DevFs {
    fn fs_type(&self) -> FileSystemType {
        FileSystemType::DevFs
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        let devices = self.devices.read();
        let device_count = devices.len() as u64;

        Ok(FileSystemStats {
            total_blocks: 0, // Virtual filesystem
            free_blocks: 0,
            available_blocks: 0,
            total_inodes: device_count + 1, // Devices + root
            free_inodes: 0,                 // All inodes are used
            block_size: 4096,
            max_filename_length: 255,
        })
    }

    fn create(&self, _path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        // Device filesystem is read-only for regular file creation
        Err(FsError::ReadOnly)
    }

    fn open(&self, path: &str, _flags: OpenFlags) -> FsResult<InodeNumber> {
        if path == "/" {
            return Ok(self.root_metadata.inode);
        }

        let device = self.find_device(path).ok_or(FsError::NotFound)?;
        Ok(device.metadata.inode)
    }

    fn read(&self, inode: InodeNumber, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        // Find device by inode
        let devices = self.devices.read();
        let device = devices
            .values()
            .find(|dev| dev.metadata.inode == inode)
            .ok_or(FsError::NotFound)?;

        match device.device_type {
            DeviceType::Null => {
                // /dev/null always returns EOF
                Ok(0)
            }
            DeviceType::Zero => {
                // /dev/zero returns zeros
                buffer.fill(0);
                Ok(buffer.len())
            }
            DeviceType::Random | DeviceType::URandom => {
                // Generate random data
                drop(devices); // Release lock before calling generate_random
                self.generate_random(buffer);
                Ok(buffer.len())
            }
            DeviceType::Console | DeviceType::Stdin => {
                // Read from keyboard buffer
                use crate::keyboard::get_scancode;
                let mut bytes_read = 0;

                // Try to read available characters from keyboard
                for i in 0..buffer.len() {
                    if let Some(scancode) = get_scancode() {
                        // Convert scancode to ASCII if possible
                        if let Some(ascii) = self.scancode_to_ascii(scancode) {
                            buffer[i] = ascii;
                            bytes_read += 1;
                        }
                    } else {
                        break;
                    }
                }

                Ok(bytes_read)
            }
            DeviceType::Full => {
                // /dev/full behaves like /dev/zero for reads
                buffer.fill(0);
                Ok(buffer.len())
            }
            DeviceType::Memory => {
                // /dev/mem: read physical memory at the given offset.
                // We use the direct physical-memory mapping established by
                // the memory manager to safely access physical addresses.
                if offset.checked_add(buffer.len() as u64).is_none() {
                    return Err(FsError::InvalidArgument);
                }
                let phys_offset = crate::memory::get_physical_memory_offset();
                let src = (phys_offset + offset) as *const u8;
                // SAFETY: We read from the direct physical-memory mapping.
                // The caller is responsible for ensuring the offset is a
                // valid physical address. We trust the memory manager's
                // mapping to cover the entire physical address space.
                unsafe {
                    core::ptr::copy_nonoverlapping(src, buffer.as_mut_ptr(), buffer.len());
                }
                Ok(buffer.len())
            }
            DeviceType::KernelMemory => {
                // /dev/kmem: read kernel virtual memory at the given offset.
                // This provides direct access to the kernel's virtual address
                // space, which is useful for debugging and crash analysis.
                let src = offset as *const u8;
                // SAFETY: The caller is responsible for ensuring the offset
                // is a valid kernel virtual address. This is the same trust
                // model as Linux /dev/kmem.
                unsafe {
                    core::ptr::copy_nonoverlapping(src, buffer.as_mut_ptr(), buffer.len());
                }
                Ok(buffer.len())
            }
            _ => Err(FsError::NotSupported),
        }
    }

    fn write(&self, inode: InodeNumber, offset: u64, buffer: &[u8]) -> FsResult<usize> {
        // Find device by inode
        let devices = self.devices.read();
        let device = devices
            .values()
            .find(|dev| dev.metadata.inode == inode)
            .ok_or(FsError::NotFound)?;

        match device.device_type {
            DeviceType::Null => {
                // /dev/null discards all data
                Ok(buffer.len())
            }
            DeviceType::Zero => {
                // /dev/zero discards writes
                Ok(buffer.len())
            }
            DeviceType::Console | DeviceType::Stdout | DeviceType::Stderr => {
                // Write to console output
                use crate::vga_buffer::{write_bytes, write_string};

                // Try to convert to string first
                if let Ok(text) = core::str::from_utf8(buffer) {
                    write_string(text);
                } else {
                    // Write raw bytes
                    write_bytes(buffer);
                }
                Ok(buffer.len())
            }
            DeviceType::Full => {
                // /dev/full always returns "no space left"
                Err(FsError::NoSpaceLeft)
            }
            DeviceType::Random | DeviceType::URandom => {
                // Random devices don't accept writes (or use them to seed)
                Ok(buffer.len())
            }
            DeviceType::Memory => {
                // /dev/mem: write to physical memory at the given offset.
                if offset.checked_add(buffer.len() as u64).is_none() {
                    return Err(FsError::InvalidArgument);
                }
                let phys_offset = crate::memory::get_physical_memory_offset();
                let dst = (phys_offset + offset) as *mut u8;
                // SAFETY: Same trust model as the read path — the caller
                // ensures the offset is a valid physical address.
                unsafe {
                    core::ptr::copy_nonoverlapping(buffer.as_ptr(), dst, buffer.len());
                }
                Ok(buffer.len())
            }
            DeviceType::KernelMemory => {
                // /dev/kmem: write to kernel virtual memory at the given offset.
                let dst = offset as *mut u8;
                // SAFETY: Same trust model as the read path.
                unsafe {
                    core::ptr::copy_nonoverlapping(buffer.as_ptr(), dst, buffer.len());
                }
                Ok(buffer.len())
            }
            _ => Err(FsError::NotSupported),
        }
    }

    fn metadata(&self, inode: InodeNumber) -> FsResult<FileMetadata> {
        if inode == self.root_metadata.inode {
            return Ok(self.root_metadata.clone());
        }

        let devices = self.devices.read();
        let device = devices
            .values()
            .find(|dev| dev.metadata.inode == inode)
            .ok_or(FsError::NotFound)?;

        Ok(device.metadata.clone())
    }

    fn set_metadata(&self, inode: InodeNumber, _metadata: &FileMetadata) -> FsResult<()> {
        if inode == self.root_metadata.inode {
            return Err(FsError::PermissionDenied);
        }

        // Device nodes are generally not modifiable
        Err(FsError::PermissionDenied)
    }

    fn mkdir(&self, _path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        // Device filesystem doesn't support creating directories
        Err(FsError::ReadOnly)
    }

    fn rmdir(&self, _path: &str) -> FsResult<()> {
        // Device filesystem doesn't support removing directories
        Err(FsError::ReadOnly)
    }

    fn unlink(&self, _path: &str) -> FsResult<()> {
        // Device filesystem doesn't support removing files
        Err(FsError::ReadOnly)
    }

    fn readdir(&self, inode: InodeNumber) -> FsResult<Vec<DirectoryEntry>> {
        if inode != self.root_metadata.inode {
            return Err(FsError::NotADirectory);
        }

        let mut entries = Vec::new();

        // Add . and .. entries
        entries.push(DirectoryEntry {
            name: ".".to_string(),
            inode: self.root_metadata.inode,
            file_type: FileType::Directory,
        });

        entries.push(DirectoryEntry {
            name: "..".to_string(),
            inode: self.root_metadata.inode,
            file_type: FileType::Directory,
        });

        // Add device entries
        let devices = self.devices.read();
        for (name, device) in devices.iter() {
            entries.push(DirectoryEntry {
                name: name.to_string(),
                inode: device.metadata.inode,
                file_type: device.metadata.file_type,
            });
        }

        Ok(entries)
    }

    fn rename(&self, _old_path: &str, _new_path: &str) -> FsResult<()> {
        // Device filesystem doesn't support renaming
        Err(FsError::ReadOnly)
    }

    fn symlink(&self, _target: &str, _link_path: &str) -> FsResult<()> {
        // Device filesystem doesn't support creating symlinks
        Err(FsError::ReadOnly)
    }

    fn readlink(&self, _path: &str) -> FsResult<String> {
        // No symlinks in device filesystem
        Err(FsError::InvalidArgument)
    }

    fn sync(&self) -> FsResult<()> {
        // Device filesystem doesn't need syncing
        Ok(())
    }
}

impl DevFs {
    /// Convert scancode to ASCII character
    fn scancode_to_ascii(&self, scancode: u8) -> Option<u8> {
        // Basic scancode to ASCII mapping for US keyboard layout
        match scancode {
            0x1E => Some(b'a'),
            0x30 => Some(b'b'),
            0x2E => Some(b'c'),
            0x20 => Some(b'd'),
            0x12 => Some(b'e'),
            0x21 => Some(b'f'),
            0x22 => Some(b'g'),
            0x23 => Some(b'h'),
            0x17 => Some(b'i'),
            0x24 => Some(b'j'),
            0x25 => Some(b'k'),
            0x26 => Some(b'l'),
            0x32 => Some(b'm'),
            0x31 => Some(b'n'),
            0x18 => Some(b'o'),
            0x19 => Some(b'p'),
            0x10 => Some(b'q'),
            0x13 => Some(b'r'),
            0x1F => Some(b's'),
            0x14 => Some(b't'),
            0x16 => Some(b'u'),
            0x2F => Some(b'v'),
            0x11 => Some(b'w'),
            0x2D => Some(b'x'),
            0x15 => Some(b'y'),
            0x2C => Some(b'z'),
            0x02 => Some(b'1'),
            0x03 => Some(b'2'),
            0x04 => Some(b'3'),
            0x05 => Some(b'4'),
            0x06 => Some(b'5'),
            0x07 => Some(b'6'),
            0x08 => Some(b'7'),
            0x09 => Some(b'8'),
            0x0A => Some(b'9'),
            0x0B => Some(b'0'),
            0x39 => Some(b' '),  // Space
            0x1C => Some(b'\n'), // Enter
            0x0E => Some(0x08),  // Backspace
            _ => None,
        }
    }
}

/// Create device node (for use by device drivers)
pub fn create_device_node(
    name: &str,
    device_type: DeviceType,
    major: u32,
    minor: u32,
    permissions: FilePermissions,
) -> FsResult<()> {
    let dev_fs = get_devfs()?;
    dev_fs.add_device(name, device_type, major, minor, permissions)?;
    Ok(())
}

/// Remove device node
pub fn remove_device_node(name: &str) -> FsResult<()> {
    let dev_fs = get_devfs()?;
    dev_fs.remove_device(name)
}

/// Get the global DevFs instance if it has been registered.
fn get_devfs() -> FsResult<&'static DevFs> {
    GLOBAL_DEVFS
        .read()
        .as_ref()
        .copied()
        .ok_or(FsError::NotSupported)
}

/// Register the global DevFs instance (called during VFS init).
pub fn register_devfs(devfs: &'static DevFs) {
    *GLOBAL_DEVFS.write() = Some(devfs);
}

static GLOBAL_DEVFS: RwLock<Option<&'static DevFs>> = RwLock::new(None);

/// Thin wrapper around a `&'static DevFs` that implements `FileSystem`.
///
/// This lets the same DevFs instance be mounted at `/dev` and also used by
/// dynamic device registration.
#[derive(Debug)]
pub struct DevFsMount(pub &'static DevFs);

impl FileSystem for DevFsMount {
    fn fs_type(&self) -> FileSystemType { self.0.fs_type() }
    fn statfs(&self) -> FsResult<FileSystemStats> { self.0.statfs() }
    fn create(&self, path: &str, permissions: FilePermissions) -> FsResult<InodeNumber> { self.0.create(path, permissions) }
    fn open(&self, path: &str, flags: OpenFlags) -> FsResult<InodeNumber> { self.0.open(path, flags) }
    fn read(&self, inode: InodeNumber, offset: u64, buffer: &mut [u8]) -> FsResult<usize> { self.0.read(inode, offset, buffer) }
    fn write(&self, inode: InodeNumber, offset: u64, buffer: &[u8]) -> FsResult<usize> { self.0.write(inode, offset, buffer) }
    fn metadata(&self, inode: InodeNumber) -> FsResult<FileMetadata> { self.0.metadata(inode) }
    fn set_metadata(&self, inode: InodeNumber, metadata: &FileMetadata) -> FsResult<()> { self.0.set_metadata(inode, metadata) }
    fn mkdir(&self, path: &str, permissions: FilePermissions) -> FsResult<InodeNumber> { self.0.mkdir(path, permissions) }
    fn rmdir(&self, path: &str) -> FsResult<()> { self.0.rmdir(path) }
    fn unlink(&self, path: &str) -> FsResult<()> { self.0.unlink(path) }
    fn readdir(&self, inode: InodeNumber) -> FsResult<Vec<DirectoryEntry>> { self.0.readdir(inode) }
    fn rename(&self, old_path: &str, new_path: &str) -> FsResult<()> { self.0.rename(old_path, new_path) }
    fn symlink(&self, target: &str, link_path: &str) -> FsResult<()> { self.0.symlink(target, link_path) }
    fn readlink(&self, path: &str) -> FsResult<String> { self.0.readlink(path) }
    fn sync(&self) -> FsResult<()> { self.0.sync() }
}
