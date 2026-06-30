//! Miscellaneous character device framework.
//!
//! Provides simplified character device registration for devices that do not
//! warrant their own major number.  Mirrors Linux's `drivers/char/misc.c` and
//! `include/linux/miscdevice.h`.
//!
//! Pure-Rust, no_std. No bindings:: calls.

#![allow(dead_code, unused_variables)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use core::any::Any;
use spin::Mutex;

// ── Error constants (Linux errno) ─────────────────────────────────────────────

pub const EINVAL: i64 = 22;
pub const ENOTTY: i64 = 25;
pub const ENODEV: i64 = 19;

// ── File abstraction ──────────────────────────────────────────────────────────

/// Open file descriptor (analogous to Linux `struct file`).
pub struct File {
    pub flags: u32,
    pub mode: u32,
    pub private_data: Option<Box<dyn Any + Send>>,
}

impl File {
    pub fn new(flags: u32, mode: u32) -> Self {
        Self {
            flags,
            mode,
            private_data: None,
        }
    }
}

// ── VMA abstraction ───────────────────────────────────────────────────────────

/// Virtual memory area passed to `mmap` (analogous to Linux `struct vm_area_struct`).
pub struct VmaRef {
    pub vm_start: u64,
    pub vm_end: u64,
    pub vm_pgoff: u64,
    pub vm_flags: u64,
}

// ── File operations trait ─────────────────────────────────────────────────────

/// File operations implemented by a misc device (analogous to `struct file_operations`).
pub trait FileOps: Send + Sync {
    fn open(&self, file: &File) -> i32 {
        0
    }
    fn release(&self, file: &File) -> i32 {
        0
    }
    /// Read up to `buf.len()` bytes; returns byte count or negative errno.
    fn read(&self, file: &File, buf: &mut [u8], offset: &mut u64) -> i64 {
        -(EINVAL as i64)
    }
    /// Write `buf`; returns byte count or negative errno.
    fn write(&self, file: &File, buf: &[u8], offset: &mut u64) -> i64 {
        -(EINVAL as i64)
    }
    /// ioctl handler; returns result or negative errno.
    fn ioctl(&self, file: &File, cmd: u32, arg: u64) -> i64 {
        -(ENOTTY as i64)
    }
    /// mmap handler; returns 0 or negative errno.
    fn mmap(&self, file: &File, vma: &VmaRef) -> i32 {
        -(ENODEV as i32)
    }
    /// poll returns a bitmask of ready events (POLLIN/POLLOUT/…).
    fn poll(&self, file: &File) -> u32 {
        0
    }
}

// ── Misc device ───────────────────────────────────────────────────────────────

/// Automatic minor allocation sentinel (mirrors `MISC_DYNAMIC_MINOR`).
pub const MISC_DYNAMIC_MINOR: i32 = 255;

/// A registered miscellaneous character device.
pub struct MiscDevice {
    /// Minor number; `MISC_DYNAMIC_MINOR` requests automatic allocation.
    pub minor: i32,
    pub name: String,
    pub fops: Arc<dyn FileOps>,
}

// ── Global misc registry ──────────────────────────────────────────────────────

static MISC_REGISTRY: Mutex<BTreeMap<i32, Arc<MiscDevice>>> = Mutex::new(BTreeMap::new());
/// Next dynamically allocated minor (range 64..=254).
static NEXT_DYNAMIC_MINOR: Mutex<i32> = Mutex::new(64);

impl MiscDevice {
    /// Register a misc device.
    ///
    /// If `dev.minor == MISC_DYNAMIC_MINOR`, a minor is allocated automatically.
    /// Returns `Ok(minor)` on success or `Err(errno)` on failure.
    pub fn register(mut dev: Arc<MiscDevice>) -> Result<i32, i32> {
        let mut registry = MISC_REGISTRY.lock();

        let minor = if dev.minor == MISC_DYNAMIC_MINOR {
            let mut next = NEXT_DYNAMIC_MINOR.lock();
            if *next > 254 {
                return Err(-28); // ENOSPC
            }
            let allocated = *next;
            *next += 1;
            allocated
        } else {
            if registry.contains_key(&dev.minor) {
                return Err(-17); // EEXIST
            }
            dev.minor
        };

        // Reconstruct with correct minor if dynamically allocated.
        let dev = if dev.minor == MISC_DYNAMIC_MINOR {
            Arc::new(MiscDevice {
                minor,
                name: dev.name.clone(),
                fops: dev.fops.clone(),
            })
        } else {
            dev
        };

        registry.insert(minor, dev);
        Ok(minor)
    }

    /// Deregister a misc device by minor number.
    pub fn deregister(minor: i32) {
        MISC_REGISTRY.lock().remove(&minor);
    }

    /// Look up a registered misc device by minor number.
    pub fn lookup(minor: i32) -> Option<Arc<MiscDevice>> {
        MISC_REGISTRY.lock().get(&minor).cloned()
    }

    /// Returns the count of currently registered misc devices.
    pub fn count() -> usize {
        MISC_REGISTRY.lock().len()
    }
}
