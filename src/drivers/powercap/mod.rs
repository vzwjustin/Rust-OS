//! Power capping subsystem
//!
//! Provides framework for enforcing power consumption limits.
//! Mirrors Linux's `drivers/powercap/`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// Power cap zone (Linux `struct powercap_zone`).
pub struct PowercapZone {
    pub id: u32,
    pub name: String,
    pub parent_id: Option<u32>,
    pub child_ids: Vec<u32>,
    pub constraint_ids: Vec<u32>,
    pub state: PowercapState,
    pub ops: PowercapZoneOps,
    pub current_power: u64, // microwatts
    pub max_power: u64,     // microwatts
    pub energy_uj: u64,     // microjoules
}

/// Power cap state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowercapState {
    Unregistered,
    Registered,
    Active,
    Constrained,
    Suspended,
}

/// Power cap zone operations (Linux `struct powercap_zone_ops`).
pub struct PowercapZoneOps {
    pub get_energy_uj: fn(zone_id: u32) -> Result<u64, &'static str>,
    pub reset_energy_uj: fn(zone_id: u32) -> Result<(), &'static str>,
    pub get_max_power_uw: fn(zone_id: u32) -> Result<u64, &'static str>,
    pub get_power_uw: fn(zone_id: u32) -> Result<u64, &'static str>,
    pub set_power_limit_uw:
        fn(zone_id: u32, constraint_id: u32, limit_uw: u64) -> Result<(), &'static str>,
    pub get_power_limit_uw: fn(zone_id: u32, constraint_id: u32) -> Result<u64, &'static str>,
    pub get_time_window_us: fn(zone_id: u32, constraint_id: u32) -> Result<u64, &'static str>,
    pub set_time_window_us:
        fn(zone_id: u32, constraint_id: u32, window_us: u64) -> Result<(), &'static str>,
}

/// Power cap constraint (Linux `struct powercap_constraint`).
pub struct PowercapConstraint {
    pub id: u32,
    pub zone_id: u32,
    pub name: String,
    pub power_limit_uw: u64,
    pub time_window_us: u64,
    pub max_power_uw: u64,
    pub min_power_uw: u64,
    pub max_time_window_us: u64,
    pub min_time_window_us: u64,
}

// ── Registry ────────────────────────────────────────────────────────────

static ZONE_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static CONSTRAINT_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static POWERCAP_ZONES: RwLock<BTreeMap<u32, PowercapZone>> = RwLock::new(BTreeMap::new());
static POWERCAP_CONSTRAINTS: RwLock<BTreeMap<u32, PowercapConstraint>> =
    RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register a power cap zone (Linux `powercap_register_zone`).
pub fn register_zone(
    name: &str,
    parent_id: Option<u32>,
    max_power: u64,
    ops: PowercapZoneOps,
) -> Result<u32, &'static str> {
    let id = ZONE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let zone = PowercapZone {
        id,
        name: String::from(name),
        parent_id,
        child_ids: Vec::new(),
        constraint_ids: Vec::new(),
        state: PowercapState::Registered,
        ops,
        current_power: 0,
        max_power,
        energy_uj: 0,
    };
    POWERCAP_ZONES.write().insert(id, zone);

    if let Some(pid) = parent_id {
        let mut zones = POWERCAP_ZONES.write();
        if let Some(parent) = zones.get_mut(&pid) {
            parent.child_ids.push(id);
        }
    }
    Ok(id)
}

/// Add a constraint to a zone (Linux `powercap_register_constraint`).
pub fn add_constraint(
    zone_id: u32,
    name: &str,
    max_power_uw: u64,
    min_power_uw: u64,
    max_time_us: u64,
    min_time_us: u64,
) -> Result<u32, &'static str> {
    let id = CONSTRAINT_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let constraint = PowercapConstraint {
        id,
        zone_id,
        name: String::from(name),
        power_limit_uw: max_power_uw,
        time_window_us: 1_000_000, // 1 second default
        max_power_uw,
        min_power_uw,
        max_time_window_us: max_time_us,
        min_time_window_us: min_time_us,
    };
    POWERCAP_CONSTRAINTS.write().insert(id, constraint);

    let mut zones = POWERCAP_ZONES.write();
    if let Some(zone) = zones.get_mut(&zone_id) {
        zone.constraint_ids.push(id);
    }
    Ok(id)
}

