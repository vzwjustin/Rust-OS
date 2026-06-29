//! PTP (Precision Time Protocol) subsystem
//!
//! Provides hardware timestamping and clock synchronization via PTP clocks.
//! Mirrors Linux's `drivers/ptp/ptp_clock.c`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// PTP clock capabilities (Linux `struct ptp_clock_info`).
pub struct PtpClockInfo {
    pub name: String,
    pub max_adj: i64,
    pub n_alarm: u32,
    pub n_ext_ts: u32,
    pub n_per_out: u32,
    pub n_pins: u32,
    pub pps: bool,
    pub adjfreq: fn(clock_id: u32, delta: i64) -> Result<(), &'static str>,
    pub adjtime: fn(clock_id: u32, delta: i64) -> Result<(), &'static str>,
    pub gettime64: fn(clock_id: u32) -> Result<u64, &'static str>,
    pub settime64: fn(clock_id: u32, ns: u64) -> Result<(), &'static str>,
    pub enable: fn(clock_id: u32, request: &PtpRequest) -> Result<(), &'static str>,
}

/// PTP request type (Linux `struct ptp_clock_request`).
#[derive(Debug, Clone)]
pub struct PtpRequest {
    pub kind: PtpRequestKind,
    pub index: u32,
    pub flags: u32,
}

/// PTP request kind (Linux `enum ptp_request_kind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PtpRequestKind {
    ExtTs,
    PerOut,
    Pps,
}

/// PTP pin configuration (Linux `struct ptp_pin_desc`).
#[derive(Debug, Clone)]
pub struct PtpPinDesc {
    pub name: String,
    pub index: u32,
    pub func: PtpPinFunc,
    pub chan: u32,
}

/// PTP pin function (Linux `enum ptp_pin_function`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PtpPinFunc {
    None,
    ExtTs,
    PerOut,
    PhySync,
    ClockRequest,
}

/// PTP clock instance.
pub struct PtpClock {
    pub info: PtpClockInfo,
    pub pins: Vec<PtpPinDesc>,
    pub enabled_requests: Vec<PtpRequest>,
}

// ── Registry ────────────────────────────────────────────────────────────

static CLOCK_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static PTP_CLOCKS: RwLock<BTreeMap<u32, PtpClock>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register a PTP clock.
pub fn register_clock(info: PtpClockInfo, pins: Vec<PtpPinDesc>) -> Result<u32, &'static str> {
    let id = CLOCK_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let clock = PtpClock {
        info,
        pins,
        enabled_requests: Vec::new(),
    };
    PTP_CLOCKS.write().insert(id, clock);
    Ok(id)
}

/// Unregister a PTP clock.
pub fn unregister_clock(clock_id: u32) -> Result<(), &'static str> {
    if PTP_CLOCKS.write().remove(&clock_id).is_none() {
        return Err("PTP clock not found");
    }
    Ok(())
}

/// Adjust the clock frequency.
pub fn adj_freq(clock_id: u32, delta: i64) -> Result<(), &'static str> {
    let adjfreq_fn = {
        let clocks = PTP_CLOCKS.read();
        let clock = clocks.get(&clock_id).ok_or("PTP clock not found")?;
        clock.info.adjfreq
    };
    (adjfreq_fn)(clock_id, delta)
}

/// Adjust the clock time by a delta.
pub fn adj_time(clock_id: u32, delta: i64) -> Result<(), &'static str> {
    let adjtime_fn = {
        let clocks = PTP_CLOCKS.read();
        let clock = clocks.get(&clock_id).ok_or("PTP clock not found")?;
        clock.info.adjtime
    };
    (adjtime_fn)(clock_id, delta)
}

/// Get the current clock time in nanoseconds.
pub fn get_time(clock_id: u32) -> Result<u64, &'static str> {
    let gettime_fn = {
        let clocks = PTP_CLOCKS.read();
        let clock = clocks.get(&clock_id).ok_or("PTP clock not found")?;
        clock.info.gettime64
    };
    (gettime_fn)(clock_id)
}

