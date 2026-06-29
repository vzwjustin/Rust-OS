//! Devfreq (Device frequency scaling) subsystem
//!
//! Provides dynamic frequency scaling for non-CPU devices (DDR, bus, GPU)
//! with governor-based policy and OPP integration. Mirrors Linux's
//! `drivers/devfreq/devfreq.c`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

use crate::drivers::opp;

// ── Types ───────────────────────────────────────────────────────────────

/// Devfreq governor type (Linux `enum devfreq_governor`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DevfreqGovernor {
    Userspace,
    SimpleOnDemand,
    Performance,
    Powersave,
    Passive,
}

impl DevfreqGovernor {
    pub fn name(self) -> &'static str {
        match self {
            DevfreqGovernor::Userspace => "userspace",
            DevfreqGovernor::SimpleOnDemand => "simple-ondemand",
            DevfreqGovernor::Performance => "performance",
            DevfreqGovernor::Powersave => "powersave",
            DevfreqGovernor::Passive => "passive",
        }
    }
}

/// Devfreq device profile (Linux `struct devfreq_dev_profile`).
pub struct DevfreqProfile {
    pub get_target_freq: fn(current_hz: u64, busy_time: u64, total_time: u64) -> u64,
    pub get_cur_freq: fn() -> u64,
    pub set_freq: fn(freq_hz: u64) -> Result<(), &'static str>,
    pub get_name: fn() -> &'static str,
    pub initial_freq: u64,
    pub polling_interval_ms: u32,
}

struct DevfreqDevice {
    id: u32,
    name: String,
    profile: &'static DevfreqProfile,
    opp_table_id: u32,
    governor: DevfreqGovernor,
    current_freq: u64,
    target_freq: u64,
    last_busy_time: u64,
    last_total_time: u64,
    min_freq: u64,
    max_freq: u64,
    polling_ms: u32,
    transitions: u64,
    total_trans_time_ns: u64,
}

// ── Governors ───────────────────────────────────────────────────────────

fn gov_userspace(_cur: u64, _busy: u64, _total: u64) -> u64 {
    // Userspace governor returns the current target (set by user).
    // The actual target is stored in the device and applied directly.
    _cur
}

fn gov_simple_ondemand(current: u64, busy: u64, total: u64) -> u64 {
    if total == 0 {
        return current;
    }
    let load = (busy * 100) / total;
    // If load > 80%, go to max. If < 20%, go to min.
    // Otherwise scale proportionally.
    if load > 80 {
        // Request max frequency (caller will clamp to max_freq).
        current * 2
    } else if load < 20 {
        current / 2
    } else {
        current
    }
}

fn gov_performance(current: u64, _busy: u64, _total: u64) -> u64 {
    // Always request max frequency.
    current * 2
}

fn gov_powersave(current: u64, _busy: u64, _total: u64) -> u64 {
    // Always request min frequency.
    current / 4
}

fn gov_passive(current: u64, _busy: u64, _total: u64) -> u64 {
    // Passive governor follows parent device.
    current
}

// ── DDR devfreq profile ─────────────────────────────────────────────────

static mut DDR_FREQ: u64 = 1_600_000_000; // 1600 MHz DDR

fn ddr_get_target(current: u64, busy: u64, total: u64) -> u64 {
    gov_simple_ondemand(current, busy, total)
}

fn ddr_get_cur_freq() -> u64 {
    unsafe { DDR_FREQ }
}

fn ddr_set_freq(freq: u64) -> Result<(), &'static str> {
    // Clamp to valid DDR frequencies (400, 800, 1600, 3200 MHz).
    let valid = [400_000_000u64, 800_000_000, 1_600_000_000, 3_200_000_000];
    let nearest = valid
        .iter()
        .min_by_key(|&&f| if f >= freq { f - freq } else { freq - f })
        .copied()
        .ok_or("No valid DDR frequency")?;
    unsafe {
        DDR_FREQ = nearest;
    }
    Ok(())
}

fn ddr_name() -> &'static str {
    "ddr-devfreq"
}

pub static DDR_DEVFREQ_PROFILE: DevfreqProfile = DevfreqProfile {
    get_target_freq: ddr_get_target,
    get_cur_freq: ddr_get_cur_freq,
    set_freq: ddr_set_freq,
    get_name: ddr_name,
    initial_freq: 1_600_000_000,
    polling_interval_ms: 100,
};

// ── Registry ────────────────────────────────────────────────────────────

static DEVFREQ_DEVICES: RwLock<BTreeMap<u32, DevfreqDevice>> = RwLock::new(BTreeMap::new());
static NEXT_DEVFREQ_ID: AtomicU32 = AtomicU32::new(0);

// ── Public API ──────────────────────────────────────────────────────────

