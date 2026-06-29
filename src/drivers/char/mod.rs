//! Character device framework
//!
//! Provides character device registration with major/minor number
//! allocation, file operations, and driver binding. Mirrors Linux's
//! `drivers/char/` and `fs/char_dev.c`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// Character device file operations (Linux `struct file_operations`).
pub struct CharDeviceOps {
    pub open: fn(minor: u32) -> Result<u32, &'static str>,
    pub read: fn(handle: u32, buf: &mut [u8]) -> Result<usize, &'static str>,
    pub write: fn(handle: u32, buf: &[u8]) -> Result<usize, &'static str>,
    pub ioctl: fn(handle: u32, cmd: u32, arg: u64) -> Result<i32, &'static str>,
    pub release: fn(handle: u32) -> Result<(), &'static str>,
    pub get_name: fn() -> &'static str,
}

struct CharDevice {
    major: u32,
    name: String,
    ops: &'static CharDeviceOps,
    minor_count: u32,
    open_count: u32,
}

/// Open file handle for a char device.
struct CharDeviceHandle {
    major: u32,
    minor: u32,
    handle: u32,
}

// ── Registry ────────────────────────────────────────────────────────────

static CHAR_DEVICES: RwLock<BTreeMap<u32, CharDevice>> = RwLock::new(BTreeMap::new());
static CHAR_HANDLES: RwLock<BTreeMap<u32, CharDeviceHandle>> = RwLock::new(BTreeMap::new());
static NEXT_HANDLE_ID: AtomicU32 = AtomicU32::new(1);

// ── Reserved major numbers (Linux `Documentation/admin-guide/devices.txt`) ──

pub const MEM_MAJOR: u32 = 1; // /dev/mem, /dev/null, /dev/zero, etc.
pub const TTY_MAJOR: u32 = 4; // /dev/tty*
pub const TTYAUX_MAJOR: u32 = 5; // /dev/console, /dev/ptmx
pub const LP_MAJOR: u32 = 6; // /dev/lp* (parallel port)
pub const MISC_MAJOR: u32 = 10; // /dev/misc/*
pub const INPUT_MAJOR: u32 = 13; // /dev/input/*
pub const SOUND_MAJOR: u32 = 14; // /dev/dsp, /dev/mixer
pub const FB_MAJOR: u32 = 29; // /dev/fb*
pub const RTC_MAJOR: u32 = 254; // /dev/rtc

// ── Public API ──────────────────────────────────────────────────────────

/// Register a character device (Linux `register_chrdev`).
pub fn register_device(
    major: u32,
    name: &str,
    minor_count: u32,
    ops: &'static CharDeviceOps,
) -> Result<u32, &'static str> {
    if minor_count == 0 {
        return Err("Character device must have at least one minor");
    }
    let mut devices = CHAR_DEVICES.write();
    if devices.contains_key(&major) {
        return Err("Major number already in use");
    }
    devices.insert(
        major,
        CharDevice {
            major,
            name: String::from(name),
            ops,
            minor_count,
            open_count: 0,
        },
    );
    Ok(major)
}

/// Unregister a character device (Linux `unregister_chrdev`).
pub fn unregister_device(major: u32) -> Result<(), &'static str> {
    let mut devices = CHAR_DEVICES.write();
    let dev = devices.get(&major).ok_or("Character device not found")?;
    if dev.open_count > 0 {
        return Err("Cannot unregister: device still open");
    }
    devices.remove(&major);
    Ok(())
}

/// Open a character device (Linux `chrdev_open`).
pub fn open(major: u32, minor: u32) -> Result<u32, &'static str> {
    let ops = {
        let devices = CHAR_DEVICES.read();
        let dev = devices.get(&major).ok_or("Character device not found")?;
        if minor >= dev.minor_count {
            return Err("Minor number out of range");
        }
        dev.ops
    };

    let handle = (ops.open)(minor)?;
    let fd = NEXT_HANDLE_ID.fetch_add(1, Ordering::SeqCst);
    CHAR_HANDLES.write().insert(
        fd,
        CharDeviceHandle {
            major,
            minor,
            handle,
        },
    );

    let mut devices = CHAR_DEVICES.write();
    if let Some(dev) = devices.get_mut(&major) {
        dev.open_count += 1;
    }

    Ok(fd)
}

/// Read from a character device (Linux `chrdev_read`).
pub fn read(fd: u32, buf: &mut [u8]) -> Result<usize, &'static str> {
    let (ops, handle) = {
        let handles = CHAR_HANDLES.read();
        let h = handles.get(&fd).ok_or("Invalid file descriptor")?;
        let devices = CHAR_DEVICES.read();
        let dev = devices.get(&h.major).ok_or("Device vanished")?;
        (dev.ops, h.handle)
    };
    (ops.read)(handle, buf)
}

/// Write to a character device (Linux `chrdev_write`).
pub fn write(fd: u32, buf: &[u8]) -> Result<usize, &'static str> {
    let (ops, handle) = {
        let handles = CHAR_HANDLES.read();
        let h = handles.get(&fd).ok_or("Invalid file descriptor")?;
        let devices = CHAR_DEVICES.read();
        let dev = devices.get(&h.major).ok_or("Device vanished")?;
        (dev.ops, h.handle)
    };
    (ops.write)(handle, buf)
}

/// IOCTL on a character device (Linux `chrdev_ioctl`).
pub fn ioctl(fd: u32, cmd: u32, arg: u64) -> Result<i32, &'static str> {
    let (ops, handle) = {
        let handles = CHAR_HANDLES.read();
        let h = handles.get(&fd).ok_or("Invalid file descriptor")?;
        let devices = CHAR_DEVICES.read();
        let dev = devices.get(&h.major).ok_or("Device vanished")?;
        (dev.ops, h.handle)
    };
    (ops.ioctl)(handle, cmd, arg)
}

/// Close a character device file descriptor (Linux `chrdev_release`).
pub fn release(fd: u32) -> Result<(), &'static str> {
    let (ops, handle, major) = {
        let mut handles = CHAR_HANDLES.write();
        let h = handles.remove(&fd).ok_or("Invalid file descriptor")?;
        let devices = CHAR_DEVICES.read();
        let dev = devices.get(&h.major).ok_or("Device vanished")?;
        (dev.ops, h.handle, h.major)
    };

    (ops.release)(handle)?;

    let mut devices = CHAR_DEVICES.write();
    if let Some(dev) = devices.get_mut(&major) {
        dev.open_count = dev.open_count.saturating_sub(1);
    }

    Ok(())
}

/// Get device name by major number.
pub fn get_device_name(major: u32) -> Result<String, &'static str> {
    let devices = CHAR_DEVICES.read();
    let dev = devices.get(&major).ok_or("Character device not found")?;
    Ok(dev.name.clone())
}

/// Number of registered character devices.
pub fn device_count() -> usize {
    CHAR_DEVICES.read().len()
}

/// Get all registered major numbers.
pub fn get_all_majors() -> Vec<(u32, String)> {
    CHAR_DEVICES
        .read()
        .iter()
        .map(|(major, dev)| (*major, dev.name.clone()))
        .collect()
}

/// Number of open file descriptors.
pub fn open_count() -> usize {
    CHAR_HANDLES.read().len()
}

/// Initialize character device framework.
pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("char: framework ready ({} devices)", device_count());
    Ok(())
}
