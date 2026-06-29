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