/// Set the clock time in nanoseconds.
pub fn set_time(clock_id: u32, ns: u64) -> Result<(), &'static str> {
    let settime_fn = {
        let clocks = PTP_CLOCKS.read();
        let clock = clocks.get(&clock_id).ok_or("PTP clock not found")?;
        clock.info.settime64
    };
    (settime_fn)(clock_id, ns)
}

/// Enable a PTP request (external timestamp, periodic output, PPS).
pub fn enable(clock_id: u32, request: &PtpRequest) -> Result<(), &'static str> {
    let enable_fn = {
        let clocks = PTP_CLOCKS.read();
        let clock = clocks.get(&clock_id).ok_or("PTP clock not found")?;
        if request.kind == PtpRequestKind::ExtTs && request.index >= clock.info.n_ext_ts {
            return Err("External TS index out of range");
        }
        if request.kind == PtpRequestKind::PerOut && request.index >= clock.info.n_per_out {
            return Err("Periodic output index out of range");
        }
        clock.info.enable
    };

    (enable_fn)(clock_id, request)?;

    let mut clocks = PTP_CLOCKS.write();
    if let Some(clock) = clocks.get_mut(&clock_id) {
        clock.enabled_requests.push(request.clone());
    }
    Ok(())
}

/// Disable a PTP request.
pub fn disable(clock_id: u32, request: &PtpRequest) -> Result<(), &'static str> {
    let mut clocks = PTP_CLOCKS.write();
    let clock = clocks.get_mut(&clock_id).ok_or("PTP clock not found")?;
    clock
        .enabled_requests
        .retain(|r| !(r.kind == request.kind && r.index == request.index));
    Ok(())
}

/// Configure a PTP pin.
pub fn pin_config(
    clock_id: u32,
    pin_index: u32,
    func: PtpPinFunc,
    chan: u32,
) -> Result<(), &'static str> {
    let mut clocks = PTP_CLOCKS.write();
    let clock = clocks.get_mut(&clock_id).ok_or("PTP clock not found")?;
    let pin = clock
        .pins
        .get_mut(pin_index as usize)
        .ok_or("Pin index out of range")?;
    pin.func = func;
    pin.chan = chan;
    Ok(())
}

/// List all registered PTP clocks.
pub fn list_clocks() -> Vec<(u32, String)> {
    PTP_CLOCKS
        .read()
        .iter()
        .map(|(id, c)| (*id, c.info.name.clone()))
        .collect()
}

/// Count registered clocks.
pub fn clock_count() -> usize {
    PTP_CLOCKS.read().len()
}

// ── Software PTP clock ──────────────────────────────────────────────────

fn sw_adjfreq(_clock_id: u32, _delta: i64) -> Result<(), &'static str> {
    Ok(())
}
fn sw_adjtime(_clock_id: u32, _delta: i64) -> Result<(), &'static str> {
    Ok(())
}
fn sw_gettime64(_clock_id: u32) -> Result<u64, &'static str> {
    Ok(crate::time::uptime_ns())
}
fn sw_settime64(_clock_id: u32, _ns: u64) -> Result<(), &'static str> {
    Ok(())
}
fn sw_enable(_clock_id: u32, _request: &PtpRequest) -> Result<(), &'static str> {
    Ok(())
}

/// Create a software PTP clock info backed by the kernel timer.
pub fn software_ptp_info() -> PtpClockInfo {
    PtpClockInfo {
        name: String::from("sw-ptp"),
        max_adj: 1_000_000,
        n_alarm: 0,
        n_ext_ts: 2,
        n_per_out: 2,
        n_pins: 2,
        pps: true,
        adjfreq: sw_adjfreq,
        adjtime: sw_adjtime,
        gettime64: sw_gettime64,
        settime64: sw_settime64,
        enable: sw_enable,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    let info = software_ptp_info();
    let mut pins = Vec::new();
    pins.push(PtpPinDesc {
        name: String::from("PIN0"),
        index: 0,
        func: PtpPinFunc::None,
        chan: 0,
    });
    pins.push(PtpPinDesc {
        name: String::from("PIN1"),
        index: 1,
        func: PtpPinFunc::None,
        chan: 0,
    });
    register_clock(info, pins)?;
    Ok(())
}