/// Get current power consumption (Linux `powercap_get_power_uw`).
pub fn get_power_uw(zone_id: u32) -> Result<u64, &'static str> {
    let get_fn = {
        let zones = POWERCAP_ZONES.read();
        let zone = zones.get(&zone_id).ok_or("Powercap zone not found")?;
        zone.ops.get_power_uw
    };
    let power = (get_fn)(zone_id)?;

    let mut zones = POWERCAP_ZONES.write();
    if let Some(zone) = zones.get_mut(&zone_id) {
        zone.current_power = power;
    }
    Ok(power)
}

/// Get energy counter (Linux `powercap_get_energy_uj`).
pub fn get_energy_uj(zone_id: u32) -> Result<u64, &'static str> {
    let get_fn = {
        let zones = POWERCAP_ZONES.read();
        let zone = zones.get(&zone_id).ok_or("Powercap zone not found")?;
        zone.ops.get_energy_uj
    };
    let energy = (get_fn)(zone_id)?;

    let mut zones = POWERCAP_ZONES.write();
    if let Some(zone) = zones.get_mut(&zone_id) {
        zone.energy_uj = energy;
    }
    Ok(energy)
}

/// Reset energy counter (Linux `powercap_reset_energy_uj`).
pub fn reset_energy_uj(zone_id: u32) -> Result<(), &'static str> {
    let reset_fn = {
        let zones = POWERCAP_ZONES.read();
        let zone = zones.get(&zone_id).ok_or("Powercap zone not found")?;
        zone.ops.reset_energy_uj
    };
    (reset_fn)(zone_id)?;

    let mut zones = POWERCAP_ZONES.write();
    if let Some(zone) = zones.get_mut(&zone_id) {
        zone.energy_uj = 0;
    }
    Ok(())
}

/// Set power limit (Linux `powercap_set_power_limit_uw`).
pub fn set_power_limit(
    zone_id: u32,
    constraint_id: u32,
    limit_uw: u64,
) -> Result<(), &'static str> {
    let set_fn = {
        let zones = POWERCAP_ZONES.read();
        let zone = zones.get(&zone_id).ok_or("Powercap zone not found")?;
        zone.ops.set_power_limit_uw
    };
    (set_fn)(zone_id, constraint_id, limit_uw)?;

    let mut constraints = POWERCAP_CONSTRAINTS.write();
    if let Some(con) = constraints.get_mut(&constraint_id) {
        con.power_limit_uw = limit_uw;
    }

    let mut zones = POWERCAP_ZONES.write();
    if let Some(zone) = zones.get_mut(&zone_id) {
        zone.state = PowercapState::Constrained;
    }
    Ok(())
}

/// Get power limit (Linux `powercap_get_power_limit_uw`).
pub fn get_power_limit(zone_id: u32, constraint_id: u32) -> Result<u64, &'static str> {
    let get_fn = {
        let zones = POWERCAP_ZONES.read();
        let zone = zones.get(&zone_id).ok_or("Powercap zone not found")?;
        zone.ops.get_power_limit_uw
    };
    (get_fn)(zone_id, constraint_id)
}

/// Set time window (Linux `powercap_set_time_window_us`).
pub fn set_time_window(
    zone_id: u32,
    constraint_id: u32,
    window_us: u64,
) -> Result<(), &'static str> {
    let set_fn = {
        let zones = POWERCAP_ZONES.read();
        let zone = zones.get(&zone_id).ok_or("Powercap zone not found")?;
        zone.ops.set_time_window_us
    };
    (set_fn)(zone_id, constraint_id, window_us)?;

    let mut constraints = POWERCAP_CONSTRAINTS.write();
    if let Some(con) = constraints.get_mut(&constraint_id) {
        con.time_window_us = window_us;
    }
    Ok(())
}

/// Get time window (Linux `powercap_get_time_window_us`).
pub fn get_time_window(zone_id: u32, constraint_id: u32) -> Result<u64, &'static str> {
    let get_fn = {
        let zones = POWERCAP_ZONES.read();
        let zone = zones.get(&zone_id).ok_or("Powercap zone not found")?;
        zone.ops.get_time_window_us
    };
    (get_fn)(zone_id, constraint_id)
}

/// List all zones.
pub fn list_zones() -> Vec<(u32, String, Option<u32>, PowercapState, u64, usize)> {
    POWERCAP_ZONES
        .read()
        .iter()
        .map(|(id, z)| {
            (
                *id,
                z.name.clone(),
                z.parent_id,
                z.state,
                z.max_power,
                z.constraint_ids.len(),
            )
        })
        .collect()
}

