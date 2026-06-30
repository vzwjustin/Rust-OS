//! Minimal perf_event_open support.
//!
//! Provides software-backed perf event file descriptors using RustOS counters.

extern crate alloc;

use alloc::collections::BTreeMap;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::{Mutex, RwLock};

pub const PERF_TYPE_HARDWARE: u32 = 0;
pub const PERF_TYPE_SOFTWARE: u32 = 1;

pub const PERF_COUNT_HW_CPU_CYCLES: u64 = 0;
pub const PERF_COUNT_HW_INSTRUCTIONS: u64 = 1;
pub const PERF_COUNT_HW_CACHE_MISSES: u64 = 3;

pub const PERF_COUNT_SW_CPU_CLOCK: u64 = 0;
pub const PERF_COUNT_SW_TASK_CLOCK: u64 = 1;
pub const PERF_COUNT_SW_PAGE_FAULTS: u64 = 2;
pub const PERF_COUNT_SW_CONTEXT_SWITCHES: u64 = 3;

pub const PERF_FORMAT_TOTAL_TIME_ENABLED: u64 = 1 << 0;
pub const PERF_FORMAT_TOTAL_TIME_RUNNING: u64 = 1 << 1;
pub const PERF_FORMAT_ID: u64 = 1 << 2;
pub const PERF_FORMAT_LOST: u64 = 1 << 4;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct PerfEventAttr {
    pub type_: u32,
    pub size: u32,
    pub config: u64,
    pub sample_period_or_freq: u64,
    pub sample_type: u64,
    pub read_format: u64,
    pub flags: u64,
}

#[derive(Clone)]
struct PerfEvent {
    id: u32,
    attr: PerfEventAttr,
    created_ns: u64,
    enabled: bool,
    lost: u64,
}

static NEXT_EVENT_ID: AtomicU32 = AtomicU32::new(1);
static EVENTS: RwLock<BTreeMap<u32, Mutex<PerfEvent>>> = RwLock::new(BTreeMap::new());

pub fn init() {
    EVENTS.write().clear();
    NEXT_EVENT_ID.store(1, Ordering::SeqCst);
    crate::serial_println!("[perf_event] perf event subsystem initialized");
}

pub fn perf_event_open(
    attr: *const PerfEventAttr,
    _pid: i32,
    _cpu: i32,
    group_fd: i32,
    flags: u64,
) -> i32 {
    if attr.is_null() {
        return -14; // EFAULT
    }
    if group_fd != -1 || flags != 0 {
        return -22; // EINVAL: groups and flags not supported in this slice
    }

    let attr_val = unsafe { *attr };
    if attr_val.size != 0 && attr_val.size < core::mem::size_of::<PerfEventAttr>() as u32 {
        return -7; // E2BIG
    }
    if !matches!(attr_val.type_, PERF_TYPE_HARDWARE | PERF_TYPE_SOFTWARE) {
        return -2; // ENOENT
    }

    let id = NEXT_EVENT_ID.fetch_add(1, Ordering::SeqCst);
    let event = PerfEvent {
        id,
        attr: attr_val,
        created_ns: crate::time::uptime_ns(),
        enabled: attr_val.flags & 1 == 0,
        lost: 0,
    };
    EVENTS.write().insert(id, Mutex::new(event));

    let fd =
        crate::linux_compat::special_fd::register_perf_event(id, crate::vfs::OpenFlags::RDONLY);
    if fd < 0 {
        EVENTS.write().remove(&id);
        return -23; // ENFILE
    }
    fd
}

pub fn close_event(id: u32) {
    EVENTS.write().remove(&id);
}

pub fn read_event(fd: i32, buf: &mut [u8]) -> isize {
    let id = match crate::linux_compat::special_fd::get_perf_event_id(fd) {
        Some(id) => id,
        None => return -9, // EBADF
    };
    let events = EVENTS.read();
    let event_mutex = match events.get(&id) {
        Some(event) => event,
        None => return -9,
    };
    let event = event_mutex.lock();

    let mut out = [0u64; 5];
    let mut count = 0usize;
    out[count] = read_value(&event);
    count += 1;
    let now = crate::time::uptime_ns();
    if event.attr.read_format & PERF_FORMAT_TOTAL_TIME_ENABLED != 0 {
        out[count] = now.saturating_sub(event.created_ns);
        count += 1;
    }
    if event.attr.read_format & PERF_FORMAT_TOTAL_TIME_RUNNING != 0 {
        out[count] = if event.enabled {
            now.saturating_sub(event.created_ns)
        } else {
            0
        };
        count += 1;
    }
    if event.attr.read_format & PERF_FORMAT_ID != 0 {
        out[count] = event.id as u64;
        count += 1;
    }
    if event.attr.read_format & PERF_FORMAT_LOST != 0 {
        out[count] = event.lost;
        count += 1;
    }

    let bytes_len = count * core::mem::size_of::<u64>();
    if buf.len() < bytes_len {
        return -22; // EINVAL
    }
    let bytes = unsafe { core::slice::from_raw_parts(out.as_ptr() as *const u8, bytes_len) };
    buf[..bytes_len].copy_from_slice(bytes);
    bytes_len as isize
}

