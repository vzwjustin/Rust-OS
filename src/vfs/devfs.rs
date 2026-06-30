//! Device nodes under `/dev` for the syscall-facing VFS.

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::string::String;

use lazy_static::lazy_static;
use spin::Mutex;

#[derive(Debug, Clone, Copy)]
pub struct BlockDeviceSpec {
    pub device_id: u32,
    pub start_sector: u64,
    pub sector_count: u64,
}

lazy_static! {
    static ref BLOCK_DEVICE_REGISTRY: Mutex<BTreeMap<String, BlockDeviceSpec>> =
        Mutex::new(BTreeMap::new());
}

/// Resolve `/dev/sda2` style paths to partition geometry.
pub fn block_device_spec(path: &str) -> Option<BlockDeviceSpec> {
    let name = path.trim().strip_prefix("/dev/")?;
    (*BLOCK_DEVICE_REGISTRY).lock().get(name).copied()
}

fn register_block_device(name: &str, spec: BlockDevSpec) {
    (*BLOCK_DEVICE_REGISTRY).lock().insert(
        String::from(name),
        BlockDeviceSpec {
            device_id: spec.device_id,
            start_sector: spec.start_sector,
            sector_count: spec.sector_count,
        },
    );
}

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
    Tty,
    Ptmx,
    InputEvent,
}

struct DevInode {
    ino: u64,
    kind: DevKind,
    mode: u32,
    prng: AtomicU64,
    /// For InputEvent nodes: which evdev device index (0=event0, 1=event1, etc.)
    dev_index: u8,
}

impl DevInode {
    fn new(ino: u64, kind: DevKind, mode: u32) -> Arc<Self> {
        Arc::new(Self {
            ino,
            kind,
            mode,
            prng: AtomicU64::new(0x1234_5678_9abc_def0),
            dev_index: 0,
        })
    }