/// Count registered zones.
pub fn zone_count() -> usize {
    POWERCAP_ZONES.read().len()
}

// ── Software powercap ───────────────────────────────────────────────────

static SW_ENERGY: AtomicU32 = AtomicU32::new(0);

fn sw_get_energy_uj(_zone_id: u32) -> Result<u64, &'static str> {
    Ok(SW_ENERGY.fetch_add(1_000_000, Ordering::Relaxed) as u64)
}
fn sw_reset_energy_uj(_zone_id: u32) -> Result<(), &'static str> {
    SW_ENERGY.store(0, Ordering::Relaxed);
    Ok(())
}
fn sw_get_max_power_uw(zone_id: u32) -> Result<u64, &'static str> {
    let zones = POWERCAP_ZONES.read();
    let zone = zones.get(&zone_id).ok_or("Powercap zone not found")?;
    Ok(zone.max_power)
}
fn sw_get_power_uw(_zone_id: u32) -> Result<u64, &'static str> {
    Ok(45_000_000) // 45W
}
fn sw_set_power_limit(
    _zone_id: u32,
    _constraint_id: u32,
    _limit_uw: u64,
) -> Result<(), &'static str> {
    Ok(())
}
fn sw_get_power_limit(_zone_id: u32, constraint_id: u32) -> Result<u64, &'static str> {
    let constraints = POWERCAP_CONSTRAINTS.read();
    let con = constraints
        .get(&constraint_id)
        .ok_or("Constraint not found")?;
    Ok(con.power_limit_uw)
}
fn sw_get_time_window(_zone_id: u32, constraint_id: u32) -> Result<u64, &'static str> {
    let constraints = POWERCAP_CONSTRAINTS.read();
    let con = constraints
        .get(&constraint_id)
        .ok_or("Constraint not found")?;
    Ok(con.time_window_us)
}
fn sw_set_time_window(
    _zone_id: u32,
    _constraint_id: u32,
    _window_us: u64,
) -> Result<(), &'static str> {
    Ok(())
}

/// Software powercap zone ops.
pub fn software_powercap_ops() -> PowercapZoneOps {
    PowercapZoneOps {
        get_energy_uj: sw_get_energy_uj,
        reset_energy_uj: sw_reset_energy_uj,
        get_max_power_uw: sw_get_max_power_uw,
        get_power_uw: sw_get_power_uw,
        set_power_limit_uw: sw_set_power_limit,
        get_power_limit_uw: sw_get_power_limit,
        get_time_window_us: sw_get_time_window,
        set_time_window_us: sw_set_time_window,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    // Register a package-level power zone
    let pkg_zone = register_zone("sw-pkg", None, 125_000_000, software_powercap_ops())?; // 125W max

    // Add a constraint (power limit)
    let constraint = add_constraint(
        pkg_zone,
        "power-limit",
        125_000_000,
        10_000_000,
        100_000_000,
        10_000,
    )?;

    // Register a core-level sub-zone
    let core_zone = register_zone(
        "sw-core0",
        Some(pkg_zone),
        65_000_000,
        software_powercap_ops(),
    )?;

    // Add constraint to core zone
    let core_con = add_constraint(
        core_zone,
        "core-power-limit",
        65_000_000,
        5_000_000,
        100_000_000,
        10_000,
    )?;

    // Get current power
    let power = get_power_uw(pkg_zone)?;
    if power == 0 {
        return Err("Powercap: power reading is zero");
    }

    // Get energy counter
    let energy1 = get_energy_uj(pkg_zone)?;
    let energy2 = get_energy_uj(pkg_zone)?;
    if energy2 <= energy1 {
        return Err("Powercap: energy counter not incrementing");
    }

    // Set a power limit
    set_power_limit(pkg_zone, constraint, 100_000_000)?; // 100W
    let limit = get_power_limit(pkg_zone, constraint)?;
    if limit != 100_000_000 {
        return Err("Powercap: power limit not set correctly");
    }

    // Set time window
    set_time_window(pkg_zone, constraint, 50_000_000)?; // 50ms
    let window = get_time_window(pkg_zone, constraint)?;
    if window != 50_000_000 {
        return Err("Powercap: time window not set correctly");
    }

    // Reset energy
    reset_energy_uj(pkg_zone)?;

    let _ = (core_zone, core_con);
    Ok(())
}