/// Register a devfreq device (Linux `devfreq_add_device`).
pub fn register_device(
    profile: &'static DevfreqProfile,
    opp_table_id: u32,
    governor: DevfreqGovernor,
) -> Result<u32, &'static str> {
    let name = (profile.get_name)();
    let initial_freq = profile.initial_freq;
    let polling_ms = profile.polling_interval_ms;

    // Get min/max from OPP table.
    let (min_freq, max_freq) = {
        let opps = opp::get_opps(opp_table_id)?;
        if opps.is_empty() {
            return Err("OPP table is empty");
        }
        let min = opps.iter().map(|o| o.frequency_hz).min().unwrap_or(0);
        let max = opps
            .iter()
            .map(|o| o.frequency_hz)
            .max()
            .unwrap_or(u64::MAX);
        (min, max)
    };

    let id = NEXT_DEVFREQ_ID.fetch_add(1, Ordering::SeqCst);
    DEVFREQ_DEVICES.write().insert(
        id,
        DevfreqDevice {
            id,
            name: String::from(name),
            profile,
            opp_table_id,
            governor,
            current_freq: initial_freq,
            target_freq: initial_freq,
            last_busy_time: 0,
            last_total_time: 0,
            min_freq,
            max_freq,
            polling_ms,
            transitions: 0,
            total_trans_time_ns: 0,
        },
    );
    Ok(id)
}

/// Update devfreq (Linux `devfreq_update_target`).
/// Called periodically by the polling timer or on demand.
pub fn update_target(device_id: u32, busy_time: u64, total_time: u64) -> Result<u64, &'static str> {
    let (profile, governor, min_freq, max_freq, current_freq) = {
        let mut devices = DEVFREQ_DEVICES.write();
        let dev = devices
            .get_mut(&device_id)
            .ok_or("Devfreq device not found")?;
        dev.last_busy_time = busy_time;
        dev.last_total_time = total_time;
        (
            dev.profile,
            dev.governor,
            dev.min_freq,
            dev.max_freq,
            dev.current_freq,
        )
    };

    // Get target frequency from governor.
    let raw_target = match governor {
        DevfreqGovernor::Userspace => current_freq,
        DevfreqGovernor::SimpleOnDemand => gov_simple_ondemand(current_freq, busy_time, total_time),
        DevfreqGovernor::Performance => gov_performance(current_freq, busy_time, total_time),
        DevfreqGovernor::Powersave => gov_powersave(current_freq, busy_time, total_time),
        DevfreqGovernor::Passive => gov_passive(current_freq, busy_time, total_time),
    };

    // Clamp to min/max.
    let clamped = raw_target.clamp(min_freq, max_freq);

    // Apply if different from current.
    if clamped != current_freq {
        (profile.set_freq)(clamped)?;
        let mut devices = DEVFREQ_DEVICES.write();
        if let Some(dev) = devices.get_mut(&device_id) {
            dev.current_freq = clamped;
            dev.target_freq = clamped;
            dev.transitions += 1;
        }
    }

    Ok(clamped)
}

/// Set governor (Linux `devfreq_governor_set`).
pub fn set_governor(device_id: u32, governor: DevfreqGovernor) -> Result<(), &'static str> {
    let mut devices = DEVFREQ_DEVICES.write();
    let dev = devices
        .get_mut(&device_id)
        .ok_or("Devfreq device not found")?;
    dev.governor = governor;
    Ok(())
}

/// Get current frequency (Linux `devfreq_get_freq`).
pub fn get_freq(device_id: u32) -> Result<u64, &'static str> {
    let devices = DEVFREQ_DEVICES.read();
    let dev = devices.get(&device_id).ok_or("Devfreq device not found")?;
    Ok(dev.current_freq)
}

/// Set frequency limits (Linux `devfreq_set_freq_range`).
pub fn set_freq_range(device_id: u32, min: u64, max: u64) -> Result<(), &'static str> {
    let mut devices = DEVFREQ_DEVICES.write();
    let dev = devices
        .get_mut(&device_id)
        .ok_or("Devfreq device not found")?;
    if min > max {
        return Err("Min frequency cannot exceed max");
    }
    dev.min_freq = min;
    dev.max_freq = max;
    Ok(())
}

/// Get device info.
pub fn get_info(
    device_id: u32,
) -> Result<(String, DevfreqGovernor, u64, u64, u64, u64), &'static str> {
    let devices = DEVFREQ_DEVICES.read();
    let dev = devices.get(&device_id).ok_or("Devfreq device not found")?;
    Ok((
        dev.name.clone(),
        dev.governor,
        dev.current_freq,
        dev.min_freq,
        dev.max_freq,
        dev.transitions,
    ))
}

/// Number of registered devfreq devices.
pub fn device_count() -> usize {
    DEVFREQ_DEVICES.read().len()
}

/// Initialize devfreq subsystem with DDR device.
pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("devfreq: subsystem ready");
    Ok(())
}
