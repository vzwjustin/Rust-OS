//! CPU idle states and menu governor.
//!
//! Drivers register C-states per CPU; the menu governor picks the deepest
//! state whose exit latency fits the predicted idle duration.

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use spin::RwLock;

/// One hardware idle state (C-state).
#[derive(Debug, Clone)]
pub struct CpuidleState {
    pub name: String,
    pub desc: String,
    pub exit_latency_us: u32,
    pub target_residency_us: u32,
    pub power_usage_mw: u32,
    pub disabled: bool,
}

impl CpuidleState {
    pub fn new(name: &str, exit_us: u32, target_us: u32, power_mw: u32) -> Self {
        Self {
            name: String::from(name),
            desc: String::from(name),
            exit_latency_us: exit_us,
            target_residency_us: target_us,
            power_usage_mw: power_mw,
            disabled: false,
        }
    }
}

/// Per-CPU idle driver.
#[derive(Debug, Clone)]
pub struct CpuidleDriver {
    pub name: String,
    pub states: Vec<CpuidleState>,
}

impl CpuidleDriver {
    /// Standard x86 C0/C1 driver used when ACPI C-states are not enumerated.
    pub fn x86_default() -> Self {
        Self {
            name: String::from("x86_default"),
            states: vec![
                CpuidleState::new("POLL", 0, 0, 0),
                CpuidleState::new("C1", 1, 2, 100),
                CpuidleState::new("C1E", 10, 20, 80),
                CpuidleState::new("C6", 100, 500, 10),
            ],
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct IdleStats {
    enters: u64,
    residency_us: u64,
}

static DRIVERS: RwLock<BTreeMap<u32, CpuidleDriver>> = RwLock::new(BTreeMap::new());
static STATS: RwLock<BTreeMap<(u32, usize), IdleStats>> = RwLock::new(BTreeMap::new());

/// Register an idle driver for `cpu`.
pub fn register_driver(cpu: u32, driver: CpuidleDriver) {
    DRIVERS.write().insert(cpu, driver);
}

/// Menu governor: select deepest state whose exit latency < predicted idle.
pub fn menu_select_state(cpu: u32, predicted_idle_us: u32) -> Option<usize> {
    let drivers = DRIVERS.read();
    let driver = drivers.get(&cpu)?;
    let mut best: Option<usize> = None;
    let mut best_power = u32::MAX;

    for (idx, state) in driver.states.iter().enumerate() {
        if state.disabled {
            continue;
        }
        if state.exit_latency_us <= predicted_idle_us
            && predicted_idle_us >= state.target_residency_us
            && state.power_usage_mw <= best_power
        {
            best = Some(idx);
            best_power = state.power_usage_mw;
        }
    }

    // Fall back to shallowest non-POLL state.
    best.or_else(|| {
        driver
            .states
            .iter()
            .enumerate()
            .filter(|(_, s)| !s.disabled && s.name != "POLL")
            .map(|(i, _)| i)
            .next()
    })
}

/// Enter idle state `state_idx` on `cpu`; returns residency in microseconds.
pub fn enter_idle_state(cpu: u32, state_idx: usize) -> u32 {
    let driver_name = {
        let drivers = DRIVERS.read();
        drivers
            .get(&cpu)
            .and_then(|d| d.states.get(state_idx))
            .map(|s| s.name.clone())
            .unwrap_or_else(|| String::from("C1"))
    };

    let residency = match driver_name.as_str() {
        "POLL" => 0,
        "C1" | "C1E" => {
            x86_64::instructions::interrupts::disable();
            x86_64::instructions::hlt();
            x86_64::instructions::interrupts::enable();
            50
        }
        "C6" => {
            x86_64::instructions::interrupts::disable();
            for _ in 0..100 {
                core::hint::spin_loop();
            }
            x86_64::instructions::hlt();
            x86_64::instructions::interrupts::enable();
            500
        }
        _ => 10,
    };

    let key = (cpu, state_idx);
    let mut stats = STATS.write();
    let entry = stats.entry(key).or_default();
    entry.enters += 1;
    entry.residency_us += residency as u64;
    residency
}

/// Idle loop helper used by the scheduler when no runnable task exists.
pub fn cpu_idle_loop(cpu: u32, predicted_us: u32) {
    if let Some(state) = menu_select_state(cpu, predicted_us) {
        let _ = enter_idle_state(cpu, state);
    } else {
        x86_64::instructions::hlt();
    }
}

/// Usage statistics for a CPU/state pair.
pub fn state_stats(cpu: u32, state_idx: usize) -> (u64, u64) {
    STATS
        .read()
        .get(&(cpu, state_idx))
        .map(|s| (s.enters, s.residency_us))
        .unwrap_or((0, 0))
}

/// Initialize cpuidle for all online CPUs.
pub fn init() {
    let count = crate::smp::cpu_count().max(1);
    for cpu in 0..count {
        if cpu == 0 || crate::smp::is_cpu_online(cpu) {
            register_driver(cpu, CpuidleDriver::x86_default());
        }
    }
    crate::serial_println!(
        "[cpuidle] initialized ({} drivers, menu governor)",
        DRIVERS.read().len()
    );
}
