//! KCSAN concurrency checker hooks.
//!
//! Data-race detection scaffolding: instrumented load/store sites call into
//! this module. Enable with the `kcsan` cargo feature.

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::string::String;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::RwLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KcsanAccessType {
    Read,
    Write,
}

#[derive(Debug, Clone)]
pub struct KcsanReport {
    pub addr: u64,
    pub size: usize,
    pub first: KcsanAccessRecord,
    pub second: KcsanAccessRecord,
}

#[derive(Debug, Clone)]
pub struct KcsanAccessRecord {
    pub access: KcsanAccessType,
    pub pid: u32,
    pub pc: u64,
}

#[derive(Debug, Clone, Copy)]
struct ShadowWord {
    access: KcsanAccessType,
    pid: u32,
    pc: u64,
    generation: u64,
}

static INITIALIZED: AtomicBool = AtomicBool::new(false);
static ENABLED: AtomicBool = AtomicBool::new(false);
static GENERATION: AtomicU64 = AtomicU64::new(1);
static REPORT_COUNT: AtomicU64 = AtomicU64::new(0);
static SHADOW: RwLock<BTreeMap<u64, ShadowWord>> = RwLock::new(BTreeMap::new());
static LAST_REPORT: RwLock<Option<KcsanReport>> = RwLock::new(None);

pub fn init() {
    if INITIALIZED.swap(true, Ordering::SeqCst) {
        return;
    }
    SHADOW.write().clear();
    #[cfg(feature = "kcsan")]
    {
        ENABLED.store(true, Ordering::Relaxed);
        crate::serial_println!("[kcsan] concurrency checker enabled (feature=on)");
    }
    #[cfg(not(feature = "kcsan"))]
    {
        ENABLED.store(false, Ordering::Relaxed);
        crate::serial_println!("[kcsan] framework ready (feature=off, hooks are no-ops)");
    }
}

pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Instrumentation hook for sized accesses.
#[inline(always)]
pub fn check_access(access: KcsanAccessType, addr: u64, size: usize, pc: u64) {
    if !is_enabled() || size == 0 {
        return;
    }
    #[cfg(feature = "kcsan")]
    {
        check_access_impl(access, addr, size, pc);
    }
}

#[cfg(feature = "kcsan")]
fn check_access_impl(access: KcsanAccessType, addr: u64, size: usize, pc: u64) {
    let pid = crate::process::current_pid();
    let key = addr & !0x7;
    let gen = GENERATION.fetch_add(1, Ordering::Relaxed);
    let record = ShadowWord {
        access,
        pid,
        pc,
        generation: gen,
    };

    let mut shadow = SHADOW.write();
    if let Some(prev) = shadow.get(&key) {
        if prev.generation != gen && prev.pid != pid && accesses_race(prev.access, access) {
            let report = KcsanReport {
                addr,
                size,
                first: KcsanAccessRecord {
                    access: prev.access,
                    pid: prev.pid,
                    pc: prev.pc,
                },
                second: KcsanAccessRecord { access, pid, pc },
            };
            drop(shadow);
            report_race(report);
            return;
        }
    }
    shadow.insert(key, record);
}

fn accesses_race(a: KcsanAccessType, b: KcsanAccessType) -> bool {
    matches!(
        (a, b),
        (KcsanAccessType::Write, KcsanAccessType::Write)
            | (KcsanAccessType::Write, KcsanAccessType::Read)
            | (KcsanAccessType::Read, KcsanAccessType::Write)
    )
}

pub fn report_race(report: KcsanReport) -> ! {
    REPORT_COUNT.fetch_add(1, Ordering::Relaxed);
    *LAST_REPORT.write() = Some(report.clone());
    crate::serial_println!(
        "[kcsan] DATA RACE at {:#x} size {}: pid {} ({:?} @ {:#x}) vs pid {} ({:?} @ {:#x})",
        report.addr,
        report.size,
        report.first.pid,
        report.first.access,
        report.first.pc,
        report.second.pid,
        report.second.access,
        report.second.pc
    );
    crate::kernel::panic(crate::kernel::PanicInfo {
        message: String::from("KCSAN data race"),
        file: file!(),
        line: line!(),
        column: 0,
    });
}

pub fn report_count() -> u64 {
    REPORT_COUNT.load(Ordering::Relaxed)
}

pub fn last_report() -> Option<KcsanReport> {
    LAST_REPORT.read().clone()
}

/// Clear shadow state (e.g. after fork or when disabling checks).
pub fn reset_shadow() {
    SHADOW.write().clear();
}

#[inline(always)]
pub fn read8(addr: u64, pc: u64) {
    check_access(KcsanAccessType::Read, addr, 1, pc);
}

#[inline(always)]
pub fn write8(addr: u64, pc: u64) {
    check_access(KcsanAccessType::Write, addr, 1, pc);
}
