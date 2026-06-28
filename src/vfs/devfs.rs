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
    InputEvent,
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

fn read_input_event(buf: &mut [u8]) -> VfsResult<usize> {
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
            DevKind::Console => Ok(0),
            DevKind::InputEvent => read_input_event(buf),
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
            DevKind::InputEvent => (13, 64),
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
        DevInode::new(alloc_dev_ino(), DevKind::Console, 0o666),
    )?;

    // /dev/input directory and event node expected by GNOME session probes/libinput.
    let input_dir = match dev.lookup("input") {
        Ok(existing) => existing,
        Err(VfsError::NotFound) => dev.create("input", InodeType::Directory, 0o755)?,
        Err(err) => return Err(err),
    };
    attach(
        &input_dir,
        "event0",
        DevInode::new(alloc_dev_ino(), DevKind::InputEvent, 0o660),
    )?;

    Ok(())
}
