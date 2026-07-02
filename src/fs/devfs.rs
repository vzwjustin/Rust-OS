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
    /// Real-time clock (/dev/rtc)
    Rtc,
    /// Watchdog timer (/dev/watchdog)
    Watchdog,
    /// Block storage device (/dev/vda, /dev/sda, etc.)
    BlockStorage,
    /// Input event device (/dev/input0, /dev/input1, ...). Emits Linux
    /// `struct input_event` records sourced from `crate::drivers::input_manager`.
    Input,
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
    /// Create a new block device node
    fn new_block_device(
        inode: InodeNumber,
        device_type: DeviceType,
        major: u32,
        minor: u32,
        permissions: FilePermissions,
    ) -> Self {
        let mut metadata = FileMetadata::new(inode, FileType::BlockDevice, 0);
        metadata.permissions = permissions;
        metadata.device_id = Some((major << 8) | minor);

        Self {
            device_type,
            metadata,
            major,
            minor,
        }
    }

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

        devices.insert(
            "rtc".to_string(),
            DeviceNode::new_char_device(
                13,
                DeviceType::Rtc,
                10,
                135,
                FilePermissions::from_octal(0o644),
            ),
        );

        devices.insert(
            "watchdog".to_string(),
            DeviceNode::new_char_device(
                14,
                DeviceType::Watchdog,
                10,
                130,
                FilePermissions::from_octal(0o600),
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

        let node = match device_type {
            DeviceType::BlockStorage => {
                DeviceNode::new_block_device(inode, device_type, major, minor, permissions)
            }
            _ => DeviceNode::new_char_device(inode, device_type, major, minor, permissions),
        };
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
            DeviceType::Rtc => {
                let time = crate::drivers::rtc::read_time().map_err(|_| FsError::IoError)?;
                let bytes = unsafe {
                    core::slice::from_raw_parts(
                        &time as *const crate::drivers::rtc::RtcTime as *const u8,
                        core::mem::size_of::<crate::drivers::rtc::RtcTime>(),
                    )
                };
                let to_copy = core::cmp::min(buffer.len(), bytes.len());
                buffer[..to_copy].copy_from_slice(&bytes[..to_copy]);
                Ok(to_copy)
            }
            DeviceType::Watchdog => {
                let left = crate::drivers::watchdog::get_timeleft();
                let s = alloc::format!("{}\n", left);
                let bytes = s.as_bytes();
                let to_copy = core::cmp::min(buffer.len(), bytes.len());
                buffer[..to_copy].copy_from_slice(&bytes[..to_copy]);
                Ok(to_copy)
            }
            DeviceType::BlockStorage => {
                // Route through the block_io layer using major/minor.
                let sector = offset / 512;
                crate::block_io::read_sectors(device.major, device.minor, sector, buffer)
                    .map_err(|_| FsError::IoError)?;
                Ok(buffer.len())
            }
            DeviceType::Input => {
                drop(devices);
                read_input_event(buffer)
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
            DeviceType::Rtc => {
                if buffer.len() < core::mem::size_of::<crate::drivers::rtc::RtcTime>() {
                    return Err(FsError::InvalidArgument);
                }
                let time = unsafe { *(buffer.as_ptr() as *const crate::drivers::rtc::RtcTime) };
                crate::drivers::rtc::write_time(&time).map_err(|_| FsError::IoError)?;
                Ok(core::mem::size_of::<crate::drivers::rtc::RtcTime>())
            }
            DeviceType::Watchdog => {
                crate::drivers::watchdog::kick();
                Ok(buffer.len())
            }
            DeviceType::BlockStorage => {
                // Route through the block_io layer using major/minor.
                let sector = offset / 512;
                crate::block_io::write_sectors(device.major, device.minor, sector, buffer)
                    .map_err(|_| FsError::IoError)?;
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

// ── /dev/inputN support ─────────────────────────────────────────────────
//
// Encodes real events from `crate::drivers::input_manager` as Linux
// `struct input_event { time, type, code, value }` records (24 bytes on
// x86_64: 2x i64 timeval + u16 + u16 + i32), mirroring the encoding used by
// `crate::vfs::devfs`'s already-wired `/dev/input/event0` node so userspace
// evdev consumers (e.g. libinput probes) see the same wire format regardless
// of which devfs tree services the open() call.

const INPUT_EVENT_SIZE: usize = 24;
const EV_SYN: u16 = 0;
const EV_KEY: u16 = 1;
const EV_REL: u16 = 2;
const EV_ABS: u16 = 3;
const SYN_REPORT: u16 = 0;
const REL_WHEEL: u16 = 8;
const ABS_X: u16 = 0;
const ABS_Y: u16 = 1;
const BTN_LEFT: u16 = 0x110;
const BTN_RIGHT: u16 = 0x111;
const BTN_MIDDLE: u16 = 0x112;
const BTN_SIDE: u16 = 0x113;
const BTN_EXTRA: u16 = 0x114;

fn put_input_event(buf: &mut [u8], ty: u16, code: u16, value: i32) -> usize {
    if buf.len() < INPUT_EVENT_SIZE {
        return 0;
    }

    let uptime_us = crate::time::uptime_ms().saturating_mul(1000);
    let sec = (uptime_us / 1_000_000) as i64;
    let usec = (uptime_us % 1_000_000) as i64;

    buf[0..8].copy_from_slice(&sec.to_le_bytes());
    buf[8..16].copy_from_slice(&usec.to_le_bytes());
    buf[16..18].copy_from_slice(&ty.to_le_bytes());
    buf[18..20].copy_from_slice(&code.to_le_bytes());
    buf[20..24].copy_from_slice(&value.to_le_bytes());
    INPUT_EVENT_SIZE
}

fn key_event_code(event: crate::keyboard::KeyEvent) -> Option<u16> {
    match event {
        crate::keyboard::KeyEvent::CharacterPress(c)
        | crate::keyboard::KeyEvent::CharacterRelease(c) => ascii_key_code(c),
        crate::keyboard::KeyEvent::SpecialPress(key)
        | crate::keyboard::KeyEvent::SpecialRelease(key) => special_key_code(key),
        crate::keyboard::KeyEvent::RawPress(code) | crate::keyboard::KeyEvent::RawRelease(code) => {
            Some(code as u16)
        }
    }
}

fn ascii_key_code(c: char) -> Option<u16> {
    let c = c.to_ascii_lowercase();
    match c {
        'a'..='z' => Some(30 + (c as u8 - b'a') as u16),
        '1'..='9' => Some(2 + (c as u8 - b'1') as u16),
        '0' => Some(11),
        ' ' => Some(57),
        '\n' | '\r' => Some(28),
        '\t' => Some(15),
        _ => None,
    }
}

fn special_key_code(key: crate::keyboard::SpecialKey) -> Option<u16> {
    use crate::keyboard::SpecialKey;

    match key {
        SpecialKey::Escape => Some(1),
        SpecialKey::Backspace => Some(14),
        SpecialKey::Tab => Some(15),
        SpecialKey::Enter => Some(28),
        SpecialKey::LeftCtrl => Some(29),
        SpecialKey::LeftShift => Some(42),
        SpecialKey::RightShift => Some(54),
        SpecialKey::LeftAlt => Some(56),
        SpecialKey::Space => Some(57),
        SpecialKey::CapsLock => Some(58),
        SpecialKey::F1 => Some(59),
        SpecialKey::F2 => Some(60),
        SpecialKey::F3 => Some(61),
        SpecialKey::F4 => Some(62),
        SpecialKey::F5 => Some(63),
        SpecialKey::F6 => Some(64),
        SpecialKey::F7 => Some(65),
        SpecialKey::F8 => Some(66),
        SpecialKey::F9 => Some(67),
        SpecialKey::F10 => Some(68),
        SpecialKey::F11 => Some(87),
        SpecialKey::F12 => Some(88),
        SpecialKey::ArrowUp => Some(103),
        SpecialKey::ArrowLeft => Some(105),
        SpecialKey::ArrowRight => Some(106),
        SpecialKey::ArrowDown => Some(108),
        SpecialKey::Home => Some(102),
        SpecialKey::End => Some(107),
        SpecialKey::PageUp => Some(104),
        SpecialKey::PageDown => Some(109),
        SpecialKey::Insert => Some(110),
        SpecialKey::Delete => Some(111),
        SpecialKey::NumLock => Some(69),
        SpecialKey::ScrollLock => Some(70),
    }
}

fn mouse_button_code(button: crate::drivers::input_manager::MouseButton) -> u16 {
    match button {
        crate::drivers::input_manager::MouseButton::Left => BTN_LEFT,
        crate::drivers::input_manager::MouseButton::Right => BTN_RIGHT,
        crate::drivers::input_manager::MouseButton::Middle => BTN_MIDDLE,
        crate::drivers::input_manager::MouseButton::Button4 => BTN_SIDE,
        crate::drivers::input_manager::MouseButton::Button5 => BTN_EXTRA,
    }
}

/// Read the next pending input event (if any) into `buf`, encoded as one or
/// more back-to-back `struct input_event` records terminated by a SYN_REPORT.
/// Returns `Ok(0)` (not an error) when no event is currently queued, matching
/// the non-blocking-read convention `crate::drivers::input_manager` callers
/// already rely on elsewhere in the kernel.
fn read_input_event(buf: &mut [u8]) -> FsResult<usize> {
    if buf.len() < INPUT_EVENT_SIZE {
        return Err(FsError::InvalidArgument);
    }

    let Some(event) = crate::drivers::input_manager::get_event() else {
        return Ok(0);
    };

    let mut written = match event {
        crate::drivers::input_manager::InputEvent::KeyPress(key) => {
            let Some(code) = key_event_code(key) else {
                return Ok(0);
            };
            put_input_event(buf, EV_KEY, code, 1)
        }
        crate::drivers::input_manager::InputEvent::KeyRelease(key) => {
            let Some(code) = key_event_code(key) else {
                return Ok(0);
            };
            put_input_event(buf, EV_KEY, code, 0)
        }
        crate::drivers::input_manager::InputEvent::MouseButtonDown { button, .. } => {
            put_input_event(buf, EV_KEY, mouse_button_code(button), 1)
        }
        crate::drivers::input_manager::InputEvent::MouseButtonUp { button, .. } => {
            put_input_event(buf, EV_KEY, mouse_button_code(button), 0)
        }
        crate::drivers::input_manager::InputEvent::MouseScroll { delta, .. } => {
            put_input_event(buf, EV_REL, REL_WHEEL, delta as i32)
        }
        crate::drivers::input_manager::InputEvent::MouseMove { x, y } => {
            let mut total = put_input_event(buf, EV_ABS, ABS_X, x as i32);
            if buf.len() >= total + INPUT_EVENT_SIZE {
                total += put_input_event(&mut buf[total..], EV_ABS, ABS_Y, y as i32);
            }
            total
        }
    };

    if buf.len() >= written + INPUT_EVENT_SIZE {
        written += put_input_event(&mut buf[written..], EV_SYN, SYN_REPORT, 0);
    }
    Ok(written)
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

/// Register all block_io devices as /dev nodes.
/// Call this after devfs has been mounted and block_io has been initialized.
pub fn register_block_devices() {
    let devices = crate::block_io::list_block_devices();
    for (major, minor, name, _capacity) in devices {
        let dev_name = if name.starts_with("virtio") {
            alloc::format!("vda{}", if minor > 0 { minor } else { 1 })
        } else if name.starts_with("sw-blk") {
            alloc::format!("ramdisk{}", if minor > 0 { minor } else { 0 })
        } else {
            alloc::format!("{}{}", name, minor)
        };
        let _ = create_device_node(
            &dev_name,
            DeviceType::BlockStorage,
            major,
            minor,
            FilePermissions::from_octal(0o660),
        );
        crate::serial_println!(
            "[devfs] registered /dev/{} (block, major={}, minor={})",
            dev_name,
            major,
            minor
        );
    }
}

static GLOBAL_DEVFS: RwLock<Option<&'static DevFs>> = RwLock::new(None);

/// Thin wrapper around a `&'static DevFs` that implements `FileSystem`.
///
/// This lets the same DevFs instance be mounted at `/dev` and also used by
/// dynamic device registration.
#[derive(Debug)]
pub struct DevFsMount(pub &'static DevFs);

impl FileSystem for DevFsMount {
    fn fs_type(&self) -> FileSystemType {
        self.0.fs_type()
    }
    fn statfs(&self) -> FsResult<FileSystemStats> {
        self.0.statfs()
    }
    fn create(&self, path: &str, permissions: FilePermissions) -> FsResult<InodeNumber> {
        self.0.create(path, permissions)
    }
    fn open(&self, path: &str, flags: OpenFlags) -> FsResult<InodeNumber> {
        self.0.open(path, flags)
    }
    fn read(&self, inode: InodeNumber, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        self.0.read(inode, offset, buffer)
    }
    fn write(&self, inode: InodeNumber, offset: u64, buffer: &[u8]) -> FsResult<usize> {
        self.0.write(inode, offset, buffer)
    }
    fn metadata(&self, inode: InodeNumber) -> FsResult<FileMetadata> {
        self.0.metadata(inode)
    }
    fn set_metadata(&self, inode: InodeNumber, metadata: &FileMetadata) -> FsResult<()> {
        self.0.set_metadata(inode, metadata)
    }
    fn mkdir(&self, path: &str, permissions: FilePermissions) -> FsResult<InodeNumber> {
        self.0.mkdir(path, permissions)
    }
    fn rmdir(&self, path: &str) -> FsResult<()> {
        self.0.rmdir(path)
    }
    fn unlink(&self, path: &str) -> FsResult<()> {
        self.0.unlink(path)
    }
    fn readdir(&self, inode: InodeNumber) -> FsResult<Vec<DirectoryEntry>> {
        self.0.readdir(inode)
    }
    fn rename(&self, old_path: &str, new_path: &str) -> FsResult<()> {
        self.0.rename(old_path, new_path)
    }
    fn symlink(&self, target: &str, link_path: &str) -> FsResult<()> {
        self.0.symlink(target, link_path)
    }
    fn readlink(&self, path: &str) -> FsResult<String> {
        self.0.readlink(path)
    }
    fn sync(&self) -> FsResult<()> {
        self.0.sync()
    }
}
