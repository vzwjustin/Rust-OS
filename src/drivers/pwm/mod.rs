//! PWM (Pulse Width Modulation) framework
//!
//! Provides PWM chip registration, period/duty cycle configuration, and
//! enable/disable control similar to Linux's `drivers/pwm/core.c`.
//! Includes a software PWM chip with virtual channels for platform use.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// PWM state (Linux `struct pwm_state`).
#[derive(Debug, Clone, Copy)]
pub struct PwmState {
    pub period_ns: u64,
    pub duty_cycle_ns: u64,
    pub enabled: bool,
    pub polarity: PwmPolarity,
}

impl Default for PwmState {
    fn default() -> Self {
        Self {
            period_ns: 1_000_000, // 1ms default
            duty_cycle_ns: 0,
            enabled: false,
            polarity: PwmPolarity::Normal,
        }
    }
}

/// PWM polarity (Linux `enum pwm_polarity`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PwmPolarity {
    Normal,
    Inversed,
}

/// Operations implemented by a PWM controller driver (Linux `struct pwm_ops`).
pub struct PwmChipOps {
    pub apply: fn(channel: u32, state: &PwmState) -> Result<(), &'static str>,
    pub get_state: fn(channel: u32) -> Result<PwmState, &'static str>,
    pub get_name: fn() -> &'static str,
    pub get_npwm: fn() -> u32,
}

struct PwmChip {
    id: u32,
    name: String,
    npwm: u32,
    ops: PwmChipOps,
}

// ── Software PWM chip ───────────────────────────────────────────────────

struct SoftwarePwmChannel {
    state: PwmState,
}

static mut SW_PWM_CHANNELS: Vec<SoftwarePwmChannel> = Vec::new();

fn sw_apply(channel: u32, state: &PwmState) -> Result<(), &'static str> {
    let channels = unsafe { &mut SW_PWM_CHANNELS };
    let idx = channel as usize;
    if idx >= channels.len() {
        return Err("PWM channel out of range");
    }
    channels[idx].state = *state;
    Ok(())
}

fn sw_get_state(channel: u32) -> Result<PwmState, &'static str> {
    let channels = unsafe { &SW_PWM_CHANNELS };
    let idx = channel as usize;
    if idx >= channels.len() {
        return Err("PWM channel out of range");
    }
    Ok(channels[idx].state)
}

fn sw_name() -> &'static str {
    "software-pwm"
}

fn sw_npwm() -> u32 {
    4
}

const SOFTWARE_PWM_OPS: PwmChipOps = PwmChipOps {
    apply: sw_apply,
    get_state: sw_get_state,
    get_name: sw_name,
    get_npwm: sw_npwm,
};

// ── Registry ────────────────────────────────────────────────────────────

static PWM_CHIPS: RwLock<BTreeMap<u32, PwmChip>> = RwLock::new(BTreeMap::new());
static NEXT_CHIP_ID: AtomicU32 = AtomicU32::new(0);

// ── Public API ──────────────────────────────────────────────────────────

/// Register a PWM chip (Linux `pwmchip_add`).
pub fn register_chip(name: &str, ops: PwmChipOps) -> Result<u32, &'static str> {
    let npwm = (ops.get_npwm)();
    if npwm == 0 {
        return Err("PWM chip must expose at least one channel");
    }
    let id = NEXT_CHIP_ID.fetch_add(1, Ordering::SeqCst);
    PWM_CHIPS.write().insert(
        id,
        PwmChip {
            id,
            name: String::from(name),
            npwm,
            ops,
        },
    );
    Ok(id)
}

/// Request a PWM channel (Linux `pwm_request`).
pub fn request_channel(chip_id: u32, _channel: u32) -> Result<(), &'static str> {
    let chips = PWM_CHIPS.read();
    let chip = chips.get(&chip_id).ok_or("PWM chip not found")?;
    if _channel >= chip.npwm {
        return Err("PWM channel out of range");
    }
    Ok(())
}

