//! KASAN shadow memory framework.
//!
//! Provides compile-time instrumentation hooks and a shadow-byte report path.
//! Enable with the `kasan` cargo feature; without it all checks compile to no-ops.

extern crate alloc;

use alloc::{format, string::String};
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::RwLock;

/// Shadow scale: one shadow byte covers 8 bytes of kernel memory.
pub const KASAN_SHADOW_SCALE: usize = 3;
pub const KASAN_SHADOW_SIZE: usize = 1 << KASAN_SHADOW_SCALE;

/// Shadow memory value for accessible 8-byte granules.
pub const KASAN_SHADOW_ACCESSIBLE: u8 = 0;
/// Redzone / out-of-bounds marker.
pub const KASAN_SHADOW_REDZONE: u8 = 0xFE;
/// Freed memory marker.
pub const KASAN_SHADOW_FREED: u8 = 0xFF;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KasanAccessType {
    Load,
    Store,
    Memcpy,
    Memset,
}

#[derive(Debug, Clone)]
pub struct KasanReport {
    pub access: KasanAccessType,
    pub addr: u64,
    pub size: usize,
    pub shadow_value: u8,
    pub task: String,
}

static INITIALIZED: AtomicBool = AtomicBool::new(false);
static ENABLED: AtomicBool = AtomicBool::new(false);
static REPORT_COUNT: AtomicU64 = AtomicU64::new(0);
static LAST_REPORT: RwLock<Option<KasanReport>> = RwLock::new(None);

/// Virtual base where shadow bytes would live (Linux-style high canonical hole).
pub const KASAN_SHADOW_START: u64 = 0xFFFF_E000_0000_0000;

#[inline(always)]
pub fn shadow_offset(addr: u64) -> u64 {
    (addr >> KASAN_SHADOW_SCALE) + KASAN_SHADOW_START
}

pub fn init() {
    if INITIALIZED.swap(true, Ordering::SeqCst) {
        return;
    }
    #[cfg(feature = "kasan")]
    {
        ENABLED.store(true, Ordering::Relaxed);
        crate::serial_println!("[kasan] shadow checker enabled (feature=on)");
    }
    #[cfg(not(feature = "kasan"))]
    {
        ENABLED.store(false, Ordering::Relaxed);
        crate::serial_println!("[kasan] framework ready (feature=off, checks are no-ops)");
    }
}

pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Instrumentation hook before a memory access.
#[inline(always)]
pub fn check_access(access: KasanAccessType, addr: u64, size: usize) {
    if !is_enabled() || size == 0 {
        return;
    }
    #[cfg(feature = "kasan")]
    {
        if let Some(shadow_val) = shadow_value_for(addr, size) {
            if shadow_val != KASAN_SHADOW_ACCESSIBLE {
                report(access, addr, size, shadow_val);
            }
        }
    }
}

#[cfg(feature = "kasan")]
fn shadow_value_for(addr: u64, size: usize) -> Option<u8> {
    // Without a live shadow mapping we treat unmapped shadow as accessible so
    // early boot does not fault; poison helpers populate redzones explicitly.
    let _ = size;
    let shadow_ptr = shadow_offset(addr) as *const u8;
    if shadow_ptr.is_null() {
        return None;
    }
    Some(unsafe { core::ptr::read_volatile(shadow_ptr) })
}

/// Mark `[addr, addr+size)` as accessible in the shadow map (when mapped).
pub fn poison_accessible(addr: u64, size: u64) {
    if !is_enabled() {
        return;
    }
    let mut off = 0u64;
    while off < size {
        let shadow_ptr = shadow_offset(addr + off) as *mut u8;
        unsafe {
            core::ptr::write_volatile(shadow_ptr, KASAN_SHADOW_ACCESSIBLE);
        }
        off += KASAN_SHADOW_SIZE as u64;
    }
}

/// Mark `[addr, addr+size)` as redzone / inaccessible.
pub fn poison_redzone(addr: u64, size: u64) {
    if !is_enabled() {
        return;
    }
    let mut off = 0u64;
    while off < size {
        let shadow_ptr = shadow_offset(addr + off) as *mut u8;
        unsafe {
            core::ptr::write_volatile(shadow_ptr, KASAN_SHADOW_REDZONE);
        }
        off += KASAN_SHADOW_SIZE as u64;
    }
}

/// Mark freed memory in the shadow map.
pub fn poison_freed(addr: u64, size: u64) {
    if !is_enabled() {
        return;
    }
    let mut off = 0u64;
    while off < size {
        let shadow_ptr = shadow_offset(addr + off) as *mut u8;
        unsafe {
            core::ptr::write_volatile(shadow_ptr, KASAN_SHADOW_FREED);
        }
        off += KASAN_SHADOW_SIZE as u64;
    }
}

pub fn report(access: KasanAccessType, addr: u64, size: usize, shadow_value: u8) -> ! {
    REPORT_COUNT.fetch_add(1, Ordering::Relaxed);
    let task = format!("{}", crate::process::current_pid());
    let report = KasanReport {
        access,
        addr,
        size,
        shadow_value,
        task,
    };
    *LAST_REPORT.write() = Some(report.clone());
    crate::serial_println!(
        "[kasan] BUG: {} at {:#x} size {} shadow={:#x} task={}",
        access_tag(access),
        addr,
        size,
        shadow_value,
        report.task
    );
    crate::kernel::panic(crate::kernel::PanicInfo {
        message: String::from("KASAN memory violation"),
        file: file!(),
        line: line!(),
        column: 0,
    });
}

fn access_tag(access: KasanAccessType) -> &'static str {
    match access {
        KasanAccessType::Load => "load",
        KasanAccessType::Store => "store",
        KasanAccessType::Memcpy => "memcpy",
        KasanAccessType::Memset => "memset",
    }
}

pub fn report_count() -> u64 {
    REPORT_COUNT.load(Ordering::Relaxed)
}

pub fn last_report() -> Option<KasanReport> {
    LAST_REPORT.read().clone()
}

/// Convenience wrappers for instrumentation macros.
#[inline(always)]
pub fn load1(addr: u64) {
    check_access(KasanAccessType::Load, addr, 1);
}
#[inline(always)]
pub fn store1(addr: u64) {
    check_access(KasanAccessType::Store, addr, 1);
}
