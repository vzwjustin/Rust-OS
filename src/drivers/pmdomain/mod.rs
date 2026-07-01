//! Power domain (genpd) subsystem
//!
//! Provides power domain management for SoC subsystems with on/off,
//! performance state, and device attachment. Mirrors Linux's
//! `drivers/base/power/domain.c` (generic power domains / genpd).

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// Power domain state (Linux `enum genpd_state`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerDomainState {
    On,
    Off,
    Retention,
    DeepRetention,
    PowerOff,
}

/// Power domain operations (Linux `struct genpd_power_state` + ops).
pub struct PowerDomainOps {
    pub power_on: fn() -> Result<(), &'static str>,
    pub power_off: fn() -> Result<(), &'static str>,
    pub set_performance_state: fn(perf: u32) -> Result<(), &'static str>,
    pub get_performance_state: fn() -> u32,
    pub get_name: fn() -> &'static str,
}

struct PowerDomain {
    id: u32,
    name: String,
    ops: &'static PowerDomainOps,
    state: PowerDomainState,
    performance_state: u32,
    device_count: u32,
    active_devices: u32,
    parent: Option<u32>,
    children: Vec<u32>,
    always_on: bool,
}

// ── Platform power domains ──────────────────────────────────────────────

static CPU_DOMAIN_ON: AtomicBool = AtomicBool::new(true);
static GPU_DOMAIN_ON: AtomicBool = AtomicBool::new(false);
static AUDIO_DOMAIN_ON: AtomicBool = AtomicBool::new(false);

fn cpu_pd_on() -> Result<(), &'static str> {
    CPU_DOMAIN_ON.store(true, Ordering::Relaxed);
    Ok(())
}
fn cpu_pd_off() -> Result<(), &'static str> {
    CPU_DOMAIN_ON.store(false, Ordering::Relaxed);
    Ok(())
}
fn cpu_pd_set_perf(p: u32) -> Result<(), &'static str> {
    let _ = p;
    Ok(())
}
fn cpu_pd_get_perf() -> u32 {
    3
}
fn cpu_pd_name() -> &'static str {
    "cpu-pd"
}

pub static CPU_PD_OPS: PowerDomainOps = PowerDomainOps {
    power_on: cpu_pd_on,
    power_off: cpu_pd_off,
    set_performance_state: cpu_pd_set_perf,
    get_performance_state: cpu_pd_get_perf,
    get_name: cpu_pd_name,
};

fn gpu_pd_on() -> Result<(), &'static str> {
    GPU_DOMAIN_ON.store(true, Ordering::Relaxed);
    Ok(())
}
fn gpu_pd_off() -> Result<(), &'static str> {
    GPU_DOMAIN_ON.store(false, Ordering::Relaxed);
    Ok(())
}
fn gpu_pd_set_perf(p: u32) -> Result<(), &'static str> {
    let _ = p;
    Ok(())
}
fn gpu_pd_get_perf() -> u32 {
    0
}
fn gpu_pd_name() -> &'static str {
    "gpu-pd"
}

pub static GPU_PD_OPS: PowerDomainOps = PowerDomainOps {
    power_on: gpu_pd_on,
    power_off: gpu_pd_off,
    set_performance_state: gpu_pd_set_perf,
    get_performance_state: gpu_pd_get_perf,
    get_name: gpu_pd_name,
};

fn audio_pd_on() -> Result<(), &'static str> {
    AUDIO_DOMAIN_ON.store(true, Ordering::Relaxed);
    Ok(())
}
fn audio_pd_off() -> Result<(), &'static str> {
    AUDIO_DOMAIN_ON.store(false, Ordering::Relaxed);
    Ok(())
}
fn audio_pd_set_perf(p: u32) -> Result<(), &'static str> {
    let _ = p;
    Ok(())
}
fn audio_pd_get_perf() -> u32 {
    0
}
fn audio_pd_name() -> &'static str {
    "audio-pd"
}

pub static AUDIO_PD_OPS: PowerDomainOps = PowerDomainOps {
    power_on: audio_pd_on,
    power_off: audio_pd_off,
    set_performance_state: audio_pd_set_perf,
    get_performance_state: audio_pd_get_perf,
    get_name: audio_pd_name,
};

// ── Registry ────────────────────────────────────────────────────────────

static POWER_DOMAINS: RwLock<BTreeMap<u32, PowerDomain>> = RwLock::new(BTreeMap::new());
static NEXT_PD_ID: AtomicU32 = AtomicU32::new(0);

// ── Public API ──────────────────────────────────────────────────────────

/// Register a power domain (Linux `pm_genpd_init`).
pub fn register_domain(
    name: &str,
    ops: &'static PowerDomainOps,
    parent: Option<u32>,
    always_on: bool,
) -> Result<u32, &'static str> {
    if let Some(pid) = parent {
        if !POWER_DOMAINS.read().contains_key(&pid) {
            return Err("Parent power domain not found");
        }
    }

    let id = NEXT_PD_ID.fetch_add(1, Ordering::SeqCst);
    let state = if always_on {
        PowerDomainState::On
    } else {
        PowerDomainState::PowerOff
    };

    POWER_DOMAINS.write().insert(
        id,
        PowerDomain {
            id,
            name: String::from(name),
            ops,
            state,
            performance_state: 0,
            device_count: 0,
            active_devices: 0,
            parent,
            children: Vec::new(),
            always_on,
        },
    );

    // Add to parent's children.
    if let Some(pid) = parent {
        let mut domains = POWER_DOMAINS.write();
        if let Some(parent_dom) = domains.get_mut(&pid) {
            parent_dom.children.push(id);
        }
    }

    Ok(id)
}