    fn new_with_index(ino: u64, kind: DevKind, mode: u32, dev_index: u8) -> Arc<Self> {
        Arc::new(Self {
            ino,
            kind,
            mode,
            prng: AtomicU64::new(0x1234_5678_9abc_def0),
            dev_index,
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

fn read_input_event(buf: &mut [u8], dev_index: u8) -> VfsResult<usize> {
    if buf.len() < INPUT_EVENT_SIZE {
        return Err(VfsError::InvalidArgument);
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
            DevKind::Console | DevKind::Tty => {
                let n = crate::drivers::tty::devfs_read(crate::drivers::tty::TtyId::Console, buf);
                Ok(n)
            }
            DevKind::Ptmx => Ok(0),
            DevKind::InputEvent => read_input_event(buf, self.dev_index),
        }
    }

    fn write_at(&self, _offset: u64, buf: &[u8]) -> VfsResult<usize> {
        match self.kind {
            DevKind::Null | DevKind::Zero | DevKind::Random | DevKind::URandom => Ok(buf.len()),
            DevKind::Full => Err(VfsError::NoSpace),
            DevKind::Console | DevKind::Tty => {
                let n = crate::drivers::tty::devfs_write(crate::drivers::tty::TtyId::Console, buf);
                Ok(n)
            }
            DevKind::Ptmx => Err(VfsError::NotSupported),
            DevKind::InputEvent => Err(VfsError::NotSupported),
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
            DevKind::Tty => (5, 0),
            DevKind::Ptmx => (5, 2),
            DevKind::InputEvent => (13, 64 + self.dev_index as u32),
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

#[derive(Debug, Clone, Copy)]
struct BlockDevSpec {
    device_id: u32,
    start_sector: u64,
    sector_count: u64,
    major: u32,
    minor: u32,
}

struct BlockDevInode {
    ino: u64,
    spec: BlockDevSpec,
    mode: u32,
    sector_size: u32,
}

impl BlockDevInode {
    fn new(ino: u64, spec: BlockDevSpec, mode: u32, sector_size: u32) -> Arc<Self> {
        Arc::new(Self {
            ino,
            spec,
            mode,
            sector_size,
        })
    }

    fn capacity_bytes(&self) -> u64 {
        self.spec
            .sector_count
            .saturating_mul(self.sector_size as u64)
    }

    fn read_bytes(&self, offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        let capacity = self.capacity_bytes();
        if offset >= capacity {
            return Ok(0);
        }

        let max_len = core::cmp::min(buf.len(), (capacity - offset) as usize);
        let sector_size = self.sector_size as u64;
        let mut done = 0usize;
        let mut current_offset = offset;

        while done < max_len {
            let sector = self.spec.start_sector + current_offset / sector_size;
            let sector_off = (current_offset % sector_size) as usize;
            let mut sector_buf = [0u8; 512];
            let read_size = core::cmp::min(512, self.sector_size as usize);
            crate::drivers::storage::read_storage_sectors(
                self.spec.device_id,
                sector,
                &mut sector_buf[..read_size],
            )
            .map_err(|_| VfsError::IoError)?;

            let avail = read_size - sector_off;
            let take = core::cmp::min(avail, max_len - done);
            buf[done..done + take].copy_from_slice(&sector_buf[sector_off..sector_off + take]);
            done += take;
            current_offset += take as u64;
        }

        Ok(done)
    }

    fn write_bytes(&self, offset: u64, buf: &[u8]) -> VfsResult<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        let capacity = self.capacity_bytes();
        if offset >= capacity {
            return Err(VfsError::InvalidArgument);
        }

        let max_len = core::cmp::min(buf.len(), (capacity - offset) as usize);
        let sector_size = self.sector_size as u64;
        let mut done = 0usize;
        let mut current_offset = offset;

        while done < max_len {
            let sector = self.spec.start_sector + current_offset / sector_size;
            let sector_off = (current_offset % sector_size) as usize;
            let mut sector_buf = [0u8; 512];
            let read_size = core::cmp::min(512, self.sector_size as usize);

            if sector_off != 0 || max_len - done < read_size {
                crate::drivers::storage::read_storage_sectors(
                    self.spec.device_id,
                    sector,
                    &mut sector_buf[..read_size],
                )
                .map_err(|_| VfsError::IoError)?;
            }

            let avail = read_size - sector_off;
            let take = core::cmp::min(avail, max_len - done);
            sector_buf[sector_off..sector_off + take].copy_from_slice(&buf[done..done + take]);

            crate::drivers::storage::write_storage_sectors(
                self.spec.device_id,
                sector,
                &sector_buf[..read_size],
            )
            .map_err(|_| VfsError::IoError)?;

            done += take;
            current_offset += take as u64;
        }

        Ok(done)
    }
}

impl InodeOps for BlockDevInode {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        self.read_bytes(offset, buf)
    }

    fn write_at(&self, offset: u64, buf: &[u8]) -> VfsResult<usize> {
        self.write_bytes(offset, buf)
    }

    fn stat(&self) -> VfsResult<Stat> {
        Ok(Stat {
            ino: self.ino,
            inode_type: InodeType::BlockDevice,
            size: self.capacity_bytes(),
            blksize: self.sector_size as u64,
            blocks: self.spec.sector_count,
            mode: self.mode,
            nlink: 1,
            uid: 0,
            gid: 0,
            rdev: ((self.spec.major as u64) << 8) | self.spec.minor as u64,
            atime: 0,
            mtime: 0,
            ctime: 0,
        })
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Err(VfsError::NotSupported)
    }

    fn sync(&self) -> VfsResult<()> {
        crate::drivers::storage::with_storage_manager(|manager| {
            if let Some(device) = manager.get_device_mut(self.spec.device_id) {
                device.driver.flush()
            } else {
                Err(crate::drivers::storage::StorageError::DeviceNotFound)
            }
        })
        .ok_or(VfsError::IoError)?
        .map_err(|_| VfsError::IoError)
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
        InodeType::BlockDevice
    }

    fn attach_child(&self, _name: &str, _child: Arc<dyn InodeOps>) -> VfsResult<()> {
        Err(VfsError::NotDirectory)
    }
}

fn disk_name(index: usize) -> alloc::string::String {
    alloc::format!(
        "sd{}",
        char::from_u32(b'a' as u32 + index as u32).unwrap_or('a')
    )
}

fn attach_block_device(
    dev_dir: &Arc<dyn InodeOps>,
    name: &str,
    spec: BlockDevSpec,
    sector_size: u32,
) -> VfsResult<()> {
    if dev_dir.lookup(name).is_ok() {
        return Ok(());
    }
    attach(
        dev_dir,
        name,
        BlockDevInode::new(alloc_dev_ino(), spec, 0o660, sector_size),
    )
}

fn should_expose_partition(
    part: &crate::drivers::storage::filesystem_interface::PartitionInfo,
    partition_count: usize,
) -> bool {
    if part.sector_count == 0 {
        return false;
    }
    partition_count > 1 || part.start_sector > 0
}

/// Register `/dev/sdX` and `/dev/sdXN` nodes from detected storage devices.
pub fn install_block_devices() -> VfsResult<()> {
    let root = crate::vfs::get_vfs().lookup("/")?;
    let dev = root.lookup("dev")?;

    let devices = crate::drivers::storage::list_block_devices();
    let partition_scan =
        crate::drivers::storage::filesystem_interface::scan_all_storage_filesystems()
            .unwrap_or_default();

    for (disk_index, device) in devices.iter().enumerate() {
        let disk = disk_name(disk_index);
        let major = 8u32;
        let base_minor = (disk_index as u32) * 16;
        let sector_size = device.sector_size();

        attach_block_device(
            &dev,
            &disk,
            BlockDevSpec {
                device_id: device.device_id(),
                start_sector: 0,
                sector_count: device.total_sectors(),
                major,
                minor: base_minor,
            },
            sector_size,
        )?;
        register_block_device(
            &disk,
            BlockDevSpec {
                device_id: device.device_id(),
                start_sector: 0,
                sector_count: device.total_sectors(),
                major,
                minor: base_minor,
            },
        );

        let partitions = partition_scan
            .iter()
            .find(|(id, _)| *id == device.device_id())
            .map(|(_, parts)| parts.as_slice())
            .unwrap_or(&[]);

        for part in partitions {
            if !should_expose_partition(part, partitions.len()) {
                continue;
            }
            let part_name = alloc::format!("{}{}", disk, part.number as u32 + 1);
            attach_block_device(
                &dev,
                &part_name,
                BlockDevSpec {
                    device_id: device.device_id(),
                    start_sector: part.start_sector,
                    sector_count: part.sector_count,
                    major,
                    minor: base_minor + part.number as u32 + 1,
                },
                sector_size,
            )?;
            register_block_device(
                &part_name,
                BlockDevSpec {
                    device_id: device.device_id(),
                    start_sector: part.start_sector,
                    sector_count: part.sector_count,
                    major,
                    minor: base_minor + part.number as u32 + 1,
                },
            );
        }
    }

    Ok(())
}

/// Populate `/dev` with standard Linux device nodes.
pub fn install_dev(root: Arc<dyn InodeOps>) -> VfsResult<()> {
    let dev = root.lookup("dev")?;
    attach(
        &dev,
        "null",
        DevInode::new(alloc_dev_ino(), DevKind::Null, 0o666),
    )?;
    attach(
        &dev,
        "zero",
        DevInode::new(alloc_dev_ino(), DevKind::Zero, 0o666),
    )?;
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
    attach(
        &dev,
        "full",
        DevInode::new(alloc_dev_ino(), DevKind::Full, 0o666),
    )?;
    attach(
        &dev,
        "console",
        DevInode::new(alloc_dev_ino(), DevKind::Console, 0o600),
    )?;
    attach(
        &dev,
        "tty",
        DevInode::new(alloc_dev_ino(), DevKind::Tty, 0o666),
    )?;
    attach(
        &dev,
        "ptmx",
        DevInode::new(alloc_dev_ino(), DevKind::Ptmx, 0o666),
    )?;

    // /dev/pts — pseudoterminal slave directory (nodes added dynamically).
    let _pts_dir = match dev.lookup("pts") {
        Ok(existing) => existing,
        Err(VfsError::NotFound) => dev.create("pts", InodeType::Directory, 0o755)?,
        Err(err) => return Err(err),
    };

    // /dev/input directory and event node expected by GNOME session probes/libinput.
    let input_dir = match dev.lookup("input") {
        Ok(existing) => existing,
        Err(VfsError::NotFound) => dev.create("input", InodeType::Directory, 0o755)?,
        Err(err) => return Err(err),
    };
    attach(
        &input_dir,
        "event0",
        DevInode::new_with_index(alloc_dev_ino(), DevKind::InputEvent, 0o660, 0),
    )?;
    // event1 = keyboard, event2 = mouse (matching typical Linux layout)
    attach(
        &input_dir,
        "event1",
        DevInode::new_with_index(alloc_dev_ino(), DevKind::InputEvent, 0o660, 1),
    )?;
    attach(
        &input_dir,
        "event2",
        DevInode::new_with_index(alloc_dev_ino(), DevKind::InputEvent, 0o660, 2),
    )?;
    // /dev/input/mice — legacy PS/2 mouse device (major 13, minor 63)
    attach(
        &input_dir,
        "mice",
        DevInode::new_with_index(alloc_dev_ino(), DevKind::InputEvent, 0o660, 0),
    )?;
    // /dev/input/js0 — joystick device (major 13, minor 0)
    attach(
        &input_dir,
        "js0",
        DevInode::new_with_index(alloc_dev_ino(), DevKind::InputEvent, 0o660, 0),
    )?;

    Ok(())
}

/// Register ALSA PCM and control nodes under `/dev/snd`.
pub fn install_sound_nodes() -> Result<usize, crate::vfs::VfsError> {
    let root = crate::vfs::get_vfs().lookup("/")?;
    let dev = root.lookup("dev")?;

    let snd_dir = match dev.lookup("snd") {
        Ok(existing) => existing,
        Err(crate::vfs::VfsError::NotFound) => dev.create("snd", InodeType::Directory, 0o755)?,
        Err(err) => return Err(err),
    };

    let mut count = 0usize;
    for pcm in crate::sound::list_pcm_devices() {
        if snd_dir.lookup(&pcm.name).is_ok() {
            continue;
        }
        attach(
            &snd_dir,
            &pcm.name,
            SoundPcmInode::new(alloc_dev_ino(), pcm.minor, pcm.playback, 0o660),
        )?;
        count += 1;
    }

    if snd_dir.lookup("controlC0").is_err() {
        attach(
            &snd_dir,
            "controlC0",
            SoundControlInode::new(alloc_dev_ino(), 0, 0o660),
        )?;
        count += 1;
    }

    Ok(count)
}

struct SoundPcmInode {
    ino: u64,
    minor: u32,
    playback: bool,
    mode: u32,
}

impl SoundPcmInode {
    fn new(ino: u64, minor: u32, playback: bool, mode: u32) -> Arc<Self> {
        Arc::new(Self {
            ino,
            minor,
            playback,
            mode,
        })
    }
}

impl InodeOps for SoundPcmInode {
    fn read_at(&self, _offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        Ok(crate::sound::pcm_read(self.minor, buf).unwrap_or(0))
    }

    fn write_at(&self, _offset: u64, buf: &[u8]) -> VfsResult<usize> {
        Ok(crate::sound::pcm_write(self.minor, buf).unwrap_or(0))
    }

    fn stat(&self) -> VfsResult<Stat> {
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
            rdev: ((116u64) << 8) | self.minor as u64,
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

struct SoundControlInode {
    ino: u64,
    card: u32,
    mode: u32,
}

impl SoundControlInode {
    fn new(ino: u64, card: u32, mode: u32) -> Arc<Self> {
        Arc::new(Self { ino, card, mode })
    }
}

impl InodeOps for SoundControlInode {
    fn read_at(&self, _offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        let info = alloc::format!("card={}\n", self.card);
        let bytes = info.as_bytes();
        let n = core::cmp::min(buf.len(), bytes.len());
        buf[..n].copy_from_slice(&bytes[..n]);
        Ok(n)
    }

    fn write_at(&self, _offset: u64, buf: &[u8]) -> VfsResult<usize> {
        Ok(buf.len())
    }

    fn stat(&self) -> VfsResult<Stat> {
        let minor = self.card;
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
            rdev: ((116u64) << 8) | minor as u64,
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