/// Apply a PWM state (Linux `pwm_apply_state`).
pub fn apply_state(chip_id: u32, channel: u32, state: &PwmState) -> Result<(), &'static str> {
    let chips = PWM_CHIPS.read();
    let chip = chips.get(&chip_id).ok_or("PWM chip not found")?;
    if channel >= chip.npwm {
        return Err("PWM channel out of range");
    }
    if state.duty_cycle_ns > state.period_ns {
        return Err("Duty cycle cannot exceed period");
    }
    (chip.ops.apply)(channel, state)
}

/// Get current PWM state (Linux `pwm_get_state`).
pub fn get_state(chip_id: u32, channel: u32) -> Result<PwmState, &'static str> {
    let chips = PWM_CHIPS.read();
    let chip = chips.get(&chip_id).ok_or("PWM chip not found")?;
    if channel >= chip.npwm {
        return Err("PWM channel out of range");
    }
    (chip.ops.get_state)(channel)
}

/// Enable a PWM channel (Linux `pwm_enable`).
pub fn enable(chip_id: u32, channel: u32) -> Result<(), &'static str> {
    let mut state = get_state(chip_id, channel)?;
    state.enabled = true;
    apply_state(chip_id, channel, &state)
}

/// Disable a PWM channel (Linux `pwm_disable`).
pub fn disable(chip_id: u32, channel: u32) -> Result<(), &'static str> {
    let mut state = get_state(chip_id, channel)?;
    state.enabled = false;
    apply_state(chip_id, channel, &state)
}

/// Set duty cycle in nanoseconds (Linux `pwm_set_duty_cycle`).
pub fn set_duty_cycle(chip_id: u32, channel: u32, duty_ns: u64) -> Result<(), &'static str> {
    let mut state = get_state(chip_id, channel)?;
    if duty_ns > state.period_ns {
        return Err("Duty cycle exceeds period");
    }
    state.duty_cycle_ns = duty_ns;
    apply_state(chip_id, channel, &state)
}

/// Set period in nanoseconds (Linux `pwm_set_period`).
pub fn set_period(chip_id: u32, channel: u32, period_ns: u64) -> Result<(), &'static str> {
    let mut state = get_state(chip_id, channel)?;
    state.period_ns = period_ns;
    if state.duty_cycle_ns > period_ns {
        state.duty_cycle_ns = period_ns;
    }
    apply_state(chip_id, channel, &state)
}

/// Set polarity (Linux `pwm_set_polarity`).
pub fn set_polarity(chip_id: u32, channel: u32, polarity: PwmPolarity) -> Result<(), &'static str> {
    let mut state = get_state(chip_id, channel)?;
    state.polarity = polarity;
    apply_state(chip_id, channel, &state)
}

/// Get duty cycle as percentage (0-100).
pub fn get_duty_percent(chip_id: u32, channel: u32) -> Result<u8, &'static str> {
    let state = get_state(chip_id, channel)?;
    if state.period_ns == 0 {
        return Ok(0);
    }
    let percent = (state.duty_cycle_ns * 100) / state.period_ns;
    Ok(percent.min(100) as u8)
}

/// Number of registered PWM chips.
pub fn chip_count() -> usize {
    PWM_CHIPS.read().len()
}

/// Total number of PWM channels across all chips.
pub fn total_channels() -> u32 {
    PWM_CHIPS.read().values().map(|c| c.npwm).sum()
}

/// Initialize PWM subsystem with software chip.
pub fn init() -> Result<(), &'static str> {
    if !PWM_CHIPS.read().is_empty() {
        return Ok(());
    }

    let npwm = sw_npwm() as usize;
    unsafe {
        SW_PWM_CHANNELS = (0..npwm)
            .map(|_| SoftwarePwmChannel {
                state: PwmState::default(),
            })
            .collect();
    }

    register_chip("software-pwm", SOFTWARE_PWM_OPS)?;
    crate::serial_println!("pwm: software chip registered ({} channels)", npwm);
    crate::serial_println!("pwm: subsystem ready");
    Ok(())
}