// ── Hardware PMU MSR programming ────────────────────────────────────────────

/// MSR addresses for Intel IA32_PERFEVTSELx (up to 4 general-purpose counters)
const IA32_PERFEVTSEL0: u32 = 0x186;
/// MSR addresses for Intel IA32_PMCx (general-purpose performance counters)
const IA32_PMC0: u32 = 0xC1;

/// Maximum number of general-purpose PMU counters supported.
const PMU_MAX_COUNTERS: u8 = 4;

/// Enable a hardware PMU counter.
///
/// Writes to `IA32_PERFEVTSELx` to configure and start counting.
///
/// # Arguments
/// * `counter` – counter index (0–3)
/// * `event`   – event select byte (e.g. 0x3C for cycles, 0xC0 for instructions)
/// * `umask`   – unit mask byte
/// * `user`    – count events in user mode (CPL > 0)
/// * `kernel`  – count events in kernel mode (CPL == 0)
pub fn enable_counter(counter: u8, event: u8, umask: u8, user: bool, kernel: bool) {
    if counter >= PMU_MAX_COUNTERS {
        return;
    }
    let msr = IA32_PERFEVTSEL0 + counter as u32;
    // Bit layout: [7:0]=EventSelect [15:8]=UMask [16]=USR [17]=OS [22]=EN
    let mut value: u64 = (event as u64) | ((umask as u64) << 8);
    if user {
        value |= 1 << 16; // USR bit
    }
    if kernel {
        value |= 1 << 17; // OS bit
    }
    value |= 1 << 22; // EN (enable) bit
    unsafe {
        core::arch::asm!(
            "wrmsr",
            in("ecx") msr,
            in("eax") (value & 0xFFFF_FFFF) as u32,
            in("edx") (value >> 32) as u32,
            options(nostack, nomem),
        );
    }
}

/// Read the current value of a hardware PMU counter via `rdmsr` on `IA32_PMCx`.
///
/// Returns 0 for out-of-range counter indices.
pub fn read_counter(counter: u8) -> u64 {
    if counter >= PMU_MAX_COUNTERS {
        return 0;
    }
    let msr = IA32_PMC0 + counter as u32;
    let (lo, hi): (u32, u32);
    unsafe {
        core::arch::asm!(
            "rdmsr",
            in("ecx") msr,
            out("eax") lo,
            out("edx") hi,
            options(nostack, nomem),
        );
    }
    ((hi as u64) << 32) | (lo as u64)
}

/// Disable a hardware PMU counter by clearing the EN bit in `IA32_PERFEVTSELx`.
pub fn disable_counter(counter: u8) {
    if counter >= PMU_MAX_COUNTERS {
        return;
    }
    let msr = IA32_PERFEVTSEL0 + counter as u32;
    // Read current value, clear EN bit (bit 22), write back
    let (lo, hi): (u32, u32);
    unsafe {
        core::arch::asm!(
            "rdmsr",
            in("ecx") msr,
            out("eax") lo,
            out("edx") hi,
            options(nostack, nomem),
        );
    }
    let mut value = ((hi as u64) << 32) | (lo as u64);
    value &= !(1u64 << 22); // Clear EN bit
    unsafe {
        core::arch::asm!(
            "wrmsr",
            in("ecx") msr,
            in("eax") (value & 0xFFFF_FFFF) as u32,
            in("edx") (value >> 32) as u32,
            options(nostack, nomem),
        );
    }
}

fn read_value(event: &PerfEvent) -> u64 {
    let stats = crate::performance_monitor::get_stats();
    match (event.attr.type_, event.attr.config) {
        (PERF_TYPE_HARDWARE, PERF_COUNT_HW_CPU_CYCLES) => stats.cpu_cycles,
        (PERF_TYPE_HARDWARE, PERF_COUNT_HW_INSTRUCTIONS) => stats.instructions_retired,
        (PERF_TYPE_HARDWARE, PERF_COUNT_HW_CACHE_MISSES) => stats.cache_misses,
        (PERF_TYPE_SOFTWARE, PERF_COUNT_SW_CPU_CLOCK) => crate::time::uptime_ns(),
        (PERF_TYPE_SOFTWARE, PERF_COUNT_SW_TASK_CLOCK) => {
            crate::time::uptime_ns().saturating_sub(event.created_ns)
        }
        (PERF_TYPE_SOFTWARE, PERF_COUNT_SW_PAGE_FAULTS) => stats.page_faults,
        (PERF_TYPE_SOFTWARE, PERF_COUNT_SW_CONTEXT_SWITCHES) => stats.context_switches,
        _ => 0,
    }
}