/// Attach a device to a power domain (Linux `genpd_dev_pm_attach`).
pub fn attach_device(domain_id: u32) -> Result<(), &'static str> {
    let mut domains = POWER_DOMAINS.write();
    let dom = domains
        .get_mut(&domain_id)
        .ok_or("Power domain not found")?;
    dom.device_count += 1;
    dom.active_devices += 1;

    // Power on the domain if this is the first active device.
    if dom.active_devices == 1 && dom.state != PowerDomainState::On {
        if !dom.always_on {
            if let Err(e) = (dom.ops.power_on)() {
                // Roll back the bookkeeping so a failed power-on does not leave
                // the domain counted as active.
                dom.device_count -= 1;
                dom.active_devices -= 1;
                return Err(e);
            }
        }
        dom.state = PowerDomainState::On;
    }
    Ok(())
}

/// Detach a device from a power domain (Linux `genpd_dev_pm_detach`).
pub fn detach_device(domain_id: u32) -> Result<(), &'static str> {
    let ops = {
        let mut domains = POWER_DOMAINS.write();
        let dom = domains
            .get_mut(&domain_id)
            .ok_or("Power domain not found")?;
        if dom.active_devices > 0 {
            dom.active_devices -= 1;
        }
        if dom.device_count > 0 {
            dom.device_count -= 1;
        }
        // Power off if no active devices and not always-on.
        if dom.active_devices == 0 && !dom.always_on {
            dom.state = PowerDomainState::PowerOff;
            Some(dom.ops)
        } else {
            None
        }
    };

    if let Some(ops) = ops {
        let _ = (ops.power_off)();
    }
    Ok(())
}

/// Power on a domain (Linux `genpd_power_on`).
pub fn power_on(domain_id: u32) -> Result<(), &'static str> {
    let (ops, already_on, parent_id) = {
        let mut domains = POWER_DOMAINS.write();
        let dom = domains
            .get_mut(&domain_id)
            .ok_or("Power domain not found")?;
        if dom.state == PowerDomainState::On {
            return Ok(());
        }
        (dom.ops, dom.always_on, dom.parent)
    };

    // Power on parent first.
    if let Some(pid) = parent_id {
        power_on(pid)?;
    }

    if !already_on {
        (ops.power_on)()?;
    }

    let mut domains = POWER_DOMAINS.write();
    if let Some(dom) = domains.get_mut(&domain_id) {
        dom.state = PowerDomainState::On;
    }
    Ok(())
}

/// Power off a domain (Linux `genpd_power_off`).
pub fn power_off(domain_id: u32) -> Result<(), &'static str> {
    let (ops, always_on, active, children) = {
        let domains = POWER_DOMAINS.read();
        let dom = domains.get(&domain_id).ok_or("Power domain not found")?;
        if dom.always_on {
            return Err("Cannot power off always-on domain");
        }
        if dom.active_devices > 0 {
            return Err("Cannot power off: active devices attached");
        }
        (
            dom.ops,
            dom.always_on,
            dom.active_devices,
            dom.children.clone(),
        )
    };

    let _ = (always_on, active);

    // Power off children first.
    for child_id in children {
        let _ = power_off(child_id);
    }

    (ops.power_off)()?;

    let mut domains = POWER_DOMAINS.write();
    if let Some(dom) = domains.get_mut(&domain_id) {
        dom.state = PowerDomainState::PowerOff;
    }
    Ok(())
}

/// Set performance state (Linux `genpd_set_performance_state`).
pub fn set_performance_state(domain_id: u32, perf: u32) -> Result<(), &'static str> {
    let ops = {
        let mut domains = POWER_DOMAINS.write();
        let dom = domains
            .get_mut(&domain_id)
            .ok_or("Power domain not found")?;
        dom.performance_state = perf;
        dom.ops
    };
    (ops.set_performance_state)(perf)
}

/// Get performance state.
pub fn get_performance_state(domain_id: u32) -> Result<u32, &'static str> {
    let domains = POWER_DOMAINS.read();
    let dom = domains.get(&domain_id).ok_or("Power domain not found")?;
    Ok(dom.performance_state)
}

/// Get domain state.
pub fn get_state(domain_id: u32) -> Result<PowerDomainState, &'static str> {
    let domains = POWER_DOMAINS.read();
    let dom = domains.get(&domain_id).ok_or("Power domain not found")?;
    Ok(dom.state)
}

/// Get domain name.
pub fn get_name(domain_id: u32) -> Result<String, &'static str> {
    let domains = POWER_DOMAINS.read();
    let dom = domains.get(&domain_id).ok_or("Power domain not found")?;
    Ok(dom.name.clone())
}

/// Number of registered power domains.
pub fn domain_count() -> usize {
    POWER_DOMAINS.read().len()
}

/// Get all domain IDs with their states.
pub fn get_all_domains() -> Vec<(u32, String, PowerDomainState)> {
    POWER_DOMAINS
        .read()
        .iter()
        .map(|(id, d)| (*id, d.name.clone(), d.state))
        .collect()
}

/// Initialize power domain subsystem with CPU, GPU, and audio domains.
pub fn init() -> Result<(), &'static str> {
    if !POWER_DOMAINS.read().is_empty() {
        return Ok(());
    }

    // CPU domain is always-on (root domain).
    register_domain("cpu-pd", &CPU_PD_OPS, None, true)?;

    // GPU domain is a child of CPU domain, not always-on.
    register_domain("gpu-pd", &GPU_PD_OPS, Some(0), false)?;

    // Audio domain is a child of CPU domain, not always-on.
    register_domain("audio-pd", &AUDIO_PD_OPS, Some(0), false)?;

    crate::serial_println!("pmdomain: {} domain(s) registered", domain_count());
    Ok(())
}
