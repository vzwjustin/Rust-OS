//! Device filesystem implementation
//!
//! This module provides a device filesystem that exposes system devices
//! as files in the /dev directory. It includes standard devices like
//! null, zero, random, and console.
// Import handled by parent module


use super::{
    FileSystem, FileSystemType, FileSystemStats, FileMetadata, FileType, FilePermissions,
    DirectoryEntry, OpenFlags, FsResult, FsError, InodeNumber,
};
use alloc::{vec::Vec, string::{String, ToString}, collections::BTreeMap};
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
}

impl DevFs {
    /// Create a new device filesystem
    pub fn new() -> Self {
        let mut devices = BTreeMap::new();
        let root_inode = 1;

        // Create standard device nodes
        devices.insert(
            "null".to_string(),
            DeviceNode::new_char_device(2, DeviceType::Null, 1, 3, FilePermissions::from_octal(0o666))
        );

        devices.insert(
            "zero".to_string(),
            DeviceNode::new_char_device(3, DeviceType::Zero, 1, 5, FilePermissions::from_octal(0o666))
        );

        devices.insert(
            "random".to_string(),
            DeviceNode::new_char_device(4, DeviceType::Random, 1, 8, FilePermissions::from_octal(0o644))
        );

        devices.insert(
            "urandom".to_string(),
            DeviceNode::new_char_device(5, DeviceType::URandom, 1, 9, FilePermissions::from_octal(0o644))
        );

        devices.insert(
            "console".to_string(),
            DeviceNode::new_char_device(6, DeviceType::Console, 5, 1, FilePermissions::from_octal(0o600))
        );

        devices.insert(
            "stdin".to_string(),
            DeviceNode::new_char_device(7, DeviceType::Stdin, 1, 0, FilePermissions::from_octal(0o400))
        );

        devices.insert(
            "stdout".to_string(),
            DeviceNode::new_char_device(8, DeviceType::Stdout, 1, 1, FilePermissions::from_octal(0o200))
        );

        devices.insert(
            "stderr".to_string(),
            DeviceNode::new_char_device(9, DeviceType::Stderr, 1, 2, FilePermissions::from_octal(0o200))
        );

        devices.insert(
            "mem".to_string(),
            DeviceNode::new_char_device(10, DeviceType::Memory, 1, 1, FilePermissions::from_octal(0o640))
        );

        devices.insert(
            "kmem".to_string(),
            DeviceNode::new_char_device(11, DeviceType::KernelMemory, 1, 2, FilePermissions::from_octal(0o640))
        );

        devices.insert(
            "full".to_string(),
            DeviceNode::new_char_device(12, DeviceType::Full, 1, 7, FilePermissions::from_octal(0o666))
        );

        let root_metadata = FileMetadata::new(root_inode, FileType::Directory, 0);

        Self {
            devices: RwLock::new(devices),
            root_metadata,
            prng_state: RwLock::new(0x123456789abcdef0),
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
            free_inodes: 0, // All inodes are used
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

    fn read(&self, inode: InodeNumber, _offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        // Find device by inode
        let devices = self.devices.read();
        let device = devices.values()
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
            DeviceType::Memory | DeviceType::KernelMemory => {
                // Memory devices require special handling
                // For now, return permission denied
                Err(FsError::PermissionDenied)
            }
            _ => Err(FsError::NotSupported),
        }
    }

    fn write(&self, inode: InodeNumber, _offset: u64, buffer: &[u8]) -> FsResult<usize> {
        // Find device by inode
        let devices = self.devices.read();
        let device = devices.values()
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
                use crate::vga_buffer::{write_string, write_bytes};

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
            DeviceType::Memory | DeviceType::KernelMemory => {
                // Memory devices require special handling
                Err(FsError::PermissionDenied)
            }
            _ => Err(FsError::NotSupported),
        }
    }

    fn metadata(&self, inode: InodeNumber) -> FsResult<FileMetadata> {
        if inode == self.root_metadata.inode {
            return Ok(self.root_metadata.clone());
        }

        let devices = self.devices.read();
        let device = devices.values()
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
                name: name.clone(),
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
            0x1E => Some(b'a'), 0x30 => Some(b'b'), 0x2E => Some(b'c'), 0x20 => Some(b'd'),
            0x12 => Some(b'e'), 0x21 => Some(b'f'), 0x22 => Some(b'g'), 0x23 => Some(b'h'),
            0x17 => Some(b'i'), 0x24 => Some(b'j'), 0x25 => Some(b'k'), 0x26 => Some(b'l'),
            0x32 => Some(b'm'), 0x31 => Some(b'n'), 0x18 => Some(b'o'), 0x19 => Some(b'p'),
            0x10 => Some(b'q'), 0x13 => Some(b'r'), 0x1F => Some(b's'), 0x14 => Some(b't'),
            0x16 => Some(b'u'), 0x2F => Some(b'v'), 0x11 => Some(b'w'), 0x2D => Some(b'x'),
            0x15 => Some(b'y'), 0x2C => Some(b'z'),
            0x02 => Some(b'1'), 0x03 => Some(b'2'), 0x04 => Some(b'3'), 0x05 => Some(b'4'),
            0x06 => Some(b'5'), 0x07 => Some(b'6'), 0x08 => Some(b'7'), 0x09 => Some(b'8'),
            0x0A => Some(b'9'), 0x0B => Some(b'0'),
            0x39 => Some(b' '), // Space
            0x1C => Some(b'\n'), // Enter
            0x0E => Some(0x08), // Backspace
            _ => None,
        }
    }
}

/// Create a device node (for use by device drivers)
pub fn create_device_node(
    _name: &str,
    _device_type: DeviceType,
    _major: u32,
    _minor: u32,
    _permissions: FilePermissions,
) -> FsResult<()> {
    // This would be called by device drivers to register new devices
    // For now, we only support the predefined devices
    Err(FsError::NotSupported)
}

/// Remove a device node
pub fn remove_device_node(_name: &str) -> FsResult<()> {
    // This would be called when a device is removed
    Err(FsError::NotSupported)
}
