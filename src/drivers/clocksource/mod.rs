//! Clocksource driver subsystem
//!
//! Provides clocksource registration and selection framework.
//! Mirrors Linux's `drivers/clocksource/`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// Clocksource (Linux `struct clocksource`).
pub struct ClockSource {
    pub id: u32,
    pub name: String,
    pub rating: i32,
    pub read: fn() -> u64,
    pub mask: u64,
    pub mult: u32,
    pub shift: u32,
    pub freq: u64,
}

/// Timer device (Linux `struct clock_event_device`).
pub struct ClockEventDevice {
    pub id: u32,
    pub name: String,
    pub features: u32,
    pub min_delta_ns: u64,
    pub max_delta_ns: u64,
    pub mult: u32,
    pub shift: u32,
    pub set_next_event: fn(delta_ns: u64) -> Result<(), &'static str>,
    pub set_state_periodic: Option<fn() -> Result<(), &'static str>>,
    pub set_state_oneshot: Option<fn() -> Result<(), &'static str>>,
    pub set_state_shutdown: Option<fn() -> Result<(), &'static str>>,
}

// ── Registry ────────────────────────────────────────────────────────────

static CS_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static CED_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static CLOCK_SOURCES: RwLock<BTreeMap<u32, ClockSource>> = RwLock::new(BTreeMap::new());
static CLOCK_EVENTS: RwLock<BTreeMap<u32, ClockEventDevice>> = RwLock::new(BTreeMap::new());

static CURRENT_CS: AtomicU32 = AtomicU32::new(u32::MAX);

// ── Public API ──────────────────────────────────────────────────────────

/// Register a clocksource (Linux `__clocksource_register`).
pub fn register_clocksource(
    name: &str,
    rating: i32,
    read: fn() -> u64,
    mask: u64,
    freq: u64,
) -> Result<u32, &'static str> {
    if freq == 0 {
        return Err("clocksource frequency must be non-zero");
    }
    let id = CS_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let (mult, shift) = compute_mult_shift(freq);
    let cs = ClockSource {
        id,
        name: String::from(name),
        rating,
        read,
        mask,
        mult,
        shift,
        freq,
    };
    CLOCK_SOURCES.write().insert(id, cs);

    if id == 0 || rating > get_rating(CURRENT_CS.load(Ordering::SeqCst)) {
        CURRENT_CS.store(id, Ordering::SeqCst);
    }
    Ok(id)
}

/// Register a clock event device (Linux `clockevents_register_device`).
pub fn register_clock_event(
    name: &str,
    features: u32,
    min_delta_ns: u64,
    max_delta_ns: u64,
    freq: u64,
    set_next_event: fn(u64) -> Result<(), &'static str>,
    set_state_periodic: Option<fn() -> Result<(), &'static str>>,
    set_state_oneshot: Option<fn() -> Result<(), &'static str>>,
    set_state_shutdown: Option<fn() -> Result<(), &'static str>>,
) -> Result<u32, &'static str> {
    if freq == 0 {
        return Err("clock event device frequency must be non-zero");
    }
    let id = CED_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let (mult, shift) = compute_mult_shift(freq);
    let ced = ClockEventDevice {
        id,
        name: String::from(name),
        features,
        min_delta_ns,
        max_delta_ns,
        mult,
        shift,
        set_next_event,
        set_state_periodic,
        set_state_oneshot,
        set_state_shutdown,
    };
    CLOCK_EVENTS.write().insert(id, ced);
    Ok(id)
}

/// Read the current clocksource.
pub fn read_current_clock() -> u64 {
    let cs_id = CURRENT_CS.load(Ordering::SeqCst);
    let read_fn = {
        let sources = CLOCK_SOURCES.read();
        sources.get(&cs_id).map(|cs| cs.read)
    };
    match read_fn {
        Some(f) => f(),
        None => 0,
    }
}

/// Get the current clocksource name.
pub fn current_clocksource_name() -> String {
    let cs_id = CURRENT_CS.load(Ordering::SeqCst);
    let sources = CLOCK_SOURCES.read();
    sources
        .get(&cs_id)
        .map(|cs| cs.name.clone())
        .unwrap_or_else(|| String::from("none"))
}

/// List all registered clocksources.
pub fn list_clocksources() -> Vec<(u32, String, i32, u64)> {
    CLOCK_SOURCES
        .read()
        .iter()
        .map(|(id, cs)| (*id, cs.name.clone(), cs.rating, cs.freq))
        .collect()
}

/// List all registered clock event devices.
pub fn list_clock_events() -> Vec<(u32, String, u64, u64)> {
    CLOCK_EVENTS
        .read()
        .iter()
        .map(|(id, ced)| (*id, ced.name.clone(), ced.min_delta_ns, ced.max_delta_ns))
        .collect()
}

fn get_rating(id: u32) -> i32 {
    if id == u32::MAX {
        return -1;
    }
    let sources = CLOCK_SOURCES.read();
    sources.get(&id).map(|cs| cs.rating).unwrap_or(-1)
}

fn compute_mult_shift(freq: u64) -> (u32, u32) {
    if freq == 0 {
        return (0, 0);
    }
    let shift = 32;
    let mult = ((1u64 << shift) + freq / 2) / freq;
    (mult as u32, shift)
}

// ── Software clocksource (TSC-based) ────────────────────────────────────

fn read_tsc() -> u64 {
    let lo: u32;
    let hi: u32;
    unsafe {
        core::arch::asm!("rdtsc", out("eax") lo, out("edx") hi, options(nostack, preserves_flags));
    }
    ((hi as u64) << 32) | (lo as u64)
}

fn sw_set_next_event(_delta_ns: u64) -> Result<(), &'static str> {
    Ok(())
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    if !CLOCK_SOURCES.read().is_empty() {
        return Ok(());
    }

    register_clocksource("tsc", 300, read_tsc, u64::MAX, 2_400_000_000)?;
    register_clock_event(
        "lapic-timer",
        0x03,
        100,
        1_000_000_000,
        100_000_000,
        sw_set_next_event,
        None,
        None,
        None,
    )?;

    crate::serial_println!("clocksource: registered TSC (rating=300) and lapic-timer");
    Ok(())
}
